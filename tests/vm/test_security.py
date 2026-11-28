"""Security baseline on a real system (spec 09, 11).

An installed default KDE system gets this branch's samurai-settings-security and samurai-tools the
way an existing install would (pacman, then the lsm= kernel parameter), reboots, and every setting is
read back as the running system sees it. Then the things hardening could break (user namespaces,
bubblewrap, Flatpak, Firefox's sandbox, Wine) are run, and the firewall presets and the tailnet zone
are proven with real connections from a network namespace.

  SAMURAI_TEST_SEC_PKGS=<dir>   packages built by ci/build-pkgs.sh (required)
  SAMURAI_TEST_BASE_DISK=<qcow2> reuse an installed base (tests/vm/base_disk.py); else install one
"""
from __future__ import annotations

import json
import os
import pathlib
import re
import shutil

import pytest

import base_disk
from conftest import REPO, TEST_USER_PASSWORD
from vm import VM, WORK

USER = base_disk.USER
PKG = REPO / "pkgbuilds" / "samurai-settings-security"
LSM = "landlock,lockdown,yama,integrity,apparmor,bpf"
AS_USER = (f"runuser -u {USER} -- env HOME=/home/{USER} XDG_RUNTIME_DIR=/run/user/1000 TERM=dumb ")
PRESETS = ("kdeconnect steam-remote-play steam-transfer sunshine localsend syncthing samba ssh "
           "minecraft ollama web-dev web").split()


class Checks:
    def __init__(self, vm: VM):
        self.vm = vm
        self.ga = vm.qga(timeout=300)
        self.failures: list[str] = []

    def run(self, name: str, cmd: str, expect: list[str] = (), absent: list[str] = (),
            timeout: float = 180) -> str:
        _, out = self.ga.exec(f"{{ {cmd} ; }} 2>&1", timeout=timeout, check=False)
        self.vm.serial_read(max_wait=0.2)
        print(f"--- {name}\n{out[-3500:]}", flush=True)
        for text in expect:
            if text not in out:
                self.failures.append(f"{name}: missing {text!r}")
        for text in absent:
            if text in out:
                self.failures.append(f"{name}: unexpected {text!r}")
        return out

    def status(self, name: str = "status") -> dict:
        _, out = self.ga.exec("samurai security status --json", timeout=120, check=False)
        print(f"--- {name}\n{out}", flush=True)
        report = json.loads(out)
        return {item["id"]: item for item in report["items"]}

    def expect_status(self, items: dict, wanted: dict[str, str]) -> None:
        for key, value in wanted.items():
            got = items.get(key, {}).get("status")
            if got != value:
                self.failures.append(f"status {key}: {got} (wanted {value}): {items.get(key)}")


def _boot(vm: VM) -> Checks:
    vm.wait_for("login:", timeout=420)
    c = Checks(vm)
    c.run("settled", "systemctl is-system-running --wait; systemctl --failed --no-legend",
          ["running"], timeout=300)
    return c


def _reboot(vm: VM, c: Checks) -> Checks:
    c.run("sync", "sync")
    c.ga.close()
    vm.reboot()
    return _boot(vm)


def _expected_sysctls() -> list[tuple[str, str]]:
    pairs = []
    for line in (PKG / "80-samurai-security.conf").read_text().splitlines():
        if line.strip() and not line.startswith("#"):
            key, value = (part.strip() for part in line.split("=", 1))
            pairs.append((key, value))
    return pairs


def _check_sysctls(c: Checks, label: str) -> None:
    script = []
    for key, value in _expected_sysctls():
        path = "/proc/sys/" + key.replace(".", "/")      # '*' stays a shell glob
        script.append(f'for f in {path}; do v=$(cat "$f"); '
                      f'if [ "$v" = {value} ]; then echo "ok $f=$v"; else echo "BAD $f=$v want {value}"; fi; done')
    out = c.run(label, "; ".join(script) + "; echo SYSCTL-DONE", ["SYSCTL-DONE"], absent=["BAD ", "No such file"])
    assert out.count("ok /proc/sys/") >= len(_expected_sysctls())


# peer network: a veth pair whose far end lives in a namespace, so connections really cross the firewall
def _peer(ns: str, iface: str, net: int) -> str:
    return (f"ip netns add {ns} && ip link add {iface} type veth peer name {ns}0 netns {ns} && "
            f"ip addr add 10.{net}.0.1/24 dev {iface} && ip link set {iface} up && "
            f"ip -n {ns} addr add 10.{net}.0.2/24 dev {ns}0 && ip -n {ns} link set {ns}0 up")


def _reach(ns: str, net: int, port: int) -> str:
    return (f"ip netns exec {ns} curl -s -o /dev/null --max-time 5 http://10.{net}.0.1:{port}/ "
            f"&& echo REACH-{port}=yes || echo REACH-{port}=no")


def _check_welcome(vm: VM, c: Checks) -> None:
    """The window opens at the first Plasma login of a user, once."""
    c.run("welcome-cli", f"{AS_USER} samurai welcome --json",
          ['"schema":1', '"id":"recovery-key"', "sudo samurai security status"], absent=['"id":"tpm"'])
    login = ("systemctl restart sddm; "
             f"for i in $(seq 90); do pgrep -u {USER} -x plasmashell >/dev/null && break; sleep 2; done; "
             f"pgrep -u {USER} -x plasmashell >/dev/null && echo SESSION-OK")
    window = "pgrep -u " + USER + " -af '^[^ ]*samurai-toolbox --page welcom[e]'"
    marker = f"/home/{USER}/.local/state/samurai/welcome-shown"
    c.run("welcome-first-login", f"rm -f {marker}; mkdir -p /etc/sddm.conf.d && "
          f"printf '[Autologin]\\nUser={USER}\\nSession=plasma\\n' > /etc/sddm.conf.d/zz-vm-autologin.conf && {login}; "
          f"for i in $(seq 60); do {window} >/dev/null && break; sleep 2; done; {window} && echo WINDOW-OK; "
          f"stat -c '%U %n' {marker}", ["SESSION-OK", "WINDOW-OK", f"{USER} {marker}"], timeout=420)
    for _ in range(25):
        vm.serial_read(max_wait=1.0)
    print("welcome screenshot:", vm.screendump("security-welcome"))
    c.run("welcome-second-login", f"systemctl stop sddm; pkill -u {USER}; sleep 5; {login}; sleep 45; "
          f"{window} || echo NOT-SHOWN-AGAIN", ["SESSION-OK", "NOT-SHOWN-AGAIN"], timeout=420)


def test_security_baseline(mirror):
    pkg_dir = os.environ.get("SAMURAI_TEST_SEC_PKGS")
    if not pkg_dir:
        pytest.skip("SAMURAI_TEST_SEC_PKGS not set (ci/build-pkgs.sh <dir> samurai-settings-security samurai-tools)")
    files = sorted(pathlib.Path(pkg_dir).glob("*.pkg.tar.zst"))
    assert len(files) >= 2, f"packages missing in {pkg_dir}"
    for f in files:
        shutil.copy(f, mirror.dir / f.name)

    given = os.environ.get("SAMURAI_TEST_BASE_DISK")
    base = pathlib.Path(given) if given else WORK / "security-base.qcow2"
    if not base.is_file():
        base_disk.install(mirror, base, "security-base")
    disk = base_disk.overlay(base, "security")
    vm = VM("security", disk=str(disk), ram="8192", smp="4")
    vm.start()
    try:
        vm.serial_connect()
        c = _boot(vm)

        # --- install, as an existing system would ---------------------------------
        fetch = " && ".join(f"curl -fsS -o /tmp/{f.name} http://10.0.2.2:8966/{f.name}" for f in files)
        c.run("deps", "printf 'Server = https://geo.mirror.pkgbuild.com/$repo/os/$arch\\n' > /etc/pacman.d/mirrorlist && "
              "pacman -Sy --noconfirm --needed apparmor arch-audit && echo DEPS-OK", ["DEPS-OK"], timeout=900)
        c.run("install", f"{fetch} && pacman -U --noconfirm {' '.join('/tmp/' + f.name for f in files)} && echo PKG-OK",
              ["PKG-OK"], absent=["permissions differ", "exists in filesystem"], timeout=600)
        c.run("sudoers", "visudo -c; stat -c '%a %U %n' /etc/sudoers.d /etc/sudoers.d/50-samurai-security",
              ["parsed OK", "440 root /etc/sudoers.d/50-samurai-security", "750 root /etc/sudoers.d"],
              absent=["syntax error"])
        before = c.status("status-before-reboot")
        c.expect_status(before, {"apps.apparmor": "fail"})   # the kernel parameter is still missing
        c.run("lsm-param", f"samurai boot set param add lsm={LSM} && grep -c 'lsm={LSM}' /boot/limine.conf && echo PARAM-OK",
              ["PARAM-OK"])
        c.run("firewall-reload", "firewall-cmd --reload && samurai firewall open localsend --yes && echo OPEN-OK",
              ["OPEN-OK", "53317/tcp"])

        c = _reboot(vm, c)

        # --- effective settings ----------------------------------------------------
        _check_sysctls(c, "sysctl")
        c.run("sysctl-new-interface", "ip link add probe0 type dummy && sleep 2 && "
              "grep . /proc/sys/net/ipv4/conf/probe0/accept_redirects /proc/sys/net/ipv4/conf/probe0/send_redirects "
              "/proc/sys/net/ipv6/conf/probe0/accept_redirects; ip link del probe0",
              ["probe0/accept_redirects:0", "probe0/send_redirects:0"], absent=["redirects:1"])
        c.run("coredump", "systemd-analyze cat-config systemd/coredump.conf | grep -E '^(Storage|MaxUse)'; "
              f"{AS_USER} sh -c 'sleep 60 & kill -SEGV $!; wait $!'; sleep 6; coredumpctl list --no-pager | tail -3; "
              "ls /var/lib/systemd/coredump | head -3",
              ["Storage=external", "MaxUse=1G", "SIGSEGV present", "core.sh."])
        c.run("sudo-defaults", f"{AS_USER} sh -c 'echo {TEST_USER_PASSWORD} | sudo -S -k -l; "
              f"echo {TEST_USER_PASSWORD} | sudo -S -k id -u'",
              ["timestamp_timeout=5", "use_pty", f"password for {USER}: 0"])
        c.run("polkit", "cat /usr/share/polkit-1/rules.d/50-default.rules", ['"unix-group:wheel"'])
        c.run("apparmor", "cat /sys/kernel/security/lsm; echo; systemctl is-active apparmor.service; "
              "aa-status 2>&1 | head -12; grep -o 'lsm=[^ ]*' /proc/cmdline",
              ["apparmor", "active", "profiles are in enforce mode", f"lsm={LSM}"],
              absent=["\n0 profiles are in enforce mode"])
        c.run("arch-audit", "command -v arch-audit", ["/usr/bin/arch-audit"])

        # --- what hardening must not break -----------------------------------------
        c.run("userns", f"{AS_USER} unshare -Ur id -u && echo USERNS-OK", ["USERNS-OK"])
        c.run("bwrap", f"{AS_USER} bwrap --ro-bind / / --dev /dev --proc /proc --unshare-all --die-with-parent "
              "id && echo BWRAP-OK", ["BWRAP-OK"])
        c.run("ptrace-child", f"{AS_USER} sh -c 'command -v strace >/dev/null && strace -f true >/dev/null 2>&1 "
              "&& echo PTRACE-OK || python3 -c \"import ctypes,os,signal; l=ctypes.CDLL(None,use_errno=True); "
              "p=os.fork(); (l.ptrace(0,0,0,0), os._exit(0)) if p==0 else None; os.waitpid(p,0); "
              "print(\\\"PTRACE-OK\\\")\"'", ["PTRACE-OK"])
        c.run("wine", f"{AS_USER} sh -c 'cd /tmp && WINEDEBUG=-all timeout 300 wine cmd /c echo WINE-RAN-OK'",
              ["WINE-RAN-OK"], timeout=360)
        c.run("flatpak", "flatpak remote-add --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo && "
              "flatpak install -y --noninteractive flathub org.freedesktop.Platform//24.08 >/tmp/flatpak.log 2>&1; "
              f"tail -3 /tmp/flatpak.log; {AS_USER} flatpak run --command=sh org.freedesktop.Platform//24.08 "
              "-c 'echo FLATPAK-SANDBOX-OK; cat /etc/os-release | head -2'",
              ["FLATPAK-SANDBOX-OK"], timeout=900)
        mirror_probe = REPO / "tests" / "vm" / "firefox_probe.py"
        shutil.copy(mirror_probe, mirror.dir / mirror_probe.name)
        c.run("firefox", f"curl -fsS -o /tmp/ffprobe.py http://10.0.2.2:8966/{mirror_probe.name} && "
              f"{AS_USER} MOZ_REMOTE_ALLOW_SYSTEM_ACCESS=1 timeout 240 python3 -I /tmp/ffprobe.py",
              ['"DisableTelemetry": true', '"uBlock": true', '"telemetry_upload": false',
               '"hasUserNamespaces": true', '"canSandboxContent": true', "FIREFOX-PROBE-OK"], timeout=300)
        c.run("steam-sysctl-keep", "sysctl user.max_user_namespaces vm.max_map_count",
              ["vm.max_map_count = 2147483642"], absent=["max_user_namespaces = 0"])
        # the pattern is split so the journal entry of this very command can never match it
        c.run("apparmor-denials", "journalctl -b -g 'apparmor=\"DENI[E]D\"' --no-pager -q > /tmp/denied; "
              "cut -c1-300 /tmp/denied | tail -20; echo DENIALS $(wc -l </tmp/denied)", ["DENIALS 0"])
        c.run("no-llmnr", "resolvectl llmnr | head -2; ss -tulnH | grep -c ':5355 ' ; echo LLMNR-CHECK", 
              ["Global: no", "\n0\nLLMNR-CHECK"])

        # --- security status ---------------------------------------------------------
        items = c.status()
        c.expect_status(items, {
            "disk.encryption": "pass", "disk.tpm": "skip", "boot.secureboot": "warn",
            "apps.apparmor": "pass", "kernel.sysctl": "pass", "net.firewall": "pass",
            "net.ssh": "pass", "accounts.root": "pass", "accounts.sudo": "pass", "apps.firefox": "pass",
            "net.listening": "pass", "net.tailnet": "pass",
        })
        c.run("real-tailscale-zone", "ip -d link show tailscale0 | head -3; firewall-cmd --get-zone-of-interface=tailscale0",
              ["tun", "\nsamurai-tailnet"])
        c.run("status-text", "samurai security status; echo RC=$?", ["pass", "RC=0"])
        c.run("status-user", f"{AS_USER} samurai security status", ["need root", "UNKNOWN"])
        c.run("status-online", "samurai security status --online --json | grep -o '\"id\":\"packages.cve\"[^}]*'",
              ["packages.cve"], absent=['"status":"unknown"'])
        c.run("drift", "sysctl -qw kernel.kptr_restrict=0 && samurai security status | grep -A3 kernel.sysctl; "
              "sysctl -qw kernel.kptr_restrict=2", ["WARN", "kernel.kptr_restrict = 0 (default: 2)"])

        # --- firewall ------------------------------------------------------------------
        c.run("persisted", "firewall-cmd --zone=public --query-service=samurai-localsend; "
              "samurai firewall close localsend --yes; echo REMAINING $(firewall-cmd --permanent --zone=public --list-services)",
              ["yes", "done: closed", "REMAINING dhcpv6-client\n"])
        c.run("presets", "samurai firewall presets", PRESETS + ["47998-48000/udp", "11434/tcp", "1714-1764/tcp"],
              absent=["unavailable", "47990/tcp "])
        c.run("user-refused", f"{AS_USER} samurai firewall open ssh --yes; echo RC=$?; "
              "firewall-cmd --zone=public --query-service=ssh", ["RC=1", "no"], absent=["done:"])
        c.run("dry-run", "samurai firewall open sunshine --dry-run; "
              "firewall-cmd --zone=public --query-service=samurai-sunshine",
              ["dry run: nothing changed", "--permanent --zone=public --add-service=samurai-sunshine", "\nno"])
        c.run("no-tty", "samurai firewall open sunshine </dev/null; echo RC=$?; "
              "firewall-cmd --zone=public --query-service=samurai-sunshine", ["RC=1", "declined", "\nno"])
        cycle = ("for p in " + " ".join(PRESETS) + "; do samurai firewall open $p --yes >/tmp/o 2>&1 || { echo FAIL open $p; cat /tmp/o; }; "
                 "samurai firewall close $p --yes >/tmp/o 2>&1 || { echo FAIL close $p; cat /tmp/o; }; done; "
                 "echo LEFT $(firewall-cmd --zone=public --list-services) / $(firewall-cmd --permanent --zone=public --list-services); echo CYCLE-DONE")
        c.run("preset-cycle", cycle, ["CYCLE-DONE", "LEFT dhcpv6-client / dhcpv6-client"], absent=["FAIL "], timeout=300)
        c.run("overlap", "samurai firewall open steam-remote-play --yes >/dev/null && samurai firewall open steam-transfer --yes >/dev/null && "
              "samurai firewall close steam-transfer --yes >/dev/null; firewall-cmd --zone=public --query-service=steam-streaming; "
              "samurai firewall close steam-remote-play --yes >/dev/null && echo OVERLAP-OK", ["yes", "OVERLAP-OK"])

        # tailscaled created the real tailscale0 (checked above); a veth takes its name so that a
        # peer can connect through it
        c.run("peers", "systemctl stop tailscaled; ip link del tailscale0 2>/dev/null; "
              + _peer("lan", "lanveth", 91) + " && " + _peer("tail", "tailscale0", 92) +
              " && sleep 3 && echo ZONES lan=$(firewall-cmd --get-zone-of-interface=lanveth) "
              "tail=$(firewall-cmd --get-zone-of-interface=tailscale0)", ["tail=samurai-tailnet"])
        c.run("listeners", "mkdir -p /tmp/empty && (cd /tmp/empty && systemd-run -q --unit=probe-http python3 -I -m http.server 11434 --bind 0.0.0.0) "
              "&& systemd-run -q --unit=probe-http2 --working-directory=/tmp/empty python3 -I -m http.server 8080 --bind 0.0.0.0 && sleep 3 && "
              "curl -s -o /dev/null -w 'LOCAL %{http_code}\\n' http://127.0.0.1:11434/", ["LOCAL 200"])
        c.run("blocked", _reach("lan", 91, 11434) + "; " + _reach("tail", 92, 11434),
              ["REACH-11434=no"], absent=["=yes"])
        c.run("open-public", "samurai firewall open ollama --yes && " + _reach("lan", 91, 11434) + "; " +
              _reach("tail", 92, 11434) + " | sed s/REACH/TAIL/",
              ["REACH-11434=yes", "TAIL-11434=no"])
        c.run("close-public", "samurai firewall close ollama --yes && " + _reach("lan", 91, 11434), ["REACH-11434=no"])
        c.run("open-tailnet", "samurai firewall open 8080/tcp --zone samurai-tailnet --yes && " +
              _reach("tail", 92, 8080) + "; " + _reach("lan", 91, 8080) + " | sed s/REACH/LAN/",
              ["REACH-8080=yes", "LAN-8080=no"])
        c.run("fw-status", "samurai firewall status; samurai firewall status --json",
              ["zone public (default)", "zone samurai-tailnet", "8080/tcp", '"schema":1'])
        c.run("close-tailnet", "samurai firewall close 8080/tcp --zone samurai-tailnet --yes && " + _reach("tail", 92, 8080),
              ["REACH-8080=no"])
        c.run("zone-change", "samurai firewall zone lanveth home --yes && firewall-cmd --get-zone-of-interface=lanveth",
              ["done: lanveth is in zone home", "\nhome"])
        c.expect_status(c.status("status-tailnet"), {"net.tailnet": "pass", "net.listening": "warn"})

        # --- first boot window (spec 08) ----------------------------------------------
        if any(f.name.startswith("samurai-toolbox-gui-") for f in files):
            _check_welcome(vm, c)

        failed = c.run("final-units", "systemctl --failed --no-legend; echo UNITS-DONE", ["UNITS-DONE"])
        if re.search(r"\.(service|mount|socket)", failed):
            c.failures.append(f"failed units: {failed}")
        c.ga.close()
        assert not c.failures, "\n".join(c.failures)
    finally:
        vm.stop()
        disk.unlink(missing_ok=True)

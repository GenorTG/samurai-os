"""Daily-driver scenario (spec 11; Genor 2026-11-21: "test run in VM our arch
build with some mundane shit").

Install the default release profile from the OFFLINE ISO, boot it, log the
user into a real Plasma (Wayland) session and do what a person does on day one:

1. desktop: Plasma session up for the user, Dolphin + Konsole + Kate open (screendumps)
2. files: write a note in ~/Documents, zip + unzip it with the archiver, compare
3. network: NetworkManager online, DNS + HTTPS to archlinux.org
4. sound + time: PipeWire/WirePlumber user services active, NTP configured
5. snapshot before changes: `samurai snap create`, visible in `samurai snap list`
6. first full update of an offline-installed system: `samurai update` (network)
7. install an everyday app afterwards (`samurai apps` when present, else pacman)
8. reboot: still boots to SDDM with zero failed units

Each step prints a DAILY-<step> marker; failures show the step's output.
"""
from __future__ import annotations

import re

from conftest import INSTALL_TIMEOUT, REPO, find_iso, password_hash, with_test_autokey
from vm import VM

USER = "samurai"
# kwin picks the socket name (wayland-0 or wayland-1 when a greeter already
# holds 0): discover it instead of hardcoding
WLSOCK = "$(cd /run/user/1000 && ls wayland-[0-9]* 2>/dev/null | grep -v '\\.lock$' | head -1)"
UENV = (
    f"runuser -u {USER} -- env XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY={WLSOCK} "
    "QT_QPA_PLATFORM=wayland DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus"
)


def _profile(mirror) -> None:
    default = (REPO / "installer" / "profile-default.toml").read_text()
    new_user = (
        f'[user]\nname = "{USER}"\nshell = "bash"\ngroups = ["wheel"]\n'
        f'password_hash = "{password_hash()}"\n'
    )
    profile = re.sub(r"\[user\].*?(?=\n\[|\Z)", new_user, default, flags=re.S)
    (mirror.dir / "profile-daily.toml").write_text(with_test_autokey(profile))


def _step(ga, name: str, cmd: str, expect: list[str], timeout: float = 300) -> str:
    _, out = ga.exec(f"{{ {cmd} ; }} 2>&1; echo DAILY-{name}-RC=$?", timeout=timeout, check=False)
    print(f"--- {name}\n{out[-1500:]}")
    assert f"DAILY-{name}-RC=0" in out, f"{name} failed:\n{out[-3000:]}"
    for e in expect:
        assert e in out, f"{name}: expected {e!r}:\n{out[-3000:]}"
    return out


def test_daily_driver(mirror):
    _profile(mirror)
    vm = VM("daily", iso=str(find_iso()), disk_size="24G", ram="6144", smp="4")
    vm.start()
    try:
        vm.serial_connect()
        vm.login_root(timeout=280)
        ga = vm.ensure_qga()
        _, out = ga.exec(
            "curl -fsSL http://10.0.2.2:8966/profile-daily.toml -o /tmp/p.toml "
            "&& SAMURAI_TEST_AUTOKEY=1 SAMURAI_SERIAL_CONSOLE=1 samurai-install --profile /tmp/p.toml --disk /dev/vda --yes "
            ">/dev/null 2>&1; rc=$?; echo DAILY-INSTALL-RC=$rc; "
            "[ $rc -eq 0 ] || tail -n 60 /var/log/samurai/install.log",
            timeout=INSTALL_TIMEOUT,
        )
        assert "DAILY-INSTALL-RC=0" in out, out[-5000:]
        ga.close()

        vm.eject_iso()
        vm.reboot()
        vm.wait_for("login:", timeout=300)
        ga = vm.qga(timeout=120)

        # 1. a real Plasma session for the user (SDDM autologin, VM-only)
        _step(ga, "SESSION", (
            "mkdir -p /etc/sddm.conf.d && "
            f"printf '[Autologin]\\nUser={USER}\\nSession=plasma\\n' > /etc/sddm.conf.d/zz-vm-autologin.conf && "
            "systemctl restart sddm && "
            "for i in $(seq 90); do pgrep -u samurai -x plasmashell >/dev/null && "
            "ls /run/user/1000/wayland-[0-9] >/dev/null 2>&1 && break; sleep 2; done; "
            "pgrep -u samurai -x plasmashell && ls /run/user/1000/wayland-[0-9] && "
            "loginctl list-sessions --no-legend"
        ), ["wayland-"], timeout=240)
        ga.exec("sleep 20", timeout=40)  # let the panel + wallpaper settle
        vm.screendump("daily-01-desktop")

        _step(ga, "APPS-OPEN", (
            f"{UENV} sh -c 'mkdir -p ~/Documents && "
            "setsid dolphin ~/Documents >/dev/null 2>&1 < /dev/null & "
            "setsid konsole >/dev/null 2>&1 < /dev/null & "
            "setsid kate >/dev/null 2>&1 < /dev/null &'; "
            "sleep 12; pgrep -u samurai -x dolphin && pgrep -u samurai -x konsole && "
            "pgrep -u samurai -x kate"
        ), [], timeout=90)
        vm.screendump("daily-02-apps")

        # 2. files: note -> zip -> unzip -> identical
        _step(ga, "FILES", (
            f"{UENV} sh -c 'cd ~/Documents && "
            "printf \"groceries: rice, nori, tea\\n\" > note.txt && "
            "ark --batch --add-to notes.zip note.txt && mkdir -p out && "
            "ark --batch --destination out notes.zip && cmp note.txt out/note.txt && "
            "echo FILES-ROUNDTRIP-OK'"
        ), ["FILES-ROUNDTRIP-OK"], timeout=120)

        # 3. network
        _step(ga, "NET", (
            "nmcli -t -f STATE general; getent hosts archlinux.org && "
            "curl -fsS -o /dev/null -w 'HTTP=%{http_code}\\n' https://archlinux.org/"
        ), ["connected", "HTTP=200"], timeout=90)

        # hardware.nvidia = auto on a VM without an NVIDIA GPU: no NVIDIA stack
        _step(ga, "GPU-AUTO", (
            "pacman -Q mesa >/dev/null && ! pacman -Q nvidia-utils 2>/dev/null && echo NO-NVIDIA-OK"
        ), ["NO-NVIDIA-OK"], timeout=30)

        # firewall default (spec 09): running, inbound only DHCPv6, no ssh
        out = _step(ga, "FIREWALL", (
            "firewall-cmd --state && firewall-cmd --get-default-zone && firewall-cmd --list-services"
        ), ["running", "public", "dhcpv6-client"], timeout=60)
        assert " ssh" not in out and "\nssh" not in out, out

        # 4. sound + time
        _step(ga, "AUDIO-TIME", (
            f"{UENV} systemctl --user is-active pipewire wireplumber && "
            f"{UENV} pactl info | grep -E 'Server Name' && "
            "timedatectl show -p NTP --value"
        ), ["PipeWire", "yes"], timeout=60)

        # 5. snapshot before touching the system
        _step(ga, "SNAP", (
            "samurai snap create 'before first update' && samurai snap list | tail -3"
        ), ["before first update"], timeout=120)

        # 6. first full update of an offline-installed system
        _step(ga, "UPDATE", (
            "samurai update --noconfirm >/tmp/up.log 2>&1; rc=$?; tail -n 25 /tmp/up.log; "
            "[ $rc -eq 0 ] && pacman -Qu | wc -l"
        ), [], timeout=3000)

        # 7. an everyday app after the update
        _step(ga, "INSTALL-APP", (
            "if samurai apps --help >/dev/null 2>&1; then "
            "samurai apps install --yes htop; else pacman -S --needed --noconfirm htop; fi; "
            "pacman -Q htop"
        ), ["htop"], timeout=600)
        ga.close()

        # 8. survives a reboot after the update
        vm.reboot()
        vm.wait_for("login:", timeout=360)
        ga = vm.qga(timeout=120)
        _step(ga, "AFTER-REBOOT", (
            "systemctl is-active sddm && systemctl --failed --no-legend | wc -l && "
            "pgrep -u samurai -x plasmashell >/dev/null; sleep 25; "
            "systemctl --failed --no-legend"
        ), ["active"], timeout=180)
        _, failed = ga.exec("systemctl --failed --no-legend | wc -l", check=False)
        assert failed.strip().splitlines()[-1] == "0", failed
        vm.screendump("daily-03-after-update")
        ga.close()
        vm.stop()
    except BaseException:
        vm.stop()
        raise

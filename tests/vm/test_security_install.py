"""A NEW install gets the security baseline (spec 09, 11).

The branch's packages are not published, so this boots an offline ISO, adds them to the medium's
local repository (with apparmor and arch-audit, fetched by the live system), replaces the installer
scripts with the branch's, installs the default KDE profile behind a dead mirror and checks the
installed system. With an ISO built from this branch the preparation is a harmless no-op.

  SAMURAI_TEST_SEC_PKGS=<dir>   packages built by ci/build-pkgs.sh
"""
from __future__ import annotations

import json
import os
import pathlib
import shutil

import pytest

import base_disk
from conftest import INSTALL_TIMEOUT, REPO, find_iso
from vm import VM

CACHE = "/usr/share/samurai/pkg-cache"
SCRIPTS = ("samurai-postinstall", "samurai-profile-plan")


def test_new_install_is_hardened(mirror):
    pkg_dir = os.environ.get("SAMURAI_TEST_SEC_PKGS")
    if not pkg_dir:
        pytest.skip("SAMURAI_TEST_SEC_PKGS not set")
    files = sorted(pathlib.Path(pkg_dir).glob("*.pkg.tar.zst"))
    assert len(files) >= 3, f"packages missing in {pkg_dir}"
    for f in files:
        shutil.copy(f, mirror.dir / f.name)
    for script in SCRIPTS:
        shutil.copy(REPO / "installer" / script, mirror.dir / script)
    base_disk.default_profile(mirror, "profile-secinstall.toml")

    vm = VM("sec-install", iso=str(find_iso()), disk_size="48G", ram="8192", smp="4")
    vm.start()
    try:
        vm.serial_connect()
        vm.login_root(timeout=420)
        ga = vm.ensure_qga()

        def run(name: str, cmd: str, expect: str, timeout: float = 600) -> str:
            _, out = ga.exec(f"{{ {cmd} ; }} 2>&1", timeout=timeout, check=False)
            vm.serial_read(max_wait=0.2)
            print(f"--- {name}\n{out[-3000:]}", flush=True)
            assert expect in out, f"{name}: missing {expect!r}\n{out[-3000:]}"
            return out

        fetch = " && ".join(f"curl -fsS -o /tmp/new/{f.name} http://10.0.2.2:8966/{f.name}" for f in files)
        run("medium", f"mkdir -p /tmp/new /tmp/dl && {fetch} && "
            "pacman -Sy --noconfirm >/dev/null && pacman -Sw --noconfirm --cachedir /tmp/dl apparmor arch-audit >/dev/null && "
            f"cp /tmp/new/*.pkg.tar.zst {CACHE}/ && "
            f"for p in /tmp/dl/*.pkg.tar.zst; do cp \"$p\" {CACHE}/; done && "
            f"cd {CACHE} && ls samurai-offline.db* && "
            "db=$(ls samurai-offline.db.tar.* | grep -v -e sig -e old | head -1) && "
            "repo-add -q \"$db\" $(cd /tmp/new && ls *.pkg.tar.zst) $(cd /tmp/dl && ls *.pkg.tar.zst) && "
            "ls -l samurai-offline.db && echo MEDIUM-OK", "MEDIUM-OK")
        run("scripts", " && ".join(f"curl -fsS -o /usr/local/bin/{s} http://10.0.2.2:8966/{s}" for s in SCRIPTS)
            + " && grep -c apparmor /usr/local/bin/samurai-postinstall && echo SCRIPTS-OK", "SCRIPTS-OK")
        run("install", "curl -fsSL http://10.0.2.2:8966/profile-secinstall.toml -o /tmp/p.toml && "
            "SAMURAI_TEST_AUTOKEY=1 SAMURAI_SERIAL_CONSOLE=1 samurai-install --profile /tmp/p.toml --disk /dev/vda --yes "
            ">/dev/null 2>&1; rc=$?; echo INSTALL-RC=$rc; "
            "grep -E 'offline install|FATAL|lsm=' /var/log/samurai/install.log | cut -c1-300; "
            "[ $rc -eq 0 ] || tail -n 60 /var/log/samurai/install.log", "INSTALL-RC=0", timeout=INSTALL_TIMEOUT)
        run("offline-path", "grep -c 'offline install: closure installed' /var/log/samurai/install.log; "
            "grep -q 'offline install: closure installed' /var/log/samurai/install.log && echo OFFLINE-OK", "OFFLINE-OK")
        ga.close()

        vm.eject_iso()
        vm.reboot()
        vm.wait_for("login:", timeout=600)
        ga = vm.qga(timeout=300)
        run("settled", "systemctl is-system-running --wait; systemctl --failed --no-legend", "running")
        run("package", "pacman -Q samurai-settings-security apparmor arch-audit samurai-tools && echo PKG-OK", "PKG-OK")
        run("cmdline", "cat /proc/cmdline", "lsm=landlock,lockdown,yama,integrity,apparmor,bpf")
        run("apparmor", "systemctl is-enabled apparmor.service; systemctl is-active apparmor.service; "
            "aa-status | grep 'enforce mode'", "enabled\nactive\n")
        run("sysctl", "sysctl -n kernel.kptr_restrict kernel.kexec_load_disabled fs.suid_dumpable | tr '\\n' ' '",
            "2 1 0")
        run("sudo", "visudo -c", "50-samurai-security: parsed OK")
        run("version", "samurai version; command -v samurai; ls /etc/xdg/autostart/samurai-welcome.desktop", "0.4.0-18")
        out = run("status", "samurai security status --json", '"schema":1')
        items = {i["id"]: i["status"] for i in json.loads(out.strip().splitlines()[-1])["items"]}
        wanted = ("disk.encryption", "apps.apparmor", "kernel.sysctl", "net.firewall", "net.tailnet",
                  "net.listening", "net.ssh", "accounts.root", "accounts.sudo", "apps.firefox")
        wrong = {k: items.get(k) for k in wanted if items.get(k) != "pass"}
        assert not wrong, f"not passing on a new install: {wrong}"
        ga.close()
    finally:
        vm.stop()
        pathlib.Path(vm.disk_path).unlink(missing_ok=True)

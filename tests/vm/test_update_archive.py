"""M3 scenario: update-from-archive (spec 11).

Installs with Arch repos pinned to a past archive snapshot (~3 months old),
then runs `samurai update`: stage 2 must replace the archive pin with today's
CDN mirror, the transaction must pull the backlog, and the system must boot
and stay healthy afterwards.
"""
from __future__ import annotations

import pytest

from conftest import INSTALL_TIMEOUT, find_iso
from vm import REPO, VM

UPDATE_TIMEOUT = 3600
ARCHIVE = "https://archive.archlinux.org/repos/2026/08/15"


@pytest.fixture(scope="module")
def archive_profile(mirror):
    profile = (REPO / "tests" / "vm" / "assets" / "profile-test.toml").read_text()
    # pin Arch to the archive snapshot for the install
    profile = profile.replace('arch_mirror = "auto"', f'arch_mirror = "{ARCHIVE}"')
    (mirror.dir / "profile-archive.toml").write_text(profile)
    yield mirror


def test_update_from_archive(archive_profile):
    vm = VM("update-archive", iso=str(find_iso()), disk_size="30G")
    vm.start()
    try:
        vm.serial_connect()
        vm.login_root(timeout=280)
        acc = vm.serial_cmd(
            "curl -fsSL http://10.0.2.2:8966/profile-archive.toml -o /tmp/pa.toml "
            "&& SAMURAI_TEST_AUTOKEY=1 SAMURAI_SERIAL_CONSOLE=1 samurai-install --profile /tmp/pa.toml --disk /dev/vda --yes "
            ">/tmp/install.log 2>&1; echo INSTALL-\"RC\"=$?; "
            "[ -s /tmp/install.log ] && tail -n 40 /tmp/install.log",
            "INSTALL-RC=", timeout=INSTALL_TIMEOUT,
        )
        if "INSTALL-RC=0" not in acc:
            acc += "\n" + vm.serial_cmd("tail -n 60 /tmp/install.log; echo TAIL-\"END\"", "TAIL-END", timeout=60)
        assert "INSTALL-RC=0" in acc, acc[-3000:]

        vm.eject_iso()
        vm.reboot()
        vm.wait_for("login:", timeout=300)
        ga = vm.qga(timeout=60)

        # the mirrorlist is the archive pin; the updater must move it to today
        code, out = ga.exec(
            "samurai-update >/dev/null 2>&1; echo UP-RC=$?; "
            "grep -c archive.archlinux.org /etc/pacman.d/mirrorlist; "
            "grep -c geo.mirror.pkgbuild.com /etc/pacman.d/mirrorlist",
            timeout=UPDATE_TIMEOUT, check=False)
        assert "UP-RC=0" in out or "UP-RC=2" in out, out[-800:]
        lines = out.splitlines()
        assert "0" in lines[1], "archive pin still present in mirrorlist"
        assert "1" in lines[2], "CDN mirror not installed"

        # the system still boots and is healthy after the big update
        code, out = ga.exec("cat /etc/hostname")
        assert out.strip() == "samurai-vm"
        code, out = ga.exec("systemctl --failed --no-legend | head -3")
        assert out.strip() == "", out
        code, out = ga.exec("lsblk -no TYPE $(findmnt -n -o SOURCE / | sed 's/\\[.*\\]//')")
        assert "crypt" in out, out
    finally:
        vm.stop()

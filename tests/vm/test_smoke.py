"""M2 smoke suite (spec 11, subset): iso-boot + install-default.

Run from one command: tests/vm/run-smoke.sh
Scenarios:
  iso_boot          — the ISO boots on OVMF to the samurai live system
  install_default   — unattended install (LUKS2 on, spec 06 subvolumes),
                      then the disk boots and identity/fingerprint hold.

Security notes:
- the test profile is fetched from the harness-run local mirror, never baked
  into the public ISO (it carries a crypt hash of a throwaway test password)
- the installed system is verified via the guest agent (virtio-serial, no
  guest network port); serial is only used to drive the install
"""
from __future__ import annotations

import pytest

from conftest import find_iso
from vm import VM


INSTALL_TIMEOUT = 2400  # pacstrap over the network takes minutes (conftest constant kept local)

def test_iso_boot():
    vm = VM("smoke-iso-boot", iso=str(find_iso()))
    vm.start()
    try:
        vm.serial_connect()
        vm.login_root()  # live ISO autologins; no login prompt may appear
        qga = vm.ensure_qga()
        _, out = qga.exec("cat /etc/hostname")
        assert out.strip() == "samurai"
        _, out = qga.exec("systemctl is-active systemd-networkd || true")
        assert out.strip() == "active"
    finally:
        vm.stop()


def test_install_default(mirror):
    vm = VM("smoke-install", iso=str(find_iso()), disk_size="30G")
    vm.start()
    try:
        vm.serial_connect()
        vm.login_root()  # live ISO autologins; no login prompt may appear
        acc = vm.serial_cmd(
            "curl -fsSL http://10.0.2.2:8966/profile-test.toml -o /tmp/profile-test.toml "
            "&& SAMURAI_TEST_AUTOKEY=1 SAMURAI_SERIAL_CONSOLE=1 samurai-install --profile /tmp/profile-test.toml --disk /dev/vda --yes "
            "2>&1 | tail -c 3000; echo INSTALL-\"RC\"=${pipestatus[1]}",
            "INSTALL-RC=", timeout=INSTALL_TIMEOUT,
        )
        assert "INSTALL-RC=0" in acc, f"installer failed:\n{acc[-1500:]}"

        # reboot from disk: eject the ISO first (OVMF prefers the CD otherwise)
        vm.eject_iso()
        vm.reboot()
        vm.wait_for("login:", timeout=300)  # installed prompt: "samurai-vm login:"
        qga = vm.qga()  # must already be up (enabled at install); no serial login needed

        _, out = qga.exec("cat /etc/hostname")
        assert out.strip() == "samurai-vm"
        _, out = qga.exec("head -1 /usr/lib/os-release")
        assert "Samurai OS" in out, out
        _, out = qga.exec(
            "awk '/^\\[samurai\\]$/{found=1; next} found && /^\\[/{exit} found{print}' /etc/pacman.conf"
        )
        assert "Include = /etc/pacman.d/samurai-mirrorlist" in out, out
        _, ml = qga.exec("cat /etc/pacman.d/samurai-mirrorlist")
        assert "github.com/GenorTG/samurai-repo/releases/download/x86_64" in ml, ml
        assert "SigLevel = Required DatabaseRequired" in out, out
        _, out = qga.exec("test -s /etc/pacman.d/samurai-testing-mirrorlist && echo present")
        assert out.strip() == "present", "testing release route must be installed for live testing"
        _, out = qga.exec("lsblk -no TYPE $(findmnt -n -o SOURCE / | sed 's/\\[.*\\]//')")
        assert "crypt" in out, "expected LUKS2 root"
        _, out = qga.exec("findmnt -n -o SOURCE /")
        assert out.strip().endswith("[/@]"), f"root must be @ subvolume: {out}"
        _, out = qga.exec("systemctl --failed --no-legend | head -3")
        assert out.strip() == "", f"failed units: {out}"
        _, out = qga.exec("test -f /var/lib/samurai/hw-fingerprint && echo yes")
        assert out.strip() == "yes"
        _, out = qga.exec("test -f /var/lib/samurai/install-profile.toml && echo yes")
        assert out.strip() == "yes"
    finally:
        vm.stop()

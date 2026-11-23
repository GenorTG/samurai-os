"""M4 scenario: rollback-after-bad-update (spec 11).

Flow:
1. unattended install (LUKS2, btrfs) -> reboot -> healthy
2. BREAK the system: corrupt the initramfs on the ESP (the boot fails)
3. boot fails -> start the live ISO against the same disk
4. `samurai rescue rollback <pre-install snapshot>` from outside the system
5. reboot -> the restored system boots and is healthy

This covers the rescue-rollback path (spec 11's rollback-after-bad-update);
the snapshot-menu boot path (rollback-from-snapshot-boot) lands with the
Limine menu interaction work.
"""
from __future__ import annotations

import pytest

from conftest import INSTALL_TIMEOUT, find_iso
from vm import VM




def _install(mirror):
    vm = VM("rollback-inst", iso=str(find_iso()), disk_size="20G")
    vm.start()
    try:
        vm.serial_connect()
        vm.login_root(timeout=280)
        ga = vm.ensure_qga()
        _, acc = ga.exec(
            "curl -fsSL http://10.0.2.2:8966/profile-test.toml -o /tmp/profile-test.toml "
            "&& SAMURAI_TEST_AUTOKEY=1 SAMURAI_SERIAL_CONSOLE=1 samurai-install --profile /tmp/profile-test.toml --disk /dev/vda --yes "
            ">/dev/null 2>&1; rc=$?; echo INSTALL-RC=$rc; [ $rc -eq 0 ] || tail -n 60 /var/log/samurai/install.log",
            timeout=INSTALL_TIMEOUT, check=False,
        )
        assert "INSTALL-RC=0" in acc, acc[-2500:]
        ga.close()
        vm.eject_iso()
        vm.reboot()
        vm.wait_for("login:", timeout=300)
        ga = vm.qga(timeout=60)
        _, out = ga.exec("cat /etc/hostname")
        assert out.strip() == "samurai-vm"
        return vm, ga
    except BaseException:
        vm.stop()
        raise


def test_rollback_after_bad_update(mirror):
    vm, ga = _install(mirror)

    # BREAK: corrupt the initramfs on the ESP (boot will fail) — verify the
    # corruption actually landed (a silent dd failure looked like a healthy
    # system and broke this test before)
    code, out = ga.exec(
        "before=$(md5sum /boot/initramfs-linux.img | cut -d' ' -f1); "
        "dd if=/dev/zero of=/boot/initramfs-linux.img bs=1M count=48 conv=notrunc 2>&1; "
        "after=$(md5sum /boot/initramfs-linux.img | cut -d' ' -f1); "
        "sync; "
        "echo CORRUPT-OK=$([ \"$before\" != \"$after\" ] && echo yes || echo no)",
        timeout=60, check=False)
    assert "CORRUPT-OK=yes" in out, f"initramfs corruption failed: {out[-500:]}"
    vm.reboot()
    with pytest.raises((TimeoutError, RuntimeError)):
        vm.wait_for("login:", timeout=180)  # the boot must NOT reach the login
    vm.stop()

    # RESCUE: boot the live ISO against the same disk
    rescue = VM("rollback-rescue", iso=str(find_iso()), disk=vm.disk_path)
    rescue.start()
    try:
        rescue.serial_connect()
        rescue.login_root(timeout=280)
        ga2 = rescue.qga(timeout=60)
        code, out = ga2.exec(
            "samurai rescue mount >/dev/null 2>&1; "
            "SNAPID=$(ls -1 /mnt/.snapshots 2>/dev/null | grep -E '^[0-9]+$' | sort -n | head -1); "
            "echo SNAPID=$SNAPID",
            timeout=300, check=False)
        lines = [l for l in out.splitlines() if l.startswith("SNAPID=")]
        assert lines, f"no snapshot id found: {out[-600:]}"
        snapid = lines[0].split("=", 1)[1]
        assert snapid.isdigit()

        # rescue rollback from OUTSIDE the system (spec 06). Re-derive SNAPID
        # in the same shell: each ga.exec is a separate process, a variable set
        # in the previous exec is gone here (empty $SNAPID hit the usage error).
        code, out = ga2.exec(
            "SNAPID=$(ls -1 /mnt/.snapshots 2>/dev/null | grep -E '^[0-9]+$' | sort -n | head -1); "
            "umount /esp 2>/dev/null; samurai rescue unmount >/dev/null 2>&1; "
            "samurai rescue rollback $SNAPID 2>&1 | tail -c 900",
            timeout=600, check=False)
        print("ROLLBACK-OUT:", out[-1200:])
        assert "Rollback staged" in out, out[-800:]
        # post-rollback forensics: the regen must have rewritten the ESP
        # initramfs (the corrupted one is exactly 48 MiB of zeros)
        code, out3 = ga2.exec(
            "samurai rescue mount >/dev/null 2>&1; "
            "mount /dev/vda1 /esp 2>/dev/null; "
            "ls -l /esp/initramfs-linux.img 2>&1 | awk '{print \"size:\", $5}'; "
            "md5sum /esp/initramfs-linux.img 2>&1 | cut -c1-12",
            timeout=180, check=False)
        print("ESP-AFTER-ROLLBACK:", out3[-300:])
    finally:
        rescue.stop()

    # the restored system boots and is healthy
    restored = VM("rollback-verify", disk=vm.disk_path)
    restored.start()
    try:
        restored.wait_for("login:", timeout=300)
        ga3 = restored.qga(timeout=60)
        _, out = ga3.exec("cat /etc/hostname")
        assert out.strip() == "samurai-vm", out
        _, out = ga3.exec("lsblk -no TYPE $(findmnt -n -o SOURCE / | sed 's/\\[.*\\]//')")
        assert "crypt" in out, out
        _, out = ga3.exec("systemctl --failed --no-legend | head -3")
        assert out.strip() == "", out
    finally:
        restored.stop()

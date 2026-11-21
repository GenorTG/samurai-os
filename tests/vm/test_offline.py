"""M5.5 scenario: OFFLINE install (Genor 2026-11-20 — "a base version of the
system installable without a lot of curls").

The ISO bakes the default release profile's full package closure + sync dbs
(/usr/share/samurai/pkg-cache). With desktop=kde the installer installs from
those files via `pacman --sysroot /mnt -U` — no repos, no dbs, no mirrorlist,
no network.

Proof: this profile pins arch_mirror to a DEAD server. If any network path
were consulted, the install would fail. It must succeed offline.
"""
from __future__ import annotations

import re

from conftest import INSTALL_TIMEOUT, REPO, find_iso, password_hash
from vm import VM

DEAD_MIRROR = "https://offline.invalid/no-such-mirror"


def _offline_profile(mirror) -> None:
    """Default release profile (desktop=kde, autokey) + dead mirror + hashed user."""
    default = (REPO / "installer" / "profile-default.toml").read_text()
    new_user = (
        '[user]\nname = "samurai"\nshell = "bash"\ngroups = ["wheel"]\n'
        f'password_hash = "{password_hash()}"\n'
    )
    profile = re.sub(r"\[user\].*?(?=\n\[|\Z)", new_user, default, flags=re.S)
    profile = re.sub(r'arch_mirror = "auto"', f'arch_mirror = "{DEAD_MIRROR}"', profile)
    assert DEAD_MIRROR in profile, "failed to pin the dead mirror"
    (mirror.dir / "profile-offline.toml").write_text(profile)


def test_offline_install(mirror):
    _offline_profile(mirror)
    vm = VM("offline-inst", iso=str(find_iso()), disk_size="20G", ram="6144", smp="4")
    vm.start()
    try:
        vm.serial_connect()
        vm.login_root(timeout=280)
        # install via the guest agent (no serial typing — byte corruption killed
        # a serial-launched install once already)
        ga = vm.ensure_qga()
        code, out = ga.exec(
            "curl -fsSL http://10.0.2.2:8966/profile-offline.toml -o /tmp/p.toml "
            "&& samurai-install --profile /tmp/p.toml --disk /dev/vda --yes "
            ">/dev/null 2>&1; echo OFFLINE-INSTALL-RC=$?",
            timeout=INSTALL_TIMEOUT,
        )
        assert "OFFLINE-INSTALL-RC=0" in out, out[-1500:]

        # the -U path must actually have been taken (not the network fallback)
        code, ev = ga.exec(
            "grep -c 'offline install: closure installed' /var/log/samurai/install.log"
        )
        assert ev.strip().endswith("1") or "1" in ev.split()[-1], f"offline path not taken: {ev[-400:]}"
        ga.close()  # the QGA chardev accepts ONE host client: free it before reboot

        vm.eject_iso()
        vm.reboot()
        # autokey affordance: no boot passphrase; serial getty still answers
        vm.wait_for("login:", timeout=300)
        ga2 = vm.qga(timeout=120)
        code, out = ga2.exec(
            "systemctl is-active sddm; "
            "pacman -Q plasma-meta sddm networkmanager linux-cachyos linux-zen 2>&1; "
            "grep LookAndFeelPackage /etc/skel/.config/kdeglobals; "
            "systemctl --failed --no-legend | wc -l; "
            "findmnt -no FSTYPE /; "
            "pacman -Q linux 2>/dev/null | wc -l"
        )
        assert "active" in out.split("\n")[0], out[-800:]
        assert "plasma-meta" in out, out[-800:]
        assert "linux-cachyos" in out, out[-800:]
        _, gpu_packages = ga2.exec(
            "pacman -Q mesa vulkan-radeon vulkan-intel vulkan-nouveau "
            "intel-media-driver libva-intel-driver nvidia-utils "
            "linux-cachyos-nvidia-open linux-zen-headers nvidia-open-dkms"
        )
        assert all(
            package in gpu_packages
            for package in (
                "mesa ",
                "vulkan-radeon ",
                "vulkan-intel ",
                "vulkan-nouveau ",
                "intel-media-driver ",
                "libva-intel-driver ",
                "nvidia-utils ",
                "linux-cachyos-nvidia-open ",
                "linux-zen-headers ",
                "nvidia-open-dkms ",
            )
        ), gpu_packages[-1200:]
        assert "org.kde.breezedark.desktop" in out, out[-800:]
        assert "\n0" in out, out[-800:]
        assert "btrfs" in out, out[-800:]
        # both selected kernels installed; the unselected generic kernel absent
        assert out.strip().split("\n")[-1] == "0", out[-800:]

        _, boot_files = ga2.exec(
            "test -s /boot/vmlinuz-linux-cachyos && "
            "test -s /boot/initramfs-linux-cachyos.img && "
            "test -s /boot/vmlinuz-linux-zen && "
            "test -s /boot/initramfs-linux-zen.img && echo OFFLINE-BOOT-FILES-OK"
        )
        assert "OFFLINE-BOOT-FILES-OK" in boot_files, boot_files[-800:]
        ga2.close()

        vm.screendump("offline-01-greeter")
        vm.stop()
    except BaseException:
        vm.stop()
        raise

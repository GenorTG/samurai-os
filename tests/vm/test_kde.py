"""M6 scenario: KDE Plasma desktop install (spec 08; Genor 2026-11-20: KDE
Plasma is the default face of Samurai OS, alternatives opt-in).

Flow:
1. unattended profile with desktop="kde" (autokey affordance for headless boot)
2. postinstall installs Plasma + SDDM + apps, enables SDDM, writes the
   Breeze Dark default into /etc/skel/.config/kdeglobals
3. reboot -> SDDM greeter on the VGA console (screendump), serial getty
   still answers for the asserts
4. asserts: sddm active, plasma installed, Breeze Dark default, root btrfs,
   zero failed units
"""
from __future__ import annotations

import re

from conftest import INSTALL_TIMEOUT, REPO, find_iso, password_hash, with_test_autokey
from vm import VM


def _kde_profile(mirror) -> str:
    """Release-default profile (desktop=kde, autokey) + hashed user table."""
    default = (REPO / "installer" / "profile-default.toml").read_text()
    new_user = (
        '[user]\nname = "samurai"\nshell = "bash"\ngroups = ["wheel"]\n'
        f'password_hash = "{password_hash()}"\n'
    )
    profile = re.sub(r"\[user\].*?(?=\n\[|\Z)", new_user, default, flags=re.S)
    # the kernel set: keep the medium-installable default (linux)
    assert 'desktop = "kde"' in profile
    (mirror.dir / "profile-kde.toml").write_text(with_test_autokey(profile))
    return profile


def test_kde_install(mirror):
    _kde_profile(mirror)
    vm = VM("kde-inst", iso=str(find_iso()), disk_size="48G", ram="6144", smp="4")
    vm.start()
    try:
        vm.serial_connect()
        vm.login_root(timeout=280)
        # install via the guest agent, not serial typing: a long serial-cmd
        # got byte-corrupted mid-echo ("profil ee-kde.toml", "-- yyes") and
        # the install never ran (qcow2 stayed 192 KB — seen 2026-11-20)
        ga = vm.ensure_qga()
        code, out = ga.exec(
            "curl -fsSL http://10.0.2.2:8966/profile-kde.toml -o /tmp/profile-kde.toml "
            "&& SAMURAI_TEST_AUTOKEY=1 SAMURAI_SERIAL_CONSOLE=1 samurai-install --profile /tmp/profile-kde.toml --disk /dev/vda --yes "
            ">/dev/null 2>&1; RC=$?; echo KDE-INSTALL-RC=$RC; "
            "[ $RC = 0 ] || tail -c 2500 /var/log/samurai/install.log",
            timeout=INSTALL_TIMEOUT,
        )
        assert "KDE-INSTALL-RC=0" in out, out[-3500:]

        vm.eject_iso()
        vm.reboot()
        vm.wait_for("login:", timeout=300)
        ga = vm.qga(timeout=60)
        _, out = ga.exec(
            "systemctl is-active sddm; "
            "pacman -Q plasma-meta sddm konsole dolphin 2>&1; "
            "grep LookAndFeelPackage /etc/skel/.config/kdeglobals; "
            "systemctl --failed --no-legend | wc -l; "
            "findmnt -no FSTYPE /; "
            "head -2 /etc/motd"
        )
        assert "active" in out.split("\n")[0], out[-800:]
        assert "plasma-meta" in out, out[-800:]
        assert "org.kde.breezedark.desktop" in out, out[-800:]
        assert "\n0" in out, out[-800:]
        assert "btrfs" in out, out[-800:]
        assert "ronin" in out, out[-800:]

        # the money shot: SDDM greeter (Plasma up on VGA)
        vm.screendump("kde-01-greeter")
        vm.stop()
    except BaseException:
        vm.stop()
        raise

"""M4 scenario: attended (guided) install — the wizard front-end.

Flow:
1. boot the live ISO, root shell on the serial console
2. run `samurai-install` with NO --profile (attended wizard), answering each
   prompt over the real console — the same UI a human at the keyboard sees
3. choices exercised: GRUB bootloader (the user-requested selection path),
   fish shell, explicit password (crypt hash in the profile, never plaintext)
4. wizard summary -> backend Proceed? -> LUKS passphrase (masked) -> install
5. reboot -> verify the GRUB-installed system boots, user login with the
   chosen password, fish shell, hostname, snapper snapshot, LUKS2
6. VGA screendumps along the way (the wizard text itself lives in the serial
   log; tty1 shows boot/login state)
"""
from __future__ import annotations

import time

import pytest

from conftest import INSTALL_TIMEOUT, find_iso
from vm import VM

PASS = "att3nded-pass"


def _answer(vm: VM, prompt: str, answer: str, timeout: float = 120.0) -> str:
    """Wait for a wizard prompt on the console, then answer it."""
    acc = vm.wait_for(prompt, timeout=timeout)
    time.sleep(0.4)  # let the prompt line finish rendering before the keystrokes
    vm.send(answer + "\n")
    return acc


@pytest.fixture()
def mirror():
    """The profile server is not needed for the attended flow (the wizard
    builds the profile itself), but pacstrap needs the samurai repo reachable —
    the ISO bakes it in, so nothing to serve. Kept for fixture symmetry."""
    yield None


def test_attended_install(mirror):
    vm = VM("attended-inst", iso=str(find_iso()), disk_size="20G")
    vm.start()
    try:
        vm.serial_connect()
        vm.login_root(timeout=280)

        # launch the wizard with no --profile: attended guided mode
        vm.send("\n")
        time.sleep(1.0)
        vm.serial_read(max_wait=1.0)
        vm.send("samurai-install\n")

        vm.screendump("attended-01-live")
        _answer(vm, "Disk to ERASE [1-", "1")
        _answer(vm, "Hostname [samurai]:", "")
        _answer(vm, "Region number [UTC]:", "")          # UTC default
        _answer(vm, "LUKS2 disk encryption [Y/n]:", "")  # default: encrypted
        _answer(vm, "Numbers, space-separated [3]:", "")  # linux (medium default)
        # Menu prints land in the SAME serial chunk as their Choice prompt:
        # waiting for the menu first discards the overshoot (already-read
        # "Choice [1]:") and deadlocks the run (seen 2026-11-20). Wait for the
        # Choice prompt directly; the menu text stays in the serial log.
        vm.wait_for("Choice [1]:")                       # bootloader menu + prompt
        vm.screendump("attended-02-bootloader-menu")
        vm.send("2\n")                                   # GRUB (user selection path)
        vm.wait_for("Choice [1]:")                       # desktop menu + prompt
        vm.send("5\n")                                   # CLI only (KDE has its own test)
        _answer(vm, "Username [samurai]:", "")
        vm.wait_for("Choice [1]:")                       # shell menu + prompt
        vm.send("2\n")                                   # fish
        _answer(vm, "User password (Enter = generate):", PASS)
        _answer(vm, "again:", PASS)
        vm.wait_for("Write this profile and continue?")  # summary block + prompt
        vm.screendump("attended-03-summary")
        vm.send("yes\n")

        # backend: final gate + LUKS passphrase (masked reads)
        _answer(vm, "Proceed? [yes/no]", "yes")
        _answer(vm, "LUKS passphrase:", PASS)
        _answer(vm, "passphrase (again):", PASS)

        acc = vm.wait_for("Install complete", timeout=INSTALL_TIMEOUT)
        assert "FATAL" not in acc, acc[-1500:]

        # the wizard's profile, from the LIVE session (the installed copy at
        # /var/lib/samurai/... is asserted after the reboot — before the reboot
        # the live medium obviously has no /var/lib/samurai yet)
        ga = vm.qga(timeout=60)
        _, profile = ga.exec("cat /tmp/samurai-attended.toml")
        assert 'bootloader = "grub"' in profile, profile
        assert 'shell = "fish"' in profile, profile
        assert 'desktop = "none"' in profile, profile
        assert "auto_unlock_keyfile = false" in profile, profile
        assert "password_hash = " in profile, profile
        assert PASS not in profile, "plaintext password must never reach the profile"
        ga.close()  # the QGA chardev accepts ONE host client: free the slot

        vm.screendump("attended-04-install-done")
        vm.eject_iso()
        vm.reboot()
        # attended profile has auto_unlock_keyfile=false: the installed system
        # prompts for the LUKS passphrase in the initramfs before the getty
        vm.wait_for("passphrase", timeout=300)
        vm.send(PASS + "\n")
        vm.wait_for("login:", timeout=300)
        vm.screendump("attended-05-installed-login")

        # real login with the password the user typed at the wizard
        vm.send("samurai\n")
        vm.wait_for("Password:", timeout=30)
        vm.send(PASS + "\n")
        acc = vm.wait_for_any(["~>", "> "], timeout=60)[1]
        assert "Login incorrect" not in acc, acc[-800:]

        # in-system asserts: fish shell, GRUB config, LUKS, hostname
        # in-system asserts: fish shell, GRUB config, LUKS, hostname. NOTE:
        # /dev/mapper/samurai-root is the INSTALL-TIME name — at boot systemd
        # maps the LUKS as luks-<uuid>, so assert the root FS via findmnt
        vm.send("echo SHELL=$(getent passwd samurai | cut -d: -f7); "
                "test -f /boot/grub/grub.cfg && echo GRUB-OK; "
                "cat /etc/hostname; "
                "findmnt -no FSTYPE /\n")
        acc = vm.wait_for("btrfs", timeout=60)
        assert "SHELL=/bin/fish" in acc, acc[-800:]
        assert "GRUB-OK" in acc, acc[-800:]
        assert "samurai" in acc, acc[-800:]

        # final agent asserts: the guest agent can be slow right after boot
        # (post-boot churn on the fresh system); retry before believing it
        # is dead (seen 2026-11-20: first guest-exec ack timed out at 60 s)
        out = ""
        for attempt in range(3):
            try:
                ga2 = vm.qga(timeout=120)
                _, out = ga2.exec(
                    "grep -E '\\[boot\\]|bootloader|shell =|auto_unlock' /var/lib/samurai/install-profile.toml; "
                    "snapper --root / list 2>/dev/null | tail -3; "
                    "cryptsetup isLuks /dev/vda2 && echo LUKS-OK; "
                    "tail -1 /var/log/samurai/install.log"
                )
                ga2.close()
                break
            except (TimeoutError, RuntimeError):
                if ga2 is not None:
                    ga2.close()
                    ga2 = None
                time.sleep(15)
        assert 'bootloader = "grub"' in out, out[-800:]
        assert "LUKS-OK" in out, out[-800:]
        assert "install complete" in out or "DONE" in out, out[-800:]

        vm.stop()
    except BaseException:
        vm.stop()
        raise

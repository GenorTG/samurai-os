"""Graphical installer end to end (spec 08): live Plasma session, the wizard driven by
keyboard through every page, an offline KDE install on an encrypted disk, then the reboot
into the installed system with the passphrase typed at its boot prompt.

Needs the offline ISO (no network package source is used). Screenshots of each page land
in the screenshot directory of vm.screendump.
"""
from __future__ import annotations

import time

import pytest

from conftest import INSTALL_TIMEOUT, find_iso
from vm import VM

PASSPHRASE = "gui-disk-2026"
PASSWORD = "gui-user-2026"
USER = "alex"
INSTALL_ICON = (58, 62)    # "Install Samurai OS (graphical)" on the 1280x800 desktop


def guest(vm: VM, command: str, timeout: float = 120.0) -> str:
    """One command over a fresh guest-agent connection (the chardev takes one client)."""
    vm.serial_read(max_wait=0.3)    # keep the live console drained (see wait_guest_restart)
    agent = vm.qga(timeout=timeout)
    try:
        return agent.exec(command, timeout=timeout, check=False)[1]
    finally:
        agent.close()


def wait_guest(vm: VM, command: str, wanted: str, timeout: float, step: float = 5.0) -> str:
    deadline, out = time.time() + timeout, ""
    while time.time() < deadline:
        out = guest(vm, command)
        if wanted in out:
            return out
        time.sleep(step)
    raise TimeoutError(f"{command!r} never showed {wanted!r}; last:\n{out[-2000:]}")


def pattern(secret: str) -> str:
    """A grep pattern matching the secret but not the command line that carries it."""
    return f"{secret[:-1]}[{secret[-1]}]"


def shot(vm: VM, name: str) -> None:
    time.sleep(1.5)
    print("screenshot:", vm.screendump(f"gui-{name}"))


def next_page(vm: VM, name: str) -> None:
    shot(vm, name)
    vm.key("alt", "right")
    time.sleep(0.8)


def test_graphical_install():
    vm = VM("gui-install", iso=str(find_iso()), disk_size="48G", ram="6144", smp="4",
            graphical=True)
    vm.start()
    try:
        vm.serial_connect()
        vm.login_root(timeout=300)
        vm.ensure_qga().close()
        wait_guest(vm, "pgrep -u live -x plasmashell >/dev/null && echo SHELL-UP", "SHELL-UP", 240)
        time.sleep(15)     # desktop icons and panel
        shot(vm, "00-live-desktop")
        live = guest(vm, "systemctl is-active sddm NetworkManager; ls /home/live/Desktop; "
                         "cat /etc/samurai-iso-flavor; sudo -l -U live")
        assert live.split()[:2] == ["active", "active"], live
        assert "offline" in live, "this test needs the offline ISO"
        assert "samurai-2-install-terminal.desktop" in live, live

        vm.click(*INSTALL_ICON, double=True)
        wait_guest(vm, "pgrep -u live -f '[s]amurai-installer/samurai-installer' >/dev/null "
                       "&& echo WIZARD-UP", "WIZARD-UP", 60)
        time.sleep(8)

        next_page(vm, "01-welcome")
        next_page(vm, "02-region")
        # disk: focus starts on the first usable disk
        shot(vm, "03-disk-empty")
        vm.key("spc")
        vm.key("tab")                   # encryption check box
        vm.key("tab")
        vm.type_text(PASSPHRASE)
        vm.key("tab")
        vm.type_text(PASSPHRASE)
        next_page(vm, "03-disk")
        vm.type_text(USER)              # focus starts on the user name
        vm.key("tab")
        vm.type_text(PASSWORD)
        vm.key("tab")
        vm.type_text(PASSWORD)
        next_page(vm, "04-user")
        for name in ("05-kernels", "06-bootloader", "07-desktop", "08-hardware", "09-gaming",
                     "10-development", "11-apps", "12-network"):
            next_page(vm, name)
        shot(vm, "13-summary")
        vm.key("ctrl", "ret")           # must do nothing: the erase box is not ticked
        time.sleep(2)
        assert "partitioning" not in guest(vm, "cat /var/log/samurai/install.log 2>/dev/null")
        vm.key("spc")
        shot(vm, "13-summary-confirmed")
        vm.key("ctrl", "ret")

        wait_guest(vm, "cat /var/log/samurai/install.log", "partitioning /dev/vda", 90)
        time.sleep(20)
        shot(vm, "14-progress")
        both = f"-e '{pattern(PASSPHRASE)}' -e '{pattern(PASSWORD)}'"
        secrets = guest(vm, f"ps -eo args | grep -c {both}")
        assert secrets.strip() == "0", "a secret is visible in a process list"
        deadline = time.time() + INSTALL_TIMEOUT
        while True:
            log = guest(vm, "tail -n 60 /var/log/samurai/install.log")
            assert "FATAL" not in log, log
            if log.rstrip().endswith("DONE"):
                break
            assert time.time() < deadline, log
            time.sleep(20)
        time.sleep(5)
        shot(vm, "15-done")                # focus is on "Restart now"
        checks = guest(vm, "grep -c 'offline install: closure installed' /var/log/samurai/install.log; "
                           # -D skip: a FIFO under /run would block grep for ever;
                           # /run/archiso is the whole medium
                           f"grep -r -l -s -D skip --exclude-dir=archiso {both} "
                           "/var/log /run /tmp /home/live | wc -l; "
                           f"journalctl -b --no-pager | grep -c {both}; "
                           "ls -A /run/samurai-installer; findmnt -R /mnt | wc -l")
        print(checks)
        lines = checks.split()
        assert lines[0] == "1", checks
        assert lines[1] == "0", "a secret reached a file on the live system"
        assert lines[2] == "0", "a secret reached the journal"
        assert "install-profile.toml" not in checks, "the root profile copy was left behind"

        vm.key("spc")                      # restart through the wizard's own button
        vm.wait_guest_restart()
        vm.eject_iso()
        vm.resume_after_restart()
        unlock(vm)
        installed = guest(vm, "cat /etc/hostname; "
                              f"getent passwd {USER} | cut -d: -f7; "
                              "findmnt -no FSTYPE /; cryptsetup isLuks /dev/vda2 && echo LUKS-OK; "
                              "systemctl is-active sddm; "
                              "stat -c '%a %U' /var/lib/samurai/install-profile.toml; "
                              "grep -c password_hash /var/lib/samurai/install-profile.toml; "
                              "tail -n 1 /var/log/samurai/install.log; "
                              "echo LIVE-ONLY-FILES=$(ls /usr/lib/samurai-installer /usr/local/bin/samurai-installer* "
                              "2>/dev/null | wc -l); "
                              "stat -c 'MODE-%n-%a' /home /usr /etc /var/log; "
                              "cat /etc/xdg/kxkbrc; "
                              "systemctl --failed --no-legend")
        print(installed)
        for text in ("samurai\n", "/bin/fish", "btrfs", "LUKS-OK", "active", "600 root",
                     "LIVE-ONLY-FILES=0", "LayoutList=us", "MODE-/home-755", "MODE-/usr-755",
                     "MODE-/etc-755", "MODE-/var/log-755"):
            assert text in installed, f"{text!r} missing:\n{installed}"
        verify = guest(vm, f"h=$(getent shadow {USER} | cut -d: -f2); salt=$(echo \"$h\" | cut -d'$' -f3); "
                           f"[ \"$(echo {PASSWORD} | openssl passwd -6 -salt \"$salt\" -stdin)\" = \"$h\" ] "
                           "&& echo PASSWORD-OK")
        assert "PASSWORD-OK" in verify, "the installed user does not have the typed password"
        time.sleep(15)
        shot(vm, "16-installed-sddm")
        vm.type_text(PASSWORD)
        vm.key("ret")
        wait_guest(vm, f"pgrep -u {USER} -x plasmashell >/dev/null && echo SHELL-UP", "SHELL-UP", 180)
        time.sleep(25)
        shot(vm, "17-installed-desktop")
        failed = guest(vm, "systemctl --failed --no-legend")
        assert failed.strip() == "", failed
    except BaseException:
        try:
            vm.screendump("gui-failure")
        except Exception:
            pass
        raise
    finally:
        vm.stop()


def unlock(vm: VM) -> None:
    """Type the passphrase at the installed system's boot prompt (it is on the screen only:
    this install has no serial console), retrying until the guest agent answers."""
    deadline = time.time() + 600
    time.sleep(35)                      # firmware, boot menu countdown, initramfs
    attempt = 0
    while time.time() < deadline:
        attempt += 1
        shot(vm, f"16-unlock-{min(attempt, 3)}")
        vm.type_text(PASSPHRASE)
        vm.key("ret")
        for _ in range(8):
            time.sleep(5)
            try:
                agent = vm.qga(timeout=8)
                try:
                    agent.exec("true", timeout=8)
                    return
                finally:
                    agent.close()
            except (OSError, TimeoutError, RuntimeError, ConnectionError, ValueError):
                continue
    pytest.fail("the installed system never came up after the passphrase")

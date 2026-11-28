"""Published package channel, end to end: nothing local is injected into the guest.

Installs from the ISO, then updates with `samurai update` from the PUBLISHED repository.

  SAMURAI_TEST_CHANNEL=testing|stable SAMURAI_TEST_TOOLS_VERSION=0.4.0 pytest -q -s test_channel.py

SAMURAI_TEST_PROFILE_REF=<tag> when the ISO is older than the checkout: its installer rejects
newer profile keys, so the test profile is built from that release's default profile.
"""
from __future__ import annotations

import os
import re
import subprocess

import pytest

from conftest import INSTALL_TIMEOUT, REPO, find_iso, password_hash, with_test_autokey
from vm import VM

TESTING_BLOCK = r"[samurai-testing]\nInclude = /etc/pacman.d/samurai-testing-mirrorlist\nSigLevel = Required DatabaseRequired\n"


def _release_profile(ref: str) -> str:
    """The headless test profile for the installer shipped in release `ref`."""
    text = subprocess.run(["git", "-C", str(REPO), "show", f"{ref}:installer/profile-default.toml"],
                          check=True, capture_output=True, text=True).stdout
    user = ('[user]\nname = "samurai"\nshell = "bash"\ngroups = ["wheel"]\n'
            f'password_hash = "{password_hash()}"\n')
    text = re.sub(r"\[user\].*?(?=\n\[|\Z)", user, text, flags=re.S)
    text = text.replace('desktop = "kde"', 'desktop = "none"')
    text = text.replace('kernels = ["linux-cachyos", "linux-zen"]', 'kernels = ["linux"]')
    # older releases carry choices this scenario does not exercise: catalog-backed keys
    # (browsers/apps) and desktop-specific tables their validator only accepts together
    # with desktop = "kde" (it refuses [kde] once the desktop is "none")
    text = re.sub(r"(?m)^(browsers|apps)\s*=.*\n", "", text)
    # desktop-coupled tables: their validator refuses them without desktop = "kde"
    for table in ("kde", "gaming", "development"):
        text = re.sub(rf"(?ms)^\[{table}\]\n.*?(?=^\[|\Z)", "", text)
    assert 'desktop = "none"' in text and 'kernels = ["linux"]' in text, "release profile changed shape"
    return with_test_autokey(text)


def test_published_channel_update(mirror):
    channel = os.environ.get("SAMURAI_TEST_CHANNEL", "")
    want = os.environ.get("SAMURAI_TEST_TOOLS_VERSION", "")
    if channel not in ("testing", "stable") or not want:
        pytest.skip("set SAMURAI_TEST_CHANNEL=testing|stable and SAMURAI_TEST_TOOLS_VERSION")
    if ref := os.environ.get("SAMURAI_TEST_PROFILE_REF", ""):
        (mirror.dir / "profile-test.toml").write_text(_release_profile(ref))
    vm = VM(f"channel-{channel}", iso=str(find_iso()), disk_size="20G")
    vm.start()
    try:
        vm.serial_connect()
        vm.login_root(timeout=280)
        ga = vm.ensure_qga()
        _, acc = ga.exec(
            "curl -fsSL http://10.0.2.2:8966/profile-test.toml -o /tmp/profile-test.toml "
            "&& SAMURAI_TEST_AUTOKEY=1 SAMURAI_SERIAL_CONSOLE=1 samurai-install --profile /tmp/profile-test.toml --disk /dev/vda --yes "
            "> /tmp/channel-install.out 2>&1; rc=$?; echo INSTALL-RC=$rc; tail -n 40 /tmp/channel-install.out; "
            "[ $rc -eq 0 ] || tail -n 20 /var/log/samurai/install.log",
            timeout=INSTALL_TIMEOUT, check=False)
        assert "INSTALL-RC=0" in acc, acc[-2500:]
        ga.close()
        vm.eject_iso()
        vm.reboot()
        vm.wait_for("login:", timeout=300)
        ga = vm.qga()
        _, before = ga.exec("pacman -Q samurai-tools || echo none", check=False)
        print("tools right after install:", before.strip())
        if channel == "testing":
            # one line on purpose: the guest-agent wrapper does not survive heredocs
            ga.exec("sed -i '/^\\[samurai\\]$/i " + TESTING_BLOCK + "' /etc/pacman.conf")
            _, conf = ga.exec("grep -A3 '^\\[samurai-testing\\]' /etc/pacman.conf; grep -n '^\\[samurai' /etc/pacman.conf")
            assert "samurai-testing-mirrorlist" in conf and conf.index("[samurai-testing]") >= 0, conf
        _, out = ga.exec("samurai-update --noconfirm >/tmp/up.log 2>&1; echo UP-RC=$?; tail -25 /tmp/up.log", timeout=1500, check=False)
        assert "UP-RC=0" in out or "UP-RC=2" in out, out
        _, ver = ga.exec("pacman -Q samurai-tools samurai-release samurai-keyring samurai-mirrorlist", check=False)
        assert f"samurai-tools {want}" in ver, f"wanted {want}: {ver}\n{out}"
        _, out = ga.exec("pacman -Qkk samurai-tools 2>&1 | tail -1; pacman -Qo /usr/local/bin/samurai", check=False)
        assert " 0 altered files" in out and "owned by samurai-tools" in out, out
        _, out = ga.exec("samurai version; samurai health; echo health-rc=$?; samurai kernel list; samurai boot sync --check", timeout=300, check=False)
        assert f"samurai {want}" in out and "check(s)" in out and "match the installed kernels" in out, out
        _, out = ga.exec("systemctl is-enabled samurai-health.timer; test -f /usr/share/libalpm/hooks/95-samurai-boot.hook && echo HOOK", check=False)
        assert "enabled" in out and "HOOK" in out, out
        ga.exec("sync", check=False)
        ga.close()
        vm.reboot()
        vm.wait_for("login:", timeout=300)
        ga = vm.qga()
        _, out = ga.exec("systemctl --failed --no-legend | head -5; samurai version", check=False)
        assert out.strip().startswith(f"samurai {want}"), f"failed units or wrong version after reboot: {out}"
        ga.close()
    finally:
        vm.stop()

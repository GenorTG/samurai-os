"""Published package channel, end to end: nothing local is injected into the guest.

Installs from the ISO, then updates with `samurai update` from the PUBLISHED repository.

  SAMURAI_TEST_CHANNEL=testing|stable SAMURAI_TEST_TOOLS_VERSION=0.4.0 pytest -q -s test_channel.py
"""
from __future__ import annotations

import os

import pytest

from conftest import INSTALL_TIMEOUT, find_iso
from vm import VM

TESTING_BLOCK = "[samurai-testing]\\nInclude = /etc/pacman.d/samurai-testing-mirrorlist\\nSigLevel = Required DatabaseRequired\\n\\n"


def test_published_channel_update(mirror):
    channel = os.environ.get("SAMURAI_TEST_CHANNEL", "")
    want = os.environ.get("SAMURAI_TEST_TOOLS_VERSION", "")
    if channel not in ("testing", "stable") or not want:
        pytest.skip("set SAMURAI_TEST_CHANNEL=testing|stable and SAMURAI_TEST_TOOLS_VERSION")
    vm = VM(f"channel-{channel}", iso=str(find_iso()), disk_size="20G")
    vm.start()
    try:
        vm.serial_connect()
        vm.login_root(timeout=280)
        ga = vm.ensure_qga()
        _, acc = ga.exec(
            "curl -fsSL http://10.0.2.2:8966/profile-test.toml -o /tmp/profile-test.toml "
            "&& SAMURAI_TEST_AUTOKEY=1 SAMURAI_SERIAL_CONSOLE=1 samurai-install --profile /tmp/profile-test.toml --disk /dev/vda --yes "
            ">/dev/null 2>&1; rc=$?; echo INSTALL-RC=$rc; [ $rc -eq 0 ] || tail -n 60 /var/log/samurai/install.log",
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
            ga.exec(
                "python3 - <<'PY'\nfrom pathlib import Path\np=Path('/etc/pacman.conf'); t=p.read_text()\n"
                f"i=t.index('[samurai]'); p.write_text(t[:i]+'{TESTING_BLOCK}'+t[i:])\nPY")
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

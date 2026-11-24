"""M5: the packaged Rust `samurai` CLI works on a real Samurai OS system.

Boots the live ISO, installs the locally built samurai-tools (x86_64, Rust CLI) over the shell shim the
ISO ships (the same --overwrite set as installer/samurai-postinstall SAMURAI_OWNED_OVERWRITE; -dd because the live image has no pacman-contrib), then exercises the dispatcher contract.

  SAMURAI_TEST_TOOLS_PKG=/abs/path/samurai-tools-<ver>-x86_64.pkg.tar.zst pytest -q -s test_toolbox_pkg.py
"""
from __future__ import annotations

import os
import pathlib
import shutil

import pytest

from conftest import find_iso
from vm import VM


def test_rust_cli_on_live_system(mirror):
    pkg_env = os.environ.get("SAMURAI_TEST_TOOLS_PKG", "")
    if not pkg_env or not pathlib.Path(pkg_env).is_file():
        pytest.skip("set SAMURAI_TEST_TOOLS_PKG to a built samurai-tools x86_64 package")
    pkg = pathlib.Path(pkg_env)
    shutil.copy2(pkg, mirror.dir / pkg.name)
    vm = VM("toolbox-pkg", iso=str(find_iso()))
    vm.start()
    try:
        vm.serial_connect()
        vm.login_root()
        qga = vm.ensure_qga()
        _, out = qga.exec("head -c 2 /usr/local/bin/samurai")
        assert out.startswith("#!"), "live ISO should still ship the shell shim"
        _, out = qga.exec(
            f"curl -fsSL http://10.0.2.2:8966/{pkg.name} -o /tmp/st.pkg.tar.zst && "
            "pacman -U --noconfirm -dd --overwrite '/usr/local/bin/samurai' --overwrite '/usr/local/bin/samurai-*' "
            "--overwrite '/usr/share/samurai/catalog/*' --overwrite '/usr/lib/samurai/interventions/*' /tmp/st.pkg.tar.zst >/tmp/pacman-U.log 2>&1; "
            "echo U-RC=$?; tail -5 /tmp/pacman-U.log", timeout=300, check=False)
        assert "U-RC=0" in out, out
        _, out = qga.exec("pacman -Qo /usr/local/bin/samurai; head -c 4 /usr/local/bin/samurai | od -An -c")
        assert "owned by samurai-tools" in out and "E   L   F" in out, out
        _, out = qga.exec("samurai version")
        assert out.startswith("samurai ") and "(samurai-tools)" in out, out
        _, out = qga.exec("samurai help | head -1")
        assert out.strip() == "usage: samurai <command> [args]", out
        _, out = qga.exec("samurai bogus >/dev/null 2>&1; echo rc=$?", check=False)
        assert out.strip().endswith("rc=2"), out
        # executors still run through the Rust dispatcher
        _, out = qga.exec("samurai apps list --json 2>&1 | head -c 200; echo; samurai apps --help >/dev/null 2>&1; echo apps-rc=$?", check=False)
        assert "apps-rc=0" in out, out
        _, out = qga.exec("samurai snap --help >/dev/null 2>&1; echo snap-rc=$?; samurai update --help >/dev/null 2>&1; echo upd-rc=$?", check=False)
        assert "snap-rc=0" in out and "upd-rc=0" in out, out
        _, out = qga.exec("pacman -Qkk samurai-tools 2>&1 | tail -1")
        assert " 0 altered files" in out, out
    finally:
        vm.stop()

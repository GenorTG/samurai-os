"""An installed default KDE system as a reusable qcow2 base for scenarios that test packages.

The base is installed once from the OFFLINE ISO behind a dead mirror; each scenario then boots its
own overlay, so a run costs a boot instead of an install. The caller owns (and deletes) the base.
"""
from __future__ import annotations

import pathlib
import re
import subprocess

from conftest import INSTALL_TIMEOUT, REPO, find_iso, password_hash, with_test_autokey
from vm import VM, WORK

USER = "samurai"
DEAD_MIRROR = "https://offline.invalid/no-such-mirror"


def default_profile(mirror, name: str) -> None:
    """Serve the release default profile (desktop=kde, every default choice) as `name`."""
    default = (REPO / "installer" / "profile-default.toml").read_text()
    new_user = (f'[user]\nname = "{USER}"\nshell = "fish"\ngroups = ["wheel"]\n'
                f'password_hash = "{password_hash()}"\n')
    profile = re.sub(r"\[user\].*?(?=\n\[|\Z)", new_user, default, flags=re.S)
    profile = profile.replace('arch_mirror = "auto"', f'arch_mirror = "{DEAD_MIRROR}"')
    assert DEAD_MIRROR in profile and 'desktop = "kde"' in profile
    (mirror.dir / name).write_text(with_test_autokey(profile))


def install(mirror, disk: pathlib.Path, name: str = "base") -> None:
    """Install the default system onto a new qcow2 at `disk` and power the VM off."""
    default_profile(mirror, "profile-base.toml")
    disk.parent.mkdir(parents=True, exist_ok=True)
    disk.unlink(missing_ok=True)
    subprocess.run(["qemu-img", "create", "-f", "qcow2", str(disk), "48G"], check=True)
    vm = VM(name, iso=str(find_iso()), disk=str(disk), ram="8192", smp="4")
    vm.start()
    try:
        vm.serial_connect()
        vm.login_root(timeout=280)
        ga = vm.ensure_qga()
        _, out = ga.exec(
            "curl -fsSL http://10.0.2.2:8966/profile-base.toml -o /tmp/p.toml "
            "&& SAMURAI_TEST_AUTOKEY=1 SAMURAI_SERIAL_CONSOLE=1 samurai-install --profile /tmp/p.toml "
            "--disk /dev/vda --yes >/dev/null 2>&1; rc=$?; echo BASE-INSTALL-RC=$rc; "
            "[ $rc -eq 0 ] || tail -n 80 /var/log/samurai/install.log; sync",
            timeout=INSTALL_TIMEOUT,
        )
        print(out[-4000:], flush=True)
        ok = "BASE-INSTALL-RC=0" in out
        ga.close()
    finally:
        vm.stop()
    if not ok:
        disk.unlink(missing_ok=True)
        raise AssertionError("base install failed")


def overlay(base: pathlib.Path, name: str) -> pathlib.Path:
    """A fresh copy-on-write disk on top of `base`."""
    path = WORK / f"{name}.qcow2"
    path.unlink(missing_ok=True)
    subprocess.run(["qemu-img", "create", "-f", "qcow2", "-b", str(base.resolve()), "-F", "qcow2",
                    str(path)], check=True)
    return path

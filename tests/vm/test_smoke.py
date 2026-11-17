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

import functools
import http.server
import pathlib
import socketserver
import subprocess
import threading

import pytest

from vm import REPO, VM

INSTALL_TIMEOUT = 2400  # pacstrap over the network takes minutes
TEST_USER_PASSWORD = "smurai-smoke-2026"  # throwaway, VM-only


def find_iso() -> pathlib.Path:
    built = sorted((REPO / "build").glob("samurai-os-*.iso"))
    if not built:
        pytest.skip("no built ISO in build/ — run ci/build-iso.sh first")
    return built[-1]


@functools.cache
def password_hash() -> str:  # not a test: helper returning the throwaway hash
    # sha-512 crypt hash of the throwaway password, generated on the host
    out = subprocess.run(
        ["openssl", "passwd", "-6", TEST_USER_PASSWORD], capture_output=True, text=True, check=True
    )
    return out.stdout.strip()


class Mirror:
    """Serves tests/vm/assets/ to the VM at http://10.0.2.2:8966/."""

    def __init__(self, port: int = 8966):
        self.dir = REPO / "tests" / "vm" / "assets"
        handler = functools.partial(http.server.SimpleHTTPRequestHandler, directory=str(self.dir))
        socketserver.TCPServer.allow_reuse_address = True
        self.httpd = socketserver.TCPServer(("0.0.0.0", port), handler)
        self.thread = threading.Thread(target=self.httpd.serve_forever, daemon=True)

    def __enter__(self):
        self.dir.mkdir(parents=True, exist_ok=True)
        self.thread.start()
        return self

    def __exit__(self, *a):
        self.httpd.shutdown()


@pytest.fixture(scope="module")
def mirror():
    with Mirror() as m:
        import re as _re
        import tomllib
        default = (REPO / "installer" / "profile-default.toml").read_text()
        # REPLACE the [user] table (appending a second one would be invalid TOML)
        new_user = (
            '[user]\nname = "samurai"\nshell = "bash"\ngroups = ["wheel"]\n'
            f'password_hash = "{password_hash()}"\n'
        )
        profile = _re.sub(r"\[user\].*?(?=\n\[|\Z)", new_user, default, flags=_re.S)
        tomllib.loads(profile)  # must parse: belt and braces against duplicate tables
        m.dir.mkdir(parents=True, exist_ok=True)
        (m.dir / "profile-test.toml").write_text(profile)
        yield m  # Mirror.__enter__ already started the server thread


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
            "&& samurai-install --profile /tmp/profile-test.toml --disk /dev/vda --yes "
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

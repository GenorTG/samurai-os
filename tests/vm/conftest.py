"""Shared fixtures/helpers for the Samurai OS VM tests (spec 11)."""
from __future__ import annotations

import functools
import http.server
import pathlib
import socketserver
import subprocess
import threading

import pytest

from vm import REPO

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


@pytest.fixture(scope="session")
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

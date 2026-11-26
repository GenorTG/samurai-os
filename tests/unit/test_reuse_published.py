"""A published package name keeps its bytes: reuse identical contents, refuse different ones."""
from __future__ import annotations

import functools
import http.server
import shutil
import subprocess
import threading

import pytest

from conftest import REPO

SCRIPT = REPO / "ci" / "reuse-published"
pytestmark = pytest.mark.skipif(shutil.which("bsdtar") is None or shutil.which("curl") is None,
                                reason="needs bsdtar and curl")


def _pkg(path, body: str, date: int, mode: int = 0o644):
    root = path.parent / f"src-{date}-{abs(hash(body))}-{mode}"
    (root / "usr").mkdir(parents=True)
    (root / ".PKGINFO").write_text(f"# built {date}\npkgname = x\nbuilddate = {date}\n")
    (root / ".BUILDINFO").write_text(str(date))
    (root / "usr" / "file").write_text(body)
    (root / "usr" / "file").chmod(mode)
    subprocess.run(["bsdtar", "-cf", str(path), "-C", str(root), ".PKGINFO", ".BUILDINFO", "usr"], check=True)


@pytest.fixture
def published(tmp_path):
    pub = tmp_path / "pub"
    pub.mkdir()
    handler = functools.partial(http.server.SimpleHTTPRequestHandler, directory=str(pub))
    srv = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    yield pub, f"http://127.0.0.1:{srv.server_address[1]}"
    srv.shutdown()


def _run(pkg, url):
    return subprocess.run([str(SCRIPT), str(pkg), url], capture_output=True, text=True)


def test_unpublished_version_is_kept(tmp_path, published):
    pkg = tmp_path / "x-1-1-any.pkg.tar"
    _pkg(pkg, "a", 1)
    before = pkg.read_bytes()
    assert _run(pkg, published[1]).returncode == 0
    assert pkg.read_bytes() == before


def test_identical_contents_reuse_the_published_bytes_and_signature(tmp_path, published):
    pub, url = published
    name = "x-1-1-any.pkg.tar"
    _pkg(pub / name, "a", 1)
    (pub / f"{name}.sig").write_text("sig")
    pkg = tmp_path / name
    _pkg(pkg, "a", 999)
    r = _run(pkg, url)
    assert r.returncode == 0, r.stderr
    assert pkg.read_bytes() == (pub / name).read_bytes()
    assert (tmp_path / f"{name}.sig").read_text() == "sig"


@pytest.mark.parametrize("body,mode", [("changed", 0o644), ("a", 0o755)])
def test_changed_contents_or_mode_demand_a_pkgrel_bump(tmp_path, published, body, mode):
    pub, url = published
    name = "x-1-1-any.pkg.tar"
    _pkg(pub / name, "a", 1)
    pkg = tmp_path / name
    _pkg(pkg, body, 2, mode)
    r = _run(pkg, url)
    assert r.returncode == 1 and "bump pkgrel" in r.stderr, r.stderr


def test_unreachable_channel_fails_closed(tmp_path):
    pkg = tmp_path / "x-1-1-any.pkg.tar"
    _pkg(pkg, "a", 1)
    assert _run(pkg, "http://127.0.0.1:9").returncode == 1

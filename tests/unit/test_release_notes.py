"""ci/release-iso.sh: release notes claim only what the per-release files say."""
from __future__ import annotations

import shutil
import subprocess

import pytest

from conftest import REPO


@pytest.fixture
def release(tmp_path):
    """The script in a scratch repository (it writes and wipes <repo>/dist/<tag>)."""
    (tmp_path / "ci").mkdir()
    shutil.copy(REPO / "ci" / "release-iso.sh", tmp_path / "ci")
    (tmp_path / "VERSION").write_text("9.9.9.9\n")
    isos = []
    for flavor in ("online", "offline"):
        iso = tmp_path / f"samurai-os-2026.01.01-{flavor}-x86_64.iso"
        iso.write_bytes(flavor.encode() * 64)
        isos.append(str(iso))

    def run(*args):
        result = subprocess.run(["bash", str(tmp_path / "ci" / "release-iso.sh"), *isos, *args],
                                capture_output=True, text=True, cwd=tmp_path)
        notes = tmp_path / "dist" / "v9.9.9.9" / "RELEASE-NOTES.md"
        return result, notes.read_text() if notes.is_file() else ""

    run.dir = tmp_path
    return run


def test_publishing_without_a_verified_file_is_refused_before_any_work(release):
    result, notes = release("--publish")
    assert result.returncode == 2 and "--verified" in result.stderr
    assert not notes and not (release.dir / "dist").exists()


@pytest.mark.parametrize("content", [None, "  \n\n"])
def test_missing_or_blank_file_is_refused(release, content):
    path = release.dir / "verified.md"
    if content is not None:
        path.write_text(content)
    result, _ = release("--verified", str(path))
    assert result.returncode == 2 and "missing or empty" in result.stderr


def test_the_script_itself_claims_nothing(release):
    result, notes = release()
    assert result.returncode == 0, result.stderr
    assert "### Verified" not in notes and "### New in this release" not in notes
    assert "cannot be published" in result.stderr and "### Pick an installer" in notes


def test_supplied_sections_are_inserted_verbatim(release):
    verified = release.dir / "verified.md"
    verified.write_text("- smoke scenario `$(id)` ${HOME}: passed\n")
    new = release.dir / "new.md"
    new.write_text("- a thing\n")
    result, notes = release("--verified", str(verified), "--new", str(new))
    assert result.returncode == 0, result.stderr
    assert "### Verified\n- smoke scenario `$(id)` ${HOME}: passed\n" in notes
    assert notes.index("### New in this release\n- a thing\n") < notes.index("### Verified")

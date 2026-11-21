"""Installer integration of `[packages] apps`, ISO copies and the samurai-tools PKGBUILD."""
from __future__ import annotations

import hashlib
import re
import subprocess

import pytest

from conftest import REPO, load_script

VALIDATOR = REPO / "installer" / "validate-profile.py"
DEFAULT_PROFILE = REPO / "installer" / "profile-default.toml"


@pytest.fixture(scope="module")
def validator():
    return load_script(VALIDATOR, "validate_profile")


def profile_with(tmp_path, apps_line: str):
    text = DEFAULT_PROFILE.read_text()
    text = re.sub(r'(?m)^desktop = "kde"$', f'desktop = "kde"\n{apps_line}', text, count=1)
    assert apps_line in text
    path = tmp_path / "p.toml"
    path.write_text(text)
    return path


def test_default_profile_still_valid(validator):
    validator.validate(str(DEFAULT_PROFILE))


@pytest.mark.parametrize("line", [
    'apps = []',
    'apps = ["firefox", "htop", "@gaming", "@dev"]',
])
def test_valid_apps(validator, tmp_path, line):
    validator.validate(str(profile_with(tmp_path, line)))


@pytest.mark.parametrize("line, msg", [
    ('apps = ["nosuchapp"]', "unknown app id"),
    ('apps = ["Firefox"]', "unknown app id"),           # exact ids only, no fuzzy matching
    ('apps = ["@nope"]', "unknown bundle"),
    ('apps = ["linuxLts"]', "not offered"),
    ('apps = ["firefox; rm -rf /"]', "no free-form"),
    ('apps = ["$(id)"]', "no free-form"),
    ('apps = "firefox"', "no free-form"),
])
def test_invalid_apps(validator, tmp_path, capsys, line, msg):
    with pytest.raises(SystemExit):
        validator.validate(str(profile_with(tmp_path, line)))
    assert msg in capsys.readouterr().err


def test_validator_cli(tmp_path):
    ok = subprocess.run(["python3", str(VALIDATOR), str(profile_with(tmp_path, 'apps = ["@media"]'))],
                        capture_output=True, text=True)
    assert ok.returncode == 0, ok.stderr


def test_postinstall_syntax_and_apps_step():
    script = (REPO / "installer" / "samurai-postinstall").read_text()
    subprocess.run(["bash", "-n", str(REPO / "installer" / "samurai-postinstall")], check=True)
    assert "samurai-apps" in script and "--skip-unavailable" in script
    assert "SAMURAI_OFFLINE" in script and "firstboot-notes" in script
    assert "pacman -Sy " not in script.split("### optional apps")[1].split("### users")[0]


# ISO airootfs carries plain copies (ci/build-iso.sh tars iso/ into the build profile);
# they must never drift from the sources.
COPIES = {
    "iso/airootfs/usr/local/bin/samurai": "updates/samurai",
    "iso/airootfs/usr/local/bin/samurai-apps": "updates/samurai-apps",
    "iso/airootfs/usr/local/bin/validate-profile.py": "installer/validate-profile.py",
    "iso/airootfs/usr/local/bin/samurai-postinstall": "installer/samurai-postinstall",
    "iso/airootfs/usr/share/samurai/catalog/samurai.toml": "catalog/samurai.toml",
    "iso/airootfs/usr/share/samurai/catalog/garuda-setup-wizard.toml": "catalog/garuda-setup-wizard.toml",
}


@pytest.mark.parametrize("copy, source", sorted(COPIES.items()))
def test_iso_copies_in_sync(copy, source):
    assert (REPO / copy).read_bytes() == (REPO / source).read_bytes(), f"{copy} drifted from {source}"


def _bash_array(text: str, name: str) -> list[str]:
    m = re.search(rf"(?ms)^{name}=\((.*?)^\)", text)
    assert m, name
    return [t.strip("'\"") for t in m.group(1).split() if not t.startswith("#")]


def test_pkgbuild_sources_and_checksums():
    pkgdir = REPO / "pkgbuilds" / "samurai-tools"
    text = (pkgdir / "PKGBUILD").read_text()
    sources = _bash_array(text, "source")
    sums = _bash_array(text, "sha256sums")
    assert len(sources) == len(sums)
    origin = {"0001-update-mechanism-demo.toml": "updates/interventions/0001-update-mechanism-demo.toml",
              "0001-update-mechanism-demo.sh": "updates/interventions/0001-update-mechanism-demo.sh",
              "garuda-setup-wizard.toml": "catalog/garuda-setup-wizard.toml",
              "samurai.toml": "catalog/samurai.toml"}
    for src, digest in zip(sources, sums):
        path = REPO / origin.get(src, f"updates/{src}")
        assert hashlib.sha256(path.read_bytes()).hexdigest() == digest, src
    assert "samurai-apps" in sources
    assert "/usr/share/samurai/catalog/samurai.toml" in text

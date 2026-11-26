"""The terminal wizard's software pages (spec 08 pages 8-11): the real shell
functions from samurai-install, fed typed answers, must write a valid profile."""
from __future__ import annotations

import os
import subprocess
import tomllib

import pytest

from conftest import REPO, load_script

INSTALL = REPO / "installer" / "samurai-install"


def wizard_functions() -> str:
    text = INSTALL.read_text()
    start = text.index("### Wizard input helpers")
    end = text.index("### Attended guided wizard")
    return text[start:end]


def run_wizard(tmp_path, desktop: str, answers: list[str]) -> subprocess.CompletedProcess:
    bin_dir = tmp_path / "bin"
    bin_dir.mkdir(exist_ok=True)
    shim = bin_dir / "samurai-profile-plan"
    shim.write_text(f"#!/bin/sh\nexec python3 {REPO}/installer/samurai-profile-plan "
                    f"--catalog {REPO}/catalog \"$@\"\n")
    shim.chmod(0o755)
    script = (
        "set -Eeuo pipefail\n"
        'die() { echo "FATAL: $*" >&2; exit 1; }\n'
        + wizard_functions()
        + f"WIZ_DESKTOP={desktop}; WIZ_WIDGETS=true; WIZ_EFFECTS=false\n"
        "wizard_software >&2\nwizard_network >&2\n"
        'printf \'[packages]\\ndesktop = "%s"\\nbrowsers = %s\\napps = %s\\n\' "$WIZ_DESKTOP" '
        '"$(toml_list "${WIZ_BROWSERS[@]}")" "$(toml_list "${WIZ_APPS[@]}")"\n'
        "wizard_software_toml\n"
    )
    env = dict(os.environ, PATH=f"{bin_dir}:{os.environ['PATH']}")
    return subprocess.run(["bash", "-c", script], input="".join(a + "\n" for a in answers),
                          capture_output=True, text=True, env=env, timeout=60)


def validate_sections(profile: dict, desktop: str) -> None:
    validator = load_script(REPO / "installer" / "validate-profile.py", "validate_profile_w")
    validator.validate_software(profile, desktop)
    validator.validate_apps(profile["packages"]["apps"])


def test_enter_everywhere_gives_the_spec_defaults(tmp_path):
    result = run_wizard(tmp_path, "kde", [""] * 14)
    assert result.returncode == 0, result.stderr
    profile = tomllib.loads(result.stdout)
    default = tomllib.loads((REPO / "installer" / "profile-default.toml").read_text())
    assert profile["packages"]["browsers"] == ["firefox"]
    assert profile["kde"] == {"theme": "breeze", "monitor_widgets": True, "reduced_effects": False}
    for section in ("gaming", "development", "network"):
        assert profile[section] == default[section], section
    validate_sections(profile, "kde")


def test_choices_reach_the_profile(tmp_path):
    answers = ["", "", "2 8",      # browsers default, gaming yes, leave out heroic + wine
               "1 4", "2", "2",    # python 3.10 + 3.13, no conda, rootless docker
               "1", "y",           # first AI CLI, ollama
               "@office htop",     # extra apps
               "n", "3", "y"]      # no tailscale, nginx, ssh
    result = run_wizard(tmp_path, "kde", answers)
    assert result.returncode == 0, result.stderr
    profile = tomllib.loads(result.stdout)
    assert profile["gaming"]["heroic"] is False and profile["gaming"]["wine"] is False
    assert profile["gaming"]["steam"] is True
    dev = profile["development"]
    assert dev["python"] == ["3.10", "3.13"] and dev["conda"] == "none" and dev["docker"] == "rootless"
    assert len(dev["ai_clis"]) == 1 and dev["ollama"] is True
    assert profile["network"] == {"tailscale": False, "reverse_proxy": "nginx"}
    assert profile["packages"]["apps"] == ["@office", "htop"]
    validate_sections(profile, "kde")


def test_no_desktop_asks_no_desktop_questions(tmp_path):
    result = run_wizard(tmp_path, "none", [""] * 10)
    assert result.returncode == 0, result.stderr
    assert "Browser" not in result.stderr and "Gaming" not in result.stderr
    profile = tomllib.loads(result.stdout)
    assert profile["gaming"] == {"enabled": False} and "kde" not in profile
    assert profile["packages"]["browsers"] == []
    validate_sections(profile, "none")


@pytest.mark.parametrize("answers", [
    ["99"],                                   # browser number out of range
    ["", "", "", "x"],                        # python: not a number
    ["", "", "", "", "7"],                    # conda: unknown choice
    ["", "", "", "", "", "", "", "", '"; reboot; "'],   # extra apps: injection
])
def test_bad_answers_abort(tmp_path, answers):
    result = run_wizard(tmp_path, "kde", answers + [""] * 12)
    assert result.returncode != 0
    assert "FATAL" in result.stderr

"""installer/samurai-profile-plan: what each profile choice installs (spec 08 pages 6a, 8-11)."""
from __future__ import annotations

import json
import re
import subprocess
import tomllib

import pytest

from conftest import REPO, load_script

PLANNER = REPO / "installer" / "samurai-profile-plan"
DEFAULT_PROFILE = REPO / "installer" / "profile-default.toml"


@pytest.fixture(scope="module")
def planner():
    return load_script(PLANNER, "samurai_profile_plan")


@pytest.fixture(scope="module")
def catalog(planner):
    return planner.load_catalog(str(REPO / "catalog"))


def names(plan) -> set[str]:
    return {p["name"] for p in plan["packages"]}


def default_profile() -> dict:
    return tomllib.loads(DEFAULT_PROFILE.read_text())


def test_default_profile_installs_every_spec_default(planner, catalog):
    plan = planner.build_plan(default_profile(), catalog, offline=True)
    assert plan["deferred"] == [], "the offline ISO must carry the whole default install"
    got = names(plan)
    for package in ("samurai-base", "samurai-settings-system", "samurai-settings-shell",
                    "samurai-settings-dev", "samurai-desktop-kde", "firefox", "firewalld",
                    "plasma-firewall", "samurai-miniforge", "docker", "docker-compose",
                    "docker-buildx", "tailscale"):
        assert package in got, package
    for service in ("sddm.service", "docker.socket", "tailscaled.service", "ananicy-cpp.service",
                    "irqbalance.service", "systemd-oomd.service"):
        assert service in plan["services"], service
    assert set(plan["user_groups"]) == {"realtime", "gamemode", "docker"}


def test_gaming_default_matches_the_catalog_toggle(planner, catalog):
    """`samurai apps install @gaming` and the installer must offer the same set."""
    got = names(planner.build_plan(default_profile(), catalog))
    toggle = set(catalog.toggles["gaming"].packages)
    assert toggle <= got, toggle - got
    assert set(catalog.toggles["docker"].packages) <= got
    assert set(catalog.toggles["tailscale"].packages) <= got


def test_untick_individual_gaming_items(planner, catalog):
    profile = default_profile()
    profile["gaming"].update(heroic=False, wine=False)
    got = names(planner.build_plan(profile, catalog))
    assert "heroic-games-launcher-bin" not in got
    assert "samurai-wine" not in got and "umu-launcher" not in got
    assert {"steam", "gamescope", "samurai-proton-ge"} <= got


def test_absent_sections_install_nothing_optional(planner, catalog):
    plan = planner.build_plan({"packages": {"desktop": "none"}}, catalog)
    assert names(plan) == set(planner.BASE_PACKAGES)
    assert plan["user_groups"] == []
    kde = names(planner.build_plan({"packages": {"desktop": "kde"}}, catalog))
    assert "firefox" in kde and "steam" not in kde and "docker" not in kde


def test_gpu_dependent_packages(planner, catalog):
    profile = default_profile()
    profile["development"]["ollama"] = True
    plain = names(planner.build_plan(profile, catalog))
    assert not {"lib32-nvidia-utils", "nvidia-container-toolkit", "ollama-cuda", "ollama-rocm"} & plain
    nvidia = names(planner.build_plan(profile, catalog, nvidia=True))
    assert {"lib32-nvidia-utils", "nvidia-container-toolkit", "ollama-cuda"} <= nvidia
    assert "ollama-rocm" in names(planner.build_plan(profile, catalog, amd=True))


def test_docker_rootless_never_joins_the_docker_group(planner, catalog):
    profile = default_profile()
    profile["development"]["docker"] = "rootless"
    plan = planner.build_plan(profile, catalog, offline=True)
    assert "docker" not in plan["user_groups"]
    assert "docker.socket" not in plan["services"]
    assert plan["user_services"] == ["docker.socket"]
    assert "docker-rootless-extras" in names(plan)


def test_offline_defers_what_the_medium_lacks(planner, catalog):
    profile = default_profile()
    profile["development"].update(ai_clis=["claude-code", "aider"], ollama=True)
    profile["packages"]["browsers"] = ["firefox", "brave"] if "brave" in catalog.apps else ["firefox"]
    plan = planner.build_plan(profile, catalog, offline=True)
    deferred = {d["id"]: d for d in plan["deferred"]}
    assert {"claude-code", "aider", "ollama"} <= set(deferred)
    assert deferred["claude-code"]["command"] == "samurai apps install claude-code"
    assert "ollama.service" not in plan["services"]
    assert "ollama" not in names(plan) and "firefox" in names(plan)
    online = planner.build_plan(profile, catalog, offline=False)
    assert "claude-code" in names(online) and "ollama.service" in online["services"]
    assert [d["id"] for d in online["deferred"]] == ["aider"]   # per-user installer


@pytest.mark.parametrize("section, key, value", [
    ("development", "python", ["3.10", "3.11", "3.12", "3.13"]),
    ("development", "docker", "rootless"),
    ("network", "reverse_proxy", "caddy"),
    ("network", "reverse_proxy", "nginx"),
    ("network", "reverse_proxy", "traefik"),
    ("security", "firewall", "ufw"),
    ("security", "ssh_server", True),
])
def test_offline_closure_covers_every_documented_choice(planner, catalog, section, key, value):
    profile = default_profile()
    profile[section][key] = value
    assert planner.build_plan(profile, catalog, offline=True, nvidia=True)["deferred"] == []


def test_every_ai_cli_in_the_schema_exists(planner, catalog):
    doc = (REPO / "docs" / "plans" / "profile-schema.md").read_text()
    row = re.search(r"(?m)^\| `ai_clis` .*$", doc).group(0)
    offered = {app_id for app_id, _ in planner.catalog_choices(catalog, "ai-cli")}
    assert offered == set(re.findall(r"`([a-z-]+)`", row.split("`ai-cli`:")[1]))


def test_line_and_json_output(tmp_path):
    base = ["python3", str(PLANNER), str(DEFAULT_PROFILE), "--catalog", str(REPO / "catalog")]
    lines = subprocess.run(base, capture_output=True, text=True, check=True).stdout.splitlines()
    assert "pkg samurai-base" in lines and "group docker" in lines
    assert all(line.split(" ", 1)[0] in {"pkg", "service", "user-service", "group", "defer"} for line in lines)
    data = json.loads(subprocess.run(base + ["--json"], capture_output=True, text=True, check=True).stdout)
    assert set(data) == {"packages", "services", "user_services", "user_groups", "deferred"}


def test_closure_has_no_duplicates(planner, catalog):
    closure = planner.offline_closure(catalog)
    assert len(closure) == len(set(closure))


def test_every_samurai_package_in_the_closure_has_a_pkgbuild(planner, catalog):
    for name in planner.offline_closure(catalog):
        if name.startswith("samurai-"):
            assert (REPO / "pkgbuilds" / name / "PKGBUILD").is_file(), name


def test_meta_dependencies_on_samurai_packages_exist():
    for pkgbuild in (REPO / "pkgbuilds").glob("*/PKGBUILD"):
        text = re.sub(r"(?m)#.*$", "", pkgbuild.read_text())
        depends = re.search(r"(?ms)^depends=\((.*?)\)", text)
        listed = re.sub(r"#.*", "", depends.group(1)) if depends else ""
        for name in re.findall(r"samurai-[a-z0-9-]+", listed):
            assert (REPO / "pkgbuilds" / name / "PKGBUILD").is_file(), f"{pkgbuild.parent.name}: {name}"

"""Install/remove planning, preflight, JSON output and exit codes (updates/samurai-apps)."""
from __future__ import annotations

import json
import os
import subprocess
import sys

import pytest

from conftest import APPS_TOOL, CATALOG_DIR, fake_index_data


def plan(apps, catalog, idx, names, **kw):
    return apps.plan_install(apps.resolve_targets(catalog, names), catalog, idx, **kw)


def test_plan_explicit_unavailable_is_an_error(apps, catalog, make_index):
    idx, _ = make_index(missing={"lutris"})
    p = plan(apps, catalog, idx, ["lutris", "htop"])
    assert [a.id for a, _ in p.errors] == ["lutris"]
    p = plan(apps, catalog, idx, ["lutris", "htop"], skip_unavailable=True)
    assert not p.errors and p.pacman == ["htop"]
    assert "not in any enabled repository" in p.skipped[0][1]


def test_plan_bundle_skips_unavailable_members(apps, catalog, make_index):
    idx, _ = make_index(missing={"steam", "samurai-proton-ge", "samurai-wine"}, installed={"gamemode"})
    p = plan(apps, catalog, idx, ["@gaming"])
    assert not p.errors
    assert "steam" not in p.pacman and "gamemode" not in p.pacman    # missing / already installed
    assert {"mangohud", "lutris", "goverlay"} <= set(p.pacman)
    assert len(p.pacman) == len(set(p.pacman))
    skipped = " ".join(m for _, m in p.skipped)
    assert "steam" in skipped and "not available yet" in skipped


def test_plan_reasons(apps, catalog, make_index):
    idx, _ = make_index(missing={"steam"}, repos=["core", "extra"])
    st = apps.app_state(catalog.toggles["gaming"], catalog, idx)
    assert st.status == "partial" and "needs the [multilib] repository" in st.reason
    st = apps.app_state(catalog.apps["codex"], catalog, idx)  # source samurai, not in sync
    idx2, _ = make_index(missing={"openai-codex-bin"})
    assert apps.app_state(catalog.apps["codex"], catalog, idx2).status == "not-yet"


def test_plan_services_and_installed(apps, catalog, make_index):
    idx, _ = make_index(installed={"htop"})
    p = plan(apps, catalog, idx, ["docker", "htop"])
    assert p.services == ["docker.socket"]
    assert [a.id for a in p.already] == ["htop"]
    assert p.pacman == ["docker", "docker-compose"]


def test_plan_flatpak(apps, catalog, make_index):
    idx, _ = make_index()
    p = plan(apps, catalog, idx, ["sober"])
    assert p.errors and "samurai apps install flatpak" in p.errors[0][1]
    idx, _ = make_index(has_flatpak=True)
    p = plan(apps, catalog, idx, ["sober"])
    assert p.flatpaks[0][1] == "org.vinegarhq.Sober"
    cmds = apps.install_commands(p, idx, False, [])
    assert cmds[0][:4] == ["flatpak", "remote-add", "--if-not-exists", "flathub"]
    assert cmds[1][:4] == ["flatpak", "install", "--noninteractive", "flathub"]


def test_install_commands(apps, catalog, make_index, monkeypatch):
    idx, _ = make_index()
    p = plan(apps, catalog, idx, ["docker"])
    monkeypatch.setenv("SAMURAI_CHROOT", "1")
    cmds = apps.install_commands(p, idx, False, ["sudo"])
    assert cmds[0] == ["sudo", "pacman", "-S", "--needed", "--noconfirm", "docker", "docker-compose"]
    assert cmds[1] == ["sudo", "systemctl", "enable", "docker.socket"]      # no --now in a chroot
    cmds = apps.install_commands(p, idx, True, [])
    assert cmds[0][:2] == ["pacman", "-Su"]
    assert not any("-Sy" in c or "-Syu" in c for cmd in cmds for c in cmd)


def test_preflight(apps, make_index):
    idx, _ = make_index()
    assert apps.preflight(idx, 3, False, False) == []
    idx.db_age_days = None
    assert "samurai update" in apps.preflight(idx, 3, False, False)[0]
    idx.db_age_days = 9.0
    assert "9.0 days old" in apps.preflight(idx, 3, False, False)[0]
    assert apps.preflight(idx, 3, False, True) == []
    idx.db_age_days = 0.0
    idx.pending_upgrades = ["glibc"]
    assert "partial upgrade" in apps.preflight(idx, 3, False, False)[0]
    assert apps.preflight(idx, 3, True, False) == []


def test_remove_keeps_shared_packages(apps, catalog, make_index):
    idx, _ = make_index(installed={"fcitx5-m17n", "fcitx5-mozc", "fcitx5-configtool"})
    p = apps.plan_remove(apps.resolve_targets(catalog, ["indic"]), catalog, idx)
    assert p.pacman == ["fcitx5-m17n"]
    assert any("fcitx5-configtool" in n for n in p.notes)
    assert apps.remove_commands(p, [])[0] == ["pacman", "-Rs", "--noconfirm", "fcitx5-m17n"]


def test_profile_app_names(apps, catalog):
    assert apps.profile_app_names(catalog, ["firefox", "@gaming"]) == ["firefox", "@gaming"]
    for bad in (["Firefox"], ["obs-studio"], ["@nope"], ["linuxLts"], "firefox", [1]):
        with pytest.raises(apps.UsageError):
            apps.profile_app_names(catalog, bad)


# ---------------------------------------------------------------- CLI subprocess
def run(args, catalog, tmp_path, **index_kw):
    idx = tmp_path / "idx.json"
    idx.write_text(json.dumps(fake_index_data(catalog, **index_kw)))
    env = dict(os.environ, SAMURAI_CATALOG_DIR=str(CATALOG_DIR), SAMURAI_APPS_FAKE_INDEX=str(idx),
               NO_COLOR="1")
    return subprocess.run([sys.executable, str(APPS_TOOL), *args], capture_output=True, text=True,
                          env=env, stdin=subprocess.DEVNULL)


def test_list_json(catalog, tmp_path):
    r = run(["list", "--json"], catalog, tmp_path, installed={"htop"})
    assert r.returncode == 0, r.stderr
    data = json.loads(r.stdout)
    assert data["schema"] == 1
    ids = {a["id"]: a for a in data["apps"]}
    assert "linuxLts" not in ids                                   # hidden by default
    assert ids["htop"]["installed"] and ids["htop"]["status"] == "installed"
    assert ids["firefox"]["packages"][0] == {"name": "firefox", "repo": "extra", "available": True,
                                             "installed": False, "reason": None}
    assert data["summary"]["total"] == len(data["apps"])
    r = run(["list", "--json", "--category", "Gaming tools", "--installed"], catalog, tmp_path)
    assert json.loads(r.stdout)["apps"] == []


def test_info_search_json(catalog, tmp_path):
    r = run(["info", "--json", "virtManager"], catalog, tmp_path)
    app = json.loads(r.stdout)["app"]
    assert app["services"] == ["libvirtd"] and app["category_name"] == "Virtualization"
    r = run(["info", "--json", "@dev"], catalog, tmp_path)
    assert json.loads(r.stdout)["bundle"]["toggles"] == ["docker"]
    r = run(["search", "--json", "firefox"], catalog, tmp_path)
    assert json.loads(r.stdout)["apps"][0]["id"] == "firefox"
    r = run(["search", "zzzzqqq"], catalog, tmp_path)
    assert r.returncode == 1


def test_exit_codes(catalog, tmp_path):
    assert run(["install", "-n", "nosuchapp"], catalog, tmp_path).returncode == 2
    assert run(["install", "-n", "codex"], catalog, tmp_path, missing={"openai-codex-bin"}).returncode == 3
    assert run(["install", "-n", "htop"], catalog, tmp_path, db_age_days=10).returncode == 4
    assert run(["install", "-n", "htop"], catalog, tmp_path, pending_upgrades=["glibc"]).returncode == 4
    r = run(["install", "htop"], catalog, tmp_path)             # no TTY, no --yes
    assert r.returncode == 2 and "--yes" in r.stderr
    assert run([], catalog, tmp_path).returncode == 2
    assert run(["install", "--help"], catalog, tmp_path).returncode == 0


def test_dry_run_prints_exact_command(catalog, tmp_path):
    r = run(["install", "--dry-run", "@media"], catalog, tmp_path, installed={"vlc"})
    assert r.returncode == 0, r.stderr
    line = next(l for l in r.stdout.splitlines() if "pacman -S" in l)
    assert line.strip().endswith("pacman -S --needed --noconfirm mpv strawberry handbrake")
    assert "vlc is already installed" in r.stdout
    assert "dry run" in r.stdout


def test_human_list_has_markers(catalog, tmp_path):
    r = run(["list", "--category", "utilities"], catalog, tmp_path, installed={"htop"})
    assert r.returncode == 0
    assert "System utilities" in r.stdout
    assert any(l.strip().startswith(("✓ htop", "+ htop")) for l in r.stdout.splitlines())

"""Catalog merge rules, bundle expansion and lookup (updates/samurai-apps)."""
from __future__ import annotations

import tomllib

import pytest

from conftest import CATALOG_DIR

REQUIRED_BUNDLES = {"gaming", "dev", "creative", "office", "media", "streaming"}


def test_real_catalog_merges_cleanly(apps, catalog):
    garuda = tomllib.loads((CATALOG_DIR / "garuda-setup-wizard.toml").read_text())["app"]
    samurai = tomllib.loads((CATALOG_DIR / "samurai.toml").read_text())
    assert len(catalog.apps) == len(garuda) + len(samurai["app"])
    assert REQUIRED_BUNDLES <= set(catalog.bundles)
    for app in catalog.apps.values():
        assert app.packages or app.source in ("flatpak", "user-script"), app.id
    # every category used has a display name (no raw camelCase ids in the UI)
    for app in catalog.apps.values():
        assert app.category in catalog.categories, app.category


def test_overrides_applied(catalog):
    assert catalog.apps["firefox"].default is True
    assert catalog.apps["intellijIdea"].packages == ["intellij-idea-oss"]
    assert all(a.hidden for a in catalog.apps.values() if a.category == "kernels")
    assert catalog.apps["wire"].hidden and catalog.apps["wire"].note
    assert "proton-cachyos-slr" in catalog.toggles["gaming"].packages


def test_bundles_reuse_toggles(catalog):
    assert catalog.bundles["gaming"].toggles == ["gaming"]
    assert catalog.bundles["dev"].toggles == ["docker"]


def _merge(apps, garuda_apps, samurai):
    return apps.merge_catalog({"app": garuda_apps}, samurai)


G = [{"category": "audio", "id": "elisa", "name": "Elisa", "packages": ["elisa"],
      "origin": "garuda", "default": False},
     {"category": "kernels", "id": "linuxLts", "name": "LTS", "packages": ["linux-lts"],
      "origin": "garuda", "default": False}]


def test_merge_rules(apps):
    cat = _merge(apps, G, {
        "app": [{"category": "audio", "id": "mpv", "name": "mpv", "packages": ["mpv"], "source": "arch"}],
        "override": [{"id": "elisa", "default": True, "packages": ["elisa", "x"]},
                     {"category": "kernels", "hidden": True}],
        "bundle": [{"id": "m", "name": "M", "apps": ["elisa", "mpv"]}],
    })
    assert list(cat.apps) == ["elisa", "linuxLts", "mpv"]
    assert cat.apps["elisa"].default and cat.apps["elisa"].packages == ["elisa", "x"]
    assert cat.apps["elisa"].origin == "garuda" and cat.apps["mpv"].origin == "samurai"
    assert cat.apps["linuxLts"].hidden


@pytest.mark.parametrize("samurai, msg", [
    ({"app": [{"category": "audio", "id": "elisa", "name": "dup", "packages": ["e"]}]}, "already exists"),
    ({"override": [{"id": "nope", "default": True}]}, "no app with id"),
    ({"override": [{"category": "nope", "hidden": True}]}, "no app in category"),
    ({"override": [{"id": "elisa", "pakages": ["typo"]}]}, "unknown field"),
    ({"app": [{"category": "a", "id": "z", "name": "z", "packages": ["z"], "source": "aur"}]}, "unknown source"),
    ({"bundle": [{"id": "b", "apps": ["missing"]}]}, "unknown app"),
    ({"bundle": [{"id": "b", "apps": ["linuxLts"]}], "override": [{"id": "linuxLts", "hidden": True}]}, "hidden"),
    ({"bundle": [{"id": "b", "toggles": ["nope"]}]}, "unknown toggle"),
    ({"frobnicate": []}, "unknown table"),
    ({"app": [{"category": "a", "id": "f", "name": "f", "source": "flatpak"}]}, "flatpak application id"),
])
def test_merge_errors(apps, samurai, msg):
    with pytest.raises(apps.CatalogError, match=msg):
        _merge(apps, G, samurai)


def test_bundle_expansion_order_and_dedupe(apps, catalog):
    targets = apps.resolve_targets(catalog, ["@gaming", "lutris", "@gaming"])
    ids = [(t.app.origin == "toggle", t.app.id) for t in targets]
    assert ids[0] == (True, "gaming")            # the toggle's packages come first
    assert len(ids) == len(set(ids))
    by_id = {t.app.id: t for t in targets}
    assert by_id["lutris"].explicit             # explicit request wins over membership
    assert by_id["goverlay"].via == "gaming"


def test_lookup(apps, catalog):
    assert apps.lookup_app(catalog, "OBSSTUDIO").id == "obsStudio"
    assert apps.lookup_app(catalog, "obs-studio").id == "obsStudio"   # by package name
    with pytest.raises(apps.UsageError, match="did you mean"):
        apps.lookup_app(catalog, "firefx")
    with pytest.raises(apps.UsageError, match="not offered"):
        apps.resolve_targets(catalog, ["linuxLts"])
    with pytest.raises(apps.UsageError, match="unknown bundle"):
        apps.resolve_targets(catalog, ["@gamng"])


def test_search_ranking(apps, catalog):
    scored = sorted(((apps.search_score(a, "firefox", catalog), a.id) for a in catalog.apps.values()),
                    reverse=True)
    assert scored[0][1] == "firefox"


def test_safe_root_commands_only(apps, catalog):
    assert catalog.apps["docker"].enable_units() == ["docker.socket"]
    a = apps.App(id="x", name="x", category="c", packages=["x"],
                 root_commands=["systemctl enable --now x.service", "rm -rf / ; echo",
                                "systemctl enable --now bad;unit"])
    assert a.enable_units() == ["x.service"]
    assert a.manual_root_steps() == ["rm -rf / ; echo", "systemctl enable --now bad;unit"]

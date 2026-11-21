"""Shared helpers for host-side unit tests (no VM, no pacman needed)."""
from __future__ import annotations

import importlib.machinery
import importlib.util
import json
import pathlib
import sys

import pytest

REPO = pathlib.Path(__file__).resolve().parents[2]
APPS_TOOL = REPO / "updates" / "samurai-apps"
CATALOG_DIR = REPO / "catalog"


def load_script(path: pathlib.Path, name: str):
    loader = importlib.machinery.SourceFileLoader(name, str(path))
    spec = importlib.util.spec_from_loader(name, loader)
    mod = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = mod   # dataclasses resolve types via sys.modules
    loader.exec_module(mod)
    return mod


@pytest.fixture(scope="session")
def apps():
    return load_script(APPS_TOOL, "samurai_apps")


@pytest.fixture(scope="session")
def catalog(apps):
    return apps.load_catalog(CATALOG_DIR)


def fake_index_data(catalog, *, missing=(), installed=(), repos=None, **extra) -> dict:
    """Every catalog package resolves to [extra] except `missing`."""
    sync = {}
    for a in list(catalog.apps.values()) + list(catalog.toggles.values()):
        for p in a.packages:
            if p not in missing:
                sync[p] = "extra"
    data = {"sync": sync, "groups": {}, "repos": repos or ["core", "extra", "multilib"],
            "installed": list(installed), "db_age_days": 0.1, "pending_upgrades": []}
    data.update(extra)
    return data


@pytest.fixture
def make_index(apps, catalog, tmp_path):
    def _make(**kw):
        data = fake_index_data(catalog, **kw)
        path = tmp_path / "index.json"
        path.write_text(json.dumps(data))
        return apps.load_fake_index(str(path)), path
    return _make

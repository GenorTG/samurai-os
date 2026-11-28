"""Spec 07: the Samurai chwd profiles. Matching against real chwd is ci/test-hw.sh."""
import pathlib
import re
import tomllib

import pytest

REPO = pathlib.Path(__file__).resolve().parents[2]
HW = REPO / "pkgbuilds" / "samurai-hardware"
FILES = sorted(HW.glob("*.toml"))
HOOKS = ("conditional_packages", "pre_install", "post_install", "pre_remove", "post_remove")


def profiles():
    for path in FILES:
        for name, table in tomllib.loads(path.read_text(encoding="utf-8")).items():
            yield path, name, table
            for child, value in table.items():
                if isinstance(value, dict):
                    yield path, f"{name}.{child}", {**table, **value}


ALL = list(profiles())
IDS = [name for _, name, _ in ALL]


def test_every_set_is_packaged():
    pkgbuild = (HW / "PKGBUILD").read_text(encoding="utf-8")
    for path in FILES:
        assert path.name in pkgbuild and path.stem in pkgbuild


@pytest.mark.parametrize("path,name,table", ALL, ids=IDS)
def test_package_list_survives_chwds_helper_script(path, name, table):
    # scripts/chwd cuts `packages` at the first character outside [A-Za-z0-9- ]
    assert re.fullmatch(r"[A-Za-z0-9\- ]+", table["packages"]), table["packages"]
    # one line per key: the helper script reads the file line by line
    assert re.search(r"^packages = '", path.read_text(encoding="utf-8"), re.M)


@pytest.mark.parametrize("path,name,table", ALL, ids=IDS)
def test_no_cachyos_only_packages(path, name, table):
    text = " ".join(str(table.get(k, "")) for k in ("packages", *HOOKS))
    for forbidden in ("cachyos-drm-awaiter", "nouveau-fw", "libva-intel-driver-irql", "sdboot-manage"):
        assert forbidden not in text, forbidden


@pytest.mark.parametrize("path,name,table", ALL, ids=IDS)
def test_hooks_stay_inside_what_samurai_hw_reads(path, name, table):
    for key in HOOKS:
        # samurai-core's reader takes """ blocks literally: no escapes allowed
        assert "\\" not in table.get(key, ""), key


def test_module_providers_are_chosen_per_kernel():
    graphics = tomllib.loads((HW / "graphic_drivers.toml").read_text(encoding="utf-8"))
    for name in ("nvidia-open-dkms", "nvidia-dkms-580xx"):
        snippet = graphics[name]["conditional_packages"]
        assert "-headers" in snippet and "dkms" in snippet
        assert not any(p.endswith("-dkms") or p == "dkms" for p in graphics[name]["packages"].split())
        assert graphics[name]["driver_conflict_group"] == "nvidia"


def test_nvidia_profiles_write_kernel_parameters_through_samurai_boot():
    for _, name, table in ALL:
        if name.startswith("nvidia"):
            assert "samurai boot set param add nvidia_drm.modeset=1" in table["post_install"], name
            assert "sed " not in table["post_install"]


def test_chwd_package_never_syncs_the_database():
    pkgbuild = (REPO / "pkgbuilds" / "samurai-chwd" / "PKGBUILD").read_text(encoding="utf-8")
    assert "! grep -qF -- '-Sy' scripts/chwd" in pkgbuild
    assert re.search(r"^_commit=[0-9a-f]{40}", pkgbuild, re.M)


def test_the_health_service_can_read_the_kernel_module_trees():
    """ProtectKernelModules=yes hides /usr/lib/modules (systemd.exec(5)): the daily run would skip the
    module checks and report a pending reboot that is not there."""
    unit = (REPO / "updates" / "samurai-health.service").read_text(encoding="utf-8")
    assert "\nProtectKernelModules=" not in unit
    assert "CapabilityBoundingSet=~CAP_SYS_MODULE" in unit and "SystemCallFilter=~@module" in unit

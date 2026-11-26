"""Settings packages: KDE defaults tool, panel layout script, checksums, pinned keys."""
from __future__ import annotations

import configparser
import re
import shutil
import subprocess

import pytest

from conftest import REPO, load_script

KDE = REPO / "pkgbuilds" / "samurai-settings-kde"
CHAOTIC_FPR = "EF925EA60F33D0CB85C44AD13056513887B78AEB"


@pytest.fixture(scope="module")
def defaults():
    return load_script(KDE / "samurai-kde-defaults", "samurai_kde_defaults")


@pytest.fixture(scope="module")
def layout():
    return load_script(KDE / "samurai-plasma-layout", "samurai_plasma_layout")


def ini(path):
    parser = configparser.RawConfigParser(strict=True)
    parser.optionxform = str
    parser.read(path)
    return parser


def test_apply_writes_spec_defaults(defaults, tmp_path):
    assert defaults.main(["apply", "--root", str(tmp_path)]) == 0
    kwin = ini(tmp_path / "etc/xdg/kwinrc")
    for effect in ("blur", "wobblywindows", "magiclamp", "squash", "maximize", "dimscreen", "logout"):
        assert kwin["Plugins"][f"{effect}Enabled"] == "false", effect
    assert "overviewEnabled" not in kwin["Plugins"]
    globals_ = ini(tmp_path / "etc/xdg/kdeglobals")
    assert globals_["KDE"]["AnimationDurationFactor"] == "0"
    assert globals_["KDE"]["LookAndFeelPackage"] == "org.kde.breezedark.desktop"
    assert ini(tmp_path / "etc/xdg/baloofilerc")["General"]["only basic indexing"] == "true"
    assert ini(tmp_path / "etc/xdg/konsolerc")["Desktop Entry"]["DefaultProfile"] == "Samurai.profile"


def test_toggle_off_restores_kde_defaults_and_keeps_other_keys(defaults, tmp_path):
    kwinrc = tmp_path / "etc/xdg/kwinrc"
    kwinrc.parent.mkdir(parents=True)
    kwinrc.write_text("# admin file\n[Windows]\nBorderlessMaximizedWindows=true\n\n[Plugins]\nshakecursorEnabled=false\n")
    defaults.main(["apply", "--root", str(tmp_path)])
    defaults.main(["apply", "--root", str(tmp_path), "--effects", "default", "--widgets", "off"])
    text = kwinrc.read_text()
    assert "# admin file" in text and "BorderlessMaximizedWindows=true" in text
    assert "shakecursorEnabled=false" in text and "blurEnabled" not in text
    assert "AnimationDurationFactor" not in (tmp_path / "etc/xdg/kdeglobals").read_text()
    # the choice is remembered: a package upgrade re-applies it
    defaults.main(["apply", "--root", str(tmp_path)])
    assert "blurEnabled" not in kwinrc.read_text()
    state = defaults.load_state(tmp_path)
    assert state == {"theme": "breeze", "reduced_effects": False, "monitor_widgets": False}


def test_apply_is_idempotent(defaults, tmp_path):
    defaults.main(["apply", "--root", str(tmp_path)])
    before = {p: p.read_text() for p in (tmp_path / "etc").rglob("*") if p.is_file()}
    defaults.main(["apply", "--root", str(tmp_path)])
    assert before == {p: p.read_text() for p in (tmp_path / "etc").rglob("*") if p.is_file()}


def test_bad_state_is_refused(defaults, tmp_path):
    state = tmp_path / "etc/samurai/kde.toml"
    state.parent.mkdir(parents=True)
    state.write_text('theme = "otto"\n')
    with pytest.raises(SystemExit):
        defaults.main(["status", "--root", str(tmp_path)])


def test_sensor_and_socket_parsing(layout):
    sensors = "cpu/all/usage  CPU\ngpu/gpu1/usage Usage\ngpu/gpu0/usage Usage\ngpu/gpu0/temperature T\ngpu/all/usage All\n"
    assert layout.parse_gpus(sensors) == ["gpu0", "gpu1"]
    assert layout.parse_gpus("") == []
    assert layout.count_sockets("# comment\n0\n0\n1\n1\n") == 2
    assert layout.count_sockets("") == 1


def test_widgets_one_per_device(layout):
    widgets = layout.monitors(["gpu0", "gpu1"], 1)
    assert [w["title"] for w in widgets] == ["CPU", "GPU0", "GPU1"]
    assert widgets[0]["sensors"] == ["cpu/all/usage", "cpu/all/averageFrequency", "cpu/all/averageTemperature"]
    assert widgets[2]["sensors"] == ["gpu/gpu1/usage", "gpu/gpu1/temperature"]
    assert widgets[2]["labels"] == {"gpu/gpu1/usage": "GPU1", "gpu/gpu1/temperature": "T"}


def test_layout_script_matches_spec(layout):
    script = layout.layout_script(layout.monitors([], 1))
    top, bottom = script.split("var bottom = new Panel;")
    for widget in ("kickoff", "pager", "systemtray", "digitalclock", "showdesktop", "systemmonitor"):
        assert f"org.kde.plasma.{widget}" in top, widget
    assert "taskmanager" not in top
    assert re.findall(r'addWidget\("([^"]+)"\)', bottom) == ["org.kde.plasma.taskmanager"]
    assert 'writeConfig("groupingStrategy", 0)' in bottom and 'lengthMode = "fill"' in bottom
    assert "systemmonitor" not in layout.layout_script([]).split("var monitors = []")[0]
    node = shutil.which("node")
    if node:   # syntax check of the generated JavaScript
        subprocess.run([node, "--check", "-"], input=script, text=True, check=True)


def test_exe_files_open_with_wine():
    wine = REPO / "pkgbuilds" / "samurai-wine"
    claimed = set(re.search(r"(?m)^MimeType=(.*);$", (wine / "samurai-wine.desktop").read_text()).group(1).split(";"))
    defaults = ini(wine / "mimeapps.list")["Default Applications"]
    assert set(defaults) == claimed
    assert "application/vnd.microsoft.portable-executable" in claimed   # what *.exe is typed as today
    assert set(defaults.values()) == {"samurai-wine.desktop"}


def test_local_source_checksums_are_current():
    result = subprocess.run(["python3", str(REPO / "ci" / "update-pkg-sums.py"), "--check"],
                            capture_output=True, text=True)
    assert result.returncode == 0, result.stdout + result.stderr


def test_every_package_ships_the_license():
    for pkgbuild in (REPO / "pkgbuilds").glob("*/PKGBUILD"):
        text = pkgbuild.read_text()
        assert "usr/share/licenses/$pkgname" in text, pkgbuild.parent.name
        assert re.search(r"(?m)^pkgrel=\d+$", text) and "epoch=" not in text, pkgbuild.parent.name


def test_downloaded_sources_are_pinned():
    """A repackaged upstream binary never skips verification."""
    for pkgbuild in (REPO / "pkgbuilds").glob("*/PKGBUILD"):
        text = pkgbuild.read_text()
        assert "'SKIP'" not in text and "(SKIP)" not in text, pkgbuild.parent.name


def test_chaotic_key_is_pinned_everywhere():
    for path in ("ci/chaotic-bootstrap.sh", "installer/samurai-postinstall"):
        assert CHAOTIC_FPR in (REPO / path).read_text(), path
    target = (REPO / "installer" / "pacman.conf.target").read_text()
    block = target.split("[chaotic-aur]\n", 1)[1]
    assert block.startswith("SigLevel = Required DatabaseOptional\nInclude = /etc/pacman.d/chaotic-mirrorlist")


@pytest.mark.skipif(shutil.which("gpg") is None, reason="gpg not installed")
def test_vendored_chaotic_key_is_the_pinned_one():
    key = REPO / "iso" / "airootfs" / "usr" / "share" / "samurai" / "chaotic-key.asc"
    out = subprocess.run(["gpg", "--batch", "--show-keys", "--with-colons", str(key)],
                         capture_output=True, text=True, check=True).stdout.splitlines()
    primaries = [i for i, line in enumerate(out) if line.startswith("pub:")]
    assert len(primaries) == 1
    fpr = next(line for line in out[primaries[0]:] if line.startswith("fpr:")).split(":")[9]
    assert fpr == CHAOTIC_FPR

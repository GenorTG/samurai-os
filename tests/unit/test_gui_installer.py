"""Graphical installer (installer/gui): the profile it writes, the facts it parses, the
progress markers it relies on and the root helper's request handling. No Qt needed."""
from __future__ import annotations

import importlib.machinery
import importlib.util
import json
import pathlib
import shutil
import stat
import sys
import threading
import tomllib

import pytest

REPO = pathlib.Path(__file__).resolve().parents[2]
GUI = REPO / "installer" / "gui"
sys.path.insert(0, str(GUI))

from samurai_installer import engine, model, progress, system  # noqa: E402

TOOLS = engine.Tools.from_checkout(REPO)


def load_helper():
    path = str(GUI / "samurai-installer-helper")
    loader = importlib.machinery.SourceFileLoader("samurai_installer_helper", path)
    spec = importlib.util.spec_from_loader(loader.name, loader)
    module = importlib.util.module_from_spec(spec)
    loader.exec_module(module)
    return module


helper = load_helper()


def choices(**changes) -> model.Choices:
    base = model.Choices(disk="/dev/vda", username="alex", timezone="UTC")
    for key, value in changes.items():
        assert key in model.field_names()
        setattr(base, key, value)
    return base


# ------------------------------------------------------------------ profile
@pytest.mark.parametrize("changes", [
    {},
    {"encryption": False},
    {"desktop": "none", "gaming_enabled": True},
    {"desktop": "gnome"},
    {"gaming_enabled": False},
    {"python": ["3.13", "3.10"], "conda": "none", "docker": "rootless", "ollama": True},
    {"ai_clis": ["aider"], "browsers": ["brave", "firefox"], "apps": ["vlc"]},
    {"kernels": ["linux"], "default_kernel": "linux", "bootloader": "grub", "firewall": "ufw",
     "reverse_proxy": "caddy", "ssh_server": True, "tailscale": False, "nvidia": "no"},
])
def test_every_wizard_profile_passes_the_validator(changes):
    reason = engine.validate(TOOLS, model.to_toml(choices(**changes), "$6$abc$" + "a" * 86))
    if reason.startswith(("unknown timezone", "keyboard layout", "locale ")):
        pytest.skip(f"host lacks the data file: {reason}")
    assert reason == ""


def test_profile_writes_every_section_and_the_default_kernel_first():
    profile = tomllib.loads(model.to_toml(choices(default_kernel="linux-zen")))
    assert profile["boot"]["kernels"] == ["linux-zen", "linux-cachyos"]
    assert set(profile) == {"version", "system", "disk", "boot", "user", "packages", "kde",
                            "gaming", "development", "network", "security", "hardware", "repos"}
    assert profile["disk"]["auto_unlock_keyfile"] is False
    assert "password_hash" not in profile["user"]


def test_no_desktop_drops_desktop_only_choices():
    profile = tomllib.loads(model.to_toml(choices(desktop="none")))
    assert "kde" not in profile
    assert profile["packages"]["browsers"] == []
    assert profile["gaming"] == {"enabled": False}


@pytest.mark.parametrize("evil", ['a"\n[user]\nname = "root', "x\\", "tab\there", "del\x7f",
                                  "zażółć"])
def test_toml_strings_cannot_break_out(evil):
    profile = tomllib.loads(model.to_toml(choices(hostname=evil)))
    assert profile["system"]["hostname"] == evil
    assert profile["user"]["name"] == "alex"


def test_saved_profile_loads_back_without_the_disk():
    original = choices(bootloader="systemd-boot", gaming_enabled=False, python=["3.12"],
                       kernels=["linux", "linux-lts"], default_kernel="linux-lts", apps=["vlc"])
    loaded = model.from_toml(model.to_toml(original), model.Choices())
    assert loaded.disk == ""
    assert model.to_toml(loaded) == model.to_toml(model.Choices(
        **{**original.as_dict(), "disk": ""}))


def test_password_is_hashed_on_stdin():
    value = engine.hash_password(TOOLS, "correct horse")
    assert value.startswith("$6$") and "correct" not in value


def test_saved_files_are_private(tmp_path):
    target = tmp_path / "p.toml"
    target.write_text("old")
    target.chmod(0o644)
    engine.write_private(target, "version = 1\n")
    assert stat.S_IMODE(target.stat().st_mode) == 0o600
    assert list(tmp_path.iterdir()) == [target]


@pytest.mark.parametrize("value, ok", [("alex", True), ("Alex", False), ("root", False),
                                       ("live", False), ("-x", False), ("", False)])
def test_username_check(value, ok):
    assert (model.username_error(value) == "") is ok


def test_secret_check():
    assert model.secret_error("short", "short", "Passphrase", 8)
    assert model.secret_error("long enough", "different!!", "Passphrase", 8)
    assert model.secret_error("line\nbreak!!", "line\nbreak!!", "Passphrase", 8)
    assert model.secret_error(" spaces kept ", " spaces kept ", "Passphrase", 8) == ""


# ------------------------------------------------------------------ machine facts
def lsblk(*devices) -> str:
    return json.dumps({"blockdevices": list(devices)})


def disk(name, size=64 * system.GIB, **extra):
    return {"name": name, "path": f"/dev/{name}", "size": size, "type": "disk", "ro": False,
            "rm": False, "mountpoints": [None], **extra}


def test_disks_block_the_live_medium_busy_and_small_disks():
    stick = disk("sdb", children=[{"name": "sdb1", "path": "/dev/sdb1", "size": 5,
                                   "type": "part", "mountpoints": [None]}])
    busy = disk("sdc", children=[{"name": "sdc1", "size": 5, "type": "part",
                                  "mountpoints": ["/mnt/x"]}])
    found = {d.path: d for d in system.parse_disks(lsblk(
        disk("vda"), stick, busy, disk("sdd", size=8 * system.GIB), disk("zram0"),
        {"name": "sr0", "type": "rom", "size": 100}), live_source="/dev/sdb1")}
    assert set(found) == {"/dev/vda", "/dev/sdb", "/dev/sdc", "/dev/sdd"}
    assert found["/dev/vda"].blocked == ""
    assert "booted" in found["/dev/sdb"].blocked
    assert "mounted" in found["/dev/sdc"].blocked
    assert "small" in found["/dev/sdd"].blocked.lower()


def test_garbage_lsblk_output_means_no_disks():
    assert system.parse_disks("") == [] == system.parse_disks("{}")


def test_gpus_locales_keymaps_parsing():
    line = ('0000:01:00.0 "VGA compatible controller [0300]" "NVIDIA Corporation [10de]" '
            '"GA102 [2204]" -r a1 "X [1043]" "Y [87b3]"')
    assert system.parse_gpus(line)[0]["nvidia"] is True
    assert system.parse_gpus('00:1f.0 "ISA bridge [0601]" "Intel [8086]" "x [1]"') == []
    assert system.parse_locales("#en_US.UTF-8 UTF-8  \n# comment line here\n"
                                "#sr_RS@latin UTF-8\npl_PL.UTF-8 UTF-8\n#en_US ISO-8859-1\n"
                                ) == ["en_US.UTF-8", "pl_PL.UTF-8"]
    assert system.parse_xkb_map("# h\npl2\tpl\tpc105\t-\tx\nuk gb pc105 - x\n")["uk"] == ("gb", "")


# ------------------------------------------------------------------ progress
def test_every_progress_marker_is_printed_by_the_engine():
    sources = ((REPO / "installer" / "samurai-install").read_text()
               + (REPO / "installer" / "samurai-postinstall").read_text())
    missing = [s.marker for s in progress.STAGES if s.marker not in sources]
    assert missing == []
    assert [s.percent for s in progress.STAGES] == sorted(s.percent for s in progress.STAGES)


def test_progress_moves_forward_and_keeps_the_first_error():
    p = progress.Progress()
    assert p.feed("2026 partitioning /dev/vda")
    assert p.feed("2026 identity: hostname=x")
    assert not p.feed("2026 partitioning /dev/vda")
    assert p.percent == 42
    p.feed("2026 FATAL: base install failed")
    p.feed("FATAL: later")
    assert p.error == "base install failed"


# ------------------------------------------------------------------ root helper
def request(profile="[disk]\nencryption = true\n", passphrase="long enough") -> str:
    return json.dumps({"profile": profile, "passphrase": passphrase})


def test_helper_accepts_a_good_request():
    assert helper.parse_request(request())[1] == "long enough"
    assert helper.parse_request(request("[disk]\nencryption = false\n", ""))[1] == ""


@pytest.mark.parametrize("raw", [
    "not json", "[]", json.dumps({"profile": "x"}),
    json.dumps({"profile": "", "passphrase": "long enough", "argv": ["sh"]}),
    request(passphrase="short"), request(passphrase="two\nlines-here"),
    request("[disk]\nencryption = false\n", "unexpected passphrase"),
    request(profile="x" * (300 * 1024)),
])
def test_helper_refuses(raw):
    with pytest.raises((ValueError, tomllib.TOMLDecodeError)):
        helper.parse_request(raw)


def test_helper_feeds_the_passphrase_on_stdin_and_follows_the_log(tmp_path, capsys):
    log = tmp_path / "install.log"
    log.write_text("old run\n")
    backend = tmp_path / "backend"
    backend.write_text(
        "#!/usr/bin/env bash\nset -eu\n[ \"$1 $3\" = '--profile --yes' ]\n"
        "IFS= read -r a; IFS= read -r b; [ \"$a\" = \"$b\" ]\n"
        f"echo \"partitioning /dev/vda\" >> {log}\n"
        f"printf '%s' \"$a\" > {tmp_path}/seen\n"
        "echo done-on-stdout\necho umask=$(umask)\n! env | grep -q 'two  words'\nexit 3\n")
    backend.chmod(0o755)
    status = helper.run_backend(tmp_path / "p.toml", " two  words ", helper.Output(),
                                backend=str(backend), log=log)
    out = capsys.readouterr().out
    assert status == 3
    assert (tmp_path / "seen").read_text() == " two  words "
    assert "partitioning /dev/vda" in out and "done-on-stdout" in out
    assert "old run" not in out and "two  words" not in out
    # the new system's directories must not inherit a private umask
    assert "umask=0022" in out


def test_helper_profile_copy_is_private_even_over_a_planted_link(tmp_path):
    target = tmp_path / "victim"
    target.write_text("keep")
    path = tmp_path / "install-profile.toml"
    path.symlink_to(target)
    helper.write_private(path, "version = 1\n")
    assert target.read_text() == "keep" and not path.is_symlink()
    assert stat.S_IMODE(path.stat().st_mode) == 0o600


def test_follow_handles_partial_lines(tmp_path, capsys):
    log = tmp_path / "log"
    log.write_bytes(b"one\ntwo without newline")
    done = threading.Event()
    done.set()
    helper.follow(log, 0, done, helper.Output())
    assert capsys.readouterr().out == "one\ntwo without newline\n"


@pytest.mark.skipif(shutil.which("bash") is None, reason="needs bash")
def test_backend_reads_secrets_verbatim():
    """samurai-install must not trim spaces from the passphrase or password it reads: the
    boot prompt takes them literally."""
    source = (REPO / "installer" / "samurai-install").read_text()
    reads = [line.strip() for line in source.splitlines() if "read -r -s" in line]
    assert len(reads) == 4
    assert all(line.startswith("IFS= read -r -s") for line in reads), reads


# ------------------------------------------------------------------ live medium wiring
def test_live_medium_ships_what_it_refers_to():
    airootfs = REPO / "iso" / "airootfs"
    profiledef = (REPO / "iso" / "profiledef.sh").read_text()
    sudoers = (airootfs / "etc/sudoers.d/10-samurai-live").read_text()
    rule = next(line for line in sudoers.splitlines() if line.startswith("live "))
    commands = [part.strip() for part in rule.split("NOPASSWD:")[1].split(",")]
    # every sudo rule is a fixed command without arguments
    assert all(c.endswith(' ""') for c in commands), commands
    assert [c.split()[0] for c in commands] == [
        "/usr/lib/samurai-installer/samurai-installer-helper",
        "/usr/local/bin/samurai-install", "/usr/local/bin/samurai-rescue"]
    for name in ("samurai-installer", "samurai-installer-helper"):
        assert (GUI / name).stat().st_mode & 0o111
        assert f'["/usr/lib/samurai-installer/{name}"]="0:0:755"' in profiledef
    packages = (REPO / "iso" / "packages.x86_64").read_text().split()
    assert {"plasma-desktop", "sddm", "pyside6", "qqc2-desktop-style", "networkmanager",
            "konsole"} <= set(packages)
    assert "installer/gui" in (REPO / "ci" / "build-iso.sh").read_text()
    for entry in (airootfs / "usr/share/samurai-installer/live-skel/Desktop").iterdir():
        assert "Exec=" in entry.read_text()
    polkit = (airootfs / "etc/polkit-1/rules.d/50-samurai-live.rules").read_text()
    granted = polkit.split("var allowed = [")[1].split("]")[0]
    assert "ignore-inhibit" not in granted and 'subject.user === "live"' in polkit

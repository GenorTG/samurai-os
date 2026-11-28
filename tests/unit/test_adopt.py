"""updates/samurai-adopt: bringing an older install to the current defaults."""
from __future__ import annotations

import gzip
import os
import signal
import subprocess
import tomllib

import pytest

from conftest import REPO, load_script

# the sections of the pacman.conf that release v0.0.1.75 installed, plus what postinstall appended
OLD_CONF = """[options]
HoldPkg     = pacman glibc
SigLevel    = Required DatabaseOptional

[core]
Include = /etc/pacman.d/mirrorlist

[extra]
Include = /etc/pacman.d/mirrorlist

[multilib]
Include = /etc/pacman.d/mirrorlist

# CachyOS: the default kernel
[cachyos]
SigLevel = Required DatabaseOptional
Server = https://nl.cachyos.org/repo/$arch/$repo

#[chaotic-aur]
#Include = /etc/pacman.d/chaotic-mirrorlist

# Samurai OS repository (spec 02): signed packages AND database
[samurai]
SigLevel = Required DatabaseRequired
Include = /etc/pacman.d/samurai-mirrorlist
"""

# what v0.0.1.75 stored: no [kde], [gaming], [development] or [network]
OLD_PROFILE = tomllib.loads("""
version = 1
[user]
name = "samurai"
[packages]
desktop = "kde"
install = ["samurai-release", "qemu-guest-agent"]
[repos]
chaotic_aur = false
[security]
firewall = "firewalld"
ssh_server = false
""")
OLD_KDE_INSTALLED = {"plasma-workspace", "firewalld", "firefox", "samurai-release"}


@pytest.fixture(scope="module")
def adopt():
    return load_script(REPO / "updates" / "samurai-adopt", "samurai_adopt")


@pytest.fixture(scope="module")
def planner():
    return load_script(REPO / "installer" / "samurai-profile-plan", "samurai_profile_plan_adopt")


@pytest.fixture(scope="module")
def plan_catalog(planner):
    return planner.load_catalog(str(REPO / "catalog"))


def planned(planner, catalog, profile) -> set[str]:
    return {p["name"] for p in planner.build_plan(profile, catalog)["packages"]}


# ------------------------------------------------------------------ pacman.conf
def test_chaotic_goes_below_arch_and_cachyos(adopt):
    new = adopt.add_chaotic(OLD_CONF)
    assert adopt.sections(new) == ["options", "core", "extra", "multilib", "cachyos",
                                   "chaotic-aur", "samurai"]
    assert "[chaotic-aur]\nSigLevel = Required DatabaseOptional\nInclude = /etc/pacman.d/chaotic-mirrorlist\n" in new
    # nothing of the old file is lost, and the [samurai] comment stays on [samurai]
    assert all(line in new.splitlines() for line in OLD_CONF.splitlines())
    assert "# Samurai OS repository (spec 02): signed packages AND database\n[samurai]" in new
    assert adopt.add_chaotic(new) == new


def test_chaotic_matches_what_new_installs_ship(adopt):
    target = (REPO / "installer" / "pacman.conf.target").read_text()
    assert adopt.add_chaotic(target) == target


def test_chaotic_below_arch_even_with_samurai_on_top(adopt):
    conf = "[options]\n[samurai]\nServer = x\n[core]\n[extra]\n"
    assert adopt.sections(adopt.add_chaotic(conf)) == ["options", "samurai", "core", "extra", "chaotic-aur"]


def test_unknown_configuration_is_refused(adopt):
    with pytest.raises(adopt.Refused):
        adopt.add_chaotic("[options]\n[myrepo]\n")


def test_a_repository_the_user_removed_stays_off(adopt):
    assert adopt.wants_chaotic(OLD_CONF, OLD_PROFILE, {}, set())[0] is True
    newer = {"repos": {"chaotic_aur": True}}
    assert adopt.wants_chaotic(OLD_CONF, newer, {}, set())[0] is False
    assert adopt.wants_chaotic(OLD_CONF, OLD_PROFILE, {"chaotic_enabled": True}, set())[0] is False
    assert adopt.wants_chaotic(OLD_CONF, newer, {}, {"chaotic"})[0] is True
    assert adopt.wants_chaotic(adopt.add_chaotic(OLD_CONF), newer, {}, set()) == (False, "")


# ------------------------------------------------------------------ user choices
def test_removed_packages_follow_the_last_log_action(adopt, tmp_path):
    old = tmp_path / "pacman.log.1.gz"
    with gzip.open(old, "wt") as fh:
        fh.write("[2026-11-01T10:00:00+0000] [ALPM] removed kate (1-1)\n"
                 "[2026-11-01T10:00:00+0000] [ALPM] removed elisa (1-1)\n")
    os.utime(old, (1, 1))
    log = tmp_path / "pacman.log"
    log.write_text("[2026-11-02T10:00:00+0000] [ALPM] installed elisa (1-1)\n"
                   "[2026-11-02T10:00:00+0000] [ALPM] removed okular (1-1)\n"
                   "[2026-11-02T10:00:00+0000] [PACMAN] Running 'pacman -R removed gwenview ('\n"
                   "[2026-11-02T10:00:00+0000] [ALPM] removed reinstalled-later (1-1)\n")
    got = adopt.removed_packages(adopt.read_pacman_logs(log), {"elisa", "reinstalled-later"})
    assert got == {"kate", "okular"}


def test_old_install_gets_the_defaults_without_optional_sets(adopt, planner, plan_catalog):
    profile, _ = adopt.target_profile(OLD_PROFILE, OLD_KDE_INSTALLED, set(), planner, plan_catalog)
    got = planned(planner, plan_catalog, profile)
    assert {"samurai-base", "samurai-settings-system", "samurai-settings-shell", "samurai-settings-dev",
            "samurai-desktop-base", "samurai-desktop-kde", "firewalld", "plasma-firewall"} == got
    # installer-time package list, browsers and gaming never come back by themselves
    assert not got & {"qemu-guest-agent", "firefox", "steam", "tailscale"}


def test_opt_in_sets_come_from_the_planner(adopt, planner, plan_catalog):
    profile, _ = adopt.target_profile(OLD_PROFILE, OLD_KDE_INSTALLED, {"gaming", "tailscale"},
                                      planner, plan_catalog)
    got = planned(planner, plan_catalog, profile)
    assert set(planner.GAMING_ITEMS["steam"]) | {"samurai-wine", "tailscale"} <= got


def test_stored_choices_of_a_newer_install_are_kept(adopt, planner, plan_catalog):
    stored = {"packages": {"desktop": "kde", "browsers": ["firefox", "no-such-browser"]},
              "gaming": {"enabled": True, "steam": False}, "network": {"tailscale": True}}
    profile, _ = adopt.target_profile(stored, OLD_KDE_INSTALLED, set(), planner, plan_catalog)
    got = planned(planner, plan_catalog, profile)
    assert {"firefox", "heroic-games-launcher-bin", "tailscale"} <= got and "steam" not in got


def test_a_desktop_that_is_gone_is_not_reinstalled(adopt, planner, plan_catalog):
    profile, notes = adopt.target_profile(OLD_PROFILE, {"samurai-release"}, set(), planner, plan_catalog)
    assert profile["packages"]["desktop"] == "none" and "no longer installed" in notes[0]
    assert "samurai-desktop-kde" not in planned(planner, plan_catalog, profile)
    with pytest.raises(adopt.Refused):
        adopt.target_profile(OLD_PROFILE, {"samurai-release"}, {"gaming"}, planner, plan_catalog)


def test_without_a_stored_profile_the_installed_system_decides(adopt, planner, plan_catalog):
    profile, notes = adopt.target_profile(None, {"plasma-workspace"}, set(), planner, plan_catalog)
    assert profile["packages"]["desktop"] == "kde" and profile["security"]["firewall"] == "none"
    assert notes


# ------------------------------------------------------------------ package selection
INFO = """Repository      : samurai
Name            : samurai-desktop
Depends On      : samurai-audio  editor
                  viewer
Provides        : None
Conflicts With  : None

Repository      : samurai
Name            : samurai-audio
Depends On      : pw-jack
Conflicts With  : None

Repository      : extra
Name            : pw-jack
Depends On      : libjack
Provides        : jack  libjack
Conflicts With  : jack  jack2

Repository      : extra
Name            : editor
Depends On      : None

Repository      : extra
Name            : viewer
Depends On      : None
Groups          : tools

Repository      : extra
Name            : player
Depends On      : codec>=2

Repository      : extra
Name            : codec
Depends On      : None
"""


def resolver(adopt, *, installed=(), explicit=(), removed=(), ignored=(), local=None):
    sync = adopt.parse_info(INFO)
    local = local or {}
    for name in installed:
        local.setdefault(name, adopt.Pkg(name))
    provided = {adopt.dep_name(p) for pkg in local.values() for p in pkg.provides}
    return adopt.Resolver(sync, local, set(explicit), set(removed), set(ignored),
                          lambda dep: adopt.dep_name(dep) in set(local) | provided)


def test_parse_info(adopt):
    sync = adopt.parse_info(INFO)
    assert sync["samurai-desktop"].depends == ["samurai-audio", "editor", "viewer"]
    assert sync["pw-jack"].conflicts == ["jack", "jack2"] and sync["editor"].depends == []
    assert sync["viewer"].groups == ["tools"]


def test_everything_available_is_selected(adopt):
    s = resolver(adopt).select(["samurai-desktop"])
    assert s.targets == ["samurai-desktop"] and not s.skipped and not s.swaps
    assert s.new == {"samurai-desktop", "samurai-audio", "pw-jack", "editor", "viewer"}


def test_installed_targets_need_nothing(adopt):
    assert resolver(adopt, installed=["samurai-desktop"]).select(["samurai-desktop"]).targets == []


def test_a_removed_program_expands_the_meta_around_it(adopt):
    s = resolver(adopt, removed=["editor"]).select(["samurai-desktop"])
    assert s.targets == ["samurai-audio", "viewer"] and "editor" not in s.new
    assert s.skipped["samurai-desktop"] == "it needs editor: you removed it"
    assert s.skipped["editor"] == "you removed it"


def test_a_removed_dependency_drops_ordinary_packages(adopt):
    s = resolver(adopt, removed=["codec"]).select(["player", "editor"])
    assert s.targets == ["editor"] and "codec" in s.skipped["player"]


def test_ignorepkg_is_a_choice(adopt):
    assert "IgnorePkg" in resolver(adopt, ignored=["editor"]).select(["editor"]).skipped["editor"]


def test_a_conflict_the_user_installed_wins(adopt):
    s = resolver(adopt, installed=["jack2"], explicit=["jack2"],
                 local={"jack2": adopt.Pkg("jack2", provides=["libjack"])}).select(["samurai-audio"])
    # the conflicting package stays out, and the meta around it is not installed either
    assert "pw-jack" not in s.new and "jack2" in s.skipped["samurai-audio"] and s.targets == []
    assert "jack2" in s.skipped["pw-jack"]


def test_a_conflict_installed_only_as_a_dependency_is_replaced_and_shown(adopt):
    s = resolver(adopt, installed=["jack2"]).select(["samurai-audio"])
    assert s.targets == ["samurai-audio"] and s.swaps == {"pw-jack": ["jack2"]}


def test_versioned_conflicts_of_installed_packages_do_not_block(adopt):
    old = {"tool": adopt.Pkg("tool", conflicts=["editor<2"])}
    assert resolver(adopt, local=old).select(["editor"]).targets == ["editor"]
    old = {"tool": adopt.Pkg("tool", conflicts=["editor"])}
    blocked = resolver(adopt, local=old, explicit=["tool"]).select(["editor"])
    assert not blocked.targets and "tool" in blocked.skipped["editor"]


def test_the_resolved_transaction_is_checked_again(adopt):
    r = resolver(adopt, removed=["codec"])
    r.check({"editor"})
    with pytest.raises(adopt.Refused, match="codec"):
        r.check({"editor", "codec"})


def test_missing_packages(adopt):
    s = resolver(adopt).select(["nowhere", "editor"])
    assert s.targets == ["editor"] and "enabled repository" in s.skipped["nowhere"]
    with pytest.raises(adopt.Refused):
        resolver(adopt).select(["samurai-nowhere"])


def test_groups_expand_so_removed_members_stay_out(adopt):
    assert resolver(adopt, removed=["viewer"]).select(["tools"]).targets == []
    assert resolver(adopt).select(["tools"]).targets == ["viewer"]


# ------------------------------------------------------------------ apply and the command
def adoption(adopt, **kw):
    values = dict(profile={"packages": {"desktop": "none"}}, features=[],
                  selection=adopt.Selection(targets=["samurai-base"]), sync={}, notes=[],
                  enable_chaotic=True, bootstrap=False, transaction=[], upgrades=[], installed=set())
    values.update(kw)
    return adopt.Adoption(**values)


@pytest.fixture
def machine(adopt, tmp_path, monkeypatch):
    """apply() against a temporary pacman.conf with faked commands."""
    conf = tmp_path / "pacman.conf"
    conf.write_text(OLD_CONF)
    for name, path in (("PACMAN_CONF", conf), ("CONF_BACKUP", tmp_path / "pacman.conf.bak"),
                       ("STATE_PATH", tmp_path / "adopt.json"), ("LOG_DIR", tmp_path / "log"),
                       ("PROFILE_PATH", tmp_path / "none.toml")):
        monkeypatch.setattr(adopt, name, path)
    state = {"installed": {"pacman"}, "commands": [], "pacman_rc": 0, "snapshot_rc": 0}

    def fake_run(cmd, **kw):
        if cmd[0] == "snapper":
            return subprocess.CompletedProcess(cmd, state["snapshot_rc"], "7\n", "snapper broke")
        if cmd[:2] == ["pacman", "-T"]:
            return subprocess.CompletedProcess(cmd, 0 if cmd[-1] in state["installed"] else 127, "", "")
        return subprocess.CompletedProcess(cmd, 0, "", "")

    def fake_stream(self, cmd):
        state["commands"].append(cmd)
        if cmd[0] == "pacman" and state["pacman_rc"] == 0:
            state["installed"] |= set(cmd[cmd.index("--") + 1:])
        return state["pacman_rc"] if cmd[0] == "pacman" else 0

    monkeypatch.setattr(adopt, "run", fake_run)
    monkeypatch.setattr(adopt, "installed_names", lambda: set(state["installed"]))
    monkeypatch.setattr(adopt.shutil, "which", lambda name, **kw: f"/usr/bin/{name}")
    monkeypatch.setattr(adopt.Log, "stream", fake_stream)
    state["conf"] = conf
    return state


def test_apply_runs_one_full_transaction_after_the_snapshot(adopt, machine, capsys):
    assert adopt.apply(adoption(adopt), no_snapshot=False) == 0
    assert machine["commands"] == [["pacman", "-Syu", "--needed", "--noconfirm", "--", "samurai-base"]]
    assert "chaotic-aur" in adopt.sections(machine["conf"].read_text())
    assert adopt.CONF_BACKUP.read_text() == OLD_CONF
    out = capsys.readouterr().out
    assert out.index("snapshot #7") < out.index("[chaotic-aur] enabled")
    assert "samurai snap rollback 7" in out
    assert adopt.load_state()["chaotic_enabled"] is True


def test_failed_transaction_restores_the_configuration(adopt, machine, capsys):
    machine["pacman_rc"] = 1
    assert adopt.apply(adoption(adopt), no_snapshot=False) == 1
    assert machine["conf"].read_text() == OLD_CONF
    out = capsys.readouterr().out
    assert "FAILED" in out and "pacman.conf restored" in out and "samurai snap rollback 7" in out
    assert not adopt.STATE_PATH.exists()


def test_no_snapshot_means_no_change(adopt, machine):
    machine["snapshot_rc"] = 1
    with pytest.raises(adopt.Refused, match="nothing changed"):
        adopt.apply(adoption(adopt), no_snapshot=False)
    assert machine["commands"] == [] and machine["conf"].read_text() == OLD_CONF
    assert adopt.apply(adoption(adopt), no_snapshot=True) == 0


@pytest.fixture
def command(adopt, monkeypatch):
    """main() as root, with the examination replaced by a canned result."""
    applied: list = []
    monkeypatch.setattr(adopt.os, "geteuid", lambda: 0)
    monkeypatch.setattr(adopt.os, "environ", dict(os.environ))
    monkeypatch.setattr(adopt.os, "umask", lambda mask: 0)
    monkeypatch.setattr(signal, "signal", lambda *a: None)
    real = adopt.tempfile.TemporaryDirectory
    monkeypatch.setattr(adopt.tempfile, "TemporaryDirectory", lambda **kw: real())
    monkeypatch.setattr(adopt, "apply", lambda a, no_snapshot: applied.append(a) or 0)

    def run_with(result, *argv):
        monkeypatch.setattr(adopt, "examine", lambda opt_in, workdir: result)
        return adopt.main(list(argv))

    run_with.applied = applied
    return run_with


def test_preview_changes_nothing(adopt, command, capsys):
    assert command(adoption(adopt), "--preview") == 0 and not command.applied
    assert "add (1 selected): samurai-base" in capsys.readouterr().out


def test_second_run_has_nothing_to_do(adopt, command, capsys):
    done = adoption(adopt, selection=adopt.Selection(), enable_chaotic=False,
                    upgrades=["glibc 1 -> 2"])
    assert command(done, "--yes") == 0 and not command.applied
    assert "nothing to do" in capsys.readouterr().out


def test_pending_upgrades_need_a_decision(adopt, command):
    pending = adoption(adopt, upgrades=["glibc 1 -> 2"])
    assert command(pending, "--yes") == adopt.EXIT_NEEDS_UPDATE and not command.applied
    assert command(pending, "--yes", "--with-upgrade") == 0 and command.applied
    stale = adoption(adopt, upgrades=["samurai-tools 1 -> 2"])
    assert command(stale, "--yes", "--with-upgrade") == adopt.EXIT_NEEDS_UPDATE


def test_confirmation(adopt, command, monkeypatch):
    monkeypatch.setattr(adopt.sys.stdin, "isatty", lambda: False, raising=False)
    assert command(adoption(adopt)) == adopt.EXIT_USAGE and not command.applied
    monkeypatch.setattr(adopt.sys.stdin, "isatty", lambda: True, raising=False)
    monkeypatch.setattr("builtins.input", lambda prompt: "")
    assert command(adoption(adopt)) == adopt.EXIT_ABORTED and not command.applied
    monkeypatch.setattr("builtins.input", lambda prompt: "y")
    assert command(adoption(adopt)) == 0 and command.applied

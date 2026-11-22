"""installer/validate-profile.py against adversarial and accepted profiles,
plus static checks of the installer scripts and the live-ISO hardening.

Every file in tests/unit/profiles/reject/ must be refused; every file in
tests/unit/profiles/accept/ must pass. Environment-dependent checks (keymaps,
locale.gen) are pinned to fixtures so the result is the same on Arch, Debian
and the CI runner.
"""
from __future__ import annotations

import os
import pathlib
import re
import shutil
import subprocess

import pytest

from conftest import REPO, load_script

VALIDATOR = REPO / "installer" / "validate-profile.py"
DEFAULT_PROFILE = REPO / "installer" / "profile-default.toml"
PROFILES = pathlib.Path(__file__).resolve().parent / "profiles"
REJECT = sorted((PROFILES / "reject").glob("*.toml"))
ACCEPT = sorted((PROFILES / "accept").glob("*.toml"))
SCRIPTS = [REPO / "installer" / "samurai-install", REPO / "installer" / "samurai-postinstall"]


@pytest.fixture(scope="module")
def validator():
    return load_script(VALIDATOR, "validate_profile_t")


@pytest.fixture
def pinned_env(validator, tmp_path, monkeypatch):
    """Deterministic locale.gen and keymap tree; no SAMURAI_TEST_AUTOKEY."""
    lg = tmp_path / "locale.gen"
    lg.write_text("#en_US.UTF-8 UTF-8  \n#pl_PL.UTF-8 UTF-8\n#de_DE.UTF-8 UTF-8\n")
    km = tmp_path / "keymaps" / "i386" / "qwerty"
    km.mkdir(parents=True)
    (km / "us.map.gz").write_bytes(b"")
    (km / "pl.map.gz").write_bytes(b"")
    monkeypatch.setattr(validator, "LOCALE_GEN", lg)
    monkeypatch.setattr(validator, "KEYMAPS", tmp_path / "keymaps")
    monkeypatch.delenv("SAMURAI_TEST_AUTOKEY", raising=False)
    return validator


def test_fixture_sets_present():
    assert len(REJECT) >= 30 and len(ACCEPT) >= 5


@pytest.mark.parametrize("path", REJECT, ids=[p.stem for p in REJECT])
def test_reject(pinned_env, capsys, path):
    with pytest.raises(SystemExit) as e:
        pinned_env.validate(str(path))
    assert e.value.code == 1
    err = capsys.readouterr().err
    assert err.startswith("VALIDATION ERROR:"), f"{path.name}: crashed instead of failing cleanly: {err}"


@pytest.mark.parametrize("path", ACCEPT, ids=[p.stem for p in ACCEPT])
def test_accept(pinned_env, monkeypatch, path):
    if "autokey" in path.name:
        monkeypatch.setenv("SAMURAI_TEST_AUTOKEY", "1")
    pinned_env.validate(str(path))


def test_default_profile_accepted_without_test_env(pinned_env):
    """C1: the shipped default never needs SAMURAI_TEST_AUTOKEY (keyfile off)."""
    text = DEFAULT_PROFILE.read_text()
    assert re.search(r"(?m)^auto_unlock_keyfile = false", text)
    pinned_env.validate(str(DEFAULT_PROFILE))


def test_autokey_needs_env(pinned_env, monkeypatch, tmp_path):
    p = tmp_path / "ak.toml"
    p.write_text(DEFAULT_PROFILE.read_text().replace("auto_unlock_keyfile = false", "auto_unlock_keyfile = true"))
    with pytest.raises(SystemExit):
        pinned_env.validate(str(p))
    monkeypatch.setenv("SAMURAI_TEST_AUTOKEY", "1")
    pinned_env.validate(str(p))


def test_validator_cli_rejects_pacman_options():
    r = subprocess.run(["python3", "-I", str(VALIDATOR), str(PROFILES / "reject" / "p01-pkg-option.toml")],
                       capture_output=True, text=True)
    assert r.returncode == 1 and "packages.install" in r.stderr


def test_validator_cli_accepts_default():
    r = subprocess.run(["python3", "-I", str(VALIDATOR), str(DEFAULT_PROFILE)],
                       capture_output=True, text=True)
    assert r.returncode == 0, r.stderr


# --- static checks of the shell scripts ----------------------------------

@pytest.mark.parametrize("script", SCRIPTS, ids=[s.name for s in SCRIPTS])
def test_bash_syntax(script):
    subprocess.run(["bash", "-n", str(script)], check=True)


@pytest.mark.skipif(shutil.which("shellcheck") is None, reason="shellcheck not installed")
@pytest.mark.parametrize("script", SCRIPTS, ids=[s.name for s in SCRIPTS])
def test_shellcheck(script):
    r = subprocess.run(["shellcheck", "-S", "warning", str(script)], capture_output=True, text=True)
    assert r.returncode == 0, r.stdout


def test_no_eval_no_hidden_failures():
    for s in SCRIPTS:
        text = s.read_text()
        assert not re.search(r"(?m)^\s*eval\b", text), f"{s.name}: eval"
        # profile-derived package lists sit behind a `--` wherever pacman is
        # called directly (pacstrap is the documented exception: it appends its
        # own options after the list)
        assert "pac_install " in text or re.search(r"--needed -- \\\n\s+\"\$\{BASE_PKGS\[@\]\}\"", text)
    pi = (REPO / "installer" / "samurai-postinstall").read_text()
    assert "snapper.timer" not in pi                      # M9: the unit does not exist
    assert "10-samurai-important-snap.hook" not in pi.split("rm -f")[0]   # M10: hook only ever removed
    assert "/etc/snap-pac.ini" in pi and "important_packages" in pi
    assert 'SNAPPER_CONFIGS="root"' in pi
    code = "\n".join(l for l in pi.splitlines() if not l.lstrip().startswith("#"))
    assert "--remove-service-from-zone=" in code
    assert "--remove-service=" not in code.replace("--remove-service-from-zone=", "")   # lokkit form is rejected
    assert "usermod -p" not in pi and "chpasswd -e" in pi  # L1
    assert "passwd -l root || true" not in pi               # M18
    assert "mkdir -p etc/" not in pi and " etc/resolv.conf" not in pi   # M11
    assert "console=ttyS0" in pi and "SAMURAI_SERIAL_CONSOLE" in pi     # M8
    assert "rd.luks.options=$LUKS_UUID=discard" in pi       # M15
    assert "systemd-detect-virt -q" in pi                   # L7
    assert "PasswordAuthentication no" in pi                # L4
    assert ". /etc/samurai-install-env" not in pi and ". /var/lib/samurai/install-env" not in pi  # L8
    si = (REPO / "installer" / "samurai-install").read_text()
    assert 'ESP="${DISK}1"' not in si and 'ESP="${DISK}${PSEP}1"' in si   # H1
    assert "trap cleanup EXIT" in si                        # M12
    assert "fmask=0077,dmask=0077" in si                    # M7
    assert "--allow-discards" in si and "discard=async" in si             # M15
    assert "luksAddKey" not in si                           # M4
    assert "--pbkdf argon2id" in si and "--key-size 512" in si
    assert "openssl passwd -6 -stdin" in si                 # L1
    assert 'firewall = "none"' not in si.split("[security]")[0] or 'firewall = "%s"' in si   # H4
    assert "cp -a /etc/pacman.d/." not in si                # M3
    assert "pacman.conf.target" in si                       # M2
    assert "grep -v '^password_hash = '" in si              # H6
    assert "chattr +C" in si and "nodatacow" not in si.split("# no-CoW")[0].split("mount -o")[-1]   # M14


# --- what the ISO ships ----------------------------------------------------

def test_pacman_conf_target():
    text = (REPO / "installer" / "pacman.conf.target").read_text()
    assert "XferCommand" not in text.replace("curl XferCommand", "")    # comment mentions it, no directive
    assert re.search(r"(?m)^DownloadUser = alpm$", text)
    assert re.search(r"(?m)^CheckSpace$", text)
    assert re.search(r"(?m)^SigLevel\s*= Required DatabaseOptional$", text)
    assert "[samurai]" not in text.replace("# [samurai]", "")          # appended by postinstall, below everything
    order = [m.group(1) for m in re.finditer(r"(?m)^\[(core|extra|multilib|cachyos)\]$", text)]
    assert order == ["core", "extra", "multilib", "cachyos"]


def test_live_iso_hardening():
    wants = REPO / "iso" / "airootfs" / "etc" / "systemd" / "system" / "multi-user.target.wants"
    assert not (wants / "sshd.service").exists()                       # M17
    assert not (wants.parent / "cloud-init.target.wants").exists()
    assert "cloud-init" not in (REPO / "iso" / "packages.x86_64").read_text().split()
    script = (REPO / "iso" / "airootfs" / "root" / ".automated_script.sh").read_text()
    assert '=~ ^https://' in script and "(http|https|ftp|tftp)://" not in script   # L5


ISO_COPIES = {
    "iso/airootfs/usr/local/bin/samurai-install": "installer/samurai-install",
    "iso/airootfs/usr/local/bin/samurai-postinstall": "installer/samurai-postinstall",
    "iso/airootfs/usr/local/bin/validate-profile.py": "installer/validate-profile.py",
    "iso/airootfs/usr/share/samurai/profile-default.toml": "installer/profile-default.toml",
    "iso/airootfs/usr/share/samurai/pacman.conf.target": "installer/pacman.conf.target",
}


@pytest.mark.parametrize("copy, source", ISO_COPIES.items(), ids=list(ISO_COPIES))
def test_iso_copies_identical(copy, source):
    assert (REPO / copy).read_bytes() == (REPO / source).read_bytes(), f"{copy} drifted from {source}"
    if copy.startswith("iso/airootfs/usr/local/bin/"):
        assert os.access(REPO / copy, os.X_OK)

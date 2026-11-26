#!/usr/bin/env python3
"""Strict validation of a Samurai OS install profile (spec 08).

Security posture (Genor 2026-11-17: security is of utmost concern):
- allowlist of known keys; unknown keys abort (typos can never change behavior)
- every value is type-checked, then matched against a strict regex or checked
  with a real tool (block device, timezone, locale, keymap)
- values that reach pacman, useradd, sed or a heredoc are bounded here: no
  leading '-' (pacman options), no shell metacharacters, no newlines
- keys the installer does not honour yet (groups beyond wheel, root_locked,
  repos toggles) must keep their default value: a profile never claims what
  the system does not do
- the test-only keyfile unlock (auto_unlock_keyfile) is refused unless the
  environment says SAMURAI_TEST_AUTOKEY=1
Exit 0 with a normalized summary on success; non-zero with reasons on failure.
"""
from __future__ import annotations

import importlib.machinery
import importlib.util
import os
import pathlib
import re
import subprocess
import sys
import tomllib

ALLOWED_TOP = {"version", "system", "disk", "boot", "user", "packages", "repos", "security", "hardware",
               "kde", "gaming", "development", "network"}
ALLOWED = {
    "system": {"hostname", "timezone", "locale", "keyboard"},
    "disk": {"device", "layout", "filesystem", "encryption", "auto_unlock_keyfile"},
    "boot": {"kernels", "bootloader"},
    "user": {"name", "shell", "groups", "root_locked", "password_hash"},
    "packages": {"install", "desktop", "apps", "browsers"},
    "repos": {"samurai", "cachyos", "chaotic_aur", "arch_mirror"},
    "security": {"firewall", "ssh_server"},
    "hardware": {"nvidia"},
    "kde": {"theme", "monitor_widgets", "reduced_effects"},
    "gaming": {"enabled", "steam", "heroic", "gamemode", "mangohud", "gamescope", "proton_ge",
               "proton_cachyos", "wine"},
    "development": {"python", "conda", "docker", "ai_clis", "ollama"},
    "network": {"tailscale", "reverse_proxy"},
}
KNOWN_KERNELS = {"linux", "linux-lts", "linux-zen", "linux-cachyos", "linux-cachyos-lts"}
KNOWN_BOOTLOADERS = {"limine", "grub", "systemd-boot", "refind"}
KNOWN_DESKTOPS = {"kde", "gnome", "xfce", "lxqt", "none"}
KNOWN_SHELLS = {"bash", "fish", "zsh"}
KNOWN_LAYOUTS = {"erase"}
KNOWN_FS = {"btrfs"}
KNOWN_FIREWALLS = {"none", "firewalld", "ufw"}
KNOWN_NVIDIA = {"auto", "yes", "no"}
KNOWN_THEMES = {"breeze"}
UNPACKAGED_THEMES = {"otto", "scratchy"}
KNOWN_PYTHONS = {"3.10", "3.11", "3.12", "3.13"}
KNOWN_CONDA = {"miniforge", "none"}
KNOWN_DOCKER = {"group", "rootless", "none"}
KNOWN_PROXIES = {"none", "caddy", "nginx", "traefik"}
# repos toggles are accepted only at their defaults until the installer honours them
REPO_DEFAULTS = {"samurai": True, "cachyos": True, "chaotic_aur": True}
USERNAME_RE = re.compile(r"^[a-z_][a-z0-9_-]{0,31}$")
HOSTNAME_RE = re.compile(r"^[a-zA-Z0-9][a-zA-Z0-9-]{0,62}$")
LOCALE_RE = re.compile(r"^[a-zA-Z]{2,3}(_[A-Za-z]{2,4})?\.[A-Za-z0-9-]+$")
KEYBOARD_RE = re.compile(r"^[a-z0-9][a-z0-9_-]{0,31}$")
GROUP_RE = re.compile(r"^[a-z_][a-z0-9_-]{0,31}$")
TZ_RE = re.compile(r"^[A-Za-z0-9_+-]+(/[A-Za-z0-9_+-]+)*$")
DEVICE_RE = re.compile(r"^/dev/[a-z0-9]+$")
# kernel naming: sdX/vdX/hdX/xvdX + digits, nvmeXnY + pZ, mmcblkX + pZ are partitions
PARTITION_RE = re.compile(r"^/dev/((sd|vd|hd|xvd)[a-z]+[0-9]+|(nvme[0-9]+n[0-9]+|mmcblk[0-9]+)p[0-9]+)$")
# never whole-disk targets: optical, loop, ram, zram, device-mapper, floppy, md (no RAID in M2)
DEVICE_DENY_RE = re.compile(r"^/dev/(sr|loop|ram|zram|dm-|fd|md|nbd)[0-9]")
# Arch package names: lowercase alnum start, then alnum @ . _ + -  (never a leading '-')
PKG_RE = re.compile(r"^[a-z0-9][a-z0-9@._+-]{0,127}$")
APP_REF_RE = re.compile(r"^@?[A-Za-z0-9][A-Za-z0-9_.+-]{0,63}$")
# sha512crypt ($6$, optional rounds=) as openssl passwd -6 / mkpasswd -m sha-512 produce,
# or yescrypt ($y$, libxcrypt's default on Arch) as mkpasswd -m yescrypt produces
PWHASH_RE = re.compile(
    r"^(\$6\$(rounds=[0-9]{4,9}\$)?[./A-Za-z0-9]{1,16}\$[./A-Za-z0-9]{86}"
    r"|\$y\$[./A-Za-z0-9]+\$[./A-Za-z0-9]{0,86}\$[./A-Za-z0-9]{43})$"
)
MIRROR_RE = re.compile(r"^https://[A-Za-z0-9.-]+(:[0-9]{1,5})?(/[A-Za-z0-9._-]+)*/?$")
ZONEINFO = pathlib.Path("/usr/share/zoneinfo")
LOCALE_GEN = pathlib.Path("/etc/locale.gen")
KEYMAPS = pathlib.Path("/usr/share/kbd/keymaps")


def _load_apps_module():
    """The catalog allowlist comes from samurai-apps itself (one merge
    implementation): next to this script on the ISO/installed system, or
    updates/ in a source checkout."""
    here = pathlib.Path(__file__).resolve().parent
    for cand in (here / "samurai-apps", here.parent / "updates" / "samurai-apps"):
        if cand.is_file():
            loader = importlib.machinery.SourceFileLoader("samurai_apps", str(cand))
            spec = importlib.util.spec_from_loader("samurai_apps", loader)
            mod = importlib.util.module_from_spec(spec)
            sys.modules[spec.name] = mod   # dataclasses resolve types via sys.modules
            loader.exec_module(mod)
            return mod
    return None


def validate_apps(apps) -> list[str]:
    """[packages] apps: strict allowlist of catalog app ids and @bundles."""
    if not isinstance(apps, list) or not all(isinstance(a, str) and APP_REF_RE.match(a) for a in apps):
        fail("packages.apps must be a list of catalog app ids or @bundles (no free-form values)")
    if not apps:
        return []
    mod = _load_apps_module()
    if mod is None:
        fail("packages.apps needs the app catalog, but samurai-apps is not installed")
    try:
        catalog = mod.load_catalog()
        return mod.profile_app_names(catalog, apps)
    except mod.CatalogError as e:
        fail(f"app catalog unusable: {e}")
    except mod.UsageError as e:
        fail(str(e))
    return []


def validate_category(ids, category: str, where: str) -> None:
    """A list of distinct catalog app ids that belong to one category."""
    if not isinstance(ids, list) or not all(isinstance(a, str) and APP_REF_RE.match(a) for a in ids):
        fail(f"{where} must be a list of catalog app ids")
    if len(set(ids)) != len(ids):
        fail(f"{where} lists an app twice")
    if not ids:
        return
    mod = _load_apps_module()
    if mod is None:
        fail(f"{where} needs the app catalog, but samurai-apps is not installed")
    try:
        catalog = mod.load_catalog()
    except mod.CatalogError as e:
        fail(f"app catalog unusable: {e}")
    for app_id in ids:
        app = catalog.apps.get(app_id)
        if app is None or app.hidden or app.category != category:
            offered = sorted(a.id for a in catalog.apps.values()
                             if a.category == category and not a.hidden)
            fail(f"{where}: {app_id!r} is not one of {offered}")


def validate_software(profile: dict, desktop: str) -> None:
    """Spec 08 pages 6a and 8-11 (docs/plans/profile-schema.md)."""
    packages = profile.get("packages", {})
    if "browsers" in packages:
        validate_category(packages["browsers"], "browsers", "packages.browsers")
        if packages["browsers"] and desktop == "none":
            fail("packages.browsers needs a desktop")

    if "kde" in profile:
        if desktop != "kde":
            fail('[kde] needs packages.desktop = "kde"')
        kde = profile["kde"]
        theme = _str(kde, "theme", "breeze", "kde")
        if theme in UNPACKAGED_THEMES:
            fail(f"kde.theme = {theme!r} is not packaged yet (redistribution licence unconfirmed)")
        if theme not in KNOWN_THEMES:
            fail(f"unsupported kde.theme: {theme!r}")
        _bool(kde, "monitor_widgets", True, "kde")
        _bool(kde, "reduced_effects", True, "kde")

    gaming = profile.get("gaming", {})
    enabled = _bool(gaming, "enabled", False, "gaming")
    chosen = [k for k in sorted(ALLOWED["gaming"] - {"enabled"})
              if _bool(gaming, k, enabled, "gaming")]
    if enabled and desktop == "none":
        fail("gaming.enabled needs a desktop")
    if not enabled and chosen:
        fail(f"gaming.enabled is false but these items are on: {', '.join(chosen)}")

    dev = profile.get("development", {})
    pythons = _str_list(dev, "python", [], "development")
    if not all(v in KNOWN_PYTHONS for v in pythons) or len(set(pythons)) != len(pythons):
        fail(f"development.python must be a list without duplicates from {sorted(KNOWN_PYTHONS)}")
    conda = _str(dev, "conda", "none", "development")
    if conda == "miniconda":
        fail("development.conda = 'miniconda' is not packaged (Anaconda licence); use 'miniforge'")
    if conda not in KNOWN_CONDA:
        fail(f"unsupported development.conda: {conda!r}")
    docker = _str(dev, "docker", "none", "development")
    if docker not in KNOWN_DOCKER:
        fail(f"development.docker must be one of {sorted(KNOWN_DOCKER)}, got {docker!r}")
    validate_category(dev.get("ai_clis", []), "ai-cli", "development.ai_clis")
    _bool(dev, "ollama", False, "development")

    net = profile.get("network", {})
    _bool(net, "tailscale", False, "network")
    proxy = _str(net, "reverse_proxy", "none", "network")
    if proxy not in KNOWN_PROXIES:
        fail(f"network.reverse_proxy must be one of {sorted(KNOWN_PROXIES)}, got {proxy!r}")


def fail(msg: str) -> None:
    print(f"VALIDATION ERROR: {msg}", file=sys.stderr)
    raise SystemExit(1)


def _str(section: dict, key: str, default: str, where: str) -> str:
    v = section.get(key, default)
    if not isinstance(v, str) or "\n" in v or "\r" in v or "\0" in v:
        fail(f"{where}.{key} must be a single-line string")
    return v


def _bool(section: dict, key: str, default: bool, where: str) -> bool:
    v = section.get(key, default)
    if not isinstance(v, bool):
        fail(f"{where}.{key} must be true or false (got {v!r})")
    return v


def _str_list(section: dict, key: str, default: list, where: str) -> list[str]:
    v = section.get(key, default)
    if not isinstance(v, list) or not all(isinstance(x, str) for x in v):
        fail(f"{where}.{key} must be a list of strings")
    return v


def check_timezone(tz: str) -> None:
    if not TZ_RE.match(tz):
        fail(f"invalid timezone: {tz!r}")
    first = tz.split("/", 1)[0]
    if first in ("posix", "right") or tz in ("localtime", "posixrules"):
        fail(f"timezone must be a canonical zone name (no posix/ or right/ prefix): {tz!r}")
    p = ZONEINFO / tz
    if not p.is_file():
        fail(f"unknown timezone: {tz!r}")


def check_locale(locale: str) -> None:
    if not LOCALE_RE.match(locale):
        fail(f"invalid locale: {locale!r}")
    if LOCALE_GEN.is_file():
        # /etc/locale.gen lists every locale glibc can generate (commented out);
        # the installer uncomments exactly that line
        pat = re.compile(r"^#?\s*" + re.escape(locale) + r"\s+\S+\s*$")
        if not any(pat.match(line) for line in LOCALE_GEN.read_text(errors="replace").splitlines()):
            fail(f"locale {locale!r} is not listed in /etc/locale.gen")


def check_keyboard(kb: str) -> None:
    if not KEYBOARD_RE.match(kb):
        fail(f"invalid keyboard: {kb!r}")
    if KEYMAPS.is_dir():
        found = any(KEYMAPS.rglob(f"{kb}.map.gz")) or any(KEYMAPS.rglob(f"{kb}.map"))
        if not found:
            fail(f"keyboard layout {kb!r} has no keymap under {KEYMAPS}")


def check_device(device: str) -> None:
    if not DEVICE_RE.match(device):
        fail(f"invalid disk device: {device!r} (expected /dev/<whole disk>)")
    if DEVICE_DENY_RE.match(device):
        fail(f"refusing {device!r}: not an installable whole disk")
    if PARTITION_RE.match(device):
        fail(f"refusing {device!r}: that is a partition, give the whole disk")
    p = pathlib.Path(device)
    if p.exists():
        # on the live system: must be a whole disk (never a partition, cdrom or
        # mapper target). A profile validated elsewhere may name a disk that
        # does not exist here; samurai-install re-checks on the live medium.
        if not p.is_block_device():
            fail(f"{device!r} exists but is not a block device")
        try:
            out = subprocess.run(["lsblk", "-dno", "TYPE", device], capture_output=True,
                                 text=True, check=True).stdout.strip()
        except (OSError, subprocess.CalledProcessError) as e:
            fail(f"cannot inspect {device!r}: {e}")
        if out != "disk":
            fail(f"{device!r} is a {out or 'unknown'} device, not a whole disk")


def validate(path: str) -> dict:
    raw = pathlib.Path(path).read_bytes()
    try:
        profile = tomllib.loads(raw.decode("utf-8"))
    except (tomllib.TOMLDecodeError, UnicodeDecodeError) as e:
        fail(f"profile is not valid TOML: {e}")

    for key in profile:
        if key not in ALLOWED_TOP:
            fail(f"unknown top-level key: {key!r}")
    version = profile.get("version")
    if not isinstance(version, int) or isinstance(version, bool) or version != 1:
        fail("unsupported profile version (must be the integer 1)")

    for section, keys in ALLOWED.items():
        s = profile.get(section, {})
        if not isinstance(s, dict):
            fail(f"[{section}] must be a table")
        for key in s:
            if key not in keys:
                fail(f"unknown key: [{section}] {key!r}")

    syssec = profile.get("system", {})
    hostname = _str(syssec, "hostname", "samurai", "system")
    if not HOSTNAME_RE.match(hostname):
        fail(f"invalid hostname: {hostname!r}")
    check_timezone(_str(syssec, "timezone", "UTC", "system"))
    check_locale(_str(syssec, "locale", "en_US.UTF-8", "system"))
    check_keyboard(_str(syssec, "keyboard", "us", "system"))

    disk = profile.get("disk", {})
    check_device(_str(disk, "device", "", "disk"))
    if _str(disk, "layout", "erase", "disk") not in KNOWN_LAYOUTS:
        fail(f"unsupported disk layout: {disk.get('layout')!r} (M2 supports 'erase' only)")
    if _str(disk, "filesystem", "btrfs", "disk") not in KNOWN_FS:
        fail(f"unsupported filesystem: {disk.get('filesystem')!r}")
    enc = _bool(disk, "encryption", True, "disk")
    autokey = _bool(disk, "auto_unlock_keyfile", False, "disk")
    if autokey and not enc:
        fail("auto_unlock_keyfile requires encryption")
    if autokey and os.environ.get("SAMURAI_TEST_AUTOKEY") != "1":
        fail("auto_unlock_keyfile stores the disk key unencrypted on the ESP; it is a VM-test "
             "affordance, refused unless the environment has SAMURAI_TEST_AUTOKEY=1")

    boot = profile.get("boot", {})
    kernels = _str_list(boot, "kernels", ["linux"], "boot")
    if not kernels or not all(k in KNOWN_KERNELS for k in kernels) or len(set(kernels)) != len(kernels):
        fail(f"kernels must be a non-empty list without duplicates from {sorted(KNOWN_KERNELS)}")
    if _str(boot, "bootloader", "limine", "boot") not in KNOWN_BOOTLOADERS:
        fail(f"unsupported bootloader: {boot.get('bootloader')!r}")

    user = profile.get("user", {})
    uname = _str(user, "name", "", "user")
    if not USERNAME_RE.match(uname):
        fail(f"invalid username: {uname!r}")
    if uname in ("root", "adm", "sudo", "bin", "daemon", "nobody", "alpm", "live"):
        fail(f"refusing reserved username: {uname!r}")
    if _str(user, "shell", "bash", "user") not in KNOWN_SHELLS:
        fail(f"unsupported shell: {user.get('shell')!r}")
    phash = _str(user, "password_hash", "", "user")
    if phash and not PWHASH_RE.match(phash):
        fail("user.password_hash must be a sha512crypt ($6$) or yescrypt ($y$) crypt hash (never plaintext)")
    groups = _str_list(user, "groups", ["wheel"], "user")
    if not all(GROUP_RE.match(g) for g in groups):
        fail("groups must be a list of valid group names")
    if "wheel" not in groups:
        fail("the user must be in wheel (sudo); refusing a root-locked box with no admin")
    if set(groups) != {"wheel"}:
        fail(f"user.groups: only [\"wheel\"] is supported in M2 (got {groups!r}); "
             "extra groups are added by samurai apps / the toolbox")
    if not _bool(user, "root_locked", True, "user"):
        fail("user.root_locked = false is not supported: root is always locked (spec 09)")

    pkgsec = profile.get("packages", {})
    packages = _str_list(pkgsec, "install", [], "packages")
    if not all(PKG_RE.match(p) for p in packages):
        fail("packages.install must be a list of pacman package names "
             "(lowercase, no leading '-', no shell metacharacters)")

    validate_apps(pkgsec.get("apps", []))

    desktop = _str(pkgsec, "desktop", "kde", "packages")
    if desktop not in KNOWN_DESKTOPS:
        fail(f"unsupported desktop: {desktop!r} (choose from {sorted(KNOWN_DESKTOPS)})")
    validate_software(profile, desktop)

    sec = profile.get("security", {})
    if _str(sec, "firewall", "none", "security") not in KNOWN_FIREWALLS:
        fail(f"unsupported firewall: {sec.get('firewall')!r}")
    _bool(sec, "ssh_server", False, "security")
    hw = profile.get("hardware", {})
    nv = _str(hw, "nvidia", "auto", "hardware")
    if nv not in KNOWN_NVIDIA:
        fail(f"hardware.nvidia must be auto, yes or no, got {nv!r}")

    repos = profile.get("repos", {})
    for key, default in REPO_DEFAULTS.items():
        if _bool(repos, key, default, "repos") != default:
            fail(f"repos.{key} = {not default} is not supported yet (the installer always uses "
                 f"{key}={default}); remove the key")
    am = _str(repos, "arch_mirror", "auto", "repos")
    if am != "auto" and (not MIRROR_RE.match(am) or "/../" in am or am.endswith("/..")):
        fail(f"repos.arch_mirror must be 'auto' or a plain https URL, got {am!r}")

    return profile


def main() -> None:
    if len(sys.argv) != 2:
        print("usage: validate-profile.py <profile.toml>", file=sys.stderr)
        raise SystemExit(2)
    profile = validate(sys.argv[1])
    print(f"profile OK: host={profile.get('system', {}).get('hostname')} "
          f"disk={profile.get('disk', {}).get('device')} "
          f"encryption={profile.get('disk', {}).get('encryption', True)}")


if __name__ == "__main__":
    main()

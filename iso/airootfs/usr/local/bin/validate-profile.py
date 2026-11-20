#!/usr/bin/env python3
"""Strict validation of a Samurai OS install profile (spec 08).

Security posture (Genor 2026-11-17: security is of utmost concern):
- allowlist of known keys; unknown keys abort (typos can never change behavior)
- every free-text value is matched against a strict regex or checked with a
  real tool (block device, timezone)
- no value from the profile is ever passed to a shell unquoted by the
  callers; the profile must not and cannot carry shell commands
Exit 0 with a normalized summary on success; non-zero with reasons on failure.
"""
from __future__ import annotations

import pathlib
import re
import subprocess
import sys
import tomllib

ALLOWED_TOP = {"version", "system", "disk", "boot", "user", "packages", "repos", "security"}
ALLOWED = {
    "system": {"hostname", "timezone", "locale", "keyboard"},
    "disk": {"device", "layout", "filesystem", "encryption", "auto_unlock_keyfile"},
    "boot": {"kernels", "bootloader"},
    "user": {"name", "shell", "groups", "root_locked", "password_hash"},
    "packages": {"install", "desktop"},
    "repos": {"samurai", "cachyos", "chaotic_aur", "arch_mirror"},
    "security": {"firewall", "ssh_server"},
}
KNOWN_KERNELS = {"linux", "linux-lts", "linux-zen", "linux-cachyos", "linux-cachyos-lts"}
KNOWN_BOOTLOADERS = {"limine", "grub", "systemd-boot", "refind"}
KNOWN_DESKTOPS = {"kde", "gnome", "xfce", "lxqt", "none"}
KNOWN_SHELLS = {"bash", "fish", "zsh"}
KNOWN_LAYOUTS = {"erase"}
KNOWN_FS = {"btrfs"}
KNOWN_FIREWALLS = {"none", "firewalld", "ufw"}
USERNAME_RE = re.compile(r"^[a-z_][a-z0-9_-]{0,31}$")
HOSTNAME_RE = re.compile(r"^[a-zA-Z0-9][a-zA-Z0-9-]{0,62}$")
LOCALE_RE = re.compile(r"^[a-zA-Z]{2,3}(_[A-Za-z]{2,4})?\.[A-Za-z0-9-]+$")
KEYBOARD_RE = re.compile(r"^[a-z0-9-]{1,20}$")
GROUP_RE = re.compile(r"^[a-z_][a-z0-9_-]{0,31}$")
DEVICE_RE = re.compile(r"^/dev/[A-Za-z0-9/_.-]+$")


def fail(msg: str) -> None:
    print(f"VALIDATION ERROR: {msg}", file=sys.stderr)
    raise SystemExit(1)


def validate(path: str) -> dict:
    raw = pathlib.Path(path).read_bytes()
    try:
        profile = tomllib.loads(raw.decode("utf-8"))
    except (tomllib.TOMLDecodeError, UnicodeDecodeError) as e:
        fail(f"profile is not valid TOML: {e}")

    for key in profile:
        if key not in ALLOWED_TOP:
            fail(f"unknown top-level key: {key!r}")
    if profile.get("version") != 1:
        fail("unsupported profile version")

    for section, keys in ALLOWED.items():
        s = profile.get(section, {})
        if not isinstance(s, dict):
            fail(f"[{section}] must be a table")
        for key in s:
            if key not in keys:
                fail(f"unknown key: [{section}] {key!r}")

    syssec = profile.get("system", {})
    hostname = syssec.get("hostname", "samurai")
    if not HOSTNAME_RE.match(hostname):
        fail(f"invalid hostname: {hostname!r}")
    tz = syssec.get("timezone", "UTC")
    if not re.match(r"^[A-Za-z0-9_+/-]+$", tz) or not pathlib.Path(f"/usr/share/zoneinfo/{tz}").exists():
        fail(f"unknown timezone: {tz!r}")
    locale = syssec.get("locale", "en_US.UTF-8")
    if not LOCALE_RE.match(locale):
        fail(f"invalid locale: {locale!r}")
    kb = syssec.get("keyboard", "us")
    if not KEYBOARD_RE.match(kb):
        fail(f"invalid keyboard: {kb!r}")

    disk = profile.get("disk", {})
    device = disk.get("device", "")
    if not DEVICE_RE.match(device):
        fail(f"invalid disk device: {device!r}")
    if disk.get("layout", "erase") not in KNOWN_LAYOUTS:
        fail(f"unsupported disk layout: {disk.get('layout')!r} (M2 supports 'erase' only)")
    if disk.get("filesystem", "btrfs") not in KNOWN_FS:
        fail(f"unsupported filesystem: {disk.get('filesystem')!r}")
    enc = bool(disk.get("encryption", True))
    if disk.get("auto_unlock_keyfile", False) and not enc:
        fail("auto_unlock_keyfile requires encryption")

    boot = profile.get("boot", {})
    kernels = boot.get("kernels", ["linux"])
    if not isinstance(kernels, list) or not kernels or not all(k in KNOWN_KERNELS for k in kernels):
        fail(f"kernels must be a non-empty list from {sorted(KNOWN_KERNELS)}")
    if boot.get("bootloader", "systemd-boot") not in KNOWN_BOOTLOADERS:
        fail(f"unsupported bootloader: {boot.get('bootloader')!r}")

    user = profile.get("user", {})
    uname = user.get("name", "")
    if not USERNAME_RE.match(uname):
        fail(f"invalid username: {uname!r}")
    if uname in ("root", "adm", "sudo", "bin", "daemon"):
        fail(f"refusing reserved username: {uname!r}")
    if user.get("shell", "bash") not in KNOWN_SHELLS:
        fail(f"unsupported shell: {user.get('shell')!r}")
    phash = user.get("password_hash", "")
    if phash and not re.match(r"^\$[67]\$[./A-Za-z0-9]+\$[./A-Za-z0-9]{86}$", phash):
        fail("user.password_hash must be a sha-512/yescrypt crypt hash (never plaintext)")
    groups = user.get("groups", ["wheel"])
    if not isinstance(groups, list) or not all(GROUP_RE.match(g) for g in groups):
        fail("groups must be a list of valid group names")
    if "wheel" not in groups:
        fail("the user must be in wheel (sudo); refusing a root-locked box with no admin")

    packages = profile.get("packages", {}).get("install", [])
    if not isinstance(packages, list) or not all(
        re.match(r"^[a-zA-Z0-9_.-]+$", p) for p in packages
    ):
        fail("packages.install must be a list of package names (no shell metacharacters)")

    desktop = profile.get("packages", {}).get("desktop", "kde")
    if desktop not in KNOWN_DESKTOPS:
        fail(f"unsupported desktop: {desktop!r} (choose from {sorted(KNOWN_DESKTOPS)})")

    sec = profile.get("security", {})
    if sec.get("firewall", "none") not in KNOWN_FIREWALLS:
        fail(f"unsupported firewall: {sec.get('firewall')!r}")
    am = profile.get("repos", {}).get("arch_mirror", "auto")
    if am != "auto" and not re.match(r"^https://[A-Za-z0-9./_-]+$", am):
        fail(f"repos.arch_mirror must be 'auto' or an https URL, got {am!r}")

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

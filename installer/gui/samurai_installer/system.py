"""Facts about the live machine the wizard shows. Fixed argv only, never a shell."""
from __future__ import annotations

import json
import os
import pathlib
import subprocess
from dataclasses import dataclass, field

GIB = 1024 ** 3
MIN_DISK_BYTES = 20 * GIB          # refused below this
RECOMMENDED_DISK_BYTES = 40 * GIB  # spec 08 page 1
MIN_MEMORY_MIB = 3500
ZONEINFO = pathlib.Path("/usr/share/zoneinfo")
LOCALE_GEN = pathlib.Path("/etc/locale.gen")
KEYMAPS = pathlib.Path("/usr/share/kbd/keymaps")
KBD_MODEL_MAP = pathlib.Path("/usr/share/systemd/kbd-model-map")
FLAVOR_FILE = pathlib.Path("/etc/samurai-iso-flavor")
LIVE_MOUNT = "/run/archiso/bootmnt"
LSBLK_COLUMNS = "NAME,PATH,SIZE,TYPE,MODEL,TRAN,RM,RO,FSTYPE,LABEL,PARTTYPENAME,MOUNTPOINTS"


@dataclass
class Disk:
    path: str
    size: int
    model: str
    transport: str
    removable: bool
    partitions: list[str] = field(default_factory=list)   # human readable, one per line
    blocked: str = ""                                     # why it cannot be chosen

    @property
    def size_text(self) -> str:
        return format_size(self.size)

    def as_dict(self) -> dict:
        return {"path": self.path, "size": self.size_text, "model": self.model or "Unknown disk",
                "transport": self.transport, "removable": self.removable,
                "partitions": self.partitions, "blocked": self.blocked,
                "small": self.size < RECOMMENDED_DISK_BYTES}


def format_size(size: int) -> str:
    value = float(size)
    for unit in ("B", "KiB", "MiB", "GiB"):
        if value < 1024:
            return f"{value:.0f} {unit}" if unit in ("B", "KiB", "MiB") else f"{value:.1f} {unit}"
        value /= 1024
    return f"{value:.1f} TiB"


def run(argv: list[str], timeout: float = 15.0) -> str:
    """stdout of a fixed command, "" when it is missing or fails."""
    try:
        done = subprocess.run(argv, capture_output=True, text=True, timeout=timeout, check=True)
    except (OSError, subprocess.SubprocessError):
        return ""
    return done.stdout


def _flatten(node: dict) -> list[dict]:
    out = [node]
    for child in node.get("children", []) or []:
        out.extend(_flatten(child))
    return out


def parse_disks(lsblk_json: str, live_source: str = "") -> list[Disk]:
    """Whole fixed disks from ``lsblk -J -b``; the live medium and busy disks are
    listed but blocked (samurai-install refuses them too)."""
    try:
        tree = json.loads(lsblk_json)["blockdevices"]
    except (ValueError, KeyError, TypeError):
        return []
    disks: list[Disk] = []
    for node in tree:
        if node.get("type") != "disk":
            continue
        path = str(node.get("path", ""))
        name = str(node.get("name", ""))
        if name.startswith(("zram", "fd", "ram", "nbd", "loop")):
            continue
        size = int(node.get("size") or 0)
        if size <= 0:
            continue
        members = _flatten(node)
        partitions = []
        for member in members[1:] if len(members) > 1 else members:
            fstype = member.get("fstype") or ""
            if member is node and not fstype:
                continue
            bits = [str(member.get("name", "")), format_size(int(member.get("size") or 0))]
            kind = fstype or member.get("parttypename") or "unformatted"
            bits.append(str(kind))
            if member.get("label"):
                bits.append(f"“{member['label']}”")
            partitions.append("  ·  ".join(bits))
        disk = Disk(path=path, size=size, model=str(node.get("model") or "").strip(),
                    transport=str(node.get("tran") or ""), removable=bool(node.get("rm")),
                    partitions=partitions)
        mounts = [m for member in members for m in (member.get("mountpoints") or []) if m]
        if live_source and any(m.get("path") == live_source for m in members):
            disk.blocked = "This is the medium you booted from."
        elif node.get("ro"):
            disk.blocked = "This disk is read-only."
        elif mounts:
            disk.blocked = "In use (mounted: " + ", ".join(mounts[:3]) + "). Unmount it first."
        elif size < MIN_DISK_BYTES:
            disk.blocked = f"Too small: at least {format_size(MIN_DISK_BYTES)} is needed."
        disks.append(disk)
    return disks


def list_disks() -> list[Disk]:
    live = run(["findmnt", "-no", "SOURCE", LIVE_MOUNT]).strip()
    return parse_disks(run(["lsblk", "-J", "-b", "-o", LSBLK_COLUMNS]), live)


def memory_mib() -> int:
    try:
        for line in pathlib.Path("/proc/meminfo").read_text().splitlines():
            if line.startswith("MemTotal:"):
                return int(line.split()[1]) // 1024
    except (OSError, ValueError, IndexError):
        pass
    return 0


def parse_gpus(lspci_mm: str) -> list[dict]:
    """Display controllers from ``lspci -mm -nn -D``: PCI classes 0300/0302/0380."""
    gpus = []
    for line in lspci_mm.splitlines():
        parts = line.split('"')
        if len(parts) < 6 or not any(f"[{c}]" in parts[1] for c in ("0300", "0302", "0380")):
            continue
        gpus.append({"address": parts[0].strip(), "vendor": parts[3], "device": parts[5],
                     "nvidia": "[10de]" in parts[3]})
    return gpus


def list_gpus() -> list[dict]:
    return parse_gpus(run(["lspci", "-mm", "-nn", "-D"]))


def timezones() -> list[str]:
    zones = run(["timedatectl", "list-timezones"]).split()
    if not zones and ZONEINFO.is_dir():
        zones = sorted(
            str(p.relative_to(ZONEINFO)) for p in ZONEINFO.glob("*/**/*")
            if p.is_file() and p.relative_to(ZONEINFO).parts[0] not in ("posix", "right"))
    return ["UTC", *(z for z in zones if z != "UTC")]


def parse_locales(locale_gen: str) -> list[str]:
    """UTF-8 locales /etc/locale.gen can generate."""
    found = []
    for line in locale_gen.splitlines():
        parts = line.lstrip("#").split()
        if len(parts) == 2 and parts[1] == "UTF-8" and "." in parts[0] and "@" not in parts[0]:
            found.append(parts[0])
    return found


def locales() -> list[str]:
    try:
        found = parse_locales(LOCALE_GEN.read_text(errors="replace"))
    except OSError:
        found = []
    return found or ["en_US.UTF-8"]


def keymaps() -> list[str]:
    names = set()
    for pattern in ("*.map.gz", "*.map"):
        for path in KEYMAPS.rglob(pattern):
            if "include" not in path.parts:
                names.add(path.name.removesuffix(".gz").removesuffix(".map"))
    ok = sorted(n for n in names if n[:1].isalnum() and n == n.lower())
    return ok or ["us"]


def parse_xkb_map(model_map: str) -> dict[str, tuple[str, str]]:
    """console keymap -> (XKB layout, XKB variant), from systemd's kbd-model-map."""
    out: dict[str, tuple[str, str]] = {}
    for line in model_map.splitlines():
        parts = line.split()
        if len(parts) >= 4 and not line.startswith("#"):
            variant = "" if parts[3] == "-" else parts[3]
            out.setdefault(parts[0], (parts[1], variant))
    return out


def xkb_for(keymap: str) -> tuple[str, str] | None:
    try:
        table = parse_xkb_map(KBD_MODEL_MAP.read_text())
    except OSError:
        table = {}
    return table.get(keymap)


def iso_flavor() -> str:
    try:
        value = FLAVOR_FILE.read_text().strip()
    except OSError:
        return "online"
    return value if value in ("online", "offline") else "online"


def network_online() -> bool:
    """NetworkManager's connectivity verdict; a default route counts when it has none."""
    state = run(["nmcli", "-t", "-f", "CONNECTIVITY", "general"]).strip()
    if state in ("full", "limited", "portal"):
        return state == "full"
    return bool(run(["ip", "-4", "route", "show", "default"]).strip())


def uefi() -> bool:
    return os.path.isdir("/sys/firmware/efi/efivars")


def current_timezone() -> str:
    try:
        target = os.readlink("/etc/localtime")
    except OSError:
        return "UTC"
    _, sep, zone = target.partition("zoneinfo/")
    return zone if sep else "UTC"

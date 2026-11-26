"""The wizard's choices and the install profile they produce (docs/plans/profile-schema.md).

Pure stdlib, no Qt: unit-tested on its own. The validator (validate-profile.py) stays the
authority; the checks here only exist to point at the right page and field early.
"""
from __future__ import annotations

import json
import re
import tomllib
from dataclasses import asdict, dataclass, field, fields

KERNELS: tuple[tuple[str, str, str], ...] = (
    ("linux-cachyos", "CachyOS", "Tuned for desktop and gaming. Recommended."),
    ("linux-zen", "Zen", "Responsive desktop kernel from Arch. A good second kernel."),
    ("linux", "Arch default", "The stock Arch kernel."),
    ("linux-lts", "Arch LTS", "Long-term support: the conservative fallback."),
    ("linux-cachyos-lts", "CachyOS LTS", "CachyOS tuning on the long-term branch."),
)
BOOTLOADERS: tuple[tuple[str, str, str], ...] = (
    ("limine", "Limine", "Default: boots snapshots and has the cleanest Secure Boot path."),
    ("systemd-boot", "systemd-boot", "Small and simple UEFI boot menu."),
    ("grub", "GRUB", "The classic. Installed with Secure Boot off."),
    ("refind", "rEFInd", "Graphical menu that detects other systems."),
)
DESKTOPS: tuple[tuple[str, str, str], ...] = (
    ("kde", "KDE Plasma", "The Samurai OS desktop: fully set up and tested."),
    ("gnome", "GNOME", "Upstream defaults plus our shell and tools. Needs network."),
    ("xfce", "Xfce", "Upstream defaults plus our shell and tools. Needs network."),
    ("lxqt", "LXQt", "Upstream defaults plus our shell and tools. Needs network."),
    ("none", "No desktop", "Command line only."),
)
SHELLS: tuple[tuple[str, str, str], ...] = (
    ("fish", "fish", "Friendly, with suggestions out of the box."),
    ("bash", "bash", "The standard Linux shell."),
    ("zsh", "zsh", "Highly configurable."),
)
GAMING_ITEMS: tuple[tuple[str, str, str], ...] = (
    ("steam", "Steam", "With 32-bit graphics drivers and Protontricks."),
    ("heroic", "Heroic Games Launcher", "Epic, GOG and Amazon games."),
    ("gamemode", "GameMode", "Performance mode while a game runs."),
    ("mangohud", "MangoHud", "In-game performance overlay, with GOverlay."),
    ("gamescope", "gamescope", "Game compositor for scaling and frame limits."),
    ("proton_ge", "GE-Proton", "Community Proton build for Steam."),
    ("proton_cachyos", "Proton-CachyOS", "CachyOS Proton build."),
    ("wine", "Wine stack", "Run Windows programs, with umu-launcher."),
)
PYTHONS: tuple[str, ...] = ("3.10", "3.11", "3.12", "3.13")
DOCKER_MODES: tuple[tuple[str, str, str], ...] = (
    ("group", "Docker group", "No sudo needed. The docker group is root-equivalent."),
    ("rootless", "Rootless Docker", "Runs as your user. Safer, with a few limitations."),
    ("none", "No Docker", ""),
)
CONDA: tuple[tuple[str, str, str], ...] = (
    ("miniforge", "Miniforge", "conda with the conda-forge channel."),
    ("none", "No conda", ""),
)
FIREWALLS: tuple[tuple[str, str, str], ...] = (
    ("firewalld", "firewalld", "Default: blocks incoming connections."),
    ("ufw", "ufw", "Simple firewall, blocks incoming connections."),
    ("none", "No firewall", "Every listening service is reachable from your network."),
)
PROXIES: tuple[tuple[str, str, str], ...] = (
    ("none", "None", ""),
    ("caddy", "Caddy", "Automatic HTTPS."),
    ("nginx", "nginx", "With certbot."),
    ("traefik", "Traefik", "Container-oriented proxy."),
)
NVIDIA_MODES: tuple[tuple[str, str, str], ...] = (
    ("auto", "Automatic", "Install the NVIDIA driver only when an NVIDIA GPU is present."),
    ("yes", "Install NVIDIA driver", "For a GPU you will add later."),
    ("no", "Do not install", "Use the open Mesa drivers only."),
)

MIN_PASSPHRASE = 8
MIN_PASSWORD = 6
# same shapes validate-profile.py enforces
HOSTNAME_RE = re.compile(r"^[a-zA-Z0-9][a-zA-Z0-9-]{0,62}$")
USERNAME_RE = re.compile(r"^[a-z_][a-z0-9_-]{0,31}$")
RESERVED_USERS = frozenset({"root", "adm", "sudo", "bin", "daemon", "nobody", "alpm", "live"})
PROFILE_PACKAGES = ("samurai-release", "samurai-mirrorlist", "samurai-keyring",
                    "qemu-guest-agent", "networkmanager", "wpa_supplicant")


@dataclass
class Choices:
    """Every value the wizard writes. Defaults are the spec 08 guided defaults."""
    hostname: str = "samurai"
    timezone: str = "UTC"
    locale: str = "en_US.UTF-8"
    keyboard: str = "us"

    disk: str = ""
    encryption: bool = True

    username: str = ""
    shell: str = "fish"

    kernels: list[str] = field(default_factory=lambda: ["linux-cachyos", "linux-zen"])
    default_kernel: str = "linux-cachyos"
    bootloader: str = "limine"

    desktop: str = "kde"
    monitor_widgets: bool = True
    reduced_effects: bool = True

    nvidia: str = "auto"

    gaming_enabled: bool = True
    steam: bool = True
    heroic: bool = True
    gamemode: bool = True
    mangohud: bool = True
    gamescope: bool = True
    proton_ge: bool = True
    proton_cachyos: bool = True
    wine: bool = True

    python: list[str] = field(default_factory=list)
    conda: str = "miniforge"
    docker: str = "group"
    ai_clis: list[str] = field(default_factory=list)
    ollama: bool = False

    browsers: list[str] = field(default_factory=lambda: ["firefox"])
    apps: list[str] = field(default_factory=list)

    firewall: str = "firewalld"
    tailscale: bool = True
    reverse_proxy: str = "none"
    ssh_server: bool = False

    # no page edits these; a loaded profile keeps its own values
    install: list[str] = field(default_factory=lambda: list(PROFILE_PACKAGES))
    arch_mirror: str = "auto"

    def ordered_kernels(self) -> list[str]:
        """Selected kernels, the default boot kernel first."""
        chosen = [k for k, _, _ in KERNELS if k in self.kernels]
        if self.default_kernel in chosen:
            chosen.remove(self.default_kernel)
            chosen.insert(0, self.default_kernel)
        return chosen

    def as_dict(self) -> dict:
        return asdict(self)


def field_names() -> set[str]:
    return {f.name for f in fields(Choices)}


# ------------------------------------------------------------------ TOML output
def _toml_value(value: object) -> str:
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, int):
        return str(value)
    if isinstance(value, str):
        # JSON string escapes are valid TOML basic-string escapes; DEL is the one
        # control character JSON leaves raw and TOML forbids
        return json.dumps(value, ensure_ascii=False).replace("\x7f", "\\u007f")
    if isinstance(value, (list, tuple)):
        return "[" + ", ".join(_toml_value(v) for v in value) + "]"
    raise TypeError(f"unsupported profile value: {type(value).__name__}")


def profile_tables(c: Choices, password_hash: str = "") -> dict[str, dict]:
    desktop = c.desktop != "none"
    user: dict = {"name": c.username, "shell": c.shell, "groups": ["wheel"], "root_locked": True}
    if password_hash:
        user["password_hash"] = password_hash
    tables: dict[str, dict] = {
        "system": {"hostname": c.hostname, "timezone": c.timezone, "locale": c.locale,
                   "keyboard": c.keyboard},
        "disk": {"device": c.disk, "layout": "erase", "filesystem": "btrfs",
                 "encryption": c.encryption, "auto_unlock_keyfile": False},
        "boot": {"kernels": c.ordered_kernels(), "bootloader": c.bootloader},
        "user": user,
        "packages": {"desktop": c.desktop, "browsers": c.browsers if desktop else [],
                     "apps": c.apps, "install": c.install},
    }
    if c.desktop == "kde":
        tables["kde"] = {"theme": "breeze", "monitor_widgets": c.monitor_widgets,
                         "reduced_effects": c.reduced_effects}
    gaming = desktop and c.gaming_enabled
    tables["gaming"] = {"enabled": gaming}
    if gaming:
        tables["gaming"].update({key: getattr(c, key) for key, _, _ in GAMING_ITEMS})
    tables["development"] = {"python": [v for v in PYTHONS if v in c.python], "conda": c.conda,
                             "docker": c.docker, "ai_clis": c.ai_clis, "ollama": c.ollama}
    tables["network"] = {"tailscale": c.tailscale, "reverse_proxy": c.reverse_proxy}
    tables["security"] = {"firewall": c.firewall, "ssh_server": c.ssh_server}
    tables["hardware"] = {"nvidia": c.nvidia}
    tables["repos"] = {"samurai": True, "cachyos": True, "chaotic_aur": True,
                       "arch_mirror": c.arch_mirror}
    return tables


def to_toml(c: Choices, password_hash: str = "") -> str:
    lines = ["# Samurai OS install profile (written by the graphical installer)", "version = 1"]
    for table, values in profile_tables(c, password_hash).items():
        lines.append("")
        lines.append(f"[{table}]")
        lines.extend(f"{key} = {_toml_value(value)}" for key, value in values.items())
    return "\n".join(lines) + "\n"


# ------------------------------------------------------------------ profile loading
def from_toml(text: str, base: Choices | None = None) -> Choices:
    """Choices from a saved profile. The caller validates the file first; the disk is
    never taken from a file (the user picks and confirms it on this machine)."""
    p = tomllib.loads(text)
    c = Choices(**(base or Choices()).as_dict())

    def table(name: str) -> dict:
        value = p.get(name, {})
        return value if isinstance(value, dict) else {}

    def take(src: dict, key: str, attr: str) -> None:
        value = src.get(key)
        if value is not None and type(value) is type(getattr(c, attr)):
            setattr(c, attr, value)

    system, disk, boot, user = table("system"), table("disk"), table("boot"), table("user")
    for key in ("hostname", "timezone", "locale", "keyboard"):
        take(system, key, key)
    take(disk, "encryption", "encryption")
    take(boot, "kernels", "kernels")
    take(boot, "bootloader", "bootloader")
    if c.kernels:
        c.default_kernel = c.kernels[0]
    take(user, "name", "username")
    take(user, "shell", "shell")
    packages = table("packages")
    take(packages, "desktop", "desktop")
    c.install = list(packages.get("install", []))
    c.arch_mirror = table("repos").get("arch_mirror", "auto")
    c.browsers = list(packages.get("browsers", ["firefox"] if c.desktop != "none" else []))
    c.apps = list(packages.get("apps", []))
    kde = table("kde")
    take(kde, "monitor_widgets", "monitor_widgets")
    take(kde, "reduced_effects", "reduced_effects")
    take(table("hardware"), "nvidia", "nvidia")
    # an absent table means "do not install" (schema rule 1)
    gaming = table("gaming")
    c.gaming_enabled = bool(gaming.get("enabled", False))
    for key, _, _ in GAMING_ITEMS:
        setattr(c, key, bool(gaming.get(key, True)))
    dev = table("development")
    c.python = list(dev.get("python", []))
    c.conda = dev.get("conda", "none")
    c.docker = dev.get("docker", "none")
    c.ai_clis = list(dev.get("ai_clis", []))
    c.ollama = bool(dev.get("ollama", False))
    network, security = table("network"), table("security")
    c.tailscale = bool(network.get("tailscale", False))
    c.reverse_proxy = network.get("reverse_proxy", "none")
    c.firewall = security.get("firewall", "none")
    c.ssh_server = bool(security.get("ssh_server", False))
    return c


# ------------------------------------------------------------------ page checks
def hostname_error(value: str) -> str:
    if not value:
        return "Enter a computer name."
    if not HOSTNAME_RE.match(value):
        return "Computer name: letters, digits and hyphens, starting with a letter or digit."
    return ""


def username_error(value: str) -> str:
    if not value:
        return "Enter a user name."
    if not USERNAME_RE.match(value):
        return "User name: lowercase letters, digits, - or _, starting with a letter or _."
    if value in RESERVED_USERS:
        return f"“{value}” is a reserved name."
    return ""


def secret_error(value: str, repeat: str, what: str, minimum: int) -> str:
    if len(value) < minimum:
        return f"{what} needs at least {minimum} characters."
    if any(ch in value for ch in "\r\n\0"):
        return f"{what} must be a single line."
    if value != repeat:
        return f"The two {what.lower()} entries do not match."
    return ""


def kernel_error(c: Choices) -> str:
    return "" if c.ordered_kernels() else "Pick at least one kernel."

#!/usr/bin/env python3
"""Writes docs/plans/defaults-inventory.md: where every spec 10/13/14 item and
every catalog/samurai.toml entry comes from, resolved against a pacman index.

  scripts/defaults-inventory.py INDEX.tsv GROUPS.txt > docs/plans/defaults-inventory.md

INDEX.tsv / GROUPS.txt come from an archlinux container with core, extra,
multilib, cachyos and chaotic-aur synced:
  pacman -S expac; expac -S '%r\\t%n\\t%v\\t%S' > INDEX.tsv; pacman -Sgg > GROUPS.txt
Item statuses are maintained in ITEMS below.
"""
from __future__ import annotations

import datetime
import pathlib
import sys
import tomllib

REPO = pathlib.Path(__file__).resolve().parents[1]
ORDER = ["core", "extra", "multilib", "cachyos", "chaotic-aur"]

# (spec, item, packages, delivered by, status)
# status: done = installed/configured by this branch; optional = installable by
# choice; gap = not delivered (reason in the "delivered by" column)
ITEMS = [
    ("10 Base", "CachyOS + Zen kernels (options linux, lts, cachyos-lts)",
     "linux-cachyos linux-zen linux linux-lts linux-cachyos-lts", "profile [boot] kernels", "done"),
    ("10 Base", "systemd initramfs hooks, Limine", "mkinitcpio limine", "installer", "done"),
    ("10 Base", "Plymouth with theme colours", "plymouth", "needs samurai-branding theme and a boot test", "gap"),
    ("10 Base", "btrfs + zstd, LUKS2", "btrfs-progs cryptsetup", "installer", "done"),
    ("10 Base", "zram = RAM/2, zstd; systemd-oomd", "zram-generator", "samurai-settings-system", "done"),
    ("10 Base", "PipeWire, WirePlumber, pulse, jack; realtime group",
     "pipewire wireplumber pipewire-pulse pipewire-jack pipewire-alsa realtime-privileges",
     "samurai-desktop-base + user group", "done"),
    ("10 Base", "NetworkManager, systemd-resolved, firewalld", "networkmanager firewalld",
     "samurai-base + profile [security]", "done"),
    ("10 Base", "Tailscale", "tailscale", "profile [network] tailscale (default on)", "done"),
    ("10 Base", "Bluetooth + Plasma applet", "bluez bluez-utils bluedevil", "samurai-desktop-base, plasma-meta", "done"),
    ("10 Base", "fish login shell and Konsole default; bash; zsh optional; starship",
     "fish bash zsh starship", "samurai-settings-shell (Konsole follows the login shell)", "done"),
    ("10 Base", "yay (paru optional)", "yay paru", "samurai-base (paru: install later)", "done"),
    ("10 Base", "Docker + Compose + buildx", "docker docker-compose docker-buildx",
     "profile [development] docker", "done"),
    ("10 Base", "Python 3.14 system, uv, Miniforge", "python uv samurai-miniforge",
     "samurai-settings-dev, profile [development]", "done"),
    ("10 Base", "CLI tools", "git github-cli base-devel micro nano btop nvtop fastfetch eza bat ripgrep fd fzf "
     "zoxide tldr rsync wget curl unzip 7zip man-db", "samurai-base (nvtop: samurai-desktop-base)", "done"),
    ("10 Base", "Fonts", "noto-fonts noto-fonts-cjk noto-fonts-emoji ttf-jetbrains-mono-nerd ttf-liberation",
     "samurai-desktop-base", "done"),
    ("10 Base", "Firmware", "linux-firmware amd-ucode intel-ucode sof-firmware fwupd", "samurai-base", "done"),
    ("10 Desktop", "KDE apps", "dolphin konsole kate ark spectacle gwenview okular kcalc partitionmanager "
     "kdeconnect haruna elisa discover", "samurai-desktop-kde", "done"),
    ("10 Desktop", "Discover for Flatpak only", "flatpak discover", "no packagekit-qt6 is installed", "done"),
    ("10 Desktop", "Firefox default, every browser selectable", "firefox", "profile [packages] browsers", "done"),
    ("10 Desktop", "Firefox policy from spec 09", "firefox", "spec 09 hardening not in this lane", "gap"),
    ("10 Performance", "ananicy-cpp + CachyOS rules, irqbalance, power-profiles-daemon",
     "ananicy-cpp cachyos-ananicy-rules irqbalance power-profiles-daemon", "samurai-settings-system", "done"),
    ("10 Performance", "I/O schedulers, sysctl, THP madvise, journald 500M, 15s stop timeout", "",
     "samurai-settings-system", "done"),
    ("10 Performance", "scx-scheds installed, sched_ext off", "scx-scheds", "samurai-settings-system", "done"),
    ("10 Gaming", "Steam, Heroic, GameMode, MangoHud, GOverlay, gamescope",
     "steam heroic-games-launcher-bin gamemode lib32-gamemode mangohud lib32-mangohud goverlay gamescope",
     "profile [gaming] (default on)", "done"),
    ("10 Gaming", "GE-Proton system-wide", "samurai-proton-ge", "new [samurai] package, pinned release", "done"),
    ("10 Gaming", "Proton-CachyOS, umu-launcher, protontricks", "proton-cachyos-slr umu-launcher protontricks",
     "profile [gaming]", "done"),
    ("10 Gaming", "samurai-wine; .exe opens with Wine", "samurai-wine wine-staging winetricks wine-mono wine-gecko",
     "new meta; handler is wine-staging's wine.desktop; Arch Wine is WoW64, no lib32 deps", "done"),
    ("10 Gaming", "lib32 GPU drivers", "lib32-mesa lib32-vulkan-radeon lib32-vulkan-intel lib32-nvidia-utils",
     "with Steam (planner)", "done"),
    ("10 Gaming", "Heroic picks up system-wide Proton (M7 check)", "", "not verified: needs a Heroic account/UI", "gap"),
    ("10 Gaming", "Bottles, Lutris as toolbox options", "bottles lutris", "samurai apps", "optional"),
    ("13 Python", "3.10-3.13 checkboxes", "samurai-python310 samurai-python311 samurai-python312 samurai-python313",
     "new [samurai] packages (python-build-standalone)", "optional"),
    ("13 Python", "CI bump of new patch releases; toolbox EOL notice for 3.10", "", "not built", "gap"),
    ("13 Python", "uv config, PEP 668 hint, ~/.local/bin on PATH", "uv", "samurai-settings-dev / -shell", "done"),
    ("13 Conda", "Miniforge read-only in /opt, no base auto-activation, shell hooks", "samurai-miniforge",
     "new [samurai] package + samurai-settings-dev", "done"),
    ("13 Conda", "Miniconda", "samurai-miniconda", "not packaged: Anaconda licence/redistribution (owner decision)", "gap"),
    ("13 Docker", "group or rootless mode", "docker-rootless-extras rootlesskit slirp4netns fuse-overlayfs",
     "profile [development] docker (rootless extras from chaotic-aur)", "done"),
    ("13 Docker", "NVIDIA container toolkit", "nvidia-container-toolkit",
     "NVIDIA + docker; configured for the rootful daemon only; no GPU test", "done"),
    ("13 AI", "Claude Code, Copilot CLI", "claude-code github-copilot-cli", "profile [development] ai_clis", "optional"),
    ("13 AI", "Codex, Gemini CLI, OpenCode", "openai-codex gemini-cli opencode",
     "profile ai_clis (now in Arch extra: no [samurai] rebuild)", "optional"),
    ("13 AI", "Aider, Cursor CLI", "", "per-user installers: first-boot note (samurai apps install)", "optional"),
    ("13 AI", "welcome app lists login commands", "", "welcome app not built", "gap"),
    ("13 Local AI", "Ollama + cuda/rocm build", "ollama ollama-cuda ollama-rocm", "profile [development] ollama", "optional"),
    ("13 Toolchains", "development category", "nodejs npm pnpm rustup go jdk-openjdk dotnet-sdk git-lfs",
     "samurai apps / pacman", "optional"),
    ("14 Theme", "Breeze Dark default", "breeze", "samurai-settings-kde", "done"),
    ("14 Theme", "Otto, Scratchy", "samurai-theme-otto samurai-theme-scratchy",
     "KDE Store items 1360125 / 1898344 declare no licence (API 2026-11-26): not packaged, refused by the validator", "gap"),
    ("14 Theme", "SDDM / Plymouth / Limine themes matched to theme", "", "samurai-branding not built (SDDM uses Breeze)", "gap"),
    ("14 Layout", "top panel + full-width task bar with names, no grouping", "",
     "samurai-plasma-layout (first login, from /etc/skel)", "done"),
    ("14 Widgets", "CPU/GPU monitor widgets (installer toggle)", "plasma-systemmonitor ksystemstats",
     "samurai-plasma-layout; multi-socket shows one CPU widget (spec fallback with per-socket temperature not built)", "done"),
    ("14 Widgets", "toolbox rebuild / two rows / GPU-change offer", "",
     "CLI: samurai-plasma-layout; toolbox pages not built", "gap"),
    ("14 Effects", "reduced effects (toggle), Konsole profile, fonts, Baloo names only, Wayland SDDM", "",
     "samurai-kde-defaults", "done"),
    ("07 (page 7)", "CUDA toggle, gpu-unlock builder, chwd profiles", "cuda cudnn chwd samurai-gpu-unlock",
     "milestone M6, outside this lane", "gap"),
    ("ADR-0007", "Chaotic-AUR enabled, signed, key pinned", "chaotic-keyring chaotic-mirrorlist",
     "samurai-base, pacman.conf.target, postinstall check", "done"),
]


def load_index(index_file: str, groups_file: str):
    index: dict[str, tuple[str, str]] = {}
    for line in pathlib.Path(index_file).read_text().splitlines():
        repo, name, version = line.split("\t")[:3]
        if name not in index or ORDER.index(repo) < ORDER.index(index[name][0]):
            index[name] = (repo, version)
    groups = {line.split()[0] for line in pathlib.Path(groups_file).read_text().splitlines() if line.strip()}
    return index, groups


def main() -> int:
    index, groups = load_index(*sys.argv[1:3])
    local = {p.parent.name: next(l.split("=", 1)[1] for l in p.read_text().splitlines() if l.startswith("pkgver="))
             for p in (REPO / "pkgbuilds").glob("*/PKGBUILD")}

    def resolve(name: str) -> str:
        if name in local:
            return f"{name} `samurai` (pkgbuilds, {local[name]})"
        if name in index:
            return f"{name} `{index[name][0]}`"
        if name in groups:
            return f"{name} `group`"
        return f"**{name}: not found**"

    samurai = tomllib.loads((REPO / "catalog" / "samurai.toml").read_text())
    garuda = tomllib.loads((REPO / "catalog" / "garuda-setup-wizard.toml").read_text())
    print("# Defaults inventory\n")
    print(f"Generated by `scripts/defaults-inventory.py` on {datetime.date.today()} against live "
          "core, extra, multilib, cachyos and chaotic-aur. Repository shown = the one pacman picks "
          "(order above). `samurai` packages are built from `pkgbuilds/` and are **not published yet**.\n")
    counts = {}
    for item in ITEMS:
        counts[item[4]] = counts.get(item[4], 0) + 1
    print("Status: " + ", ".join(f"{k} {v}" for k, v in sorted(counts.items())) +
          ". `done` = installed/configured by default or by its default-on choice; "
          "`optional` = selectable at install or later; `gap` = not delivered.\n")
    print("## Spec items\n\n| Spec | Item | Packages (source) | Delivered by / reason | Status |")
    print("|---|---|---|---|---|")
    for spec, item, packages, how, status in ITEMS:
        resolved = "<br>".join(resolve(p) for p in packages.split()) or "—"
        print(f"| {spec} | {item} | {resolved} | {how} | **{status}** |")

    print("\n## catalog/samurai.toml\n\n### Toggles and choices\n\n| Entry | Packages (source) |\n|---|---|")
    for toggle in samurai.get("toggle", []):
        print(f"| toggle `{toggle['id']}` (default {'on' if toggle.get('default') else 'off'}) | "
              f"{', '.join(resolve(p) for p in toggle.get('packages', [])) or '—'} |")
    for choice in samurai.get("choice", []):
        for option in choice.get("options", []):
            names = list(option.get("packages", [])) + ([option["meta"]] if option.get("meta") else [])
            if names:
                print(f"| choice `{choice['id']}` = `{option['id']}` | {', '.join(map(resolve, names))} |")
    apps = {a["id"]: dict(a) for a in garuda["app"]}
    apps.update({a["id"]: dict(a) for a in samurai.get("app", [])})
    touched = [a["id"] for a in samurai.get("app", [])]
    for override in samurai.get("override", []):
        if "id" in override:
            apps[override["id"]].update(override)
            touched.append(override["id"])
        else:
            for app in apps.values():
                if app["category"] == override["category"]:
                    app.update({k: v for k, v in override.items() if k != "category"})
    print("\n### Apps added or overridden by samurai.toml\n\n| App | Packages (source) | Offered |\n|---|---|---|")
    for app_id in dict.fromkeys(touched):
        app = apps[app_id]
        packages = ", ".join(map(resolve, app.get("packages", [])))
        if app.get("flatpak"):
            packages = f"{app['flatpak']} `flathub` (not checked here)"
        if not packages:
            packages = "per-user installer: `" + "; ".join(app.get("user_commands", [])) + "`"
        offered = "hidden: " + app.get("note", "") if app.get("hidden") else "yes"
        print(f"| `{app_id}` | {packages} | {offered} |")

    broken = [(a, [p for p in a.get("packages", []) if p not in index and p not in groups and p not in local])
              for a in apps.values()]
    broken = [(a, missing) for a, missing in broken if missing]
    visible = [a for a, _ in broken if not a.get("hidden")]
    print(f"\n## Whole catalog (Garuda + samurai)\n\n{len(apps)} apps; {len(apps) - len(broken)} resolve fully. "
          f"Unresolved: {len(broken)}, of which offered to users: {len(visible)}.\n")
    print("| App | Missing | Offered |\n|---|---|---|")
    for app, missing in broken:
        print(f"| `{app['id']}` | {', '.join(missing)} | {'no (hidden)' if app.get('hidden') else 'shown as not available'} |")
    return 0


if __name__ == "__main__":
    sys.exit(main())

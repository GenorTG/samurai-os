# 10 — Default software, desktop, performance and gaming

Everything here is a default, shipped as meta- and settings packages (spec 01) and chosen through the
install catalog (spec 08, `catalog/`). Each entry can be removed without breaking the system.

## Base
| Area | Default |
|------|---------|
| Kernels | **`linux-cachyos`** (default boot entry) + **`linux-zen`** (second entry, also the fallback). Options: `linux`, `linux-lts`, `linux-cachyos-lts`. All from their official repos (Arch, CachyOS). We never build kernels (ADR-0001) |
| Init / boot | systemd, systemd-based mkinitcpio hooks, Limine (ADR-0003), Plymouth with the theme's colours |
| Filesystem | btrfs + zstd, layout in spec 06; LUKS2 default on |
| Memory | zram (`zram-generator`: size = RAM/2, zstd), no disk swap; systemd-oomd on |
| Audio | PipeWire + WirePlumber, `pipewire-pulse`, `pipewire-jack`, realtime group for the user |
| Network | NetworkManager (iwd as a toolbox toggle), systemd-resolved, firewalld, **Tailscale** (spec 09) |
| Bluetooth | bluez, enabled; Plasma applet |
| Shells | **fish is the user's login shell** and Konsole's default; **bash** always installed (root's shell, scripts, `/bin/sh` stays bash); zsh optional. starship prompt, configs in `samurai-settings-shell` |
| AUR helper | **yay** (Chaotic-AUR), paru optional (spec 02 AUR policy) |
| Containers | **Docker + Compose + buildx** on (spec 13) |
| Python | 3.14 system + 3.10–3.13 + **uv**; Miniforge (spec 13) |
| CLI tools | `git`, `github-cli`, `base-devel`, `micro` + `nano`, `btop`, `nvtop` (GPU machines), `fastfetch`, `eza`, `bat`, `ripgrep`, `fd`, `fzf`, `zoxide`, `tldr`, `rsync`, `wget`, `curl`, `unzip`/`7zip`, `man-db` |
| Fonts | `noto-fonts`, `noto-fonts-cjk`, `noto-fonts-emoji`, `ttf-jetbrains-mono-nerd`, `ttf-liberation` |
| Firmware | `linux-firmware`, CPU microcode, `sof-firmware`, `fwupd` |

## Desktop
KDE Plasma (Wayland) is the default and the tested desktop. Its setup (Otto/Scratchy theme, top
panel + bottom task bar, monitor widgets, reduced effects) is in spec 14. Other desktops are
installer choices with upstream defaults (spec 08 page 6).

KDE apps: Dolphin, Konsole, Kate, Ark, Spectacle, Gwenview, Okular, KCalc, Partition Manager, KDE
Connect (firewall closed until opened), Haruna (video), Elisa (music), Discover for Flatpak only.
Browser: **Firefox** ticked by default with the policy from spec 09; every browser from Garuda's list
can be ticked instead or as well.

## Performance defaults (CachyOS/Garuda ideas, conservative values)
| Item | Default | Why |
|------|---------|-----|
| `ananicy-cpp` + CachyOS rules | on | foreground apps get CPU priority |
| `irqbalance` | on | |
| `power-profiles-daemon` | on | Plasma integrates it; TLP/auto-cpufreq are mutually exclusive toolbox alternatives |
| I/O scheduler udev rules | `none` NVMe, `mq-deadline` SATA SSD, `bfq` HDD | CachyOS settings |
| sysctl | `vm.swappiness=100` (zram), `vm.vfs_cache_pressure=50`, desktop-sized `vm.dirty_*bytes`, `vm.max_map_count=2147483642` (some games need it; SteamOS value) | |
| THP | `madvise` | |
| Scheduler | `linux-cachyos` ships BORE; `scx-scheds` installed, sched_ext **off** by default, toolbox lets you pick one (e.g. `scx_lavd`) | big effect, still young |
| journald | `SystemMaxUse=500M` | |
| Shutdown | `DefaultTimeoutStopSec=15s` | |

## Gaming (installer page 8, default on)
| Component | Package | Source |
|-----------|---------|--------|
| Steam (native) | `steam` | Arch multilib |
| Heroic Games Launcher (Epic, GOG, Amazon) | `heroic-games-launcher-bin` | Chaotic-AUR |
| Feral GameMode | `gamemode`, `lib32-gamemode` | Arch |
| MangoHud (+ GOverlay to configure it) | `mangohud`, `lib32-mangohud`, `goverlay` | Arch |
| gamescope | `gamescope` | Arch |
| GE-Proton, latest | `samurai-proton-ge` → `/usr/share/steam/compatibilitytools.d/` | `[samurai]`, auto-bumped from GE's GitHub releases |
| Proton-CachyOS | `proton-cachyos` | CachyOS repo |
| umu-launcher, protontricks | `umu-launcher`, `protontricks` | Arch |
| Wine stack | `samurai-wine` meta: `wine-staging`, `winetricks`, `wine-mono`, `wine-gecko`, plus Wine's lib32 optional deps for sound, fonts, GnuTLS, GStreamer codecs, Vulkan, OpenCL | Arch + multilib |
| lib32 GPU drivers | from the chwd profile (spec 07) | Arch |

- GE-Proton and Proton-CachyOS are installed system-wide, so Steam lists them under compatibility
  tools without a download. Heroic gets them as Wine/Proton choices too (it reads Steam's
  compatibility-tool folders; confirm the system-wide path is picked up in the M7 VM check, otherwise
  `samurai-settings-gaming` adds a Heroic config that points at it).
- "Run Windows apps normally": `.exe` files open with Wine through a `.desktop` handler; winetricks
  and protontricks are installed; Bottles and Lutris are toolbox options. The Wine meta sticks to
  Wine's documented optional dependencies, so there is no extra launcher bloat.
- `vm.max_map_count` and the `standard` security profile keep Linux-supported anti-cheat titles working.

## Application catalog
Garuda's setup-wizard list (16 categories, 175 apps) plus our categories (Craft suite, AI CLIs, local
AI) and defaults, in `catalog/`. The installer, setup assistant and toolbox all use it (spec 08).

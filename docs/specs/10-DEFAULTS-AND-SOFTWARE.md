# 10 — Default software, desktop, performance and gaming

Everything here is a default, shipped as meta- and settings packages (spec 01). Each entry can be
removed without breaking the system. Items marked **(OQ)** wait on Genor (OPEN-QUESTIONS).

## Base
| Area | Default |
|------|---------|
| Kernels | `linux-zen` (main) + `linux-lts` (fallback). Installer alternatives: `linux`, or `linux-cachyos` (turns on the CachyOS repo below Arch) **(OQ1)** |
| Init / boot | systemd, systemd-based mkinitcpio hooks, Limine (ADR-0003), Plymouth with our theme |
| Filesystem | btrfs + zstd, layout in spec 06; LUKS2 default on |
| Memory | zram (`zram-generator`: size = RAM/2, zstd), no disk swap; systemd-oomd on |
| Audio | PipeWire + WirePlumber, `pipewire-pulse`, `pipewire-jack`, realtime group for the user |
| Network | NetworkManager (wpa_supplicant; iwd as a toolbox toggle), systemd-resolved, firewalld |
| Bluetooth | bluez, enabled; Plasma applet |
| Shell | **fish** as the login shell for the installer user (Garuda default; Genor uses fish), bash stays root's shell and the scripting shell; starship prompt; configs in `samurai-settings-shell` |
| CLI tools | `git`, `base-devel`, `paru`, `micro` + `nano`, `htop`/`btop`, `fastfetch`, `eza`, `bat`, `ripgrep`, `fd`, `fzf`, `zoxide`, `tldr`, `rsync`, `wget`, `curl`, `unzip`/`7zip`, `man-db` |
| Fonts | `noto-fonts`, `noto-fonts-cjk`, `noto-fonts-emoji`, `ttf-jetbrains-mono-nerd`, `ttf-liberation` (metric-compatible Office fonts) |
| Firmware | `linux-firmware`, CPU microcode, `sof-firmware`, `fwupd` |

## Desktop: KDE Plasma (Wayland)
- Plasma 6, Wayland session by default (X11 session not installed), SDDM with our theme.
- Apps: Dolphin, Konsole, Kate, Ark, Spectacle, Gwenview, Okular, KCalc, Partition Manager,
  KDE Connect (firewall closed until enabled), Haruna (mpv) for video, Elisa for music,
  Discover **for Flatpak only** (no PackageKit, spec 04).
- Browser: Firefox with the policy from spec 09 **(OQ9)**.
- Look: Breeze Dark with our accent colour, our wallpapers, icons and SDDM/Plymouth/Limine themes in
  `samurai-branding`. Deliberately less loud than Garuda's dr460nized theme **(OQ4)**.
- Defaults that Garuda's toolbox exposes as toggles and we pre-set: borderless maximized windows off,
  audio node suspend left on (toggle exists), Discord auto-update off when Discord is installed.

## Performance defaults (CachyOS/Garuda ideas, conservative values)
| Item | Default | Why |
|------|---------|-----|
| `ananicy-cpp` + rules | on | foreground apps get CPU priority (Garuda, CachyOS) |
| `irqbalance` | on | |
| `power-profiles-daemon` | on (desktop + laptop) | Plasma integrates it; TLP/auto-cpufreq are toolbox alternatives, mutually exclusive |
| I/O scheduler udev rules | `none` NVMe, `mq-deadline` SATA SSD, `bfq` HDD | CachyOS settings |
| sysctl | `vm.swappiness=100` (zram), `vm.vfs_cache_pressure=50`, `vm.dirty_bytes`/`dirty_background_bytes` sized for desktops, `vm.max_map_count=2147483642` (some games need it; Arch/SteamOS value) | |
| THP | `madvise` | |
| sched_ext | `scx-scheds` installed, **off** by default; toolbox lets you pick a scheduler (e.g. `scx_lavd` for gaming) on kernels that support it | big effect, still young; opt-in |
| profile-sync-daemon | off, toolbox toggle | |
| journald | `SystemMaxUse=500M` | |
| Faster shutdown | `DefaultTimeoutStopSec=15s` | |

## Gaming (`samurai-gaming`, offered in the installer, default **yes** on machines with a dGPU)
Steam, multilib enabled, lib32 GPU drivers via the chwd profile, gamemode, gamescope, MangoHud,
ProtonUp-Qt, protontricks, Wine (staging) + winetricks, `vkd3d`/DXVK through Proton. Optional via the
toolbox: Lutris, Heroic, Bottles, `proton-cachyos` (needs the CachyOS repo). `vm.max_map_count` above
and the default security profile are chosen so anti-cheat titles that support Linux keep working.

## Setup assistant catalog
`/usr/lib/samurai/catalog.toml` lists optional software by group: office (LibreOffice, OnlyOffice),
creative (GIMP, Krita, Inkscape, Blender, Kdenlive, OBS), dev (VS Code OSS / VSCodium, Docker or Podman,
language toolchains), communication (Discord, Signal, Telegram), printing/scanning, virtualization,
containers, gaming extras. Each entry: Arch package or Flatpak id, optional service to enable, optional
group to join. The installer's package chooser, the setup assistant and the toolbox's components page
all read this one file (spec 04).

# 04 — Toolbox

The toolbox is Garuda's feature set rebuilt on our CLI-first architecture (spec 01). The inventory
below comes from Garuda's current toolbox source (`gitlab.com/garuda-linux/applications/toolbox`,
Electron/Angular, inspected 2026-10-27) plus `garuda-update`, `garuda-health`,
`garuda-system-maintenance` and `snapper-tools`. "Ours" rows are additions.

Every row is a CLI command first. The GUI page calls the same helper method. **Done** for a row
means: CLI command, helper method with polkit action, GUI control, VM test.

## Parity table
### Home, status and notifications
| Feature | Garuda | Ours | CLI |
|---------|--------|------|-----|
| Welcome / first-run page: links, release notes, "what to do next" | Welcome, setup wizard | yes | `samurai welcome` (GUI) |
| Setup assistant: pick extra software by category after install | `garuda-setup-assistant` | yes, driven by a package catalog file (spec 10) | `samurai setup` |
| System status: pending updates, reboot pending, `.pacnew` files, health warnings, not updated in 14 days | System status | yes | `samurai status` |
| Tray notifier for the above | `garuda-system-maintenance` | `samurai-notifier` (spec 03) | — |
| Pending operations queue: stack changes, apply in one go | operation manager | yes (task queue, spec 01) | `samurai queue list/apply/clear` |

### Maintenance
| Feature | Garuda | Ours | CLI |
|---------|--------|------|-----|
| Update system | `garuda-update` | `samurai update` (spec 03) | `samurai update` |
| Refresh Arch mirrorlist | yes | yes | `samurai mirrors refresh` |
| Refresh third-party mirrorlists (Chaotic, CachyOS) | Chaotic only | CachyOS + Chaotic-AUR (both on by default, ADR-0007) | `samurai mirrors refresh --all` |
| Refresh / reset keyring | yes ("update keyring") | yes: refresh, or full reset (`pacman-key --init` + populate) | `samurai fix keyring [--reset]` |
| Remove pacman lock | yes | yes, only if no pacman process holds it | `samurai fix lock` |
| Clean package cache | yes | `paccache` keep N, uninstalled, or all | `samurai clean pkgcache` |
| Clear orphans | yes | yes, shows the list, never automatic | `samurai clean orphans` |
| Merge `.pacnew`/`.pacsave` | pacdiff + Meld | yes, snapshot first (Garuda does too) | `samurai pacdiff` |
| Reinstall all packages | yes | yes, snapshot first | `samurai fix reinstall-all` |
| Reinstall / rebuild DKMS modules | yes | yes | `samurai fix dkms` |
| Reset `pacman.conf`, mirrorlists and keyring to distro defaults ("update fix") | yes | yes | `samurai fix pacman` |
| Full fix: the above + reinstall everything | yes | yes | `samurai fix pacman --full` |
| Reset audio (PipeWire configs + reinstall) | yes | yes | `samurai fix audio` |
| Reset snapper configs | yes | yes | `samurai snap repair` |
| Reset app/user configs to defaults (bash, fish, zsh, KDE, GTK, Kvantum, mpv, ...; backs up first) | yes (≈20 targets) | yes, list defined in `/usr/lib/samurai/resettable.toml` | `samurai reset-config <target>` |
| Edit repositories | Pace (GUI) | our own Repos page: toggle testing, CachyOS, Chaotic-AUR, multilib, with warnings | `samurai repo list/enable/disable` |
| Btrfs maintenance (scrub, balance, trim) | btrfs-assistant | btrfs-assistant + timers enabled by default (spec 06) | `samurai btrfs scrub/balance` |
| **System cleaner**: journal vacuum, coredumps, `~/.cache`, thumbnails, unused Flatpak runtimes, old snapshots, optional `docker system prune` | partial (BleachBit) | **ours**, with a preview of how much each frees up | `samurai clean all --preview` |

### Health and diagnostics (details in spec 05)
| Feature | Garuda | Ours | CLI |
|---------|--------|------|-----|
| Health check | `garuda-health` | `samurai health` | `samurai health` |
| Automatic repair of health issues | "repair system health" | yes, per check | `samurai health --fix [<check>]` |
| Common info (inxi) | yes | yes | `samurai diag info` |
| System services / boot timing | yes | yes | `samurai diag services` |
| Journal, kernel log, pacman log | yes | yes, with an error-only filter | `samurai diag logs [--kernel/--pacman]` |
| Full diagnostics bundle | yes | yes, redacted (spec 05) | `samurai diag bundle` |
| Upload to a pastebin | PrivateBin | opt-in, redacted, our own paste host | `samurai diag bundle --upload` |
| **Disk health** (SMART, NVMe wear, btrfs device errors) | in garuda-health | dedicated page | `samurai diag disks` |
| **Firmware updates** (fwupd/LVFS) | no | yes | `samurai firmware check/update` |

### Kernels, boot and recovery
| Feature | Garuda | Ours | CLI |
|---------|--------|------|-----|
| Kernel manager: install/remove, headers, missing/broken DKMS warnings, set default | yes | yes; kernels from CachyOS (default `linux-cachyos`) and Arch (`linux-zen` and others) | `samurai kernel list/install/remove/default` |
| Boot options: timeout, default entry, saved default, kernel parameters, quiet level, Plymouth on/off and theme, os-prober, SysRq, mitigations off | yes (GRUB) | yes, for our bootloader (ADR-0003). "Mitigations off" stays, behind a security warning | `samurai boot get/set` |
| Boot repair: reinstall bootloader, regenerate config, back up / restore MBR/PBR, open LUKS, chroot | yes | yes, works from the live ISO (spec 06) | `samurai rescue` |
| Snapshot manager: list, compare, restore, delete, settings | snapper-tools + btrfs-assistant | yes (spec 06) | `samurai snap ...` |

### Hardware and drivers (spec 07)
| Feature | Garuda | Ours | CLI |
|---------|--------|------|-----|
| Driver profiles: detect, install, reinstall after a GPU swap | mhwd/chwd in settings | yes, chwd + our profiles | `samurai hw detect/apply/list` |
| Network assistant: status, hardware, loaded/blacklisted drivers, load/unload/blacklist, ping/traceroute, Wi-Fi/BT/airplane toggles, hotspot | yes | yes | `samurai net ...` |

### System settings
| Feature | Garuda | Ours | CLI |
|---------|--------|------|-----|
| System components (install + enable service + add user to group): audio (PipeWire, JACK, EasyEffects, plugins), Bluetooth, printing/scanning (CUPS, SANE), virtualization (KVM, QEMU, libvirt, virt-manager, VirtualBox), containers (Docker, Podman, Distrobox, BoxBuddy, WinBoat), firewall (firewalld, ufw, OpenSnitch), reverse proxy (Caddy default, nginx, traefik; `samurai proxy` pages), ModemManager | yes | yes, from a component catalog (`/usr/lib/samurai/components.toml`) so new entries need no code — anything the installer offers is also installable later (Genor 2026-11-17) | `samurai component list/enable/disable` |
| Systemd services: list, start/stop/enable/disable, failed units | yes | yes | `samurai service ...` |
| Users and groups | yes | yes (thin; KDE has its own) | `samurai user ...` |
| Locales, language packs, keyboard | yes | yes | `samurai locale ...` |
| Common tweaks: profile-sync-daemon, systemd-oomd, guest user | yes | yes | `samurai tweak ...` |
| Performance: performance sysctl set, ananicy-cpp, bpftune, irqbalance, preload | yes | yes; defaults in spec 10 | `samurai tweak perf ...` |
| Power saving: power-profiles-daemon, TLP, auto-cpufreq, thermald, intel-undervolt, powersave sysctls (mutually exclusive ones are enforced) | yes | yes | `samurai tweak power ...` |
| Config toggles: audio no-suspend, Discord auto-update off, KDE borderless maximize, touchpad right-click emulation | yes | yes | `samurai tweak config ...` |
| Login shell + default shell configs | yes | yes | `samurai shell set fish` |
| DNS provider | yes | yes, writes systemd-resolved/NM config (encrypted DNS option, spec 09) | `samurai dns set <provider>` |
| hblock (hosts blocklist) | yes | yes, opt-in | `samurai tweak hblock on` |
| iwd instead of wpa_supplicant | yes | yes | `samurai tweak iwd on` |
| **Security page**: firewall state, AppArmor, Secure Boot, LUKS, TPM unlock, CVE packages, SSH exposure | no | **ours** (spec 09) | `samurai security status` |
| **Firewall page**: switch firewalld/ufw, zones per network, open/close ports and one-click service presets, Docker-published ports, live listening + connection view, per-interface traffic (vnstat), stale-hole warnings | no (component toggle only) | **ours** (spec 09) | `samurai firewall ...` |
| **Reverse proxy page**: site list/add/disable, engine choice (Caddy/nginx/traefik), config test + reload, cert status | no | **ours** (spec 09), installable at install or later | `samurai proxy ...` |
| **Tailscale page**: on/off, login, exit node, tailnet zone services | no | **ours** (spec 09) | `samurai tailscale ...` |
| **GPU page**: driver profile, CUDA, gpu-unlock per card (state, toggle, last boot result) | no | **ours** (spec 07) | `samurai hw gpu` |
| **Python page**: installed versions (EOL badges), uv config, conda on/off | no | **ours** (spec 13) | `samurai python ...` |
| **Desktop page** (KDE): theme switch, rebuild panel widgets, reduced effects on/off | no | **ours** (spec 14) | `samurai desktop ...` |

### Gaming
| Feature | Garuda | Ours | CLI |
|---------|--------|------|-----|
| Launchers (Steam, Lutris, Heroic, Bottles, itch, Minigalaxy), Wine flavours, Proton tools (ProtonUp-Qt, protontricks, proton-cachyos), DXVK, gamemode, gamescope sessions, MangoHud | Garuda Gamer page | yes, from the component catalog | `samurai component enable gaming/steam` |

### Not copied
- Garuda Nix Subsystem: niche, large surface. Distrobox covers "run software from elsewhere".
- Firefly (Garuda's upcoming GUI package manager): KDE Discover with the pacman backend is good enough
  for v1. Note: Discover's PackageKit backend can do partial upgrades. We ship Discover **without
  `packagekit`** and use it only for Flatpak, and the health check flags `packagekit` (Garuda checks
  for it too).

## GUI shape
- Pages: Home/Status · Update · Maintenance · Health · Diagnostics · Snapshots · Kernels · Boot ·
  Hardware · Network · Components · Settings & Tweaks · Security · Gaming · Repos · Logs.
- An embedded log console under every action, fed by the task's log signal. A "copy as command"
  button shows the equivalent CLI, so the GUI also teaches the CLI.
- A command palette (Garuda has one) that searches every action by name.
- Every destructive action shows what will change and confirms. Actions that can take a snapshot
  do so.

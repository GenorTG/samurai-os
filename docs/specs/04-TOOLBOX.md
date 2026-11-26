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
| Optional apps: browse, search, install/remove catalog apps and curated bundles | setup wizard | **ours**: `samurai apps` (section below), the backend of the setup assistant and the GUI Apps page | `samurai apps list/search/info/install/remove/bundles` |
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

## Optional apps (`samurai apps`)
One tool over the merged catalog (`catalog/garuda-setup-wizard.toml` + `catalog/samurai.toml`, merge
rules at the top of `samurai.toml`), installed by `samurai-tools` to `/usr/share/samurai/catalog/`
(`SAMURAI_CATALOG_DIR` overrides it for tests). The catalog is the allowlist: anything else is plain
`pacman -S`.

| Command | Does |
|---------|------|
| `samurai apps list [--category X] [--installed] [--available] [--all] [--json]` | apps by category, ✓ installed, ✗ not available with the reason |
| `samurai apps categories [--json]` | categories with app / ready / installed counts |
| `samurai apps search <term> [--json]` | ranked match on id, name, package, category |
| `samurai apps info <id\|@bundle> [--json]` | packages with their repo, services, manual steps, bundles |
| `samurai apps bundles [--json]` | curated sets: `@gaming` (reuses the installer's gaming toggle), `@dev` (docker toggle), `@creative`, `@office`, `@media`, `@streaming` |
| `samurai apps install <id\|@bundle...> [--yes] [--dry-run] [--skip-unavailable] [--with-upgrade]` | one `pacman -S --needed` transaction, then `systemctl enable --now` for the apps' services |
| `samurai apps remove <id\|@bundle...> [--yes] [--dry-run]` | `pacman -Rs`, keeping packages another installed catalog app still uses |
| `samurai apps check [--json] [--strict]` | how many catalog packages resolve in the enabled repos (CI and diagnostics) |

- **Availability first.** Every package is resolved against the local sync databases (`pacman -Sl`,
  `-Sgg`, `-Sp` for provides) before anything runs. Not-available apps say why: needs `[multilib]`,
  needs Chaotic-AUR, `[samurai]` package not built yet, or not in any enabled repo. An explicitly
  requested unavailable app fails up front (exit 3); bundle members are skipped with a warning.
- **No partial upgrades.** The tool never runs `pacman -Sy`. Sync databases older than 3 days
  (`SAMURAI_APPS_MAX_DB_AGE_DAYS`) or pending upgrades stop the install with "run `samurai update`
  first" (exit 4); `--with-upgrade` instead runs `pacman -Su --needed <apps>`, a full upgrade and the
  install in one transaction.
- **Confirmation.** The plan (apps, packages, services, exact commands) is shown and confirmed with
  y/N unless `--yes`; `--dry-run` prints it and changes nothing. Runs pacman through `sudo` (or
  `pkexec`) when not root.
- **Flatpak apps** (`source = "flatpak"`) install with `flatpak install --noninteractive flathub`
  only when Flatpak is present; otherwise the hint is `samurai apps install flatpak`.
  **Vendor-script apps** (`user-script`) show their commands and run them as the invoking user,
  never as root. Catalog `root_commands` are executed only in the exact form
  `systemctl enable --now <unit>`; anything else is shown as a manual step.
- **Output.** Columns and category headings for people; `--json` (schema 1) for the Qt toolbox and
  the installer's catalog pages. Exit codes: 0 ok, 1 command failed, 2 usage/unknown app, 3 not
  available, 4 run `samurai update` first, 5 aborted, 6 catalog error.
- **Install profile.** `[packages] apps = ["firefox", "@gaming"]` is validated by
  `validate-profile.py` against the catalog (exact ids and bundle names only). `samurai-postinstall`
  applies it at the end of an online install through the same tool (`--with-upgrade
  --skip-unavailable`, vendor scripts deferred). Offline installs skip it with a log line. Anything
  skipped is written to `/var/lib/samurai/firstboot-notes` as the exact `samurai apps install ...`
  command and shown in `/etc/motd`; `samurai welcome` will read the same file.

## Machine-readable output (the GUI's only data source)
| Command | JSON |
|---------|------|
| `samurai status --json` / `--report` | last update check / last `samurai update` report |
| `samurai health --json` | results, each with `fixable` |
| `samurai clean --preview --json <target...>` | sizes, notes, items, description |
| `samurai apps list/search/info/bundles --json` | see below; list and search carry `db_age_days` (null: no databases yet) |
| `samurai kernel list --json` | installed/running/default/bootable, install warnings, remove blockers |
| `samurai boot get --json` | timeout, default kernel, parameters marked `protected` |
| `samurai mirrors status --json` | age and server URLs |
| `samurai snap list --json` | snapshots |
| `samurai update --json` | final report on stdout, progress on stderr |
| `samurai diag bundle --out -` | redacted bundle on stdout (nothing written to disk) |

All documents carry `"schema": 1` (health, clean, apps, kernel, boot, mirrors, snap, update).

## GUI shape
- Pages: Home/Status · Update · Maintenance · Health · Diagnostics · Snapshots · Kernels · Boot ·
  Hardware · Network · Components · Settings & Tweaks · Security · Gaming · Repos · Logs.
- An embedded log console under every action, fed by the task's log signal. A "copy as command"
  button shows the equivalent CLI, so the GUI also teaches the CLI.
- A command palette (Garuda has one) that searches every action by name.
- Every destructive action shows what will change and confirms. Actions that can take a snapshot
  do so.

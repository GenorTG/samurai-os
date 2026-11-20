# 08 — Installer and ISO

## ISO build
- `archiso` profile in `iso/`, derived from Arch's `releng` profile. Changes: our `pacman.conf`
  (`[samurai]`, `[cachyos]`, `[chaotic-aur]`, spec 02), the KDE live session, Calamares, the terminal
  installer, the toolbox (for rescue), branding.
- Build: `ci/build-iso.sh` runs `mkarchiso` inside a privileged `archlinux:latest` container. The same
  script runs locally (genorbox1, a Debian host) and in CI. Output: `samurai-<VERSION>-x86_64.iso` +
  `.sha256` + `.sig`.
- UEFI only, x86_64 only. The live ISO boots with Secure Boot **off** (as Arch's ISO does). Secure
  Boot is set up after install with our own keys (spec 09).
- Live session: Plasma (Wayland), auto-login user `live`, desktop icons "Install Samurai OS
  (graphical)", "Install Samurai OS (terminal)" and "Samurai Rescue". The TTY login prints both
  install commands, and `sshd` can be started from the live menu for a remote terminal install.
- Live graphics: Mesa for AMD/Intel/VMs, nouveau for NVIDIA. A boot menu entry "NVIDIA
  (proprietary)" loads the NVIDIA open modules live for GPUs nouveau cannot drive (Blackwell).
- Offline package cache on the ISO: base, kernels, KDE, firmware, NVIDIA/AMD/Intel driver stacks.
  Everything else in the catalog needs network. The installer marks those choices "downloads".
- Size budget: ≤ 2 GiB (spec 02). The size report per build lists the cache's biggest packages.

## One install profile, two front-ends
Both installers are only UIs that produce one **install profile** (`install-profile.toml`). One
backend, `samurai-postinstall`, applies it. So the graphical and terminal installs give identical
systems, and the VM tests (which feed a profile file directly) test the same code humans use.

| Front-end | Tech | Use |
|-----------|------|-----|
| Graphical | Calamares (built in `[samurai]`, as Garuda and CachyOS do) with our own QML pages for the catalog steps | default on the live desktop |
| Terminal | `samurai-install`, a guided TUI (Rust + ratatui) with the same pages | TTY, SSH into the live ISO, low-RAM machines, power users |
| Unattended | `samurai-install --profile my.toml --disk /dev/nvme0n1 --yes` | VM tests, reinstalling a machine the same way |

Every summary page has **Save profile**. You get a TOML file you can reuse for the next install,
and it is also written to `/var/lib/samurai/install-profile.toml` on the installed system.

### Catalog
The app lists come from `catalog/`:
- `garuda-setup-wizard.toml`: Garuda's setup-wizard list, imported verbatim (16 categories, 175 apps,
  same packages, same root/user commands) by `scripts/import-garuda-catalog.py`. Re-run the script
  against Garuda's toolbox to stay in sync; CI diffs it monthly.
- `samurai.toml`: our choices, defaults, extra categories (Craft suite, AI CLIs, local AI) and overrides.
- CI resolves every package to a repo (Arch, multilib, CachyOS, Chaotic-AUR, `[samurai]`, Flatpak). A
  package that resolves nowhere fails the build, so the installer never offers something it cannot install.

## Guided flow (same pages in both front-ends)
| # | Page | Defaults | Notes |
|---|------|----------|-------|
| 1 | Welcome | mode **Guided**; Expert (every option visible); Load profile | requirements: UEFI, 4 GiB RAM (8 with KDE + gaming), 40 GiB disk, AC power |
| 2 | Language & region | geo-IP suggestion when online | locale, timezone, keyboard |
| 3 | Disk | erase disk; **encryption on**; btrfs layout from spec 06 | alongside another OS (dual boot), replace partition, manual |
| 4 | User | user in `wheel`; login shell **fish** (bash and zsh offered; bash always installed); hostname | root locked |
| 5 | Kernels | **linux-cachyos** (default boot) + **linux-zen** | also `linux`, `linux-lts`, `linux-cachyos-lts` (spec 10) |
| 5a | Bootloader | **Limine** (ADR-0003; snapshot boot + clean Secure Boot) | alternatives: **GRUB**, **systemd-boot**, **rEFInd**; same kernel cmdline contract; Secure Boot story best on Limine/systemd-boot, GRUB ships with Secure Boot off (sbctl follow-up) |
| 6 | Desktop | **KDE Plasma**; GNOME, Hyprland, COSMIC, Xfce, Cinnamon, Sway, i3, none | only KDE gets our full setup and is tested; the others get upstream defaults plus our shell, tools and toolbox |
| 6a | KDE setup (KDE only) | theme **Breeze Dark** (Otto, Scratchy offered), **monitor widgets on**, **reduced effects on**, panel layout top + bottom task bar | spec 14 |
| 7 | Hardware | detected devices + recommended driver profiles (chwd); NVIDIA: **CUDA on** (Turing and newer); **gpu-unlock on** for each supported card, listed per card with its PCI address | spec 07; unsupported cards show why |
| 8 | Gaming | **on**: Steam, Heroic, GameMode, MangoHud, gamescope, GE-Proton (latest), Proton-CachyOS, Wine stack | spec 10; untick individually |
| 9 | Development | **Python versions as checkboxes** (3.10–3.13, all unticked; 3.14 is the system python; uv fetches missing versions on demand), conda: **Miniforge**, **Docker + Compose on** with mode choice: **docker group** or **rootless** (asked during install), AI CLIs (checkboxes, all off), local AI (Ollama, off) | spec 13 |
| 10 | Applications | Garuda's 16 categories + Craft suite; **Firefox ticked** in Browsers | search box; "selected: N, download: X GiB" |
| 11 | Network & security | firewall **firewalld** (ufw, none), **Tailscale on**, SSH server off, security profile **standard**; **reverse proxy optional** (off; Caddy default, nginx, traefik; "open 80/443?" note about router port forwarding) | spec 09 |
| 12 | Summary | everything chosen, disk changes in red, total download and disk use | Save profile; Install |
| 13 | Install | progress per step, log view, "copy log" | on failure: keep the log and offer "open rescue shell" |
| 14 | Done | reboot; list of first-boot steps (Secure Boot, TPM unlock, Tailscale login) | |

## `samurai-postinstall` (applies the profile)
Runs in the target chroot, idempotent, every step logged to `/var/log/samurai/install.log`:
1. Partition, encrypt, create subvolumes, unpack the live root (Calamares `unpackfs` / the same code in
   `samurai-install`), generate fstab, locale, users.
2. Remove live-only packages.
3. Repos and keyrings: `[samurai]`, `[cachyos]`, `[chaotic-aur]` with their keyrings (spec 02).
4. Kernels and headers.
5. Hardware: `chwd` profiles; CUDA if chosen; gpu-unlock local build if chosen (spec 07). These need
   network. Without it they are queued for `samurai-hw-firstboot`.
6. One pacman transaction with every selected package (desktop meta, gaming, dev, apps, AI CLIs).
7. Root commands from the catalog (enable services), then user commands as the new user via
   `runuser` (uv tools, vendor installers), each shown in the log with its exact command.
8. Desktop setup: theme, panel layout script, effects (spec 14) staged for first login.
9. initramfs (systemd hooks, `sd-encrypt`, `btrfs-overlayfs`, NVIDIA + gpu-unlock modules),
   bootloader install — **Limine + `limine-snapper-sync`** by default, or the profile's
   `[boot].bootloader` choice (GRUB / systemd-boot / rEFInd; ADR-0003) — kernel command line via
   `samurai boot` (same `rd.luks.uuid` / keyfile contract for every bootloader).
10. Snapper, snap-pac, btrfs timers (spec 06); security baseline (spec 09).
11. Write `hw-fingerprint`, `install-info.json`, `install-profile.toml`; first snapshot `install-complete`.

## First boot
`samurai welcome` opens once: release notes; **Connect Tailscale** (if on); **Set up Secure Boot**
(if the firmware supports it); **Enrol TPM unlock** (if TPM2 and encryption); anything step 5 had to
defer; links to the setup assistant (the same catalog) and the toolbox.

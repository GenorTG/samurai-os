# 01 — Architecture

## Layering
```
┌──────────────────────────────────────────────────────────────────┐
│ Samurai Toolbox GUI (Qt6/QML)      tray notifier      Welcome app │  clients
├──────────────────────────────────────────────────────────────────┤
│ `samurai` CLI  ──►  samurai-helper (system D-Bus, polkit-gated)  │  our tools
│   update · health · diag · snap · hw · kernel · boot · clean ...  │
├──────────────────────────────────────────────────────────────────┤
│ samurai-*-settings (config only)   pacman hooks   meta-packages   │  our layer
├──────────────────────────────────────────────────────────────────┤
│ pacman · snapper/btrfs · chwd · bootloader · systemd · polkit     │  upstream tools
├──────────────────────────────────────────────────────────────────┤
│ Arch repos: core, extra, multilib  (+ optional cachyos, chaotic)  │  upstream packages
└──────────────────────────────────────────────────────────────────┘
```
We own only the top three layers. A machine where every one of our packages is removed is a
plain, working Arch install. That is a design test: `samurai-uninstall-check` in spec 11.

## Source layout (one monorepo: `samurai-os`)
```
samurai-os/
  docs/specs, docs/adr          this spec set
  pkgbuilds/<pkg>/PKGBUILD      every package in [samurai]
  toolbox/                      Rust workspace
    crates/samurai-core         task model, config, pacman/snapper/chwd wrappers, health checks
    crates/samurai-cli          `samurai` binary
    crates/samurai-helper       privileged D-Bus service
    crates/samurai-notifier     user session tray/notification agent
    gui/                        Qt6/QML toolbox and welcome app (ADR-0004)
  settings/                     source trees for samurai-*-settings packages
  iso/                          archiso profile (releng-derived) + Calamares config
  tests/vm/                     QEMU/libvirt harness and scenarios (spec 11)
  ci/                           build container, repo publish, ISO build scripts
  .github/workflows/            CI (spec 02)
```
One repo keeps PKGBUILD versions, toolbox code and tests changing together in one commit. The
published package repository lives in a second, data-only repo (spec 02).

## Packages we ship
| Package | Kind | Contents |
|---------|------|----------|
| `samurai-keyring` | keyring | our signing public key(s), `pacman-key --populate samurai` hook |
| `samurai-mirrorlist` | config | `/etc/pacman.d/samurai-mirrorlist` (GitHub primary, Pages fallback) |
| `samurai-release` | config | `/usr/lib/os-release`, `lsb-release`, distro version; bumped every distro release |
| `samurai-base` | meta | base system: `base`, `linux-firmware`, microcode, `samurai-*` core packages |
| `samurai-settings-system` | config | sysctl, zram-generator, journald limits, udev rules, systemd-oomd, NetworkManager, firewall zone |
| `samurai-settings-security` | config | AppArmor, hardening sysctls, polkit rules, sudo defaults (spec 09) |
| `samurai-settings-kde` | config | Plasma defaults via `/etc/xdg` + `/etc/skel` |
| `samurai-settings-shell` | config | fish (default), bash, zsh configs, starship prompt |
| `samurai-hooks` | config | pacman hooks: snapshot, pacnew notice, DKMS/initramfs verification, reboot-required flag |
| `samurai-snapper` | config | snapper configs, cleanup policy, snapshot-boot integration (spec 06) |
| `samurai-hardware` | data+tool | our chwd profile set (spec 07) |
| `chwd` | tool | rebuilt from CachyOS source (GPL-3.0) so we do not depend on the CachyOS repo |
| `samurai-toolbox` | tool | `samurai` CLI, helper, notifier, health checks, polkit policy, systemd units |
| `samurai-toolbox-gui` | app | GUI toolbox + welcome/setup assistant |
| `samurai-update` | tool | thin package: `/usr/bin/samurai-update` → `samurai update`; updated first in every run (spec 03) |
| `samurai-branding` | assets | wallpapers, Plymouth theme, bootloader theme, icons, logo |
| `samurai-desktop-kde` | meta | Plasma + our app selection (spec 10) |
| `samurai-gaming` | meta | Steam, gamemode, gamescope, Proton tools (opt-in in installer and toolbox) |
| `samurai-iso-live`, `samurai-calamares-config` | ISO only | live session and installer config, removed at the end of install |

Target size of `[samurai]`: about 20–30 packages. A new package needs a reason in its PKGBUILD header.

## Toolbox runtime model
- **CLI**: `samurai <area> <action> [--json] [--yes] [--dry-run]`. Exit codes: 0 ok, 1 failed, 2 needs
  reboot, 3 needs user decision, 4 refused (precondition such as low disk, running on battery).
- **Helper**: `samurai-helper` is D-Bus activated (`org.samurai.Helper1`), runs as root, exposes
  one method per privileged action, and checks a polkit action per method
  (`org.samurai.helper.update`, `.snapshot`, `.repair`, ...). It refuses arbitrary commands; every
  action is a typed function in `samurai-core`.
- **Tasks**: long actions run as tasks with an id, a streamed log (D-Bus signal + journald with
  `SYSLOG_IDENTIFIER=samurai`) and a final JSON result. One write task at a time (pacman lock);
  others queue. The GUI's "pending operations" list (Garuda has one) is this queue.
- **No GUI dependency**: the CLI works without the helper when run as root (TTY, chroot, live ISO).
- **Config**: `/usr/lib/samurai/*.toml` (defaults, owned by packages), `/etc/samurai/*.toml`
  (admin overrides, merged key by key). State in `/var/lib/samurai/`, logs in `/var/log/samurai/`.

## Upstream tools we build on
| Need | Tool | Why |
|------|------|-----|
| packages | pacman, pacman-contrib (`checkupdates`, `paccache`, `pacdiff`) | the package manager |
| AUR (optional) | paru | Garuda/CachyOS default; never run by the updater as root |
| mirrors | reflector or rate-mirrors | ranked Arch mirrors |
| snapshots | snapper + snap-pac + btrfs-progs; btrfs-assistant as GUI | Garuda's proven stack |
| drivers | chwd | CachyOS's maintained detector (spec 07) |
| hardware info | inxi, lspci, lsusb, hwinfo, smartmontools, fwupd | diagnostics |
| boot | bootloader per ADR-0003; sbctl for Secure Boot | spec 06, 09 |
| installer | Calamares | used by both Garuda and CachyOS |
| ISO | archiso | official Arch ISO builder |

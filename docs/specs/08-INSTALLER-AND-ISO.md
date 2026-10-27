# 08 — Installer and ISO

## ISO build
- `archiso` profile in `iso/`, derived from Arch's `releng` profile. Changes: our `pacman.conf`
  (with `[samurai]`), our packages, the KDE live session, Calamares, the toolbox (for rescue), and
  branding.
- Build: `ci/build-iso.sh` runs `mkarchiso` inside a privileged `archlinux:latest` container. Same
  script locally (genorbox1, Debian host) and in CI. Output: `samurai-<VERSION>-x86_64.iso` +
  `.sha256` + `.sig` (detached, signed with the build key; spec 02).
- UEFI only, x86_64 only (spec 00). The live ISO boots with Secure Boot **off**. Arch's ISO has the
  same limitation, and shipping a signed shim chain is a non-goal for v1. Secure Boot is set up
  after install with our own keys (spec 09).
- Live session: Plasma (Wayland), auto-login user `live`, desktop icons "Install Samurai OS" and
  "Samurai Rescue" (opens `samurai rescue` in a terminal), and the toolbox's diagnostics.
- Live session graphics: Mesa for AMD/Intel/VMs, nouveau for NVIDIA. The proprietary NVIDIA stack
  is in the ISO's offline package cache and gets installed into the target, not loaded in the live
  session. Boot menu entry "NVIDIA (proprietary)" loads it live for GPUs nouveau cannot drive
  (Blackwell).
- Size budget: **≤ 2 GiB**, so the ISO fits in one GitHub release asset (spec 02). Contents are
  tracked in `iso/packages.x86_64` with a size report per build. If the budget cannot hold, the ISO
  moves to the mirror and GitHub keeps checksum + signature.

## Install flow (Calamares)
Calamares is not in the Arch repos. We build it in `[samurai]` from upstream source, the way
Garuda and CachyOS do; this is a documented exception to "no rebuilds".

| Step | Module | Our defaults |
|------|--------|--------------|
| Welcome | welcome | requirements: UEFI, 4 GiB RAM, 30 GiB disk, AC power, network optional |
| Locale, keyboard | locale, keyboard | geo-IP suggestion if online; Europe/Warsaw + `pl` offered first only when detected |
| Partitioning | partition | erase disk: ESP 2 GiB + LUKS2 btrfs root (encryption **checked** by default); manual mode allowed; "replace partition" for dual-boot |
| Users | users | user in `wheel`; root locked; "same password for root" disabled |
| Software | packagechooser / netinstall | kernel: `linux-zen` (default) or `linux` or `linux-cachyos` (enables the CachyOS repo); gaming meta yes/no; extra app groups from the catalog (spec 10) |
| Summary | summary | shows partitions, encryption, kernel, groups |
| Execute | partition, mount, **unpackfs** (copies the live rootfs, so no network is needed), machineid, fstab, locale, keyboard, users, displaymanager (SDDM), networkcfg, hwclock, services-systemd, then `shellprocess` → `samurai-postinstall` | |

### `samurai-postinstall` (shared by Calamares and the scripted installer)
Runs in the target chroot, idempotent, each step logged to `/var/log/samurai/install.log`:
1. Remove live-only packages (`samurai-iso-live`, `samurai-calamares-config`, Calamares).
2. Install the chosen kernel(s) + `linux-lts` fallback, from the offline cache or the network.
3. `chwd` detection and apply (spec 07).
4. mkinitcpio config (systemd-based hooks, `sd-encrypt`, `btrfs-overlayfs`), rebuild images.
5. Bootloader: install Limine to the ESP, create the UEFI entry, generate the config, enable
   `limine-snapper-sync` (ADR-0003).
6. Snapper configs, snap-pac, btrfs maintenance timers (spec 06).
7. Security baseline (spec 09): firewall, AppArmor, sysctls. Secure Boot is offered at first boot,
   not during install, because key enrolment needs the firmware in setup mode.
8. Write `/var/lib/samurai/hw-fingerprint` and `install-info.json` (versions, choices).
9. First snapshot `install-complete`.

### Scripted installer (`samurai-install`, for tests and power users)
`samurai-install --answers answers.toml --disk /dev/vda` runs the same partitioning, unpack and
`samurai-postinstall` steps without a UI. VM tests (spec 11) use it for every automated scenario.
Calamares itself is covered by one semi-automated test that drives the UI.

## First boot
`samurai welcome` opens once: release notes, "set up Secure Boot" (if the firmware supports it),
"enrol TPM unlock" (if there is a TPM2 and encryption is on), the setup assistant, and a link to the
toolbox. `samurai-hw-firstboot` runs in the background (spec 07).

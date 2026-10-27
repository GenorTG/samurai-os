# 00 — Overview

## Vision
A daily-driver Arch-based desktop for one owner (Genor) first, built so that it could later be
offered to other people without a rewrite. It should feel like Garuda (a toolbox that fixes things
for you, snapshots before every risky change, guided updates) and run like CachyOS (hardware
detection, sensible performance defaults), while staying as close to plain Arch as possible so
that upstream does most of the work.

## Goals
| # | Goal | Measured by |
|---|------|-------------|
| G1 | Install from an ISO to a working KDE desktop with the right drivers, no manual steps | VM test `install-default` passes; real hardware checklist (spec 11) |
| G2 | Updates come straight from Arch (kernels included); our layer updates from our own repo | `samurai-update` pulls from Arch mirrors + `[samurai]`; no Arch package rebuilt by us |
| G3 | Every update is reversible | snapshot before every transaction; VM test `rollback-after-bad-update` passes |
| G4 | A toolbox that covers Garuda's feature set (spec 04 parity table) | each row has CLI command + test |
| G5 | Secure by default without hurting gaming | spec 09 baseline enabled on a fresh install; mitigations on |
| G6 | Everything testable in a VM on genorbox1 | spec 11 suite runs headless from one command |
| G7 | Small maintenance cost | our own packages stay at roughly 30 or fewer (config, tools, meta); repackaged upstream binaries are auto-bumped by CI (spec 02) |
| G8 | Guided install, graphical or terminal, with app choices copied from Garuda | both installers produce the same install profile; VM test `install-profile-parity` (spec 08, 11) |

## Non-goals
- Building or patching kernels. The defaults are CachyOS's `linux-cachyos` and Arch's `linux-zen`,
  each from its own official repo (ADR-0001).
- Rebuilding or optimising Arch packages. CachyOS's x86-64-v3 rebuilds are out of scope: their repo
  sits below Arch, so it only adds packages Arch lacks (spec 02, ADR-0007).
- A second package format, a store, or an "image-based" OS. This is a classic mutable Arch system
  with snapshots.
- Support for anything but x86_64 UEFI machines. BIOS/MBR boot is not supported (boot repair can
  still restore an MBR for dual-boot rescue, spec 06).
- Polishing every desktop. The installer offers several (spec 08), but only KDE Plasma gets our full
  setup and test coverage in v1.

## Principles
1. **Upstream first.** If Arch or an upstream project ships it, we configure it rather than repackage it.
2. **pacman is the package manager.** Our tools call pacman; they never download or install packages
   by another route (ADR-0002).
3. **CLI first.** Every toolbox action is a CLI command with machine-readable output (`--json`). The
   GUI is a client of the same backend. Repairs must work from a TTY or a live ISO when the
   desktop is broken.
4. **Reversible by default.** Anything that changes system state takes a snapshot first, or says
   loudly why it cannot.
5. **Config as packages.** Defaults ship in `samurai-*-settings` packages that drop files into
   `/usr/lib`, `/etc/*.d/` or `/etc/skel`, so updates fix defaults and `.pacnew` handling works.
   Users override in `/etc` and `~`, which we never overwrite silently.
6. **Signed everything.** Our packages, repo database, ISO checksums and release notes are signed
   with our key (spec 09).
7. **Prove it in a VM.** No feature is done until a VM test exercises it (spec 11).

## Naming
- Working name **Samurai OS**, ID `samurai` (matches the Smart Samurai brand). Rename before the first
  public release; every identifier is derived from one variable in the build (`DISTRO_ID`,
  `DISTRO_NAME`), so a rename is a search-and-replace plus a new signing key UID.
- Package prefix `samurai-`, repo name `[samurai]`, CLI `samurai` (subcommands), GUI "Samurai Toolbox".
- Versions follow the house scheme (`MAJOR.MINOR.PATCH.BUILD`, see `VERSION`). Distro releases (ISOs)
  are tagged `v<VERSION>`. Individual packages carry their own `pkgver`.

## Spec map
| Spec | Topic |
|------|-------|
| 01 | Architecture: components, repositories, layering |
| 02 | Package repository: build, signing, hosting, third-party repos |
| 03 | Updates: `samurai-update`, update checker, interventions, distro releases |
| 04 | Toolbox: feature inventory and Garuda parity |
| 05 | Health checks, fixes and diagnostics |
| 06 | Snapshots, rollback, boot repair, rescue |
| 07 | Hardware and driver detection |
| 08 | Installer and ISO |
| 09 | Security baseline |
| 10 | Default software, desktop, performance and gaming tweaks |
| 11 | Testing in VMs and on real hardware |
| 12 | Roadmap and milestones |
| 13 | Developer environment: Python versions, uv, conda, Docker, AI CLIs |
| 14 | KDE Plasma setup: theme, panels, monitor widgets, effects |
| ADRs | `docs/adr/` — decisions with their reasons |
| OPEN-QUESTIONS | Things Genor needs to decide |

## Glossary
- **Intervention**: a one-time scripted fix shipped through the update tool (Garuda's mechanism),
  applied once per machine and recorded so it never runs twice.
- **Settings package**: a package that only installs configuration files.
- **Profile** (driver): a hardware-match rule plus the packages and config it installs (chwd model).
- **Snapshot boot**: booting a read-only btrfs snapshot from the boot menu to recover.

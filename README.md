# Samurai OS

A personal Arch Linux based distribution: Arch underneath, a thin signed layer of our own on top.
Ideas borrowed from CachyOS (performance kernels, hardware detection) and Garuda (toolbox,
snapshots, guided updates and repairs). Website: <https://genortg.github.io/samurai-os/>.

> **Status:** early and personal. Every feature below passes its VM scenario on QEMU/KVM; there is
> no physical-hardware guarantee yet. Back up before installing on a real machine.

## Download

[Latest release](https://github.com/GenorTG/samurai-os/releases/latest) has two installers:

| Installer | Size | Network during install |
|-----------|------|------------------------|
| `samurai-os-<date>-online-x86_64.iso` | ~1.3 GB | required: pulls current packages from the mirrors |
| `samurai-os-<date>-offline-x86_64.iso` | ~4.6 GB, in three parts | none: KDE, kernels and GPU drivers are on the ISO |

GitHub limits release files to 2 GiB, so the offline ISO ships in parts. Put every part and
`SHA256SUMS` in one folder:

```sh
sha256sum -c --ignore-missing SHA256SUMS                      # the parts
cat samurai-os-*-offline-x86_64.iso.0* > samurai-os-<date>-offline-x86_64.iso
sha256sum -c --ignore-missing SHA256SUMS                      # the joined ISO
```

Write it to a USB stick (`sudo dd if=<iso> of=/dev/sdX bs=4M status=progress oflag=sync`, or copy
it onto a Ventoy stick), boot it and run `samurai-install`.

## What works today
- Guided terminal installer (`samurai-install`) and unattended profile installs (`--profile`).
- LUKS2 full-disk encryption + btrfs subvolumes by default; Limine boot entries per kernel.
- KDE Plasma (Wayland) default desktop, SDDM, Breeze Dark; `linux-cachyos` + `linux-zen` kernels.
- Offline install of the full desktop from the offline ISO (no network, no mirror).
- GPU stacks: Mesa (RADV, ANV, NVK), Intel media, NVIDIA open modules per installed kernel.
- `samurai update`: one staged, snapshot-protected `pacman -Syu`, plus a desktop update notifier.
- `samurai snap` (snapper) and `samurai rescue` with rollback to a previous snapshot.
- `samurai apps`: optional apps and bundles from a curated catalog, at install time or any time.
- Signed `[samurai]` package repository published from GitHub Releases.

## What it is not
- Not a custom kernel, not a fork of Arch packages, not a rebuild of the Arch repos.
- Not a second package manager. pacman is the installer and updater; our tools wrap it.

## Build and test
```sh
bash scripts/install-hooks.sh                                  # once per clone: per-commit build number
SAMURAI_ISO_FLAVOR=offline ci/build-iso.sh "$PWD/build/off-1"  # or online
SAMURAI_TEST_ISO=<iso> SAMURAI_VM_RUN_ID=<id> pytest -q -s tests/vm/test_offline.py
ci/release-iso.sh <online.iso> <offline.iso> [--publish]       # dist/v<VERSION>/ + GitHub release
```
Arch tooling runs inside `archlinux` containers; VM tests need QEMU/KVM and OVMF.

## Specs
Start at [`docs/specs/00-OVERVIEW.md`](docs/specs/00-OVERVIEW.md). Decisions live in
[`docs/adr/`](docs/adr/). Roadmap: [`docs/specs/12-ROADMAP.md`](docs/specs/12-ROADMAP.md).

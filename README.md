# Samurai OS

A personal Arch Linux based distribution: Arch underneath, a thin signed layer of our own on top.
Ideas borrowed from CachyOS (performance kernels, hardware detection) and Garuda (toolbox,
snapshots, guided updates and repairs). Website: <https://genortg.github.io/samurai-os/>.

> **Status:** early and personal. Every feature below passes its VM scenario on QEMU/KVM; there is
> no physical-hardware guarantee yet. Back up before installing on a real machine.

## Download

[Latest release](https://github.com/GenorTG/samurai-os/releases/latest) has two installers. Releases
are tagged `v<MAJOR.MINOR.PATCH.BUILD>` from `VERSION` (the first one is `v0.1.0.0`); the ISO file
names carry the build date.

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

A signed release also ships `SHA256SUMS.sig`, a detached signature by the Samurai build key
(primary fingerprint `DFE8C148C042AA3C199AAEA08436E12731EDD3FA`, see [`docs/KEYS.md`](docs/KEYS.md)).
The public key is the one every installed system trusts for `[samurai]`:

```sh
curl -fsSLO https://raw.githubusercontent.com/GenorTG/samurai-os/main/pkgbuilds/samurai-keyring/build-key.txt
gpg --import build-key.txt
gpg --verify SHA256SUMS.sig SHA256SUMS                        # then the sha256sum checks above
```

Write it to a USB stick (`sudo dd if=<iso> of=/dev/sdX bs=4M status=progress oflag=sync`, or copy
it onto a Ventoy stick), boot it and run `samurai-install`.

## What works today
- Guided terminal installer (`samurai-install`) and unattended profile installs (`--profile`).
- LUKS2 full-disk encryption + btrfs subvolumes by default; Limine boot entries per kernel.
- CPU microcode (`amd-ucode`/`intel-ucode`, mkinitcpio `microcode` hook), zram swap
  (`zram-generator`, half of RAM up to 8 GiB, zstd) and NTP via `systemd-timesyncd` on every install.
- KDE Plasma (Wayland) default desktop, SDDM, Breeze Dark; `linux-cachyos` + `linux-zen` kernels.
- Offline install of the full desktop from the offline ISO: the installer resolves from a local
  pacman repository on the medium (no network, no mirror).
- GPU stacks: Mesa (RADV, ANV, NVK), Intel media, NVIDIA open modules per installed kernel.
- firewalld on by default (zone `public`, inbound closed except DHCPv6, SSH only with
  `ssh_server = true`); the wizard asks, profiles set `[security] firewall`.
- `samurai update`: one staged, snapshot-protected `pacman -Syu`, plus a desktop update notifier
  (`checkupdates` from `pacman-contrib`, whose weekly `paccache` timer trims the package cache).
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
ci/release-iso.sh <online.iso> <offline.iso> [--sign] [--publish]  # dist/v<VERSION>/ + GitHub release
```
Arch tooling runs inside `archlinux` containers; VM tests need QEMU/KVM and OVMF.

Fast checks, the same ones `.github/workflows/ci.yml` runs on every push and pull request:
```sh
ci/test-repo-checks.sh          # no epoch, no shadowing of Arch package names (plus negative fixtures)
ci/check-keyring                # build key, PKGBUILD pin, docs, installer and ISO copy agree
python3 -I ci/check-site.py     # site structure, accessibility basics, CSP-compatible markup
python3 -m pytest -q tests/unit
```
`ci/site-browser-check.py` additionally loads the site in headless Chromium (Playwright) and runs
axe-core; it is a local check, not part of CI.

Workflows: `ci.yml` (checks), `pages.yml` (site), `publish-testing-repo.yml` (signed
`[samurai-testing]` repo on push to `main`), `release-iso.yml` (manual, self-hosted KVM runner: builds
both ISOs, runs the smoke suite, signs `SHA256SUMS`, publishes `v<VERSION>`). Every action is pinned
to a commit SHA. Signing keys and secrets: [`docs/KEYS.md`](docs/KEYS.md).

## Specs
Start at [`docs/specs/00-OVERVIEW.md`](docs/specs/00-OVERVIEW.md). Decisions live in
[`docs/adr/`](docs/adr/). Roadmap: [`docs/specs/12-ROADMAP.md`](docs/specs/12-ROADMAP.md).

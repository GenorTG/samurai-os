# ADR-0003 — Limine as the bootloader

**Status:** Limine shipped. The Secure Boot part is **not built** (decision 2026-11-28): no boot
entry carries a file hash and no checksum is enrolled, so Limine verifies nothing today. Everything
below about hashes and signing is the design to be proven (`docs/security/secure-boot.md`).

## Options
| | GRUB + grub-btrfs (Garuda) | systemd-boot + UKIs | **Limine + limine-snapper-sync (CachyOS)** |
|--|--|--|--|
| Boot into snapshots | yes, mature | no good story | yes (`limine-snapper-sync` + `btrfs-overlayfs` hook) |
| Secure Boot with own keys | awkward: GRUB modules, shim or signed standalone GRUB, no verification of kernels from `/boot` | best: sign the UKI | possible: sign the Limine EFI binary, enrol the config checksum, put BLAKE2B hashes of the kernels in the config (not implemented) |
| LUKS2 + argon2id root | GRUB's LUKS2 support for argon2 is limited, so `/boot` usually goes outside LUKS anyway | fine (ESP outside) | fine (ESP outside) |
| Maintained by a distro we copy | Garuda | — | CachyOS (default bootloader) |

## Decision
Limine, with kernels on a 2 GiB FAT32 ESP mounted at `/boot`. Snapshot boot via
`limine-snapper-sync`. Secure Boot via sbctl signing Limine plus `limine-enroll-config`.

## Amendment (2026-11-18): bootloader is a user choice

The installer exposes `[boot].bootloader`: `limine` (default), `grub`, `systemd-boot`, `refind`
("more modern ones if someone wants" — Genor). All four get the same kernel command line contract
(`rd.luks.uuid`, embedded keyfile), so rescue/rollback and snapshot boot keep working; only
`limine-snapper-sync` (snapshot entries in the boot menu) is Limine-exclusive. Secure Boot: best on
Limine, good on systemd-boot (UKIs later), off by default on GRUB/rEFInd until their signing story
is worked out. `samurai rescue bootloader` is bootloader-aware (detects the target's config).

## Why
It is the only option that offers both snapshot boot (which we copy from Garuda) and a path to a
Secure Boot chain, and CachyOS has proven the combination in daily use.

## Risks and checks (M4 spike)
- Every change to `limine.conf` (new kernel, new snapshot entry) requires re-enrolling the checksum
  and re-signing. Confirm the hook ordering is reliable and fast enough to run on every snapper
  snapshot.
- The ESP holds the kernels each snapshot entry needs. Confirm the retention rule keeps the ESP
  inside 2 GiB with 10 snapshot entries and 2–3 kernels.
- `limine-snapper-sync` and `limine-mkinitcpio-hook` come from AUR/CachyOS. We package them in
  `[samurai]` and track upstream.
- Fallback if the spike fails: GRUB + grub-btrfs with Secure Boot off by default.

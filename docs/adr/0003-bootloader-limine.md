# ADR-0003 — Limine as the bootloader

**Status:** proposed; confirm with the M4 spike (snapshot entries + Secure Boot together)

## Options
| | GRUB + grub-btrfs (Garuda) | systemd-boot + UKIs | **Limine + limine-snapper-sync (CachyOS)** |
|--|--|--|--|
| Boot into snapshots | yes, mature | no good story | yes (`limine-snapper-sync` + `btrfs-overlayfs` hook) |
| Secure Boot with own keys | awkward: GRUB modules, shim or signed standalone GRUB, no verification of kernels from `/boot` | best: sign the UKI | good: sign the Limine EFI binary, enrol the config checksum; kernels verified by BLAKE2B hashes in the config |
| LUKS2 + argon2id root | GRUB's LUKS2 support for argon2 is limited, so `/boot` usually goes outside LUKS anyway | fine (ESP outside) | fine (ESP outside) |
| Maintained by a distro we copy | Garuda | — | CachyOS (default bootloader) |

## Decision
Limine, with kernels on a 2 GiB FAT32 ESP mounted at `/boot`. Snapshot boot via
`limine-snapper-sync`. Secure Boot via sbctl signing Limine plus `limine-enroll-config`.

## Why
It is the only option that gives both snapshot boot (which we copy from Garuda) and a clean Secure
Boot chain, and CachyOS has proven the combination in daily use.

## Risks and checks (M4 spike)
- Every change to `limine.conf` (new kernel, new snapshot entry) requires re-enrolling the checksum
  and re-signing. Confirm the hook ordering is reliable and fast enough to run on every snapper
  snapshot.
- The ESP holds the kernels each snapshot entry needs. Confirm the retention rule keeps the ESP
  inside 2 GiB with 10 snapshot entries and 2–3 kernels.
- `limine-snapper-sync` and `limine-mkinitcpio-hook` come from AUR/CachyOS. We package them in
  `[samurai]` and track upstream.
- Fallback if the spike fails: GRUB + grub-btrfs with Secure Boot off by default.

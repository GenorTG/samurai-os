# Secure Boot: why there is no setup command yet (2026-11-28)

Spec 09 describes `samurai security secureboot setup` (sbctl keys, sign Limine, enrol the config
checksum). It is not implemented, on purpose: with today's boot menu writers it would either verify
nothing or stop the machine from booting.

## Facts
- No boot entry carries a file hash: nothing in `installer/samurai-postinstall`,
  `toolbox/crates/samurai-core/src/boot/`, `updates/samurai-snap` or `updates/samurai-rescue` writes
  `#<blake2b>` after a path or runs `limine enroll-config`. Spec 09 "Disk encryption" and ADR-0003
  say Limine verifies kernel and initramfs hashes: **that is not true of the current code**. Anyone
  who can write the ESP can replace the kernel or initramfs.
- Limine `CONFIG.md` (trunk): "When Secure Boot is active and a config hash is enrolled, all file
  paths must have a hash appended or Limine will panic" and "Without an enrolled hash Limine treats
  Secure Boot as inactive". `hash_mismatch_panic` and `editor_enabled=no` are forced in that mode.
- So: signing `BOOTX64.EFI` alone gives a green "Secure Boot: on" that loads any kernel and any
  edited config. Enrolling the checksum requires every hash to be right after every change.

## What a safe implementation needs (all of it, before any of it ships)
1. One function that renders `limine.conf` with hashes, used by every writer: install, `samurai boot
   sync/set/default`, snapshot entries, `rescue rollback`/`bootloader`, `snap rollback`.
2. A post-hook after **every** initramfs or kernel change, including a hand-run `mkinitcpio -P` (an
   mkinitcpio post hook, not only a pacman hook), that re-hashes, runs `limine enroll-config`, signs
   with sbctl and verifies, writing the new files so that a power cut leaves a bootable pair.
3. A fallback boot path that survives a bad update: a second signed Limine binary with its own
   enrolled config pointing at the previous kernel, registered as a second firmware entry.
4. Recovery, tested in a VM: turn Secure Boot off in firmware setup, boot the live ISO,
   `samurai rescue bootloader` reinstalls an un-enrolled Limine (an enrolled binary still panics on a
   changed config even with Secure Boot off, to be confirmed in the VM).
5. Setup only in firmware setup mode, always `sbctl enroll-keys --microsoft`.
6. VM tests (OVMF `OVMF_CODE_4M.secboot.fd` + empty `OVMF_VARS_4M.fd`, `smm=on`) for: enable, kernel
   update, snapshot entry, hand-run `mkinitcpio`, tampered kernel refused, recovery.

The alternative to weigh: signed unified kernel images (systemd-boot or Limine chainloading UKIs).
The firmware then verifies one signed file per kernel and there is no config-hash bookkeeping, at the
cost of snapshot entries in the menu.

TPM unlock (`samurai security tpm enroll`) works without this, but PCR 7 with Secure Boot off does
not detect a replaced boot loader: the PIN and the passphrase are what protect the disk then.

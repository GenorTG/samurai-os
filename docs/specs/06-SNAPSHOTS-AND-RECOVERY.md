# 06 — Snapshots, rollback, boot repair, rescue

## Disk layout (installer default)
| Partition | Size | FS | Notes |
|-----------|------|----|-------|
| ESP, mounted at `/boot` | 2 GiB | FAT32 | kernels and initramfs live here (Limine reads them; ADR-0003). Not encrypted; integrity comes from Limine's hash check + Secure Boot (spec 09) |
| root | rest | LUKS2 (default on) → btrfs | `zstd:1` compression, `noatime` |

Swap: zram (spec 10). There is no swap partition, because hibernation is a non-goal for v1.

btrfs subvolumes (flat layout, same as Garuda/CachyOS):
| Subvolume | Mount | Snapshotted | Why separate |
|-----------|-------|-------------|--------------|
| `@` | `/` | yes (snapper `root`) | the system |
| `@home` | `/home` | own config `home`, off by default | rollback of `/` must never roll back your files |
| `@root` | `/root` | no | |
| `@srv` | `/srv` | no | |
| `@cache` | `/var/cache` | no | pacman cache must survive rollback (we need it to roll forward again) |
| `@log` | `/var/log` | no | logs explain what happened, so a rollback must not erase them |
| `@tmp` | `/var/tmp` | no | |
| `@containers` | `/var/lib/docker`, `/var/lib/containers` | no | huge, churns, `nodatacow` optional |
| `@vm` | `/var/lib/libvirt/images` | no, `nodatacow` | VM disk images |
| `@snapshots` | `/.snapshots` | — | snapper storage |

## Snapshot policy
- `snap-pac`: pre/post pair around every pacman transaction (so every update, install and removal).
- `samurai` actions that change the system (fixes, resets, pacdiff merge, component toggles) take a
  single snapshot labelled with the action name.
- Timeline snapshots: **off** for root (pre/post covers changes; timelines just waste space). On for
  `home` only if the user enables home snapshots.
- Cleanup (`number` algorithm): keep the last 10 pre/post pairs + any snapshot marked `important`
  (kernel, systemd and glibc transactions get marked by our hook), at most 50% of free space. Snapper
  timers enabled.
- btrfs maintenance timers on: scrub monthly, balance (usage filter) monthly, fstrim weekly.

## Snapshot boot (recover when the system will not start)
- `limine-snapper-sync` adds a boot menu entry for each root snapshot (newest first, capped at 10).
  It also keeps the kernel + initramfs each snapshot needs on the ESP (verify the retention rule in
  the M4 spike).
- Snapshots are read-only. The `btrfs-overlayfs` mkinitcpio hook boots them with a tmpfs overlay, so
  the desktop works while booted into an old snapshot. Changes made there are lost on reboot.
- When booted from a snapshot, the desktop shows a banner: "You are running snapshot #N from
  <date>. [Restore this snapshot] [Reboot to current]". `samurai update` refuses to run in this state.

## Rollback
`samurai snap rollback [<id>]` (and the banner button):
1. Take a fresh snapshot of the current (broken) `@` labelled `before-rollback`, so the rollback
   itself can be undone.
2. Create a writable snapshot of the target as the new `@` (by renaming subvolumes; the old `@`
   becomes `@-broken-<timestamp>`, cleaned up after 14 days or on demand).
3. Make the bootloader and kernel images match the restored root: reinstall the kernel packages the
   snapshot expects, from the `@cache` package cache (this is why the cache is outside `@`).
4. Regenerate the boot entries; reboot.
VM tests `rollback-after-bad-update` and `rollback-from-snapshot-boot` (spec 11) cover both entry paths.

`samurai snap` also covers: `list`, `diff <a> <b>` (files and package versions), `restore-file <id>
<path>`, `delete`, `create <label>`, `repair` (recreate snapper configs, matching Garuda's reset).
btrfs-assistant ships as the advanced GUI.

## Boot repair and rescue
`samurai rescue` runs on the installed system **and from the live ISO** (the ISO includes the toolbox).
| Action | Does |
|--------|------|
| `rescue mount` | find our install (btrfs with `@`), unlock LUKS (passphrase or recovery key), mount all subvolumes + ESP under `/mnt` |
| `rescue chroot` | mount + `arch-chroot` with the toolbox available inside |
| `rescue bootloader` | reinstall Limine to the ESP, recreate the UEFI boot entry, regenerate config, re-sign + re-enroll for Secure Boot |
| `rescue initramfs` | rebuild all initramfs images |
| `rescue kernel` | reinstall kernel packages from the cache, or from the network |
| `rescue rollback` | snapshot rollback from outside the system (same steps as above) |
| `rescue mbr backup/restore` | back up / restore a disk's boot code for dual-boot setups (Garuda has this) |
| `rescue update-fix` | `samurai fix pacman --full` inside the chroot (stuck or broken update) |
| `rescue bundle` | diagnostics bundle of the installed system, written to a USB stick or uploaded |

Rule: `rescue` never formats anything and never writes outside the selected install's partitions.

# 05 — Health checks, fixes and diagnostics

## Check model
Each check is a typed unit in `samurai-core`:
```
id            stable string, e.g. "boot.initramfs"
category      packages | boot | drivers | storage | snapshots | services | security | hardware | config
severity      info | warning | error | critical   (critical = may not boot)
detect()      → Ok | Issue{ summary, details, evidence[] }   read-only, no root unless declared
fix()         optional; snapshot-first; returns what changed
fix_risk      safe | needs-confirm | manual-only
```
- `samurai health` runs every check (`--category`, `--id`, `--json`). `samurai health --fix` runs the
  fixes for failed checks, safe ones automatically and needs-confirm ones after a prompt. Manual-only
  checks print the steps instead.
- A system timer runs `samurai health --json` daily and after every update. It writes
  `/var/lib/samurai/health.json`, which the notifier and the GUI read without root.
- Checks must be fast (whole run under 5 s; SMART and `pacman -Qkk` sampling are the slow ones,
  cached for 24 h).

## Check list (v1)
Rows marked **G** exist in `garuda-health` (inspected 2026-10-27); unmarked rows are ours.

| id | G | Detects | Fix |
|----|---|---------|-----|
| `packages.core` | G | core packages missing (base, linux-firmware, microcode for this CPU, our base) | install, safe |
| `packages.important` | G | important packages missing (snapper, snap-pac, chwd, firewall) | install, confirm |
| `packages.deprecated` | G | packages we replaced or Arch dropped | replace, confirm |
| `packages.partial-upgrade` | G | sync db newer than installed set (someone ran `-Sy` only) | `samurai update`, confirm |
| `packages.outdated` | G | no full update for 14 days | `samurai update` |
| `packages.packagekit` | G | `packagekit` installed (partial-upgrade risk via Discover) | remove, confirm |
| `packages.integrity` |  | `pacman -Qkk` on a sample of boot-critical packages finds modified or missing files | reinstall those packages, confirm |
| `packages.foreign-broken` |  | foreign/AUR packages linked against a missing soname | list + suggest rebuild, manual |
| `packages.holds` |  | `IgnorePkg` entries added by `samurai kernel hold` past their expiry | remove hold, confirm |
| `pacman.conf` | G | our repos missing or reordered, SigLevel weakened, unknown repos above Arch | restore defaults (keeps user repos below), confirm |
| `pacman.keyring` |  | `archlinux-keyring`/`samurai-keyring` outdated, or keys expired | `samurai fix keyring` |
| `pacman.mirrors` |  | mirrorlist older than 30 days or the first mirror failing | refresh, safe |
| `pacman.pacnew` |  | pending `.pacnew`/`.pacsave` files | open `samurai pacdiff`, manual |
| `boot.initramfs` | G | last mkinitcpio run failed, or an image is missing for an installed kernel | rebuild, safe |
| `boot.kernels-present` | G | an installed kernel has no boot entry or no image on the ESP | regenerate entries, safe |
| `boot.efi-entry` | G | no UEFI boot entry for our bootloader | recreate with efibootmgr, confirm |
| `boot.esp-space` |  | ESP under 20% free (kernels + UKIs fill it) | `samurai clean boot` (old images), confirm |
| `boot.pending-reboot` | G | running kernel's modules dir gone, or a core lib updated since boot | prompt to reboot |
| `drivers.profile` |  | chwd's recommended profile differs from what is installed (e.g. GPU swapped from RTX 3090 to RTX 5080 without a driver reinstall) | `samurai hw apply --reinstall`, confirm |
| `drivers.nvidia` | G | NVIDIA GPU present without a matching driver; wrong driver family for the GPU generation (open modules needed for Blackwell) | install the profile, confirm |
| `drivers.nvidia-module` | G | NVIDIA module not loaded / not built for an installed kernel | `samurai fix dkms` |
| `drivers.nvidia-params` |  | `nvidia_drm.modeset=1` / `fbdev=1` missing on Wayland | add kernel params, confirm |
| `drivers.microcode` |  | microcode package missing for the CPU vendor | install, safe |
| `storage.btrfs-space` | G | root btrfs: free or unallocated space low | `samurai clean all --preview`, balance suggestion |
| `storage.btrfs-errors` |  | `btrfs device stats` error counters non-zero | manual: show disk page |
| `storage.btrfs-timers` | G | scrub/balance/trim timers disabled | enable, safe |
| `storage.smart` | G | SMART/NVMe health failing or wear above 90% | manual: back up now |
| `storage.disk-free` |  | any mounted fs above 90% full | cleaner, manual |
| `snapshots.services` | G | snapper timers disabled | enable, safe |
| `snapshots.config` | G | snapper root config missing or invalid | `samurai snap repair` |
| `snapshots.old` | G | snapshots beyond policy, or older than 6 months taking space | cleanup per policy, confirm |
| `snapshots.boot-entries` |  | snapshot boot entries out of sync with the snapper list | regenerate, safe |
| `services.failed` | G | failed systemd units (system + user) | show logs; restart, confirm |
| `services.time-sync` | G | clock not synchronized | enable timesyncd, safe |
| `services.audio` | G | PipeWire not running for the user, or PulseAudio still installed | `samurai fix audio`, confirm |
| `hardware.cpu-temp` | G | CPU above 90 °C at idle | manual: cooling advice |
| `hardware.ram` | G | low available RAM / swap pressure; no zram | enable zram, safe |
| `hardware.firmware` |  | fwupd has updates | `samurai firmware update`, confirm |
| `security.firewall` |  | firewall not active | enable, safe |
| `security.apparmor` |  | AppArmor not enabled at boot | add LSM param, confirm |
| `security.secureboot` |  | Secure Boot expected (installed with it) but off, or an unsigned EFI binary on the ESP | `samurai security sign`, confirm |
| `security.cve` |  | `arch-audit` reports fixed CVEs not yet installed | `samurai update` |
| `security.ssh` |  | sshd running with password auth or root login | harden config, confirm |
| `security.listening` |  | unexpected ports open on non-loopback interfaces | manual: show list |
| `config.multilib` |  | Steam installed but multilib disabled | enable, confirm |
| `config.shell` |  | login shell missing or not in `/etc/shells` | reset to bash, confirm |

New checks get added the same way. Each check has a VM test that breaks the system on purpose,
runs `detect()`, runs `fix()`, and checks the issue is gone (spec 11 `health-*`).

## Diagnostics (`samurai diag`)
| Command | Content |
|---------|---------|
| `diag info` | `inxi -Fazy`-style summary: CPU, GPU(s) + driver in use, RAM, disks, kernel, DE/session, display server, bootloader, Secure Boot, firmware versions |
| `diag services` | `systemd-analyze` (time, blame, critical-chain), failed units |
| `diag logs` | journal errors this boot and previous boot, `--kernel` (dmesg), `--pacman` (last N transactions), `--update` (last `samurai update` log) |
| `diag disks` | SMART/NVMe summary, btrfs usage and device stats, mounts |
| `diag gpu` | driver, modules, Vulkan/GL renderer, kernel params, chwd profile |
| `diag bundle` | all of the above + `samurai health --json` + `pacman -Qe`/`-Qm` + `pacman.conf` + boot config, in one text file under `~/samurai-diag-<timestamp>.txt` |

### Redaction (on by default for `bundle`)
Replaced with stable placeholders (`<user1>`, `<mac1>`, ...): user names and home paths, hostname,
MAC addresses, IPv4/IPv6 addresses outside loopback, Wi-Fi SSIDs, disk/board serial numbers, email
addresses, anything that looks like a token or key. `--no-redact` exists but asks twice.

### Upload (opt-in)
- `diag bundle --upload` shows the redacted file, asks, then uploads it to our own paste service
  (an end-to-end-encrypted PrivateBin-style service, decryption key only in the URL fragment), with
  7-day expiry. It prints the URL and copies it to the clipboard.
- Host: a paste service on genorbox1 behind `paste.smart-samurai.pl`. Check whether
  `~/projects/genor-paste` can serve; otherwise deploy PrivateBin (OPEN-QUESTIONS Q7).

# 03 — Updates

Three separate things, often lumped together:
1. **System updates**: Arch packages, kernels included. They come from Arch mirrors via `pacman -Syu`.
2. **Our layer**: `[samurai]` packages. Same pacman transaction, from our repo.
3. **Distro releases**: a versioned milestone (`v0.3.0.x`). It is a new `samurai-release` package plus
   a new ISO, and release notes. Installed systems reach it through (2). Nothing else downloads anything.

ADR-0002 explains why there is no separate GitHub-Releases updater.

## `samurai update` (alias `samurai-update`, `update`)
A wrapper around one `pacman -Syu`, modelled on `garuda-update`. Stages run in order. Each stage
logs to `/var/log/samurai/update-<timestamp>.log` and the journal. Each can be skipped by config
where noted.

| # | Stage | What it does | Refuses / warns when |
|---|-------|--------------|----------------------|
| 0 | Preflight | not booted from a snapshot; network up; pacman lock free (a stale lock with no pacman process is removed after a prompt); clock synced; free space on `/` ≥ download+install size + 2 GiB (btrfs-aware, `btrfs fi usage`); AC power on laptops | booted from snapshot → offer `samurai snap rollback`; low space → offer `samurai clean` |
| 1 | Self-update | `pacman -Sy`, then if `samurai-update`, `samurai-keyring` or `archlinux-keyring` are outdated: `pacman -S --needed` those only, then re-exec the new updater | — |
| 2 | Mirrors | refresh Arch mirrorlist if older than `mirrors.max_age_days` (default 7) | skippable: `--skip-mirrors` |
| 3 | News | fetch Arch news since the last successful update; show new items; items matching our "manual intervention" list need an acknowledgement unless an intervention (below) handles them | unacknowledged intervention news → stop with exit 3 |
| 4 | Pre interventions | run pending interventions of phase `pre` | an intervention fails → stop, nothing changed yet |
| 5 | Snapshot | snap-pac takes the pre/post pair automatically. We tag the pre snapshot `samurai-update <updater version>`, and refuse to continue if snapper is broken unless `--no-snapshot` is given | snapper unhealthy → stop, suggest `samurai snap repair` |
| 6 | Transaction | `pacman -Syu` + arguments contributed by interventions (e.g. `--overwrite <path>` for a known file conflict, answers to a replace prompt). `--noconfirm` only if `update.noconfirm = true` | pacman fails → print the snapshot id and the exact rollback command; exit 1 |
| 7 | Post checks | DKMS modules built for every installed kernel; initramfs and bootloader entry generation succeeded (parsed from the pacman log); kernel modules dir present for the running kernel; `.pacnew` count; failed units | any boot-critical check fails → big "do not reboot" warning, run `samurai health --fix boot` |
| 8 | Post interventions | phase `post` | — |
| 9 | Housekeeping | `paccache -rk3` (keep 3), `paccache -ruk0` (uninstalled); orphan notice (never auto-remove); Flatpak update if installed; AUR as the user only if `update.aur = true` | — |
| 10 | Report | summary: counts, kernel changed, reboot needed (kernel, systemd, glibc, mesa, nvidia, plasma, dbus), `.pacnew` files, news shown. JSON with `--json`; desktop notification via the notifier | exit 2 if reboot needed |

### Why stage 1 is not a partial-upgrade hazard
`pacman -Sy` followed by installing a few packages is exactly how Arch systems get broken. It is
safe here only because of these rules, which CI enforces for the self-update set:
- `samurai-update` is a **static musl binary** with no library dependencies, so a new build never
  needs a newer glibc than the one installed.
- Keyring packages have no dependencies.
- The self-update set never contains anything else.

### Kernel safety
- The default install has two kernels: the main one (spec 10) and `linux-lts` as a fallback. Both
  appear in the boot menu.
- If the main kernel's DKMS build fails, the post check says so, and the toolbox offers to make LTS
  the default until it is fixed.
- We never pin or hold kernel versions by default. `samurai kernel hold <pkg>` exists for emergencies
  and adds an `IgnorePkg` entry with an expiry date that the health check reminds you about.

## Interventions (Garuda's best idea, kept)
When Arch ships a change that needs a manual step (an Arch news item, a file conflict, a package
replacement), we ship the fix inside `samurai-update`. Because stage 1 updates the updater first,
machines get the fix before the transaction that needs it.

- Location: `/usr/lib/samurai/interventions/<NNNN>-<slug>.toml`, plus an optional script beside it.
- Fields: `id`, `title`, `news_url`, `phase` (`pre` | `pacman-args` | `post`), `applies_if` (package
  installed/version constraints, path exists, file contains), `script` or `pacman_args`, `idempotent = true`.
- State: `/var/lib/samurai/interventions.done` (id + timestamp + result). Each intervention runs at most
  once per machine and is skipped on fresh installs that are already past it (`applies_if` decides).
- Every intervention gets a VM test: prepare a VM in the "before" state, run the update, assert the
  "after" state (spec 11).

## Update checker (`samurai-notifier`, user session)
- systemd **user** timer: 5 minutes after login, then every 6 hours.
- Uses `checkupdates` (temporary db copy, so it never touches the real sync db and cannot create a
  partial upgrade). Separate counts for Arch and `[samurai]`. With `arch-audit`, it flags packages
  that have known CVEs (security updates).
- Tray icon states: up to date / updates available (n) / security updates / reboot pending / health
  warning / snapshot problem. Clicking it opens the toolbox on the right page.
- Notices: not updated for 14 days (Garuda uses the same threshold); `.pacnew` files pending; new
  distro release notes installed.
- Optional pre-download (`notifier.predownload = true`): `checkupdates -d` fetches packages into the
  cache using the temp db. Nothing is installed. (Check during M3 that `-d` behaves this way without root.)
- Never installs anything on its own. Unattended updates are a non-goal for a rolling desktop.

## Distro releases
- A release = bump `VERSION` and `samurai-release`, regenerate the ISO, tag `v<VERSION>`, publish
  the GitHub Release with the ISO, `.sha256` and `.sig` (spec 08).
- Release notes ship **inside** `samurai-release` (`/usr/share/samurai/release-notes/<version>.md`).
  Installed systems get them through the signed package, so the notifier needs no network fetch and
  no extra trust path. The toolbox shows them once after the update that brought them.
- Release cadence: whenever the layer changes meaningfully; there is no fixed schedule (it is rolling).

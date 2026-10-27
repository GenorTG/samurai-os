# ADR-0002 — pacman is the updater; GitHub only hosts our repo

**Status:** accepted (proposed to Genor 2026-10-27; his original idea was "the full update route
through GitHub Releases")

## Context
The original idea: deliver updates through GitHub Releases of our own public repo, with our own
update checker and installer.

## Decision
- The right core of that idea is kept: our packages are built by GitHub Actions and published on
  GitHub Releases as a **signed pacman repository** (spec 02), and distro releases (ISO + notes) are
  GitHub Releases.
- What changes: there is no separate download-and-install path. `samurai update` wraps one
  `pacman -Syu` that takes Arch packages from Arch mirrors and ours from `[samurai]` in the same
  transaction (spec 03). The update checker uses `checkupdates`.

## Why
- On Arch, installing some packages without upgrading the rest (a partial upgrade) is the common way
  to break a system: a library and its users get out of sync. An updater that installs our parts
  from GitHub separately from pacman would create exactly that situation, and would fight pacman's
  lock, database and hooks.
- pacman already does dependency resolution, signature checks, hooks (snapshots, initramfs, DKMS) and
  rollback-friendly logging. A second installer would have to redo all of it.
- Kernels and drivers must come from Arch anyway (ADR-0001), so the GitHub route could never be the
  whole story.

## Consequences
- The only place we bend the rule is stage 1 of `samurai update` (self-update of the updater and
  keyrings before the main transaction), with the constraints spec 03 lists so it cannot cause a
  partial upgrade.
- GitHub Releases' asset rules (no `:` in names, size caps) shape the repo (spec 02).

# ADR-0007 — CachyOS and Chaotic-AUR repos enabled by default, below Arch

**Status:** accepted (follows from Genor 2026-10-27: CachyOS kernel as main; "copy Garuda's solution"
for app choices)

## Decision
`[cachyos]` and `[chaotic-aur]` are in the default `pacman.conf`, **after** core/extra/multilib, with
their keyrings installed by the installer (spec 02, spec 08 step 3).

## Why
- The default kernel and Proton-CachyOS live in `[cachyos]`.
- Much of Garuda's app list, and things Genor asked for (Heroic, yay, Claude Code, most ArtCraft
  apps), exist only in the AUR. Chaotic-AUR is Garuda's own answer to that: AUR packages built and
  signed on its build servers. Installing them from there keeps installs fast, offline-cacheable and
  signed, with no source builds on the user's machine.
- Placing both repos below Arch means they can only add packages, never replace Arch's.

## Consequences
- We trust two more signing keys and two more mirror networks. That is the same trust CachyOS and
  Garuda users already give them.
- The toolbox Repos page can disable either. It first lists which installed packages would stop
  getting updates.
- Health check `pacman.conf` verifies the order (Arch first) on every run.
- What Chaotic lacks and we need goes into `[samurai]` (spec 02 table), with a CI job that drops our
  copy once Chaotic or Arch carries it.

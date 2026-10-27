# HANDOFF — samurai-os

## Mission
A personal Arch-based distro: Arch repos unchanged, CachyOS + Chaotic-AUR below Arch, a small signed
`[samurai]` repo built by GitHub Actions on GitHub Releases. Plus a Garuda-level toolbox, graphical +
terminal guided installers fed by Garuda's app catalog, a dev environment (Python 3.10–3.14, uv,
conda, Docker, AI CLIs), GPU setup (CUDA + local gpu-unlock build), and a customised KDE. Everything
proven in QEMU VMs on genorbox1.

## State (2026-10-27)
- Specs 00–14, ADR-0001…0008, OPEN-QUESTIONS (decided + open tables).
- Round 2 (Genor's brainstorm) folded in: installers (spec 08), dev env (spec 13), KDE setup
  (spec 14), CUDA + gpu-unlock (spec 07, ADR-0008), repos default on (ADR-0007), kernels cachyos+zen
  (ADR-0001), gaming/Wine (spec 10), firewall UX + Tailscale (spec 09).
- `catalog/garuda-setup-wizard.toml`: 175 apps / 16 categories imported from Garuda toolbox
  `setup-wizard.data.ts` via `scripts/import-garuda-catalog.py`. `catalog/samurai.toml`: our
  choices/toggles/apps/overrides.
- Package availability checked 2026-10-27 (Arch, AUR, Chaotic db): see `catalog/samurai.toml`
  `source=`. Arch python = 3.14.8, cuda 13.4, uv 0.13.
- Local git only. No GitHub repos, no signing key, no code beyond the catalog importer.

## Next steps
1. Genor answers open questions (Q2 name, Q3 GUI stack, Q4 theme, Q6 repo visibility, Q12–Q16).
2. M0: `ci/build-iso.sh` + VM boot. M1 spike: pacman from GitHub Releases with a throwaway key.
3. Check Otto/Scratchy licences before M7a.

## Commands
- `bash scripts/install-hooks.sh` (per-commit build number).
- `python3 -I scripts/import-garuda-catalog.py <setup-wizard.data.ts> catalog/garuda-setup-wizard.toml`

## Blockers
- Public repos / real build key: waiting for Genor's OK.

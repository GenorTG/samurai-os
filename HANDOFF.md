# HANDOFF — samurai-os

## Mission
A personal Arch-based distro: Arch repos unchanged (kernels included), a small signed `[samurai]`
repo built by GitHub Actions and published on GitHub Releases, a Garuda-level toolbox (updates,
maintenance, health checks + fixes, diagnostics, snapshots, drivers, boot repair), a Calamares ISO,
a security baseline, all proven in QEMU VMs on genorbox1.

## State (2026-10-27)
- Spec set v0 written: `docs/specs/00`–`12`, ADR-0001…0006, `OPEN-QUESTIONS.md`.
- Research inputs: Garuda toolbox source (Electron/Angular, feature inventory in spec 04),
  `garuda-update` (stages, interventions), `garuda-health` (check list in spec 05), CachyOS chwd
  (profile paths/format, spec 07), CachyOS Limine + Secure Boot approach (ADR-0003).
- Local git repo only. No GitHub repos created, no signing key generated, no code yet.
- Test host checked: `/dev/kvm` usable by genorbox1, OVMF (incl. secboot) + swtpm + QEMU 7.2 + Docker present.

## Next steps
1. Genor answers OPEN-QUESTIONS (at least Q1 kernel, Q2 name, Q3 GUI stack, Q6 repo visibility).
2. M0: `ci/build-iso.sh` (mkarchiso in a privileged archlinux container) + `tests/vm/boot-iso.sh`; ISO boots on OVMF.
3. M1 spike: pacman from GitHub Releases with a throwaway test key (real key needs Genor).
4. M2: `samurai-install` + harness with QMP/guest agent.

## Commands
- `bash scripts/install-hooks.sh` (per-commit build number).

## Blockers
- Creating public repos / the real build key: waiting for Genor's OK.

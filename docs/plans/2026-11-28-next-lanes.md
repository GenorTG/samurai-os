# Plan: next lanes (2026-11-28), after release v0.0.1.90

| Lane | Branch / worktree under ~/projects/samurai-os-wt | Delivers |
|---|---|---|
| D adopt | `feat/adopt-defaults` (`adopt`) | opt-in `samurai` command that brings an older install to the current defaults (Chaotic-AUR + metas + toolbox), previewed, snapshot-first; remove the ArtCraft catalog entry without a package; release notes "Verified" text no longer hard-coded; ADR-0009 marked accepted |
| E hardware | `feat/hardware` (`hardware`) | `samurai hw` (spec 07): detect, driver profiles via chwd, CUDA toggle, health checks, installer/toolbox hooks; gpu-unlock builder only within ADR-0008 |
| F security | `feat/security` (`security`) | spec 09 baseline: `samurai security status`, firewall presets, hardening settings package, Secure Boot / TPM setup where VM-testable; first-boot welcome app (spec 08) |

Rules: work only on your branch; never publish, tag, release or push `main`; `SAMURAI_NO_BUMP=1`;
VM runs under `flock /tmp/samurai-vm.lock`; delete VM disks and builds you create; no new package may
rebuild or shadow an Arch package. The parent merges, verifies the combination and releases.

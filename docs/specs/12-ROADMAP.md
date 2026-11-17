# 12 — Roadmap

Milestones are ordered by dependency. Each one ends with a VM test that proves it (spec 11), so
nothing is "done" on paper only. Estimates assume part-time work with agents doing most of the
typing.

| # | Milestone | Delivers | Exit criteria | Est. |
|---|-----------|----------|---------------|------|
| M0 ✅ 2026-11-17 | Build substrate | `ci/build-iso.sh` in an archlinux container; releng-derived profile with our `pacman.conf` stub (Arch + CachyOS + Chaotic); QEMU boot script | our ISO boots to a console on OVMF on genorbox1 | 1–2 days |
| M1 ✅ 2026-11-17 | Signed repo | key runbook; `samurai-keyring`, `samurai-mirrorlist`, `samurai-release`; build+sign+publish workflow; `samurai-repo`; local mirror; upstream-release watcher for repackages | **spike**: a VM installs `samurai-release` from GitHub Releases with `SigLevel = Required`; CI checks for shadowing/epochs/unresolvable catalog entries fail on purpose | 2–4 days |
| M2 | Profile + unattended install + harness | install-profile schema; `samurai-postinstall`; `samurai-install --profile`; `tests/vm` harness (QMP + guest agent) | `smoke` suite green from one command | 1 week |
| M3 | Updates | `samurai update` stages, interventions, notifier, Arch news | `update-from-archive`, one `intervention-*` green | 1 week |
| M4 | Snapshots and recovery | snapper policy, Limine snapshot boot, `samurai snap`, `samurai rescue` | rollback + rescue scenarios green; **spike**: snapshot entries + Secure Boot | 1–2 weeks |
| M5 | Toolbox CLI | helper + polkit, task queue, maintenance/clean/fix/diag/health, firewall, tailscale, python pages (CLI side) | `toolbox-actions`, `health-*`, `firewall-ux` green | 2–3 weeks |
| M6 | Drivers + GPU | chwd + our profiles, fingerprint service, CUDA rule, `samurai-gpu-unlock` builder | fixture tests; `hw-fingerprint-change`, `gpu-unlock-builder` green; **hardware**: gpu-unlock end to end on stealth-dragon's 3090 | 1–2 weeks |
| M6b | Developer environment | `samurai-python310…313`, uv config, Miniforge/Miniconda packages, Docker defaults, AI-CLI catalog entries + `[samurai]` rebuilds | `python-versions`, `docker` green | 1 week |
| M7 | Desktop ISO + installers | KDE live session, Calamares build + our catalog pages, terminal guided installer, catalog import + resolver, `samurai-gaming` + `samurai-wine` + `samurai-proton-ge`, size report ≤ 2 GiB | `iso-boot`, `install-calamares`, `install-tui`, `install-profile-parity`, `catalog-resolve` green | 2–3 weeks |
| M7a | KDE setup | theme licence check + packages (Breeze Dark default; Otto, Scratchy optional), `samurai-plasma-layout` (panels + monitor widgets), reduced-effects defaults; **spike**: ksystemstats sensor ids, per-socket option | `kde-layout` green | 1 week |
| M8 | Security | `samurai-settings-security`, Secure Boot setup, TPM+PIN, firewall presets, Tailscale zone, AppArmor | `secureboot`, `tpm-unlock` green; `security status` all pass on a fresh install | 1 week |
| M9 | GUI toolbox | Qt6/QML app over the helper's D-Bus API: every page in spec 04; welcome app; tray notifier | each page reaches the same helper calls (D-Bus mock tests) + manual review | 3–4 weeks |
| M10 | Real hardware, daily driver | hardware checklist on stealth-dragon, Genor's spare laptop and genorbox1 hardware; **testing order (Genor 2026-11-17): genorbox1 first**, other hosts later (their disk space is not guaranteed) | Genor daily-drives it for 2 weeks without a manual repair; his own laptop install (below) | 2+ weeks |
| M10a | Install-drive creator | `samurai drive write` (dd-based, USB installer drive) + a full install drive image, so Genor can hand-test on his spare laptop without SSH workflows | writes a bootable drive from the latest ISO in one command; verified in VM | 2–3 days |
| v1.0 | Release | first `v1.0.0.x` ISO + public repo | all suites green; hardware checklist passed | — |

Rough total to v1.0: **4–5 months part-time** (grew by about a month with the installers' catalog
pages, developer environment and KDE setup). An install that updates and rolls back (M0–M4) still
arrives in the first month, via the unattended installer.

## Ongoing maintenance (after v1.0)
- Weekly: CachyOS profile diff, Arch news triage (does anything need an intervention?), `full` suite on
  the latest Arch snapshot, upstream-release watcher PRs (GE-Proton, python-build-standalone,
  Miniforge, AUR rebuilds, gpu-unlock).
- Monthly: re-import Garuda's catalog and review the diff.
- Per Arch breaking change: write an intervention plus its test, ship it through `samurai-update`.
- Yearly: rotate the signing subkey (spec 02).

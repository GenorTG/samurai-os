# 12 — Roadmap

Milestones are ordered by dependency. Each one ends with a VM test that proves it (spec 11), so
nothing is "done" on paper only. Estimates assume part-time work with agents doing most of the
typing.

| # | Milestone | Delivers | Exit criteria | Est. |
|---|-----------|----------|---------------|------|
| M0 | Build substrate | `ci/build-iso.sh` in an archlinux container; releng-derived profile with our `pacman.conf` stub; QEMU boot script | our ISO boots to a console on OVMF on genorbox1 | 1–2 days |
| M1 | Signed repo | key generation runbook; `samurai-keyring`, `samurai-mirrorlist`, `samurai-release`; build+sign+publish workflow; data repo `samurai-repo`; local mirror | **spike**: a VM installs `samurai-release` from GitHub Releases with `SigLevel = Required`; no-shadowing/no-epoch checks fail on purpose | 2–4 days |
| M2 | Scripted install + harness | `samurai-install`, `samurai-postinstall` (layout, LUKS, Limine, snapper), `tests/vm` harness with QMP + guest agent, `install-default` + `firstboot` | `smoke` suite green from one command | 1 week |
| M3 | Updates | `samurai update` stages 0–10, interventions engine, notifier (CLI output + desktop notification), Arch-news parsing | `update-from-archive` and one sample `intervention-*` green | 1 week |
| M4 | Snapshots and recovery | snapper policy, snapshot boot via `limine-snapper-sync`, `samurai snap`, `samurai rescue` | `rollback-after-bad-update`, `rollback-from-snapshot-boot`, `rescue-bootloader` green; **spike**: snapshot entries + Secure Boot together | 1–2 weeks |
| M5 | Toolbox CLI | helper daemon + polkit, task queue, maintenance/clean/fix/diag/health (spec 04/05 CLI rows) | `toolbox-actions` and all `health-*` green | 2–3 weeks |
| M6 | Drivers | `chwd` + `samurai-hardware` profiles, fingerprint service, CachyOS profile-diff job | fixture tests for all our machines; `hw-fingerprint-change` green | 1 week |
| M7 | Desktop ISO | KDE live session, Calamares build + config, branding, `samurai-desktop-kde`, `samurai-gaming`, catalog, size report ≤ 2 GiB | `iso-boot`, `install-calamares` green | 1–2 weeks |
| M8 | Security | `samurai-settings-security`, Secure Boot setup, TPM+PIN, firewall, AppArmor, `samurai security` | `secureboot`, `tpm-unlock` green; `security status` all pass on a fresh install | 1 week |
| M9 | GUI toolbox | Qt6/QML app over the helper's D-Bus API: every page in spec 04, log console, command palette; welcome + setup assistant; tray notifier | each GUI page's actions reach the same helper calls (D-Bus mock tests) + manual review | 3–4 weeks |
| M10 | Real hardware, daily driver | hardware checklist on fan-dragon (spare disk), stealth-dragon, a laptop; fix list | Genor uses it as a daily driver for 2 weeks without a manual repair | 2+ weeks |
| v1.0 | Release | first `v1.0.0.x` ISO + public repo, docs site | all suites green; hardware checklist passed | — |

Rough total to v1.0: **3–4 months part-time**. A usable install that updates and rolls back
(M0–M4) comes out of the first month. The GUI (M9) is deliberately late: the CLI must be solid first,
because the GUI is only a client of it.

## Ongoing maintenance (after v1.0)
- Weekly: CachyOS profile diff, Arch news triage (does anything need an intervention?), `full` suite on
  the latest Arch snapshot.
- Per Arch breaking change: write an intervention plus its test, ship through `samurai-update`.
- Yearly: rotate the signing subkey (spec 02).

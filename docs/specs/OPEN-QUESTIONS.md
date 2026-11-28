# Open questions (Genor decides)

Work proceeds on the default until he answers.

All open questions answered in the native form on 2026-11-17; none remain open.

## Decided
| # | Question | Decision | Where |
|---|----------|----------|-------|
| Q1 | Main kernel | `linux-cachyos` default + `linux-zen` (Genor 2026-10-27) | ADR-0001, spec 10 |
| Q5 | Encryption default, TPM | on / TPM+PIN offered at first boot (no objection) | spec 09 |
| Q8 | Chaotic-AUR | **on by default** ("copy Garuda's solution") | ADR-0007 |
| Q9 | Browser | Firefox ticked by default, every Garuda-list browser selectable (Genor) | spec 08 page 10 |
| Q11 | Installers | graphical (own Qt wizard instead of Calamares, ADR-0009, accepted 2026-11-27: the owner did not object) + terminal guided + unattended profile (Genor) | spec 08 |

## Open

None. Every Q1–Q16 has a decision above. New questions go here when they appear.
| # | Question | Default in the specs | Notes |
|---|----------|----------------------|-------|
| Q2 | Final name | **"Samurai OS", id `samurai` — confirmed (Genor 2026-11-17)** | one-variable rename is now moot |
| Q3 | Toolbox GUI stack | **Qt6/QML — confirmed (Genor 2026-11-17)** | ADR-0004; CLI/helper are the same either way |
| Q4 | Default KDE theme | **Breeze Dark — confirmed (Genor 2026-11-17)** | Otto and Scratchy offered in installer; spec 14 needs default switched |
| Q6 | Repo visibility | **Source repo public now — confirmed (Genor 2026-11-17)** | `samurai-os` public at M1 alongside `samurai-repo` |
| Q7 | Diagnostics upload | **Manual opt-in only — confirmed (Genor 2026-11-17)** | `paste` prints/copies a command; nothing uploads by itself; PrivateBin host stays the target |
| Q10 | Test hardware | **genorbox1 VMs first; other hosts later (Genor 2026-11-17)** | disk space on other machines not guaranteed; SSH workflow avoided; a bootable install-drive creator (USB) is in scope so Genor can hand-test on his spare laptop |
| Q12 | Conda | **Miniforge — confirmed (Genor 2026-11-17)** | Miniconda offered |
| Q13 | Python versions | **All versions 3.10–3.14 selectable per-version during install — confirmed (Genor 2026-11-17)** | spec 13: change default-all to a per-version checkbox list |
| Q14 | Docker mode | **Both offered; choice made during install — confirmed (Genor 2026-11-17)** | group vs rootless becomes an installer option, not a global default |
| Q15 | AI CLIs | **All unticked by default — confirmed (Genor 2026-11-17)** | spec 13 as written |
| Q16 | gpu-unlock scope | **Include forks for RTX 20 + non-4090 RTX 40 — confirmed (Genor 2026-11-17)** | ALL recipes (upstream included) marked "untested / potentially unstable" until we validate on real hardware; spec 07 + ADR-0008 updated |

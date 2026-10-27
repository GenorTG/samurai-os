# Open questions (Genor decides)

Work proceeds on the default until he answers.

## Decided
| # | Question | Decision | Where |
|---|----------|----------|-------|
| Q1 | Main kernel | `linux-cachyos` default + `linux-zen` (Genor 2026-10-27) | ADR-0001, spec 10 |
| Q5 | Encryption default, TPM | on / TPM+PIN offered at first boot (no objection) | spec 09 |
| Q8 | Chaotic-AUR | **on by default** ("copy Garuda's solution") | ADR-0007 |
| Q9 | Browser | Firefox ticked by default, every Garuda-list browser selectable (Genor) | spec 08 page 10 |
| Q11 | Installers | graphical (Calamares) + terminal guided + unattended profile (Genor) | spec 08 |

## Open
| # | Question | Default in the specs | Notes |
|---|----------|----------------------|-------|
| Q2 | Final name | "Samurai OS", id `samurai` | one-variable rename (spec 00) |
| Q3 | Toolbox GUI stack: Qt6/QML or Tauri/web? | Qt6/QML (ADR-0004) | CLI/helper are the same either way |
| Q4 | Default KDE theme: Otto or Scratchy? | Otto, Scratchy offered | spec 14 |
| Q6 | Source repo public from day one, or private until near v1? The package repo must be public (pacman cannot log in to private release downloads) | `samurai-os` private until M7, `samurai-repo` public from M1 | creating either repo waits for Genor's OK |
| Q7 | Diagnostics upload host | PrivateBin at `paste.smart-samurai.pl` | |
| Q10 | Spare disk / USB SSD on fan-dragon for real installs? | live USB only until one exists | |
| Q12 | Conda: Miniforge (recommended) or Miniconda as the default? | Miniforge, Miniconda offered | spec 13: same `conda` command; Miniforge avoids Anaconda's channel terms |
| Q13 | Python 3.10 went EOL this month: still install by default? | yes, with an EOL badge | spec 13 |
| Q14 | Docker group membership by default (root-equivalent) or rootless Docker? | docker group, checkbox on | spec 13 |
| Q15 | AI CLIs: anything to tick by default? | all off | spec 13 |
| Q16 | gpu-unlock on Turing / non-4090 Ada from forks, or only upstream recipes? | upstream only | spec 07, ADR-0008 |

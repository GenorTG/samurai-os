# ADR-0009 — Graphical installer: a small Qt/QML wizard, not Calamares

**Status:** proposed (implemented on `feat/installer-gui`, needs the owner's OK)

## Context
Spec 08 first named Calamares with our own pages. The hard contract is one install engine: a
front-end writes `install-profile.toml` and runs `samurai-install --profile`.

## Findings (checked 2026-11-26, Calamares source and the repo databases)
- Packaging: Arch has no `calamares` package (`pacman -Si calamares`: not found) and Chaotic-AUR
  ships none. CachyOS ships ~20 `cachyos-calamares*` builds preconfigured for CachyOS. We would build
  and maintain our own C++ package (KPMcore, Boost, Python) and rebuild it on Qt updates.
- Disk: the `partition` view module owns the disk jobs (`PartitionViewStep::jobs()` returns
  `PartitionCoreModule`'s jobs) and its page offers manual, replace and alongside installs that our
  engine refuses. We would have to replace it.
- Secrets: the `users` module stores the password in GlobalStorage only reversibly scrambled
  (`users/Config.cpp`: `gs->insert("password", Calamares::String::obscure(...))`), and the partition
  jobs publish `luksPassphrase` there. Our contract needs a crypt hash and a passphrase on stdin.
- Pages: kernels, gaming, development, the searchable catalog and the profile summary have no stock
  module that produces our schema; QML pages need a C++ view-step plugin each.
- Privileges: Calamares runs its whole GUI as root.

What would remain of Calamares is the window frame, so it adds cost and risk without reuse.

## Decision
`installer/gui/`: PySide6 + Qt Quick Controls with the Plasma style (`org.kde.desktop`).
- The wizard runs unprivileged. It imports no install logic: it calls `validate-profile.py`,
  `samurai-profile-plan` and `samurai-apps`.
- `samurai-installer-helper` (root, via a fixed no-argument sudo rule that exists only on the live
  medium) reads `{profile, passphrase}` on stdin and runs `samurai-install --profile FILE --yes`.
- Progress comes from the engine's own log lines (`progress.py`; a unit test pins the markers).

## Consequences
- No new package to maintain; `pyside6` comes from Arch.
- Differs from ADR-0004 (toolbox GUI in C++/QML): the installer is Python so it can ship as files on
  the live medium without a build step. It can move to the toolbox's stack later; the contract (a
  TOML file and one helper) does not change.
- Not covered that Calamares would have given: UI translations, manual partitioning (the engine has
  no support for it yet either).

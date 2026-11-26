# Plan: defaults, GUI toolbox, graphical installer (Genor 2026-11-26)

Goal: a fresh KDE install has everything in specs 10/13/14 preinstalled, optional software is
selectable at install and later, there is a KDE GUI toolbox and a graphical installer.

## Dependency order
| Lane | Branch / worktree | Delivers | Needs |
|---|---|---|---|
| A defaults | `feat/defaults` (`~/projects/samurai-os-wt/defaults`) | meta + settings packages (spec 10, 14), Chaotic-AUR on (ADR-0007), install-profile toggles/choices (spec 08 pages 6a, 8–11) applied by postinstall, offline closure | — |
| B toolbox GUI | `feat/gui` (`~/projects/samurai-os-wt/gui`) | Qt6/QML app + .desktop entry, client of `samurai ... --json` / CLI; pages: status, health (+fix), apps, clean, kernels/boot, diagnostics | existing CLI |
| C graphical installer | `feat/installer-gui` | live Plasma session + graphical wizard producing the same install profile | A's profile schema |

## Contracts
- One backend: both installers only write `install-profile.toml`; `samurai-postinstall` applies it.
- The GUI holds no system logic: it runs the CLI; root actions go through `pkexec` with fixed argv.
- New packages live in `pkgbuilds/`; no Arch package is rebuilt or shadowed (AGENTS.md).
- Work only on your branch and worktree. Never publish, tag, or push `main`. Commit with
  `SAMURAI_NO_BUMP=1`; push your branch. The parent session merges, bumps pkgrel, publishes.
- VM tests share host port 8966 and 6 cores: wrap every VM run in
  `flock /tmp/samurai-vm.lock <command>`. Delete your VM disks and superseded ISOs afterwards.
- Never `pkill -f` with a pattern contained in your own command; never leave a QMP client attached.

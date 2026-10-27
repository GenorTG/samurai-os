# ADR-0004 — Toolbox: Rust core + CLI + D-Bus helper, Qt6/QML GUI

**Status:** proposed (OPEN-QUESTIONS Q3)

## Decision
- `samurai-core`, `samurai` (CLI), `samurai-helper` (D-Bus, polkit) and `samurai-notifier` in Rust.
- GUI in Qt6/QML (Kirigami) talking to the helper over D-Bus.

## Why
- CLI first (spec 00 principle 3): repairs must work from a TTY, a chroot and the live ISO, where no
  GUI or browser runtime exists. The GUI therefore cannot own any logic.
- Rust: typed and testable; chwd (which we use) is Rust; a static musl build gives the self-updating
  `samurai-update` no library dependencies (spec 03 stage 1). zbus, polkit and TOML crates are mature.
- Qt6/QML: native on our KDE desktop, small, follows the Plasma theme, and needs no browser engine.
  Garuda's toolbox is Electron (a bundled browser, a large dependency to keep updated) and their
  older tools were Qt.

## Alternatives
- Tauri (web UI): fine for Genor's web skills, uses WebKitGTK, looks foreign on Plasma.
- Python for everything: faster to write, but weaker typing, and runtime breakage after Python
  upgrades. Arch's Python 3.x bumps are exactly the kind of event `garuda-update` had to ship
  interventions for.

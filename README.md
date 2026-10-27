# Samurai OS (working name)

A personal Arch Linux based distribution: Arch underneath, a thin layer of our own on top.
Ideas borrowed from CachyOS (hardware detection, performance defaults) and Garuda (toolbox,
snapshots, guided updates and repairs).

**Status:** specification phase. Nothing is built yet.

## What it is
- Plain Arch Linux repositories for everything upstream ships: kernels, Mesa, NVIDIA, KDE.
- One signed package repository of our own (`[samurai]`), built by GitHub Actions and published
  through GitHub. It holds only our tools, configs and meta-packages.
- A toolbox (CLI first, GUI on top) covering updates, maintenance, diagnostics, health checks with
  automatic fixes, snapshots, drivers, kernels, boot options and boot repair.
- A Calamares-based installer ISO built with `archiso`.
- Every step proven in a QEMU/KVM virtual machine before it touches real hardware.

## What it is not
- Not a custom kernel, not a fork of Arch packages, not a rebuild of the Arch repos.
- Not a second package manager. pacman is the updater; our tools wrap it.

## Specs
Start at [`docs/specs/00-OVERVIEW.md`](docs/specs/00-OVERVIEW.md). Decisions live in
[`docs/adr/`](docs/adr/). Open questions: [`docs/specs/OPEN-QUESTIONS.md`](docs/specs/OPEN-QUESTIONS.md).

## Development
```sh
bash scripts/install-hooks.sh   # once per clone: per-commit build number in VERSION
```

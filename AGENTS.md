# AGENTS.md — samurai-os

Personal Arch-based distro: a thin layer on plain Arch (ADR-0001). Specs in `docs/specs/` are the
source of truth; decisions in `docs/adr/`; open decisions in `docs/specs/OPEN-QUESTIONS.md`.

## Layout
See `docs/specs/01-ARCHITECTURE.md` "Source layout". Today only `docs/`, `scripts/` exist.

## Commands
- `bash scripts/install-hooks.sh`: once per clone; per-commit BUILD bump in `VERSION`
  (`SAMURAI_NO_BUMP=1` to skip).
- Builds and VM tests: not built yet (M0/M2 in `docs/specs/12-ROADMAP.md`).

## Rules
- Never rebuild or shadow an Arch package; never add a package epoch (spec 02).
- pacman is the only installer; tools wrap it (ADR-0002).
- Every toolbox feature is a CLI command first; the GUI is a client (spec 01).
- Every feature needs a VM test before it counts as done (spec 11).
- Creating or pushing to public GitHub repos, generating the real signing key, and any install on
  real hardware need Genor's explicit OK.
- VM disks live in `.vm/` (git-ignored). Clean overlays and old ISOs after test runs.

## Gotchas
- genorbox1 is Debian: archiso, makepkg and pacman run only inside `archlinux` containers.
- `lsblk -no NAME,PARTTYPENAME` prints TREE prefixes (`|-vda1`, `├─vda1`) — parsing it with awk
  yields a garbage device (`/dev/|-vda1`). Always use `lsblk -lno` for column parsing (2026-11-18:
  this silently broke the rescue ESP keyfile detection under LC_ALL=C).
- Garuda's wiki pages are JS-rendered (`web_fetch` returns nothing). Read Garuda sources on GitLab
  (`garuda-linux/applications/*`, `garuda-linux/pkgbuilds`) instead.
- chwd reads profiles only from `/var/lib/chwd/db/{pci,usb}/` (hard-coded in `src/consts.rs`).
- gpu-unlock firmware/modules must never be published (repo, ISO, CI artifacts): NVIDIA firmware has no
  redistribution approval (ADR-0008). Build on the target only.
- `catalog/garuda-setup-wizard.toml` is generated: edit `catalog/samurai.toml` (overrides) or re-run
  `scripts/import-garuda-catalog.py` against Garuda's toolbox `setup-wizard.data.ts`.
- Never replace `/usr/bin/python` (Arch's python is the system python; extra versions live in `/opt/samurai/python`).

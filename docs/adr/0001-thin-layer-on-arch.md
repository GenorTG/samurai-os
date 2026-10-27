# ADR-0001 — A thin layer on plain Arch; no kernel or package rebuilds

**Status:** accepted (Genor: "we don't want to build our own custom kernel")

## Decision
Samurai OS uses the Arch repositories unchanged. We ship only our own tools, configuration packages,
meta-packages, branding, and the few things Arch does not package (Calamares, chwd, limine helper
scripts). Kernels come from Arch (`linux`, `linux-zen`, `linux-lts`), optionally `linux-cachyos`
from the CachyOS repo placed below Arch.

## Why
- Every rebuilt package is a package we must rebuild on every upstream change, forever. CachyOS does
  this with a team and build farm; one person cannot.
- Arch's packages are already signed, mirrored and tested by Arch. Rebuilding them adds risk and no
  security.
- A thin layer passes the design test in spec 01: remove our packages and you still have a working
  Arch system. Users (and Genor) can always get help from Arch documentation.

## Consequences
- No x86-64-v3 optimised rebuilds (CachyOS's main performance claim). Users who want them can enable
  the CachyOS repo themselves; our toolbox warns that this changes the package source of the whole
  system.
- Our repo stays small (target ≤ 30 packages), and CI rejects any package whose name shadows Arch.

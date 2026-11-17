# ADR-0008 — gpu-unlock is built on each machine, never redistributed

**Status:** accepted

## Context
Genor wants the gpu-unlock FP16/BF16 unlock (github.com/Zagitta/gpu-unlock) applied automatically
during driver install on supported cards. It was tested on genorbox1 on 2026-10-27, and a test on
stealth-dragon (Arch) is running. The module depends on firmware extracted from an NVIDIA driver
package. Upstream's `firmware/NOTICE.md`: "Personal local packaging is the initial delivery target …
redistribution approval is not claimed."

## Decision
- `[samurai]` ships `samurai-gpu-unlock`: a pinned upstream commit plus a builder that runs
  upstream's own Arch PKGBUILD **on the target machine**. It downloads NVIDIA's inputs from nvidia.com,
  checks them against pinned SHA-256 sums, and builds and installs `gpu-unlock-dkms` +
  `gpu-unlock-firmware` locally.
- No built module, firmware or NVIDIA archive is ever published in our repo, on the ISO or in CI
  artifacts. CI builds only to test, in a throwaway container, and discards the output.
- Enabled per card by PCI address (`gpu_unlock.targets=0000:01:00.0`), for cards in the recipe
  table (upstream + pinned forks — Genor 2026-11-17 wants fork coverage for RTX 20 / non-4090 Ada).
  Every recipe is labelled "untested, potentially unstable" (spec 07) until validated on hardware.

## Why
This keeps a feature Genor explicitly wants without publishing NVIDIA firmware, which upstream itself
says it has no permission to distribute. Building locally is the same model as an AUR package.

## Consequences
- Needs network during install (or defers to first boot).
- Each driver or module update rebuilds locally (DKMS, plus our package hook when the pin changes).
- If upstream later gets redistribution approval, we can switch to a normal binary package.

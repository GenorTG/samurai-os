# Lane E: hardware and drivers (`feat/hardware`, 2026-11-28)

Spec 07 (engine, `samurai hw`, CUDA, fingerprint), spec 05 `drivers.*`. Branch only: nothing
published, tagged or released.

## Delivered
| Part | Where |
|------|-------|
| chwd 1.25.0 built without CachyOS profiles, no `-Sy` | `pkgbuilds/samurai-chwd` |
| Our graphics + network profiles | `pkgbuilds/samurai-hardware` |
| `samurai hw detect\|list\|apply\|remove\|fingerprint` (`--json`) | `toolbox/crates/samurai-core/src/hw/` |
| `drivers.profile`, `.nvidia`, `.nvidia-module`, `.nvidia-params` | `health/drivers.rs` |
| Boot-time hardware change notice + notifier | `updates/samurai-hw-fingerprint.service`, `samurai-notifier` |
| `[hardware] cuda`, `--cuda-gpu`, both installers | planner, validator, wizard, `installer/gui` |
| Installer `auto` keeps nouveau for pre-Turing cards (bug fix) | `installer/samurai-install` |
| Daily health service can read `/usr/lib/modules` (bug fix) | `updates/samurai-health.service` |
| Toolbox Hardware page, `hw-apply`, `hw-apply-switch`, `hw-accept` | `toolbox/gui` |

## How it is verified
| What | Command | Level |
|------|---------|-------|
| Safety rules, parsing, restore paths, health checks | `cargo test` | fake System |
| Profile format, installer GPU rules, CUDA planner, GUI helper | `pytest tests/unit` | host |
| Profile selection on 12 machines | `ci/test-hw.sh` | **fixtures**: real chwd/lspci/pacman, fake sysfs |
| Real apply, 580xx -> open switch, failed switch restore, no db sync | `ci/test-hw.sh --e2e` | **fixture GPU**, real packages, DKMS build, container; no boot, no module load |
| QEMU profile, real snapshot, sandboxed daily health, GPU added/removed, notifier, root-only accept | `tests/vm/test_hw.py`: **2 passed** (11 min) on the v0.0.1.90 offline ISO upgraded to the branch packages (`SAMURAI_TEST_ISO_PREDATES_CUDA=1`, `SAMURAI_TEST_HW_PKGS=<chwd>:<hardware>`) | VM, no NVIDIA |
| GUI pages | `ci/test-gui.sh` | offscreen, fake CLI |

## Observed
- A second virtio GPU (class 0380) is offered only `fallback` by chwd (upstream's `qemu` matches
  0300): `detect` lists it as optional packages; no health issue.
- `drivers.profile` is not broken-and-repaired in the VM (a QEMU guest has no core package to miss);
  that path is covered by unit tests and the container runs.
- Suspected, **not verified**, not changed: `samurai-notifier.service` also sets
  `ProtectKernelModules=yes` and tests `/usr/lib/modules/$(uname -r)`; if systemd applies it to the
  user unit, the timer reports "reboot pending" permanently.

## Not verified
- **No real GPU.** No NVIDIA/AMD/Intel driver was loaded or rendered anywhere; PRIME laptops, early
  KMS, suspend services, `nvidia_drm` parameters on a real boot, CUDA installation: all untested.
- NVIDIA mkinitcpio drop-ins were generated in a container, never booted.
- Broadcom: profile selection only.
- Fixtures other than genorbox1 are synthetic; fan-dragon and stealth-dragon are not captured.
- Toolbox Hardware page in a Plasma VM (`test_toolbox_gui.py` was updated, not run).
- A branch ISO (offline closure with the new packages) was not built: the new `samurai-install` auto
  rule, wizard CUDA question and postinstall fingerprint/CUDA steps are unit-tested only.

## Not built
- gpu-unlock builder: ADR-0008 / spec 07 ask for upstream recipes **plus "our pinned fork list"** for
  Turing and non-4090 Ada; no fork, repository or commit is named anywhere, and how recipes are
  merged is undefined. Nothing was implemented.
- `samurai-hw-firstboot.service`, the installer's chwd step, `samurai hw gpu`, `diag gpu` chwd output,
  handheld/Framework/T2/Intel LPMD/AI SDK/fingerprint-reader profiles, weekly upstream profile diff.
- `samurai hw apply` does not install CUDA (installer only).

## Decisions needed
1. `samurai-chwd` name and the one-line `-Sy` patch (spec 07 said package `chwd`, unpatched): accept?
2. First boot: spec 07 wants `samurai-hw-firstboot.service` to re-run detection. Auto-installing
   drivers unattended was not built; today the daily health check and notifier report it.
3. gpu-unlock: name the pinned fork(s) and commits and how their recipes merge, or ship upstream
   recipes only.

## Merge notes
- Package bumps: samurai-tools 0.4.0-18, samurai-base 0.1.0-2, samurai-toolbox-gui 0.1.0-2; other
  lanes may bump the same packages: one bump after merging, then `ci/update-pkg-sums.py`.
- Two new packages must be in the `[samurai]` repo before an ISO of the merged branch installs online.
- Existing installs get the profiles through samurai-base's new dependency.

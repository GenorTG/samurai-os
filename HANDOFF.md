# HANDOFF — samurai-os

## Mission
Personal Arch-based distro: upstream Arch repos unchanged, CachyOS + Chaotic-AUR below Arch, signed Samurai repo, guided installer and CLI/toolbox. VM validation runs on genorbox1.

## State (2026-11-21)
- Specs 00–14 and ADR-0001…0008 remain the product source of truth. All 11 open questions answered (Genor, 2026-11-17).
- M0–M4 complete: Arch ISO identity; LUKS2/btrfs installer; update/notifier; rescue rollback; guided installer. See milestone notes below.
- M5.5 offline release install is green: full KDE + `linux-cachyos` + `linux-zen` package closure baked into ISO; installation succeeds with `arch_mirror=https://offline.invalid/no-such-mirror`, reboots, and verifies installed system plus boot artifacts.
- Driver coverage: base online installs select AMD/Intel/Nouveau stacks plus per-kernel NVIDIA open modules/userspace; the offline ISO bundles those stacks and the kernel-matched NVIDIA package alternatives, with the installer filtering to selected kernels. Custom gpu-unlock firmware/modules remain excluded under ADR-0008. Offline VM assertions cover installed GPU packages; no physical GPU hardware test has been run.
- Live updates: installed systems get `[samurai]` from signed `GenorTG/samurai-repo` GitHub Releases and use `samurai update` / pacman for whole-system transactions. Added `samurai-tools` to package the updater/CLI/notifier and made `samurai-release` depend on it so executor changes arrive in place. `.github/workflows/publish-testing-repo.yml` publishes signed package builds to `x86_64-testing` on main changes/manual dispatch; test installs opt in via `[samurai-testing]`. Workflow requires `samurai-testing` environment secrets `SAMURAI_SIGNING_SUBKEY` and `SAMURAI_REPO_TOKEN`; stable promotion remains separate. VM assertions cover installed repo URL/signature policy/test mirrorlist; update scenario asserts package ownership after the testing transaction.
- Update-route verification: `samurai-tools` built successfully in the Arch container (source checksums passed); install smoke against the existing f5 ISO passed **1 test in 541.30s**, including GitHub URL/signature and testing mirrorlist assertions. GitHub activation is not yet live: `samurai-testing` Actions environment returns 404 and `x86_64-testing` Release does not exist; configure the documented environment secrets, then first workflow publish bootstraps that release. No new ISO was built.
- GPU ISO rebuild attempt: offline closure resolved to 844 files (including NVIDIA, Radeon, Intel and Nouveau stacks), but mkarchiso was stopped after repeated upstream 404/DNS timeouts while syncing/installing the live ISO packages. Existing f5 ISO is unchanged and does not contain the new install closure; retry build when mirrors recover, then run `tests/vm/test_offline.py` against it.
- Latest verified ISO: `build/validation-2026.11.21f5/samurai-os-2026.11.21-x86_64.iso`, 3,466,493,952 bytes; SHA-256 `7b0807fe140c14564e01d718107d51ee1c79d8d95c69965d27b259ab51defd43`. Bootable ISO9660/MBR. Run `SAMURAI_TEST_ISO=<absolute ISO> SAMURAI_VM_RUN_ID=<unique-id> pytest -q -s tests/vm/test_offline.py`; verified run `20261121r6`: **1 passed in 358.18s**.
- Offline path uses `pacman --sysroot /mnt -U`, target-staged config and non-recursive binds for /dev,/proc,/sys,/run. Hooks may start a target-root GPG agent pinning /mnt/dev/null; kill it inside chroot before each teardown boundary. Require selected kernel + initramfs files before unmount. Details in AGENTS.md Gotchas.
- QGA is single-host-client: close GuestAgent before opening another or rebooting, or next guest-exec may connect but time out without a response.
- Working tree contains changes beyond this milestone; review `git status` before staging anything. No public push without Genor explicit OK.

## Next
1. M5: replace the Rust CLI shell shim; keep `updates/` shell scripts as executors until ported.
2. M6/M7: KDE + catalog packages, then the requested in-desktop visual/toolbox walkthrough.
3. Check Otto/Scratchy licences before M7a; validate gpu-unlock recipes on stealth-dragon 3090 before removing untested labels.

## Commands and build gotchas
- `bash scripts/install-hooks.sh` once per clone (per-commit BUILD bump).
- `ci/build-iso.sh <absolute-fresh-output-dir>`; Arch tooling only inside archlinux containers. Fresh profile excludes old generated pkg-cache, then injects exact current offline closure.
- VM artifacts live in ignored `.vm/`. Set both `SAMURAI_TEST_ISO` and unique `SAMURAI_VM_RUN_ID` to preserve evidence.
- pacstrap `-C` is resolution-only; copy live `pacman.conf` and `pacman.d` into target after install. For offline installs validate cached packages locally; do not use `pacman -S`.
- See `docs/specs/12-ROADMAP.md` for milestone sequencing.

## Milestones
- **M0 (2026-11-17):** archiso 92 releng profile customized; OVMF serial boot verified.
- **M1 (2026-11-17):** public source/data repos and signed rolling Samurai repo established; signing-key backup remains TODO.
- **M2 (2026-11-17):** smoke suite green: LUKS2, btrfs @ subvolume, auto-unlock, installed identity and QGA verification.
- **M3 (2026-11-18):** updater/notifier and archive update scenarios green; package mirror URLs must include `$repo/os/$arch`.
- **M4 (2026-11-20):** rollback and attended wizard scenarios green. VM shutdown uses ACPI powerdown before forced quit to flush filesystems.
- **M5.5 (2026-11-21):** offline KDE + multi-kernel install green as recorded above. Test asserts SDDM active, KDE packages/theme, no failed units, btrfs, generic linux absent, both selected kernels present, and both kernel/initramfs pairs under /boot.

## Durable product decisions
- Name Samurai OS; Qt6/QML toolbox; Breeze Dark default (Otto/Scratchy optional, licence check before use).
- Python versions per-version opt-in; Miniforge; Docker group-vs-rootless selected during install; AI CLIs opt-in; diagnostics manual opt-in.
- gpu-unlock includes fork recipes for RTX 20/non-4090 Ada, each labelled untested/potentially unstable; NVIDIA firmware/modules must never be published (ADR-0008).

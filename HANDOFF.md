# HANDOFF — samurai-os

## Mission
Personal Arch-based distro: upstream Arch repos unchanged, CachyOS below Arch, signed Samurai repo,
guided installer and CLI/toolbox. VM validation runs on genorbox1. Site: <https://genortg.github.io/samurai-os/>.

## Release candidate (2026-11-26): verified, NOT published
- `main` = defaults + graphical installer + toolbox GUI (merge ae9585e) plus fixes up to 973a42d
  (ISO/package sources). 39aba4c and later touch only tests and docs.
- ISO: `build/rc-offline/samurai-os-2026.11.26-offline-x86_64.iso`, 7,709,970,432 bytes, sha256
  `790775dc6c0accc16b5351b95cc9364634b655bc6ee8969a276a9273ef550f05`.
- Packages: `build/repo/x86_64`, 19 files, manifest `docs/releases/2026-11-26-rc-packages.sha256`;
  identical to the copies inside the ISO. samurai-tools **0.4.0-17**, samurai-toolbox-gui 0.1.0-1,
  keyring 20261117-4 / mirrorlist 1-4 / release 0.0.1-8 (published files reused), 14 new metas and
  repackages (base, desktop-*, settings-*, wine, proton-ge 11-14, python310-313, miniforge).
- **Publishing needs Genor's OK.** Publish these exact files (testing, then stable); a rebuild changes
  bytes. Order: repo first, ISOs second (online installs of this tree fail until packages are published).
  `publish-testing-repo.yml` ran on today's pushes and skipped every publish step (no secrets); once
  secrets exist, a package push to main publishes to testing by itself.

## Verification on that ISO (2026-11-26, each scenario one run, all PASS)
| scenario | cases | time |
|---|---|---|
| rollback (rescue + running system) | 2 | 12m27s |
| defaults (KDE default system) | 1 | 6m34s |
| installer_gui | 1 | 8m16s |
| offline (forced NVIDIA DKMS, dead mirror) | 1 | 14m08s |
| smoke | 2 | 4m17s |
| attended | 1 | 3m54s |
| toolbox_gui | 11 | 13m20s |
| health | 9 | 5m59s |
| update | 1 | 3m19s |
| daily (full update, app install, reboot) | 1 | 6m44s |
| kde | 1 | 5m39s |
| toolbox_pkg | 1 | 1m44s |
- Non-VM: unit 314 passed (2 shellcheck cases skipped on host, both pass in an Arch container);
  Rust 145 passed, clippy `-D warnings` + fmt clean; `ci/test-gui.sh` all pages OK (55 advisory
  qmllint warnings); repo no-shadowing / no-epoch OK for 19 packages.

## Fixed during integration
- c18e79e: `samurai snap rollback` works on a running system; both rollbacks restore every kernel to the ESP.
- 973a42d: `rescue rollback` leaves nothing mounted and syncs the ESP (power-off left a 0-byte kernel);
  accepts early-cpio initramfs; tools 0.4.0-17.
- VM tests: b567f26 (wait for Plasma splash), 973d1c2 (full update before installs), f28d445 (page switch).
- Light-theme bug fixed by the merged defaults (system `/etc/xdg/kdeglobals` `[KDE]` only).

## Unverified / gaps
- Not run: `test_channel` (needs published packages), `test_update_archive`. Online ISO of this tree not
  built. Upgrade of an installed 0.4.0-15 system to 0.4.0-17 not tested. `ci/release-iso.sh` split not run.
- VM only: no real hardware, no real NVIDIA GPU; installer keyboard layout verified for `us` only;
  CachyOS kernels, systemd-boot and GRUB covered by unit tests only.
- GUI not VM-exercised: fix-* repairs, mirrors refresh, boot param add/remove, apps upgrade-and-install,
  news acknowledgement.
- Existing installs do not receive the new defaults via `samurai update` (no `[chaotic-aur]`, nothing
  depends on the new metas): needs an update intervention.
- ADR-0009 (Qt installer instead of Calamares) needs Genor's OK. `site/index.html` still describes the
  released terminal installer and a Calamares plan: update it when this ships.

## Released (2026-11-25/26)
- Published stable + testing (`GenorTG/samurai-repo`): samurai-tools 0.4.0-15, keyring 20261117-4,
  mirrorlist 1-4, release 0.0.1-8. Testing db also uploaded as `samurai-testing.db`.
  `repo.smart-samurai.pl` fallback mirror: not checked/synced.
- ISOs: GitHub release `v0.0.1.75` (online + split offline, signed SHA256SUMS; carries 0.4.0-14 live tools).
  Local copies `build/rel-*-15`. `release-iso.sh` needs `gh` on PATH.
- Website: Pages from `pages.yml` on changes under `site/`; keep its claims true on each release.

## Map
- Specs `docs/specs/` and ADRs are the product truth. M0–M4 done.
- Two ISO flavors: `SAMURAI_ISO_FLAVOR=offline|online ci/build-iso.sh <fresh-abs-dir>`; unpublished
  packages: add `SAMURAI_LOCAL_PKGS=<repo dir>` (offline only). Offline bakes a local pacman repo.
- Package selection: `installer/samurai-profile-plan` only; schema `docs/plans/profile-schema.md`;
  inventory `docs/plans/defaults-inventory.md`. NVIDIA: one `NVIDIA-MODULE` provider per install.
- Live ISO boots Plasma (user `live`); `installer/gui` writes the profile, root helper runs `samurai-install`.
- `toolbox/`: Rust CLI (`samurai-core`, `samurai-cli`: status, health, diag, clean, fix, mirrors, boot, kernel);
  executors stay shell. `samurai apps` = `updates/samurai-apps` (never `-Sy`, catalog `catalog/`).
- GUI `toolbox/gui` (Kirigami): CLI `--json` only; `pkexec` helper allowlist, one polkit action per verb
  (`docs/reviews/toolbox-gui-threat-review.md`).
- Security default: firewalld public zone (dhcpv6-client; ssh only with ssh_server).
- **Root hardening rules (keep):** programs resolve only from `TRUSTED_PATH` with a scrubbed env; under
  sudo `diag bundle --out` stays inside the caller's home and user-file targets run via `System::run_as`;
  VM harness HTTP mirror binds 127.0.0.1.

## Next
1. Genor: OK to publish (testing → `test_channel` incl. 15→17 upgrade → stable), then build online +
   offline ISOs and release; update the site.
2. Intervention to bring existing installs to the new defaults.
3. Installer: Expert mode, geo-IP, size estimate, chwd/CUDA/gpu-unlock, manual/dual-boot disks,
   translations, NVIDIA live entry. Defaults: Plymouth/branding, Firefox policy (spec 09), welcome app.
4. CLI/GUI: more spec-05 checks, `fix audio|pacman|reinstall-all`, `hw`, `queue`, D-Bus helper.
5. Firewall presets, Secure Boot (sbctl), TPM2 unlock (spec 09, M8).
6. CI publish secrets (docs/KEYS.md, needs Genor); signing-key backup still TODO.

## Commands
- `bash scripts/install-hooks.sh` once per clone (per-commit BUILD bump).
- Arch tooling only inside archlinux containers; never edit a running script (AGENTS.md gotchas).
- `python3 -m pytest -q tests/unit`; `bash ci/test-gui.sh`; VM:
  `SAMURAI_TEST_ISO=<iso> SAMURAI_VM_RUN_ID=<id> pytest -q -s tests/vm/test_<x>.py` under
  `/tmp/samurai-vm.lock` (+ `SAMURAI_TEST_TOOLS_PKG` / `SAMURAI_TEST_GUI_PKG` where needed).

## Durable product decisions
- Name Samurai OS; Qt6/QML toolbox; Breeze Dark default (Otto/Scratchy optional, licence check first).
- Python versions per-version opt-in; Miniforge; Docker group-vs-rootless at install; AI CLIs opt-in.
- gpu-unlock firmware/modules never published (ADR-0008).

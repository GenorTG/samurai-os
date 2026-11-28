# HANDOFF — samurai-os

## Mission
Personal Arch-based distro: upstream Arch repos unchanged, CachyOS below Arch, signed Samurai repo,
guided installers and CLI/GUI toolbox. VM validation runs on genorbox1. Site: <https://genortg.github.io/samurai-os/>.

## Integrated on main (2026-11-28, UNRELEASED)
- Merged `feat/adopt-defaults`, `feat/hardware`, `feat/security` (branches and worktrees removed).
- New for the next release: `samurai adopt`, `samurai hw`, `samurai security|firewall|welcome`,
  Hardware + Welcome toolbox pages, CUDA toggle. Packages: new `samurai-chwd`, `samurai-hardware`,
  `samurai-settings-security`; bumped `samurai-tools` 0.4.0-18, `samurai-base` 0.1.0-2,
  `samurai-settings-kde` 0.1.0-2, `samurai-toolbox-gui` 0.1.0-2 (22 packages, 15 unchanged).
- Integration fixes: adopt adds the AppArmor `lsm=` parameter (previewed) and keeps a user's Firefox
  `policies.json`; Plasma Welcome Center no longer autostarts; notifier unit saw no kernel modules
  ("reboot pending" for ever) and failed on accounts without `~/.cache/samurai`.
- Decisions recorded in specs (`docs/specs/OPEN-QUESTIONS.md`, last section).
- Release records: `docs/releases/2026-11-28-verified.md` and `-new.md` (VM results are there).
- **The release ISOs must be built fresh from main after the repo is published**: the VM-tested
  offline ISO `build/int2-offline` predates the two notifier fixes. No online ISO was built.

## Released (2026-11-26): desktop release, owner-approved
- ISOs: GitHub release `v0.0.1.90` (latest, tag at e8e3c06): online ISO, offline ISO in 4 parts, signed
  `SHA256SUMS`. Asset digests match `SHA256SUMS`, signature verifies with the shipped build key.
  online `9ffde5be19c2fab39fe51576e23543b2548fb79ee0c3b4f64a2c83e5eb8a7158`,
  offline (joined) `84b803eaecd00eb7e7ce4be23340ce33597f42d2f306ba1d2b92b20cfde86d8e`.
- Packages: stable and testing (`GenorTG/samurai-repo`) carry the same 19 packages, including
  samurai-tools **0.4.0-17**; digests equal `docs/releases/2026-11-26-rc-packages.sha256`.
  `repo.smart-samurai.pl` fallback mirror: not checked/synced.
- Previous release `v0.0.1.75` stays on GitHub (carries 0.4.0-14 live tools).
- Local copies: `build/rel-online-16` (online ISO); the released offline ISO is on GitHub only.
  `build/repo` holds the unpublished 22-package build; published file names must keep their bytes.
- Website: Pages from `pages.yml` on changes under `site/`; keep its claims true on each release.

## Verification of v0.0.1.90 (QEMU/KVM, each scenario one run, all PASS)
- Final offline ISO: defaults, installer_gui, offline (forced NVIDIA DKMS, dead mirror).
- Final online ISO: smoke, attended, update, rollback (rescue + running system), daily, kde.
- Published stable repo: `test_channel`, installing from the `v0.0.1.75` online ISO and upgrading to
  0.4.0-17 (10 scenarios in the release gate, counting this one).
- Release candidate only (same Samurai packages, earlier ISO; NOT rerun on the final ISOs): toolbox_gui
  (11 cases), health (9), toolbox_pkg (1).
- Non-VM, at the release candidate: unit 314 passed (2 shellcheck cases skipped on host, pass in an Arch
  container); Rust 145 passed, clippy `-D warnings` + fmt clean; `ci/test-gui.sh` all pages OK (55
  advisory qmllint warnings); repo no-shadowing / no-epoch OK.

## Open / unverified
- VM only: no real hardware, TPM or NVIDIA GPU, no game launched; installer keyboard layout verified
  for `us` only; CachyOS kernel switching, systemd-boot and GRUB covered by unit tests only.
- `nl.cachyos.org` no longer resolves (2026-11-28). v0.0.1.75 installs list it first, newer configs
  last in `installer/pacman.conf.target` and `iso/pacman.conf`: consider removing it.
- Offline ISO build is slow because mksquashfs re-compresses the 5.5 GB package cache with xz.
- Not run: `test_update_archive`.
- GUI not VM-exercised: fix-* repairs, mirrors refresh, boot param add/remove, apps upgrade-and-install,
  news acknowledgement.
- Never re-run `ci/release-iso.sh --publish` for a published tag after a commit.
- `publish-testing-repo.yml` skips publishing (no secrets); once secrets exist, a package push to main
  publishes to testing by itself.

## Fixed in v0.0.1.90
- `samurai snap rollback` works on a running system; both rollbacks restore every kernel to the ESP.
- `rescue rollback` leaves nothing mounted and syncs the ESP (power-off left a 0-byte kernel); accepts
  early-cpio initramfs.
- Light theme instead of Breeze Dark (system `/etc/xdg/kdeglobals` `[KDE]` only).

## Map
- Specs `docs/specs/` and ADRs are the product truth. M0–M4 done.
- Two ISO flavors: `SAMURAI_ISO_FLAVOR=offline|online ci/build-iso.sh <fresh-abs-dir>`; unpublished
  packages: add `SAMURAI_LOCAL_PKGS=<repo dir>` (offline only). Offline bakes a local pacman repo.
- Release: `ci/release-iso.sh <online> <offline> --sign --publish` (needs `gh` on PATH). Order: repo
  first, ISOs second.
- Package selection: `installer/samurai-profile-plan` only; schema `docs/plans/profile-schema.md`;
  inventory `docs/plans/defaults-inventory.md`. NVIDIA: one `NVIDIA-MODULE` provider per install.
- Live ISO boots Plasma (user `live`); `installer/gui` writes the profile, root helper runs `samurai-install`.
- `toolbox/`: Rust CLI (`samurai-core`, `samurai-cli`: status, health, diag, clean, fix, mirrors, boot,
  kernel, hw, security, firewall, welcome); executors stay shell (`adopt` is Python).
  `samurai apps` = `updates/samurai-apps` (never `-Sy`, catalog `catalog/`).
- GUI `toolbox/gui` (Kirigami): CLI `--json` only; `pkexec` helper allowlist, one polkit action per verb
  (`docs/reviews/toolbox-gui-threat-review.md`).
- Security default: firewalld public zone (dhcpv6-client; ssh only with ssh_server).
- **Root hardening rules (keep):** programs resolve only from `TRUSTED_PATH` with a scrubbed env; under
  sudo `diag bundle --out` stays inside the caller's home and user-file targets run via `System::run_as`;
  VM harness HTTP mirror binds 127.0.0.1.

## Next
1. Release (owner OK): publish the 22 packages (`build/repo`, repo first), build online + offline
   ISOs from main, rerun smoke/attended/update/test_channel on them, release with the notes files.
2. Installer: Expert mode, geo-IP, size estimate, chwd step, manual/dual-boot disks, translations,
   NVIDIA live entry. Defaults: Plymouth/branding.
3. CLI/GUI: more spec-05 checks, `fix audio|pacman|reinstall-all`, `queue`, D-Bus helper,
   `firewall watch`.
4. Secure Boot (`docs/security/secure-boot.md`), gpu-unlock (deferred, spec 07).
- Signing key backup: Vaultwarden, restore-tested 2026-11-27 (docs/KEYS.md).

## Commands
- `bash scripts/install-hooks.sh` once per clone (per-commit BUILD bump).
- Arch tooling only inside archlinux containers; never edit a running script (AGENTS.md gotchas).
- `python3 -m pytest -q tests/unit`; `bash ci/test-gui.sh`; `bash ci/test-hw.sh [--e2e]`; VM:
  `SAMURAI_TEST_ISO=<iso> SAMURAI_VM_RUN_ID=<id> pytest -q -s tests/vm/test_<x>.py` under
  `/tmp/samurai-vm.lock`. Host has no cargo: `.tmp/cargo.sh` (container).
- Integration suite (local, `.tmp/`): `int2.sh` (repo + offline ISO), `int2-vm.sh <run> <scenario>...`
  (sets every `SAMURAI_TEST_*` variable from `build/int-pkgs`, `build/int-secpkgs`).
- `test_adopt` needs the v0.0.1.75 online ISO, `SAMURAI_TEST_PROFILE_REF=v0.0.1.75`, the new tools
  package and `SAMURAI_TEST_EXTRA_PKGS` (the other unpublished packages).
- `test_channel` from an old release: `SAMURAI_TEST_ISO=<old online iso>`, `SAMURAI_TEST_CHANNEL=stable`,
  `SAMURAI_TEST_TOOLS_VERSION=<new>`, `SAMURAI_TEST_PROFILE_REF=<old tag>` (download the old ISO from its
  release).

## Durable product decisions
- Name Samurai OS; Qt6/QML toolbox; Breeze Dark default (Otto/Scratchy optional, licence check first).
- Python versions per-version opt-in; Miniforge; Docker group-vs-rootless at install; AI CLIs opt-in.
- gpu-unlock firmware/modules never published (ADR-0008).

# HANDOFF — samurai-os

## Mission
Personal Arch-based distro: upstream Arch repos unchanged, CachyOS below Arch, signed Samurai repo,
guided installers and CLI/GUI toolbox. VM validation runs on genorbox1. Site: <https://genortg.github.io/samurai-os/>.

## Lane F security (branch `feat/security`, unpublished, 2026-11-28)
- Built: `samurai-settings-security` 0.1.0 (in `BASE_PACKAGES`; installer adds `lsm=...apparmor...`),
  `samurai security status|tpm enroll/remove|recovery-key`, `samurai firewall`, `samurai welcome` +
  toolbox Welcome page and login autostart. samurai-tools 0.4.0-18, samurai-toolbox-gui 0.1.0-2.
- Docs: `docs/security/hardening.md` (breakage notes, TPM recovery table), `docs/security/secure-boot.md`.
- Verified (QEMU, all PASS): `test_security.py` (existing install), `test_security_tpm.py` (swtpm: PIN,
  forgotten PIN, replaced TPM, removal, recovery key), `test_security_install.py` (new KDE install:
  release offline ISO 2026-11-26 with the branch's packages and installer scripts injected). Unit 320
  passed 3 skipped; Rust 172, clippy + fmt clean; `ci/test-gui.sh` OK. After the VM runs only the TPM
  enrolment message changed.
- Not verified: an ISO built from this branch, real hardware/TPM, Steam or a Proton game (userns,
  bubblewrap, Flatpak, Firefox sandbox and system Wine were run), `toolbox_gui` VM tests (changed only
  to pre-mark the welcome window), `firewall zone` on a NetworkManager-managed interface.
- **No boot entry has a file hash**: Limine verifies nothing today (spec 09 / ADR-0003 claim it does).
  Secure Boot setup is deliberately not built.
- Release: publish settings-security + tools + toolbox-gui before any online ISO of this branch.
  A user's own `/etc/firefox/policies/policies.json` makes the first package install conflict.
- Open: KDE's Welcome Center opens on top of `samurai welcome` at first login. Existing installs need
  the package (lane D) plus `samurai boot set param add lsm=...`.
- VM env: `SAMURAI_TEST_SEC_PKGS=<ci/build-pkgs.sh output>`, optional `SAMURAI_TEST_BASE_DISK`.

## Released (2026-11-26): desktop release, owner-approved
- ISOs: GitHub release `v0.0.1.90` (latest, tag at e8e3c06): online ISO, offline ISO in 4 parts, signed
  `SHA256SUMS`. Asset digests match `SHA256SUMS`, signature verifies with the shipped build key.
  online `9ffde5be19c2fab39fe51576e23543b2548fb79ee0c3b4f64a2c83e5eb8a7158`,
  offline (joined) `84b803eaecd00eb7e7ce4be23340ce33597f42d2f306ba1d2b92b20cfde86d8e`.
- Packages: stable and testing (`GenorTG/samurai-repo`) carry the same 19 packages, including
  samurai-tools **0.4.0-17**; digests equal `docs/releases/2026-11-26-rc-packages.sha256`.
  `repo.smart-samurai.pl` fallback mirror: not checked/synced.
- Previous release `v0.0.1.75` stays on GitHub (carries 0.4.0-14 live tools).
- Local copies: `build/rel-online-16`, `build/rel-offline-16` (ISOs only). Keep `build/repo`: published
  file names must keep their bytes.
- Website: Pages from `pages.yml` on changes under `site/`; keep its claims true on each release.

## Verification (QEMU/KVM, each scenario one run, all PASS)
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
- Existing installs do not receive the new defaults via `samurai update` (no `[chaotic-aur]`, nothing
  depends on the new metas): needs an update intervention.
- ArtCraft: catalog entry `craft-launcher` points at `artcraft-launcher-appimage`, which no repo has
  (shown as not available).
- ADR-0009 (Qt installer instead of Calamares): shipped; default is accepted unless the owner objects.
  The ADR file still says "proposed".
- VM only: no real hardware, no real NVIDIA GPU, no game or Windows program launched; installer keyboard
  layout verified for `us` only; CachyOS kernel switching, systemd-boot and GRUB covered by unit tests only.
- Not run: `test_update_archive`.
- GUI not VM-exercised: fix-* repairs, mirrors refresh, boot param add/remove, apps upgrade-and-install,
  news acknowledgement.
- `ci/release-iso.sh` hard-codes the notes' "New" and "Verified" text; v0.0.1.90's Verified section was
  corrected by hand. Update the template before the next release (and never re-run it for a published tag
  after a commit).
- `publish-testing-repo.yml` skips publishing (no secrets); once secrets exist, a package push to main
  publishes to testing by itself.

## Fixed in this release
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
- `toolbox/`: Rust CLI (`samurai-core`, `samurai-cli`: status, health, diag, clean, fix, mirrors, boot, kernel);
  executors stay shell. `samurai apps` = `updates/samurai-apps` (never `-Sy`, catalog `catalog/`).
- GUI `toolbox/gui` (Kirigami): CLI `--json` only; `pkexec` helper allowlist, one polkit action per verb
  (`docs/reviews/toolbox-gui-threat-review.md`).
- Security default: firewalld public zone (dhcpv6-client; ssh only with ssh_server).
- **Root hardening rules (keep):** programs resolve only from `TRUSTED_PATH` with a scrubbed env; under
  sudo `diag bundle --out` stays inside the caller's home and user-file targets run via `System::run_as`;
  VM harness HTTP mirror binds 127.0.0.1.

## Next
1. Intervention to bring existing installs to the new defaults.
2. Installer: Expert mode, geo-IP, size estimate, chwd/CUDA/gpu-unlock, manual/dual-boot disks,
   translations, NVIDIA live entry. Defaults: Plymouth/branding, Firefox policy (spec 09), welcome app.
3. CLI/GUI: more spec-05 checks, `fix audio|pacman|reinstall-all`, `hw`, `queue`, D-Bus helper.
4. Firewall presets, Secure Boot (sbctl), TPM2 unlock (spec 09, M8).
- Signing key backup: Vaultwarden, restore-tested 2026-11-27 (docs/KEYS.md).

## Commands
- `bash scripts/install-hooks.sh` once per clone (per-commit BUILD bump).
- Arch tooling only inside archlinux containers; never edit a running script (AGENTS.md gotchas).
- `python3 -m pytest -q tests/unit`; `bash ci/test-gui.sh`; VM:
  `SAMURAI_TEST_ISO=<iso> SAMURAI_VM_RUN_ID=<id> pytest -q -s tests/vm/test_<x>.py` under
  `/tmp/samurai-vm.lock` (+ `SAMURAI_TEST_TOOLS_PKG` / `SAMURAI_TEST_GUI_PKG` where needed).
- `test_channel` from an old release: `SAMURAI_TEST_ISO=<old online iso>`, `SAMURAI_TEST_CHANNEL=stable`,
  `SAMURAI_TEST_TOOLS_VERSION=<new>`, `SAMURAI_TEST_PROFILE_REF=<old tag>` (download the old ISO from its
  release).

## Durable product decisions
- Name Samurai OS; Qt6/QML toolbox; Breeze Dark default (Otto/Scratchy optional, licence check first).
- Python versions per-version opt-in; Miniforge; Docker group-vs-rootless at install; AI CLIs opt-in.
- gpu-unlock firmware/modules never published (ADR-0008).

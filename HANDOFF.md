# HANDOFF — samurai-os

## Mission
Personal Arch-based distro: upstream Arch repos unchanged, CachyOS below Arch, signed Samurai repo,
guided installer and CLI/toolbox. VM validation runs on genorbox1. Site: <https://genortg.github.io/samurai-os/>.

## Released (2026-11-25)
- Packages: `x86_64` (stable) and `x86_64-testing` on `GenorTG/samurai-repo`, built from 562ee45+ and published
  locally with `ci/build-repo.sh build/repo` + `ci/publish-repo.sh build/repo <channel>` (stable = the exact
  files tested on testing): samurai-tools 0.4.0-14, keyring 20261117-4, mirrorlist 1-4, release 0.0.1-8.
  Genor OK'd publishing 2026-11-25. Testing db is also uploaded as `samurai-testing.db` (pacman section name).
  `repo.smart-samurai.pl` fallback mirror: not checked/synced.
- ISOs: GitHub release `v0.0.1.75` (online + split offline, signed SHA256SUMS), built as `build/rel-*-15`.
  `release-iso.sh` needs `gh` on PATH (pass `--setenv=PATH` under systemd-run).
  All 4 ISO asset digests match the signed SHA256SUMS (checked via the GitHub API).
- Website live (Genor OK 2026-11-26): <https://genortg.github.io/samurai-os/>, Pages from `pages.yml` on changes under
  `site/`. Keep its claims true (feature list, `samurai help`, VM test count, signing) whenever a release ships.

- 2026-11-26: samurai-tools **0.4.0-15** (adds `boot get/set`) on testing (VM upgrade 14->15 passed) and stable;
  base packages unchanged (published files reused). ISO release v0.0.1.75 still carries 0.4.0-14 live tools.

## Defaults lane (branch `feat/defaults`, 2026-11-26; pushed, not merged, not published)
- New packages: `samurai-base`, `-desktop-base`, `-desktop-kde`, `-settings-{system,shell,dev,kde}`,
  `-wine`, `-proton-ge` (11-14), `-python310…313`, `-miniforge`. Upstream checksums pinned.
  `ci/build-packages.sh` is shared by `build-repo.sh` (signed) and `build-pkgs.sh` (unsigned tests).
- Profile schema `docs/plans/profile-schema.md`; selection logic only in `installer/samurai-profile-plan`;
  one pacman transaction in postinstall; Chaotic-AUR enabled with pinned key; offline ISO ≈6.7 GB
  (was ≈4.6). Inventory: `docs/plans/defaults-inventory.md`.
- Light-theme bug (seen by the GUI lane): main's postinstall puts `LookAndFeelPackage` under
  `[General]` in skel; KDE reads `[KDE]`. Fixed here (system `/etc/xdg/kdeglobals` only). **main and
  `feat/gui` still carry the bug** until this branch is merged; on conflict keep this postinstall.
- Verified 2026-11-26 on `build/defaults-2` offline ISO (HEAD 260487d, sha256 `f6dd45e4…`):
  unit 184 passed, 2 skipped (no shellcheck on host); `test_defaults.py` PASS (12m49s, no package overlay, ColorScheme=BreezeDark);
  `test_attended.py` PASS (4m18s). Not re-run on this branch: smoke, update, rollback, daily, kde, offline,
  channel, health, toolbox_pkg, update_archive; online flavor not buildable before publishing.
- **Release order (required):** bump `samurai-tools` pkgrel (catalog changed), publish the repo, then
  build ISOs. Until the new packages are published, online installs of this branch fail.
- Existing installs do not get the new defaults by `samurai update`: they lack `[chaotic-aur]` and
  nothing depends on the new metas (needs an update intervention).
- Gaps: Otto/Scratchy (no licence on KDE Store), Miniconda (Anaconda terms), ArtCraft launcher (brand
  licence), gpu-unlock/chwd/CUDA toggle, Plymouth + samurai-branding, Firefox policy/security
  settings (spec 09), welcome app, version-bump CI for repackages, `samurai-gaming` meta (not made:
  the installer installs items individually), Heroic system-wide Proton unverified, Aider/Cursor CLI
  are first-boot notes only.

## State (2026-11-22)
- Specs 00–14 and ADR-0001…0008 remain the product source of truth.
- M0–M4 complete (ISO identity; LUKS2/btrfs installer; update/notifier; rescue rollback; guided installer).
- **Two ISO flavors** (`SAMURAI_ISO_FLAVOR=offline|online ci/build-iso.sh <fresh-abs-dir>`), renamed
  `samurai-os-<date>-<flavor>-x86_64.iso`. Online ≈1.3 GB (netinstall). Offline bakes a **local pacman
  repo** (`/usr/share/samurai/pkg-cache/samurai-offline.db` + packages, `repo-add` at build time).
- **Offline install resolves** (`pacman --sysroot /mnt -S` against `[samurai-offline]`, cache bound
  read-only into the target at `/var/cache/samurai-offline`) instead of `-U`-ing the whole cache.
  postinstall installs through `pac` (= pacman with `SAMURAI_PACMAN_CONF`), so online and offline use
  the same package sets. Installer removes the offline conf/db/bind after postinstall.
- **GPU**: Mesa/Intel stacks always; NVIDIA only when `lspci` shows vendor 10de class 0300/0302 or
  `[hardware] nvidia = "yes"`. Module providers conflict: prebuilt only when every kernel has one,
  else `nvidia-open-dkms` for all. Headers for every selected kernel are installed.
  `linux-cachyos-headers` needs CachyOS's newer `pahole` → closure lists `cachyos/pahole`.
- **`samurai apps`** (`updates/samurai-apps`, stdlib Python): list/search/info/install/remove/bundles/check,
  `--json`; merged catalogs at `/usr/share/samurai/catalog/`; never `-Sy`; bundles @gaming @dev
  @creative @office @media @streaming. Profile `[packages] apps = [...]` validated against the
  catalog; online installs apply them, offline installs write a first-boot note. 51 unit tests in
  `tests/unit/`. Packaged in samurai-tools 0.2.0 (`ci/build-repo.sh` stages it + catalogs).
- **Security default**: firewalld on (zone public, only dhcpv6-client; ssh only with ssh_server).
- KDE day-one set adds firefox, PipeWire, spectacle, noto fonts/emoji, flatpak, power-profiles-daemon,
  bluez. `[multilib]` enabled in `iso/pacman.conf` (copied to targets).
- NetworkManager desktops no longer also enable systemd-networkd.
- Release tooling: `ci/release-iso.sh <online.iso> <offline.iso> [--publish]` → `dist/v<VERSION>/`
  (offline split in 1900 MiB parts, SHA256SUMS incl. joined ISO, notes; `--publish` tags + uploads).
- Site: `site/` (washi look from Krzy-Kut website), deployed by `.github/workflows/pages.yml`.
- New VM scenario `tests/vm/test_daily.py`: Plasma autologin session, Dolphin/Konsole/Kate,
  ark zip round trip, network/HTTPS, firewall, PipeWire, snapshot, full `samurai update`, app
  install, reboot with zero failed units. `test_offline.py` forces NVIDIA and asserts DKMS for both kernels.

## Verification status
- r15 ISOs + published stable repo (2026-11-25): live-ISO CLI, published-channel update (testing and stable,
  `tests/vm/test_channel.py`), smoke, update, rollback, daily, attended, kde, offline PASS.
- r15 offline FAILED once under host load ~11-25 (no login 300 s after the post-install reboot; squashfs
  errors in the log are just the ejected live disc); cause not captured. Then 2/2 PASS at lower load. The test
  now waits 600 s and screendumps `offline-boot-failed` if it recurs. Never run a QMP watcher beside the
  harness: QMP takes one client and `makefile()` keeps it open (blocked a run).
- r14 ISOs (installer ee802dd, 2026-11-24): smoke, update, rollback, attended, daily, kde, offline PASS (`.tmp/regress.sh`).
- r13 ISOs (installer 7573e08): offline, daily, kde, attended, update, rollback, smoke, archive PASS.
- smoke: PASS (2 passed, 8m12s) after the mirrorlist assertion fix. All r13 scenarios are now green.
- archive (test_update_archive, Arch 2026/08/15 snapshot): PASS on re-run (14m16s). Earlier rc=1 did not reproduce (likely transient archive/mirror fetch); test now keeps /tmp/install.log and dumps its tail on failure.

## Graphical installer (feat/installer-gui, 2026-11-26)
- Live ISO boots Plasma (user `live`, SDDM autologin, NetworkManager+iwd; tty2/serial keep root autologin;
  boot entry "terminal only"). Desktop icons: graphical install, terminal install, rescue.
- `installer/gui` (PySide6/QML, ADR-0009, needs Genor's OK: replaces Calamares) writes only the profile;
  root helper runs `samurai-install --profile --yes`, passphrase on stdin. Live files in
  `/usr/lib/samurai-installer/` (copied by `ci/build-iso.sh`).
- Backend changes: secrets read verbatim (`IFS= read`); desktop installs now set the login-screen/Plasma
  keyboard layout from the console keymap.
- Tests: `tests/unit/test_gui_installer.py`, `tests/gui/*.py` (Arch container), `tests/vm/test_installer_gui.py`.
- Not done: Expert mode, geo-IP, size estimate, chwd/CUDA/gpu-unlock page, alongside/manual disk layouts,
  UI translation, NVIDIA live boot entry, `sshd` toggle in a live menu.

## Next
1. M5 (Rust CLI): `toolbox/` workspace (`samurai-core` lib, `samurai-cli` bin, std-only; 141 tests, clippy+fmt clean). samurai-tools **0.4.0-15** is `arch=x86_64`, `build()` runs cargo (build-repo.sh stages `toolbox/`, installs `rust`), `options=(!debug)`; `/usr/local/bin/samurai` is the Rust binary, executors stay shell. Native in Rust: dispatcher, `status`, `health`, `diag`, `clean`, `fix`, `mirrors`, `boot sync`, `kernel`. Shared `health::system::System` trait (read/exists/list_dir/run) + `Fake` for unit tests; all commands are fixed-argv (no shell).
   - `health` (spec 05): 25 checks in `health/checks.rs` (newest: pacman.mirrors, packages.outdated, packages.partial-upgrade, security.ssh, boot.esp-space); `--category --id --json --list --write --fix [--yes]`; exit 0/1/2. `--write` -> `/var/lib/samurai/health.json` via sandboxed `samurai-health.service` + daily timer (package `.install` enables it); notifier counts warning+ from it (field order severity/fix_risk/status is a contract). `--fix` (root): snapper snapshot first, safe fixes direct, needs-confirm prompt/`--yes`; fixes: firewall, time-sync, snapper cleanup timer (spec 06: timeline OFF), initramfs, btrfs timers.
   - `diag` (spec 05): `info services logs disks gpu bundle`. `bundle` = everything + health json + package lists + pacman.conf + boot config, redacted by default (`diag/redact.rs`: users, hostname, MACs, IPv4/6, emails, SSIDs, serials, keyed secrets, bearer/JWT/32+ char tokens, PEM keys; stable placeholders; UTF-8 safe; `PUBLIC_NAMES` like `samurai`/`admin` are never redacted), file mode 0600, never overwrites, owned by `$SUDO_USER`; `--no-redact` needs a tty + two typed confirmations. Not done: `--upload` (needs paste host, Q7).
   - `clean` (spec 04): `pkgcache journal coredumps thumbnails` (= `all`, safe only) + `orphans`, `cache` (always ask; never in `all`); `--preview [--json] --yes --keep N`; user paths from /etc/passwd, never raw env. Not done: flatpak, docker prune, old snapshots.
   - `fix` (spec 04): `lock` (refuses while a package manager runs), `keyring [--reset]` (reset asks, snapshot first), `dkms`.
   - `mirrors status|refresh` (reflector, now a samurai-tools dependency): validated https-only list, `.bak` kept, never `-Sy`. Not done: `--all` (CachyOS/Chaotic lists). `tests/advice.rs` fails if any message suggests a `samurai` command that does not parse; `tests/unit/test_banner_commands.py` does the same for the welcome banners (fixed: they advertised `samurai toolbox`; next ISO carries it).
   - `boot sync [--check]` (`boot/mod.rs`) + hook `updates/95-samurai-boot.hook` (PostTransaction on kernel install/remove) + check `boot.entries` (fix): Limine, systemd-boot, GRUB. Touches only our entries, copies the existing cmdline, never empties the menu, keeps `limine.conf.bak`; if our default entry is removed one of OURS takes first place. Boot checks report "skipped" for non-root (ESP is 0700; was a false critical). The live ISO's tool copy ships no hook: it arrives with the package.
   - `kernel list|install|remove|default` (`kernel/mod.rs`): fixed catalog of 6; pacman only, never `-Sy`; headers when dkms (+`cachyos/pahole`); warns before adding a kernel to a prebuilt-NVIDIA system; refuses to remove the running or last bootable kernel; `default` for Limine/systemd-boot (not GRUB). Not VM-tested: CachyOS kernels, NVIDIA, systemd-boot/GRUB (unit tests only).
   - `boot get` / `boot set timeout|param add|remove` (`boot/settings.rs`): Limine + systemd-boot (GRUB: read-only message), only our entries, `.bak` first, root/unlock parameters refused. VM-verified on Limine incl. reboot (`/proc/cmdline`); 0.4.0-15, 9 passed 12m25s.
   - **Root hardening rules (keep them):** `RealSystem` resolves programs only from `TRUSTED_PATH` with a scrubbed environment (never the caller's `$PATH`); under sudo, `diag bundle --out` must resolve inside the caller's home; under sudo, user-file targets run via `System::run_as` (uid/gid of the caller; std drops supplementary groups). VM harness HTTP mirror binds 127.0.0.1 only.
   - Installer enables `btrfs-scrub@-.timer` (spec 06); released in v0.0.1.75. The live ISO now builds and ships the Rust CLI.
   VM-verified on 0.4.0-14 (`test_health.py` 9 passed, 9m43s) (full install, then `pacman -U` of the local pkg): `tests/vm/test_toolbox_pkg.py` (live ISO) + `tests/vm/test_health.py` (9 tests; also fix lock/keyring/dkms, age checks, sudo hardening, mirrors, kernel swap + reboot: baseline, break/detect/repair, timer+sandbox, notifier, `--fix` snapshot-first, initramfs rebuild, btrfs timers, diag with planted user/IP/MAC/email/password, clean incl. orphan confirmation). Run: `SAMURAI_TEST_TOOLS_PKG=<pkg> SAMURAI_TEST_ISO=<iso> SAMURAI_VM_RUN_ID=<id> pytest -q -s tests/vm/test_health.py`.
   Not done: needs-confirm health fixes, ~15 more spec-05 checks, `fix audio|pacman|reinstall-all`, `hw`, `queue`, helper+polkit (D-Bus), GUI.
2. Qt toolbox GUI as a client of `samurai apps --json` / `samurai update --json`.
3. Firewall presets (KDE Connect, Steam, Sunshine), Secure Boot (sbctl), TPM2 unlock (spec 09, M8).
4. Optional automation: `samurai-testing` environment secrets (signing subkey + fine-grained repo token,
   docs/KEYS.md) so `publish-testing-repo.yml` publishes on push; needs Genor. Until then publish locally.
5. Signing-key backup (docs/KEYS.md) still TODO.

## Commands and build gotchas
- `bash scripts/install-hooks.sh` once per clone (per-commit BUILD bump).
- Arch tooling only inside archlinux containers. Never edit a script while it runs (see AGENTS.md).
- VM artifacts in ignored `.vm/`; set `SAMURAI_TEST_ISO` + unique `SAMURAI_VM_RUN_ID`.
- `python3 -m pytest -q tests/unit` (fast); VM: `pytest -q -s tests/vm/test_<x>.py`.

## Durable product decisions
- Name Samurai OS; Qt6/QML toolbox; Breeze Dark default (Otto/Scratchy optional, licence check first).
- Python versions per-version opt-in; Miniforge; Docker group-vs-rootless at install; AI CLIs opt-in.
- gpu-unlock firmware/modules never published (ADR-0008).

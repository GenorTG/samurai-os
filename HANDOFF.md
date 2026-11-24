# HANDOFF — samurai-os

## Mission
Personal Arch-based distro: upstream Arch repos unchanged, CachyOS below Arch, signed Samurai repo,
guided installer and CLI/toolbox. VM validation runs on genorbox1. Site: <https://genortg.github.io/samurai-os/>.

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
- r13 ISOs (installer 7573e08): offline, daily, kde, attended, update, rollback PASS.
- smoke: only failure was a stale assertion (expected repo URL in pacman.conf; it now lives in samurai-mirrorlist) - test fixed, needs re-run.
- archive (test_update_archive, Arch 2026/08/15 snapshot): install rc=1, cause unknown (installer output goes to /dev/null) - TODO: capture install log (tee to /tmp/install.log, dump on failure) and re-run.

## Next
1. M5: replace the shell `samurai` dispatcher with the Rust CLI; keep executors.
2. Qt toolbox GUI as a client of `samurai apps --json` / `samurai update --json`.
3. Firewall presets (KDE Connect, Steam, Sunshine), Secure Boot (sbctl), TPM2 unlock (spec 09, M8).
4. Configure `samurai-testing` environment secrets so `publish-testing-repo.yml` can publish
   samurai-tools 0.2.0 (until then installed systems carry the ISO's tool copies).
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

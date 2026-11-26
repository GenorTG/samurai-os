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
- Retention (Genor 2026-11-25): keep only the newest ISO per flavor in `build/`; delete superseded builds, their work/profile/offline-pkgs dirs, VM disks and old test logs as new ones roll out. No need to ask.

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
- `iso/pacman.conf` configures mkarchiso package resolution, not the live ISO's runtime pacman; mirror it into `iso/airootfs/etc/pacman.conf` when live installs need extra repos. Offline `pacman -U` targets have no sync DBs, so postinstall must verify cached packages locally rather than use `pacman -S`.
- pacstrap's `-C` config is for resolving packages only; copy the live `pacman.conf` and `pacman.d` into `/mnt` after either online or offline install so CachyOS/Samurai remain enabled on the installed system.
- Build ISO packages only from a fresh staged profile: exclude the prior generated `pkg-cache` when copying `iso/`, then inject the current output's exact offline closure; otherwise old versions accumulate and pacman `-U` reports duplicate targets.
- Arch's current KDE editor package is `kate` (not `kwrite`); keep offline package checks keyed to package names actually present in the baked closure.
- Set `SAMURAI_VM_RUN_ID` when rerunning VM scenarios to namespace qcow2, serial logs, screendumps, and generated profiles instead of overwriting prior evidence.
- Set `SAMURAI_TEST_ISO` to run the VM suite against an isolated-build ISO without copying or renaming the image under `build/`.
- VM serial waits should fail immediately on pacstrap's explicit install errors as well as `FATAL`; otherwise a known failed install burns the full 40-minute scenario timeout.
- For package transactions against an already mounted target, use pacman's `--sysroot`, not `--root`; `--root` relocates files but runs hooks against the live ISO, leaving target kernel/initramfs artifacts ungenerated. Stage `--config` inside the sysroot too: `--sysroot /mnt` resolves `/tmp/foo.conf` as `/mnt/tmp/foo.conf` (pacman(8), `--root` / `--sysroot`, https://man.archlinux.org/man/pacman.8.en).
- Before offline `pacman --sysroot` transactions, bind `/dev`, `/proc`, `/sys`, and `/run` into the target; hooks otherwise fail on `/dev/stdin` and procfs while pacman can still return success. Use non-recursive binds: recursive binds clone busy live-ISO mounts. Pacman hooks may start a target-root `gpg-agent` holding `/mnt/dev/null`; stop it with `chroot /mnt gpgconf --homedir /etc/pacman.d/gnupg --kill gpg-agent` before unmounting. Assert selected `/boot` kernel/initramfs files before unmounting.
- Never edit `ci/build-iso.sh` (or any running script) while a build runs: bash reads scripts incrementally, so the running build executes shifted bytes (2026-11-22: bogus `s: unbound variable` after mkarchiso, rename step skipped).
- NVIDIA kernel-module packages all provide `NVIDIA-MODULE` and conflict: one provider per install. Prebuilt modules only when every selected kernel has one; otherwise `nvidia-open-dkms` for all (2026-11-22: cachyos-prebuilt + zen-dkms made the offline `-U` unresolvable).
- The VM harness QGA chardev accepts one host client. Close each `GuestAgent` before opening another or rebooting; otherwise the next guest-exec can connect but never receive a response.
- `linux-cachyos-headers` depends on CachyOS's `pahole` (newer pkgrel than extra's); with CachyOS below Arch, plain `pahole` resolves to extra and the transaction fails. Install headers only for DKMS, and pin `cachyos/pahole` on the online path; the offline repo carries the CachyOS build.
- `samurai-tools` is x86_64 since 0.4.0 (Rust CLI built in `build()`); build Rust only inside the archlinux container (`ci/build-repo.sh` installs `rust`). Its `.install` legacy cleanup cannot help first installs: pacman checks file conflicts before `pre_install`, so the installer passes `--overwrite` (see `SAMURAI_OWNED_OVERWRITE`).
- `cargo fmt` rewraps macro calls and struct literals across lines: python/sed edits that match one-line text silently miss them (2026-11-24, three failed replaces in one session). After `cargo fmt`, grep the real text before scripting an edit, or edit with the native edit tool.
- Limine boots the FIRST entry in `limine.conf`. Any code editing the menu must decide who is first (2026-11-25: removing the default kernel entry made a foreign entry the default; VM screendump showed the panic). When a VM reboot test hangs, take a QMP `screendump` before the timeout kills the VM: bootloader errors never reach the serial log.
- Never `pkill -f <pattern>` from an exec command that contains the pattern: it kills its own shell. Stop the systemd unit instead.
- A published package file name never changes bytes: pacman caches by name and verifies the db checksum; makepkg is not reproducible. `ci/build-repo.sh` reuses the stable file when contents match and fails with "bump pkgrel" when they differ (2026-11-26: a release nearly overwrote three unchanged base packages).
- Package selection for an install lives only in `installer/samurai-profile-plan` (postinstall, the
  wizard's lists and the offline ISO closure read it). A new choice = planner + `validate-profile.py`
  + `docs/plans/profile-schema.md` + unit test; offline-carried choices go into
  `everything_offline_profiles()`.
- Local PKGBUILD sources: run `ci/update-pkg-sums.py` after editing one. `iso/airootfs` holds
  committed copies of installer files (unit tests fail on drift; copies in `usr/local/bin` are 755).
- A branch whose `[samurai]` packages are unpublished is tested with `ci/build-pkgs.sh` +
  `SAMURAI_LOCAL_PKGS=<dir> ci/build-iso.sh` (offline flavor only). Online installs of such a branch
  fail until the packages are published; release order is repo first, ISO second.
- KDE reads `LookAndFeelPackage` only from the `[KDE]` group; the system default lives in
  `/etc/xdg/kdeglobals` (`samurai-kde-defaults`). A `[General]` copy in skel was ignored and installs came
  up light while the tests grepped the file and passed (2026-11-26). Assert the effective value in a live
  session (`kreadconfig6 ... ColorScheme`), not file contents.
- Arch builds Wine as pure WoW64 (2026-11-26): most `lib32-*` Wine libraries no longer exist.
- Hold `/tmp/samurai-vm.lock` only while a VM test runs: an interactive VM session under the lock
  blocks every other lane (2026-11-26).
- Graphical installer (`installer/gui`, ADR-0009) is a front-end only: new choices go to the planner and
  schema first, then `model.py` + a QML page. Layout check without a VM, in an Arch container:
  `tests/gui/render_pages.py OUT` and `tests/gui/drive_keyboard.py` (`QT_QPA_PLATFORM=offscreen`).
- The Plasma controls style gives every button an automatic Alt+letter mnemonic: a letter `Shortcut`
  becomes ambiguous and silently never fires. Use arrows / Ctrl combinations.
- A root helper that sets a private umask passes it to `samurai-install`, which `mkdir`s directories of
  the new system: spawn the engine with `umask=0o022` and create secret files with explicit modes.
- Live-only files live in `/usr/lib/samurai-installer/`: `samurai-install` copies every
  `/usr/local/bin/samurai-*` into the installed system.
- VM tests must keep draining the serial socket: with nobody reading, the guest blocks on the serial
  console (PID 1 hung in `tty_wait_until_sent` mid-reboot, `systemctl` timing out).
- Plasma autologin needs `DisplayServer=wayland` in SDDM: with the default X11 server, stopping it right
  after the autologin kills the new session. Live keyboard changes need `kwriteconfig6 --notify`.

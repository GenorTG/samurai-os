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

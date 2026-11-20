# HANDOFF — samurai-os

## Mission
A personal Arch-based distro: Arch repos unchanged, CachyOS + Chaotic-AUR below Arch, a small signed
`[samurai]` repo built by GitHub Actions on GitHub Releases. Plus a Garuda-level toolbox, graphical +
terminal guided installers fed by Garuda's app catalog, a dev environment (Python 3.10–3.14, uv,
conda, Docker, AI CLIs), GPU setup (CUDA + local gpu-unlock build), and a customised KDE. Everything
proven in QEMU VMs on genorbox1.

## State (2026-11-20)
- Specs 00–14, ADR-0001…0008, OPEN-QUESTIONS: **all 11 remaining questions answered** (Genor,
  native form 2026-11-17). Decisions applied to the specs: name Samurai OS confirmed; toolbox
  Qt6/QML; source repo public now (both repos at M1); KDE theme default **Breeze Dark** (Otto,
  Scratchy optional); Miniforge; Python versions per-version checkboxes all unticked; Docker
  group-vs-rootless chosen during install; AI CLIs all unticked; gpu-unlock includes fork recipes
  for RTX 20 / non-4090 Ada with **every** recipe labelled "untested / potentially unstable"
  (spec 07, ADR-0008); diagnostics manual opt-in only; testing genorbox1-first, plus M10a
  `samurai drive write` (install-drive creator) so Genor can hand-test on his spare laptop.
- Round 2 (Genor's brainstorm) folded in: installers (spec 08), dev env (spec 13), KDE setup
  (spec 14), CUDA + gpu-unlock (spec 07, ADR-0008), repos default on (ADR-0007), kernels cachyos+zen
  (ADR-0001), gaming/Wine (spec 10), firewall UX + Tailscale (spec 09).
- `catalog/garuda-setup-wizard.toml`: 175 apps / 16 categories imported from Garuda toolbox
  `setup-wizard.data.ts` via `scripts/import-garuda-catalog.py`. `catalog/samurai.toml`: our
  choices/toggles/apps/overrides.
- Package availability checked 2026-10-27 (Arch, AUR, Chaotic db): see `catalog/samurai.toml`
  `source=`. Arch python = 3.14.8, cuda 13.4, uv 0.13.
- Local git only. **GitHub repos approved (public now)** — created at M1 with the signing key.

## Next steps (updated 2026-11-20 after M4)
1. M5: Rust CLI (`samurai` dispatcher) replacing the shell shim; keep `updates/` shell scripts as its executors until each is ported.
2. M6/M7: KDE + catalog packages -> then the in-desktop walkthrough (menus, visuals, toolbox) Genor asked for.
3. Check Otto/Scratchy licences before M7a; gpu-unlock hardware validation on stealth-dragon's 3090 (fork recipes too) to retire the "untested" labels.

## Commands
- `bash scripts/install-hooks.sh` (per-commit build number).
- `python3 -I scripts/import-garuda-catalog.py <setup-wizard.data.ts> catalog/garuda-setup-wizard.toml`

## Blockers
- None currently. Public repos + signing key live since M1.
## M0 done (2026-11-17)
- `iso/`: archiso 92 releng profile, samurai-ified (name/label/publisher, hostname `samurai`,
  motd + /etc/issue, `kernel_params_x86_64="console=tty1 console=ttyS0,115200"`, serial getty
  autologin, pacman.conf with commented `[cachyos]`/`[chaotic-aur]`/`[samurai]` stubs — enabled at M1).
- `ci/build-iso.sh`: archlinux:base-devel container, `--privileged` (pacstrap bind-mounts
  proc/sys/dev — unprivileged fails), `pacman -Syu` FIRST (stale image openssl/pacman soname drift),
  out_dir must be ABSOLUTE (relative arg = docker named volume!), work dir mounted for caching.
- `ci/boot-vm.sh`: OVMF q35 + KVM, serial to build/serial.log, PASS = `root@samurai` on serial.
- Verified: build/samurai-os-2026.11.17-x86_64.iso (1.3 GB) boots to root shell in OVMF VM. Boot ~40 s.
## M1 done (2026-11-17)
- GitHub (public, per Genor's "public now"): `GenorTG/samurai-os` (source, main pushed) +
  `GenorTG/samurai-repo` (data-only, README initialized, release tag `x86_64` rolling:
  keyring/mirrorlist/release packages + sigs, `samurai.db`/`samurai.files` + sigs uploaded LAST).
- Keys (docs/KEYS.md): primary `DFE8C148C042AA3C199AAEA08436E12731EDD3FA` offline in
  `~/samurai-keys/gnupg` (0700); signing subkey ed25519 `9849CC7F67786E5037FBB35B5385F81F2AD03400`
  expires 2027-11-17. No passphrase yet (TODO). Backup of `~/samurai-keys/` still TODO.
- `samurai-release` 0.0.1-4 design (spike-verified): NEVER lists /usr/lib/os-release (owned by
  Arch `filesystem`; --overwrite leaves ambiguous ownership and a filesystem upgrade reverts
  content). Hook + `samurai-apply-os-release` re-apply identity after every filesystem or
  samurai-release transaction. `provides/conflicts/replaces=(lsb-release)` (provides satisfies
  open-vm-tools). mktemp template must end in X.
- `ci/build-repo.sh` gotchas learned: `docker run -i` REQUIRED when the script arrives via stdin
  heredoc (without -i the container runs an empty script and exits 0); EOS terminator must not be
  eaten by later edits; repo-add's trailing re-sign aborts harmlessly after both dbs are valid —
  VERIFY dbs instead of trusting its exit code; chown handback must run on the HOST side.
- makepkg gotchas: sha256sums=('SKIP') means "verify PGP instead" (use real sums); `*.asc`
  sources are treated as detached signatures (key export renamed build-key.txt).
- Checks: ci/check-no-shadowing + check-no-epoch; negative tests pass (reject 'linux', '1:2.0');
  db name parsing must read %NAME% records, not entry dir names.
- Spike transcript detail: `--noconfirm` does NOT answer conflict questions (default N) — use
  `--ask 4`; raw.githubusercontent 404s are CDN cache lag; QEMU user-net host = 10.0.2.2.
- ISO: serial autologin drop-in now targets serial-getty@ttyS0.service (getty@ttyS0 was wrong);
  existing ISO predates the fix — rebuild before next VM run.
## M2 done (2026-11-17)
- Smoke suite GREEN from one command: `tests/vm/run-smoke.sh` (iso_boot + install_default, 2 passed
  in ~3.5 min). Full proven cycle: profile -> strict validation -> erase+LUKS2+spec 06 subvolumes ->
  pacstrap -> [samurai] repo from baked key (inline Server lines, NO pre-placed mirrorlist - the
  package owns that path) -> postinstall -> systemd-boot -> LUKS auto-unlock via EMBEDDED keyfile
  (mkinitcpio FILES=, rd.luks.key without keydev) -> rootflags=subvol=@ -> reboot -> installed system
  verified via QGA (hostname, Samurai OS identity, crypt root, @ subvol, zero failed units).
- The M2 install set: base + linux + linux-firmware + mkinitcpio + sudo + fish + python + pciutils +
  samurai-{release,mirrorlist,keyring} + qemu-guest-agent. mkinitcpio is NOT in base. pacman vs
  repo pitfalls: db csize must equal uploaded asset size (makepkg zstd not deterministic) - use
  ci/publish-repo.sh for publishes (uploads full consistent set, db LAST, csize-verified).
- Harness lessons: strip ANSI before matching (ESC ( B splits 'root@samurai'); qemu-ga here rejects
  capture-output and returns buf-b64 from guest-file-read; serial_cmd needs a settle-nudge before
  sending; OVMF prefers the CD - eject the ISO (QMP) before the reboot; mkarchiso stamp-skips on a
  reused work dir; docker stdin needs -i for heredoc scripts; pkill -f self-matches the exec shell
  (use -x or kill by PID).
- Debug-disk trick: mount the failed run's qcow2 via the LIVE ISO + QGA (vda1 = ESP, vda2 = LUKS,
  keyfile at /esp/keys/samurai.key) - found the missing FILES= line that way.
## M3 done (2026-11-18)
- `updates/samurai-update`: 11 stages per spec 03. Key fixes found by testing:
  - HEADLESS detection: stdin not a TTY -> auto --noconfirm (pacman prompts abort on EOF);
    interactive terminals still prompt.
  - stage 6 failure detection: capture rc=${PIPESTATUS[0]} after pacman|tee (the pipeline masked
    pacman failures — the updater reported success while nothing was installed).
  - pacman -Qdt pipefail trap: an empty orphan query returned nonzero under `set -euo pipefail`
    and killed the script in stage 9 — guard with `{ ... || true; } | wc -l`.
  - preflight: use curl (ping/iputils is not in base).
- Interventions: TOML + script + .done state, at-most-once; validator allowlist
  (`updates/validate-interventions.py`); 0001 mechanism test green.
- `updates/samurai-notifier` + user timer units (checkupdates, arch-audit security counts,
  reboot-pending detection); never installs anything.
- Installed-system networking: the pacstrap'd target has NO network config — postinstall writes
  20-samurai.network (DHCP on en*/eth*/wl*) + the resolved stub. Without it the updater preflight
  died on "no network".
- Pinned mirrors (repos.arch_mirror profile key): pacman expands $repo/os/$arch ONLY when the
  Server line contains them — a plain URL fetches <server>/core.db (404). Installer appends the
  placeholders + strips a trailing slash. Archive layout: /repos/YYYY/MM/DD/ (FULL date).
- ci/build-iso.sh: curl XferCommand with retries (pacman parallel libcurl downloads flaked against
  the CDN); the container script is single-quoted — no apostrophes inside it ("pacman's" killed it).
- Scenarios green: test_update.py (intervention fires once, idempotent) +
  test_update_archive.py (2026/08/15 -> today, ~660 MiB backlog, boots healthy).

## M4 done (2026-11-20)
- Rollback scenario GREEN: unattended install -> initramfs corrupted (verified) -> boot fails ->
  live-ISO `samurai rescue rollback <snap>` -> reboot -> healthy system, `1 passed in 548.76s`.
  Root cause of the earlier flakes: abrupt QMP `quit` killed guests mid-write; `vm.stop()` now does
  ACPI `system_powerdown`, waits for flush, falls back to quit/kill. Rescue script also got an
  /esp already-mounted guard, `.img.tmp` recovery and a zstd magic gate.
- Attended (guided) installer GREEN: `samurai-install` with no --profile runs the wizard —
  requirements, disk pick (lsblk TYPE=disk only), hostname, two-level timezone, LUKS2 on/off,
  kernel multi-select (default = linux until the release ISO ships CachyOS keys), bootloader
  choice (limine/grub/systemd-boot/refind), user + shell + password (openssl passwd -6 hash only).
  Falls through to the validated unattended backend unchanged. `test_attended_install 1 passed
  in 327.81s`: wizard -> LUKS2 passphrase -> GRUB install -> boot passphrase -> fish login as the
  created user -> hostname/GRUB/btrfs/LUKS/profile/log asserts.
- Bugs the test cycle caught: pacstrap quoted multi-word targets ("grub os-prober efibootmgr"),
  CD-ROM offered as an erase target, install log only on the live medium (now copied into the
  target), QGA chardev single-client (close GuestAgent objects or connect() hangs).
- vm.py gained `screendump()` (QMP VGA -> PNG) for install-flow screenshots.
- Commits: 2586cde (rollback GREEN), 7e9a54d (wizard WIP), 622272f (test-cycle fixes), 9a3527c (attended GREEN).
- Next milestone (M5): the real Rust CLI replacing the shell shim; then M6/M7 desktop + toolbox GUI —
  the KDE/menus/visuals walkthrough Genor wants needs those packages first.

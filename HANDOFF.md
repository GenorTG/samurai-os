# HANDOFF — samurai-os

## Mission
A personal Arch-based distro: Arch repos unchanged, CachyOS + Chaotic-AUR below Arch, a small signed
`[samurai]` repo built by GitHub Actions on GitHub Releases. Plus a Garuda-level toolbox, graphical +
terminal guided installers fed by Garuda's app catalog, a dev environment (Python 3.10–3.14, uv,
conda, Docker, AI CLIs), GPU setup (CUDA + local gpu-unlock build), and a customised KDE. Everything
proven in QEMU VMs on genorbox1.

## State (2026-11-17)
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

## Next steps (updated 2026-11-17 after M1)
1. M0: `ci/build-iso.sh` + VM boot on genorbox1 (QEMU/OVMF).
2. M1 spike: pacman from GitHub Releases with a throwaway key; create both public repos + signing key.
3. Check Otto/Scratchy licences before M7a; gpu-unlock hardware validation on stealth-dragon's 3090
   (fork recipes too) to retire the "untested" labels.

## Commands
- `bash scripts/install-hooks.sh` (per-commit build number).
- `python3 -I scripts/import-garuda-catalog.py <setup-wizard.data.ts> catalog/garuda-setup-wizard.toml`

## Blockers
- Public repos / real build key: waiting for Genor's OK.
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

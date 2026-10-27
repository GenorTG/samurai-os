# 02 — Package repository

## Repositories on an installed system (`/etc/pacman.conf`, in order)
```ini
[samurai-testing]        # commented out by default; enabled with `samurai repo testing on`
[samurai]
Include = /etc/pacman.d/samurai-mirrorlist
[core]
Include = /etc/pacman.d/mirrorlist
[extra]
Include = /etc/pacman.d/mirrorlist
[multilib]
Include = /etc/pacman.d/mirrorlist
# enabled by default (ADR-0007), always BELOW Arch so they only add packages and never replace Arch ones:
[cachyos]        # linux-cachyos kernels + prebuilt NVIDIA modules, proton-cachyos, chwd upstream
Include = /etc/pacman.d/cachyos-mirrorlist
[chaotic-aur]    # Garuda's prebuilt AUR repo: heroic, yay, claude-code, the ArtCraft apps, brave, vscode ...
Include = /etc/pacman.d/chaotic-mirrorlist
```
`[samurai]` sits above Arch so that `samurai-*` names always resolve to us, but it never contains a
package with an Arch name. CI enforces that rule (`ci/check-no-shadowing`): the build fails if any
package name exists in core, extra or multilib.

The third-party repos go below Arch on purpose. CachyOS's own docs put their repo above Arch to swap
in x86-64-v3 rebuilds; we do not want that (non-goal in spec 00). Below Arch we only get packages Arch
does not have, such as `linux-cachyos` and `proton-cachyos`. Chaotic-AUR is how Garuda ships AUR
software without building it on your machine; we copy that. Both can be turned off in the toolbox's
Repos page. The page warns which installed packages would then stop getting updates.

## What `[samurai]` contains
| Kind | Examples | Rule |
|------|----------|------|
| Our own packages | toolbox, settings, meta, keyring, branding, hooks | the ≤ 30 target from spec 00 |
| Builders | `samurai-gpu-unlock` | ships source pins + build scripts only; output is built on the user's machine (spec 07) |
| Repackaged upstream binaries | `samurai-proton-ge` (GE-Proton from its GitHub releases), `samurai-python310…313` (python-build-standalone), `samurai-miniforge`, `samurai-miniconda`, `samurai-theme-otto/scratchy` | only when no Arch/CachyOS/Chaotic package exists; a CI job watches each upstream's releases and opens a version-bump PR |
| AUR-only packages Chaotic lacks | `openai-codex-bin`, `opencode-bin`, `vectorcraft-bin`, `artcraft-launcher-appimage` | built from the AUR PKGBUILD (pinned commit, reviewed diff); dropped again when Chaotic or Arch picks them up |

Names: AUR names are kept where we rebuild an AUR package unchanged, so yay sees the same package. Our
own repackages use the `samurai-` prefix and `provides=` the common name (e.g. `samurai-proton-ge`
provides `proton-ge-custom`), which avoids epochs (the AUR `proton-ge-custom-bin` has one; the epoch ban
below forbids them).

## Hosting
| What | Where | Why |
|------|-------|-----|
| Packages + repo db (primary) | GitHub Releases on a data-only public repo `GenorTG/samurai-repo`, one rolling release per channel: tags `x86_64` and `x86_64-testing` | free, CDN-backed, no server to run |
| Mirror (fallback) | `https://repo.smart-samurai.pl/$repo/$arch` on genorbox1 (Caddy, static files), synced by CI | GitHub outage or rate limit; also our LAN/VM test mirror |
| ISOs | GitHub Releases on `samurai-os`, tag `v<VERSION>`, plus the mirror | see the size note below |

`samurai-mirrorlist`:
```ini
Server = https://github.com/GenorTG/samurai-repo/releases/download/$arch
Server = https://repo.smart-samurai.pl/$repo/$arch
```
(The testing channel uses the `$arch-testing` tag and a `samurai-testing` path; the exact URL shape
is fixed in the M1 spike.)

### GitHub Releases constraints (verify in the M1 spike)
- pacman follows the 302 redirect from `releases/download/...` to GitHub's object storage. This
  works with pacman's libcurl downloader; confirm it with `pacman -Syy` in a VM.
- GitHub rewrites some characters in asset names (`:` in particular). So **no package epochs**
  (`1:2.0-1`) in `[samurai]`; CI rejects them.
- A single release asset is capped at 2 GiB, and a release has a limit on how many assets it can
  hold. The repo keeps the **current plus 2 previous** builds of each package (enough to downgrade
  with `pacman -U`), pruned by CI. Older builds live only on the mirror.
- The KDE ISO must stay under 2 GiB to fit as a single asset. Budget it in spec 08. If it grows
  past that, publish it on the mirror and attach only the checksum and signature on GitHub.
- The repo db is replaced atomically: upload new package files first, then the `.db`/`.files`
  (+`.sig`) last, so a client never sees a db that names a missing file.

## Build pipeline (GitHub Actions)
1. **Trigger**: push to `main` touching `pkgbuilds/**`, `toolbox/**` or `settings/**`; also nightly and
   manual dispatch.
2. **Detect**: diff `pkgver-pkgrel` of each PKGBUILD against the published db; build only the
   changed packages, in dependency order.
3. **Build**: in an `archlinux:base-devel` container, as a non-root builder user, `makepkg -s` with a
   pinned `makepkg.conf` (no `-march` tuning, `x86-64` baseline). Toolbox packages run `cargo test`
   in `check()`.
4. **Lint**: `namcap` on every PKGBUILD and package; the no-shadowing and no-epoch checks; shellcheck
   on scripts.
5. **Sign**: detached `.sig` per package with the CI signing subkey; `repo-add --sign --verify` for
   the db.
6. **Publish to testing**: upload to `x86_64-testing`, rsync to the mirror's testing path.
7. **VM gate**: spec 11 `smoke` suite runs on genorbox1 against testing (self-hosted runner, labels
   `kvm`). This is the only self-hosted step. GitHub-hosted runners have no nested KVM we can rely on.
8. **Promote**: on green (automatic for patch-only changes, manual approval otherwise), copy the
   same signed files to the stable `x86_64` release. No rebuild on promotion: stable gets exactly
   what was tested.

## Signing keys
- One OpenPGP primary key (certify only) kept **offline** on Genor's machine plus a paper or USB
  backup. UID `Samurai OS Build Key <build@smart-samurai.pl>`.
- One signing subkey, ed25519, expiring after 1 year, stored as a GitHub Actions secret, used only
  by step 5.
- `samurai-keyring` ships the public key, a `samurai-trusted` file and a `samurai-revoked` file
  (Arch keyring format). Rotating the subkey means: publish a new keyring that contains both keys,
  wait a release, then sign with the new one.
- `SigLevel = Required DatabaseRequired` on `[samurai]`. An unsigned package or db fails to install.
- The ISO's `pacman.conf` carries the same SigLevel, and the live ISO trusts the key through
  `samurai-keyring`.

## AUR policy
- **yay** is installed by default (from Chaotic-AUR); paru is a catalog option. yay never runs as root
  and never runs inside `samurai update` unless `update.aur = true`. Even then it runs as the user after
  the pacman transaction, with a snapshot already taken. Garuda's toolbox also refuses to run AUR
  updates itself.
- Packages in the install catalog never come from a live AUR build: they come from Arch, CachyOS,
  Chaotic-AUR or `[samurai]`, so installs are fast, signed and reproducible. yay is for what you add later.
- Nothing in `[samurai]` depends on an AUR package. If we need one, we package it ourselves in
  `pkgbuilds/` with a reason.

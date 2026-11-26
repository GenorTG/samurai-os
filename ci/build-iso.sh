#!/usr/bin/env bash
# M0: build the Samurai OS ISO in an archlinux container (roadmap spec 12).
#
# Usage: [SAMURAI_ISO_FLAVOR=offline|online] ci/build-iso.sh [output-dir]   (default: build/)
#   offline (default): the full KDE + kernels + GPU-driver closure is baked into
#                      the ISO; the default install needs no network (~4.3 GB).
#   online:            netinstall; no baked cache, the installer pulls current
#                      packages from the mirrors (~1.3 GB).
# The ISO is renamed samurai-os-<date>-<flavor>-x86_64.iso.
# Env:
#   SAMURAI_LOCAL_PKGS=<dir>  test builds only: take these unsigned [samurai]
#                             packages (ci/build-pkgs.sh) ahead of the
#                             published ones in the offline cache
# Requires: docker, ~30 GB free, network access to the package mirrors.
# The container runs privileged: mkarchiso's pacstrap bind-mounts
# /proc, /sys and /dev into the airootfs (CAP_SYS_ADMIN).
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out_dir="${1:-$repo_root/build}"
out_dir="$(realpath -m "$out_dir")"   # docker needs an absolute path: a relative arg
                                   # becomes a named volume, not the host dir
work_dir="$out_dir/work"
profile_dir="$out_dir/profile"
image="archlinux:base-devel"
container="samurai-iso-build-$$"
flavor="${SAMURAI_ISO_FLAVOR:-offline}"
case "$flavor" in
    offline|online) ;;
    *) echo "error: SAMURAI_ISO_FLAVOR must be offline or online (got: $flavor)" >&2; exit 2 ;;
esac

local_pkgs="${SAMURAI_LOCAL_PKGS:-}"
local_mount=()
if [ -n "$local_pkgs" ]; then
    [ "$flavor" = offline ] || { echo "error: SAMURAI_LOCAL_PKGS only applies to the offline flavor" >&2; exit 2; }
    local_pkgs="$(realpath -e "$local_pkgs")"
    local_mount=(-v "$local_pkgs":/localpkgs:ro)
fi

mkdir -p "$out_dir"
# Build from a staged profile so generated packages cannot accumulate in the
# checkout's airootfs between runs. Exclude the generated offline cache from
# the source copy; this build will populate it from its exact resolved set.
if [ -e "$profile_dir" ]; then
    echo "error: build profile already exists: $profile_dir (choose a fresh output dir)" >&2
    exit 2
fi
mkdir -p "$profile_dir"
tar --exclude='./airootfs/usr/share/samurai/pkg-cache' -C "$repo_root/iso" -cf - . | tar -C "$profile_dir" -xf -

# Sync the live-side tooling into the ISO airootfs. The airootfs files are
# plain copies; without this step they silently drift from installer/ and
# updates/ (same stale-state bug class as stale mkarchiso stamps, stale repo
# db, stale qcow2 — a build would happily ship yesterday's installer).
bin_dir="$profile_dir/airootfs/usr/local/bin"
lib_dir="$profile_dir/airootfs/usr/lib/samurai"
mkdir -p "$bin_dir" "$lib_dir" "$profile_dir/airootfs/etc"
# mkarchiso uses iso/pacman.conf while building, but the live root otherwise
# keeps Arch's stock config. Install the same repo set into the live ISO so
# attended installs can resolve the CachyOS default kernel.
cp -f "$repo_root/iso/pacman.conf" "$profile_dir/airootfs/etc/pacman.conf"
for f in samurai-install samurai-postinstall samurai-profile-plan validate-profile.py; do
    cp -f "$repo_root/installer/$f" "$bin_dir/$f"
done
# target pacman.conf template (samurai-install installs it on the new system)
mkdir -p "$profile_dir/airootfs/usr/share/samurai"
cp -f "$repo_root/installer/pacman.conf.target" "$profile_dir/airootfs/usr/share/samurai/pacman.conf.target"
cp -f "$repo_root/installer/profile-default.toml" "$profile_dir/airootfs/usr/share/samurai/profile-default.toml"
# license texts travel with the image (PolyForm Noncommercial notice, scope summary)
mkdir -p "$profile_dir/airootfs/usr/share/licenses/samurai-os"
cp -f "$repo_root/LICENSE" "$repo_root/LICENSING.md" "$profile_dir/airootfs/usr/share/licenses/samurai-os/"
# every executable tool under updates/ (samurai, samurai-*): new tools ship
# without editing this list
for f in "$repo_root"/updates/samurai "$repo_root"/updates/samurai-*; do
    [ -f "$f" ] && [ -x "$f" ] || continue
    case "$f" in *.service|*.timer) continue ;; esac
    cp -f "$f" "$bin_dir/$(basename "$f")"
    chmod 755 "$bin_dir/$(basename "$f")"
done
# the live medium (and the target's first boot, which copies these tools) runs the
# real Rust CLI, not the old shell shim: build it from this tree, offline, in a
# throwaway container
echo "==> building the samurai CLI for the live medium"
docker run --rm -v "$repo_root/toolbox":/src:ro -v "$bin_dir":/out "$image" bash -c '
set -euo pipefail
pacman -Syu --noconfirm --needed rust gcc >/dev/null
cd /src && CARGO_TARGET_DIR=/tmp/target cargo build --release --locked --offline -p samurai-cli
install -m755 /tmp/target/release/samurai /out/samurai
'
for f in samurai-notifier.service samurai-notifier.timer; do
    cp -f "$repo_root/updates/$f" "$lib_dir/$f"
done
# app catalog: the installer validates [packages] apps against it and the
# live session can browse it (`samurai apps`)
mkdir -p "$profile_dir/airootfs/usr/share/samurai/catalog"
cp -f "$repo_root"/catalog/*.toml "$profile_dir/airootfs/usr/share/samurai/catalog/"
printf '%s\n' "$flavor" > "$profile_dir/airootfs/etc/samurai-iso-flavor"
chmod 755 "$bin_dir"/samurai-install "$bin_dir"/samurai-postinstall \
    "$bin_dir"/samurai-profile-plan "$bin_dir"/validate-profile.py
# profiledef's file_permissions would otherwise reset copied tools to 644
for f in "$bin_dir"/samurai*; do
    rel="/usr/local/bin/$(basename "$f")"
    grep -q "\[\"$rel\"\]" "$profile_dir/profiledef.sh" ||
        sed -i "s|^file_permissions=(|file_permissions=(\n  [\"$rel\"]=\"0:0:755\"|" "$profile_dir/profiledef.sh"
done
sed -i "s|^iso_application=.*|iso_application=\"Samurai OS Live/Installer ($flavor)\"|" "$profile_dir/profiledef.sh"

### Offline install cache (Genor 2026-11-20): bake the default release
### profile's full package closure + sync dbs into the ISO, so the default
### install runs WITHOUT network ("a base version installable without a lot
### of curls"). Every package is downloaded with signature checking ON
### (Arch, CachyOS, Chaotic-AUR and [samurai] keys populated first, their
### primary fingerprints pinned), so the cache only contains packages that
### verified at build time. The installer then reads it with SigLevel Never:
### the ISO (checksummed/signed with the release) is the install-time anchor.
### This MUST run before mkarchiso: the cache is injected into the airootfs.
if [ "$flavor" = offline ]; then
echo "==> downloading offline install cache (release default set)"
# stale-state rule: a reused download dir would keep two versions of one
# package and pacman -U would refuse the pair — purge first
docker run --rm -v "$out_dir":/b alpine sh -c 'rm -rf /b/offline-pkgs'
docker run --rm -v "$out_dir":/b -v "$repo_root":/src:ro "${local_mount[@]}" "$image" bash -c '
set -euo pipefail
pacman-key --init >/dev/null 2>&1
pacman -Syu --noconfirm >/dev/null
# NOTE: no XferCommand override here — curl -C - (resume) corrupts downloads
# on the CachyOS CDN ("Unrecognized archive format" on cachyos.db, seen
# 2026-11-20); the pacman internal downloader fetched it clean in the probe.
sed -i "s|^ParallelDownloads.*|ParallelDownloads = 2|" /etc/pacman.conf
# --- trust setup: signatures stay ON for every repo ----------------------
# CachyOS keyring: the package cannot vouch for itself, so it is fetched over
# HTTPS from the four mirrors and its PRIMARY key fingerprint is pinned.
CACHYOS_FPR=882DCFE48E2051D48E2562ABF3B607488DB35A47
for s in nl aur mirror build; do
    curl -fsSL --retry 4 -o /tmp/ck.db "https://$s.cachyos.org/repo/x86_64/cachyos/cachyos.db" || continue
    kr=$(bsdtar -tf /tmp/ck.db | grep -o "^cachyos-keyring-[^/]*" | head -1) || continue
    curl -fsSL --retry 4 -o /tmp/ck.pkg.tar.zst "https://$s.cachyos.org/repo/x86_64/cachyos/$kr-any.pkg.tar.zst" && break
done
[ -s /tmp/ck.pkg.tar.zst ] || { echo "error: cannot fetch cachyos-keyring" >&2; exit 1; }
pacman -U --noconfirm /tmp/ck.pkg.tar.zst >/dev/null
pacman-key --populate archlinux cachyos >/dev/null
pacman-key --finger "$CACHYOS_FPR" | tr -d " " | grep -q "$CACHYOS_FPR" ||
    { echo "error: CachyOS key $CACHYOS_FPR not in the populated keyring" >&2; exit 1; }
pacman-key --lsign-key "$CACHYOS_FPR" >/dev/null
# [samurai]: the pinned build key shipped in the repo
SAMURAI_FPR=DFE8C148C042AA3C199AAEA08436E12731EDD3FA
pacman-key --add /src/iso/airootfs/usr/share/samurai/build-key.txt >/dev/null
pacman-key --finger "$SAMURAI_FPR" | tr -d " " | grep -q "$SAMURAI_FPR" || { echo "error: samurai build key mismatch" >&2; exit 1; }
pacman-key --lsign-key "$SAMURAI_FPR" >/dev/null
/src/ci/chaotic-bootstrap.sh
pacman -S --noconfirm --needed python >/dev/null
# test builds: unsigned branch packages take precedence over the published ones
LOCAL_REPO=""
if compgen -G "/localpkgs/*.pkg.tar.zst" >/dev/null; then
    echo "==> using LOCAL unsigned packages (test build, never release this ISO)"
    mkdir -p /tmp/localrepo
    cp /localpkgs/*.pkg.tar.zst /tmp/localrepo/
    repo-add -q /tmp/localrepo/samurai-local.db.tar.gz /tmp/localrepo/*.pkg.tar.zst
    LOCAL_REPO="[samurai-local]
SigLevel = Never
Server = file:///tmp/localrepo
"
fi
cat >> /etc/pacman.conf <<REPOS

[multilib]
Include = /etc/pacman.d/mirrorlist

[cachyos]
Server = https://nl.cachyos.org/repo/\$arch/\$repo
Server = https://aur.cachyos.org/repo/\$arch/\$repo
Server = https://mirror.cachyos.org/repo/\$arch/\$repo
Server = https://build.cachyos.org/repo/\$arch/\$repo

[chaotic-aur]
Include = /etc/pacman.d/chaotic-mirrorlist

$LOCAL_REPO
[samurai]
SigLevel = Required DatabaseRequired
Server = https://github.com/GenorTG/samurai-repo/releases/download/x86_64
Server = https://repo.smart-samurai.pl/samurai/x86_64
REPOS
pacman -Sy >/dev/null
mkdir -p /b/offline-pkgs /tmp/freshdb
# resolve against an EMPTY db: pacman -Sw skips installed packages, so a
# plain -Sw would miss everything the build container already has (glibc,
# bash, coreutils — the -U target is empty and needs them all)
pacman --dbpath /tmp/freshdb -Sy >/dev/null
# everything an install profile can select without network (single source:
# the planner the installer uses)
plan="$(python3 /src/installer/samurai-profile-plan --offline-closure --catalog /src/catalog)"
read -r -d "" -a planned <<<"$plan" || true
[ "${#planned[@]}" -gt 20 ] || { echo "error: the offline closure plan looks empty" >&2; exit 1; }
pacman --dbpath /tmp/freshdb -Sw --noconfirm --cachedir /b/offline-pkgs \
    base linux-cachyos linux-cachyos-headers linux-zen linux-zen-headers \
    linux linux-headers linux-lts linux-lts-headers linux-cachyos-lts linux-cachyos-lts-headers \
    linux-firmware mkinitcpio dkms linux-cachyos-nvidia-open nvidia-open \
    chaotic-keyring chaotic-mirrorlist \
    nvidia-open-lts nvidia-open-dkms nvidia-utils nvidia-settings \
    mesa vulkan-radeon vulkan-intel vulkan-nouveau \
    intel-media-driver libva-intel-driver \
    amd-ucode intel-ucode zram-generator pacman-contrib sudo fish zsh python pciutils snapper snap-pac \
    grub limine refind efibootmgr os-prober \
    networkmanager wpa_supplicant qemu-guest-agent archlinux-keyring cachyos-keyring \
    samurai-release samurai-mirrorlist samurai-keyring \
    "${planned[@]}" \
    cachyos/pahole
# cachyos/pahole: linux-cachyos-headers needs pahole>=1:1.32-2, newer than
# extra (repo order makes plain "pahole" resolve to extra and fail)
# Local repository: the installer RESOLVES against it (pacman -S), so a
# machine gets only the packages it needs, never the whole cache.
repo-add -q /b/offline-pkgs/samurai-offline.db.tar.gz /b/offline-pkgs/*.pkg.tar.zst
rm -f /b/offline-pkgs/*.old
'
mkdir -p "$profile_dir/airootfs/usr/share/samurai/pkg-cache"
# -L: samurai-offline.db / .files are symlinks to the .tar.gz archives
# package .sig files stay with their packages (later offline verification)
find "$out_dir/offline-pkgs" -maxdepth 1 \( -type f -o -type l \) \
    -exec cp -L {} "$profile_dir/airootfs/usr/share/samurai/pkg-cache/" \;
[ -f "$profile_dir/airootfs/usr/share/samurai/pkg-cache/samurai-offline.db" ] ||
    { echo "error: offline repository database missing" >&2; exit 1; }
echo "==> offline cache: $(ls "$profile_dir/airootfs/usr/share/samurai/pkg-cache" | wc -l) files"
# SAMURAI_ISO_DOWNLOAD_ONLY=1: verify that the closure resolves and every
# signature checks out, then stop (minutes, not the full ISO build)
if [ "${SAMURAI_ISO_DOWNLOAD_ONLY:-0}" = 1 ]; then echo "==> download-only: done"; exit 0; fi
else
    echo "==> online flavor: no baked package cache (installer uses the mirrors)"
fi
# mkarchiso skips every stamped step when the work dir holds stamps from a
# previous build — a reused work dir "succeeds" in 2 s and produces NO ISO.
# Keep build outputs isolated: refuse a non-empty work dir rather than
# deleting prior build artifacts. Use a fresh output directory for each build.
if [ -d "$work_dir" ] && [ -n "$(find "$work_dir" -mindepth 1 -maxdepth 1 -print -quit)" ]; then
    echo "error: non-empty mkarchiso work dir: $work_dir (choose a fresh output dir)" >&2
    exit 2
fi
mkdir -p "$work_dir"

echo "==> building ISO in $image (container $container)"
docker run --rm --privileged \
    --name "$container" \
    -v "$repo_root":/src:ro \
    -v "$out_dir":/out \
    -v "$work_dir":/work \
    "$image" \
    bash -c '
set -euo pipefail
pacman-key --init >/dev/null 2>&1
# The base-devel image is often stale vs the repos: upgrade everything
# together first, then install archiso (openssl/pacman soname drift).
# ALSO: pacman parallel libcurl downloads have been flaking against the
# CDN (SSL_ERROR_SSL, errno 0) while a single curl works — switch to a
# curl-based XferCommand with retries and reduce parallelism.
pacman -Syu --noconfirm >/dev/null
# pacman parallel libcurl downloads have been flaking against the CDN
# (SSL_ERROR_SSL, errno 0) while a single curl works — switch to a curl-based
# XferCommand with retries and reduce parallelism. (double-quoted seds: this
# whole script sits inside single quotes)
sed -i -e "s|^#XferCommand = /usr/bin/wget.*||" -e "s|^#XferCommand = /usr/bin/curl.*|XferCommand = /usr/bin/curl -L -f --retry 8 --retry-delay 2 --retry-all-errors -o %o %u|" /etc/pacman.conf
# no -C - (resume) in the XferCommand: resumed offsets corrupt files mid-retry
# (GPGME No data on sync dbs — seen 2026-11-20 on the Arch CDN)
sed -i "s|^ParallelDownloads.*|ParallelDownloads = 2|" /etc/pacman.conf
pacman -S --noconfirm --needed archiso >/dev/null
# the cachyos db IS signed: a present sig from an unknown key fails even
# under DatabaseOptional, and mkarchiso cannot answer the import prompt.
# Import the keyring from the offline cache (already downloaded via the
# 4-server fallback — the mirror desyncs, seen 2026-11-20)
if compgen -G "/out/offline-pkgs/cachyos-keyring-*.pkg.tar.zst" >/dev/null; then
    # copy WITHOUT the .sig: a keyring package cannot vouch for itself in this
    # fresh container (its signer is not trusted yet); it was verified against
    # the pinned CachyOS fingerprint when the cache was downloaded above
    cp /out/offline-pkgs/cachyos-keyring-*.pkg.tar.zst /tmp/ck-cache.pkg.tar.zst
    pacman -U --noconfirm /tmp/ck-cache.pkg.tar.zst >/dev/null
else
    # online flavor: no offline cache, fetch the keyring through the same
    # 4-server fallback the offline download uses
    for s in nl aur mirror build; do
        curl -fsSL --retry 4 -o /tmp/ck.db "https://$s.cachyos.org/repo/x86_64/cachyos/cachyos.db" || continue
        kr=$(bsdtar -tf /tmp/ck.db | grep -o "^cachyos-keyring-[^/]*" | head -1) || continue
        curl -fsSL --retry 4 -o /tmp/ck.pkg.tar.zst "https://$s.cachyos.org/repo/x86_64/cachyos/$kr-any.pkg.tar.zst" && break
    done
    pacman -U --noconfirm /tmp/ck.pkg.tar.zst >/dev/null
fi
# [chaotic-aur] is in the ISO profile: trust it and provide its mirrorlist
/src/ci/chaotic-bootstrap.sh
mkarchiso -v -w /work -o /out /out/profile
'

built=$(find "$out_dir" -maxdepth 1 -name "samurai-os-*-x86_64.iso" ! -name "*-online-*" ! -name "*-offline-*" | head -1)
if [ -n "$built" ]; then
    stem="${built%-x86_64.iso}"
    docker run --rm -v "$out_dir":/b alpine mv "/b/$(basename "$built")" "/b/$(basename "$stem")-$flavor-x86_64.iso"
fi

echo "==> done:"
ls -lh "$out_dir"

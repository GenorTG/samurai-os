#!/usr/bin/env bash
# M0: build the Samurai OS ISO in an archlinux container (roadmap spec 12).
#
# Usage: ci/build-iso.sh [output-dir]   (default: build/)
# Requires: docker, ~4 GB free in /var, network access to Arch mirrors.
# The container runs privileged: mkarchiso's pacstrap bind-mounts
# /proc, /sys and /dev into the airootfs (CAP_SYS_ADMIN).
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out_dir="${1:-$repo_root/build}"
out_dir="$(realpath -m "$out_dir")"   # docker needs an absolute path: a relative arg
                                   # becomes a named volume, not the host dir
image="archlinux:base-devel"
container="samurai-iso-build-$$"

mkdir -p "$out_dir"
# Sync the live-side tooling into the ISO airootfs. The airootfs files are
# plain copies; without this step they silently drift from installer/ and
# updates/ (same stale-state bug class as stale mkarchiso stamps, stale repo
# db, stale qcow2 — a build would happily ship yesterday's installer).
bin_dir="$repo_root/iso/airootfs/usr/local/bin"
lib_dir="$repo_root/iso/airootfs/usr/lib/samurai"
mkdir -p "$bin_dir" "$lib_dir"
for f in samurai-install samurai-postinstall validate-profile.py; do
    cp -f "$repo_root/installer/$f" "$bin_dir/$f"
done
for f in samurai samurai-notifier samurai-rescue samurai-snap samurai-update; do
    cp -f "$repo_root/updates/$f" "$bin_dir/$f"
done
for f in samurai-notifier.service samurai-notifier.timer; do
    cp -f "$repo_root/updates/$f" "$lib_dir/$f"
done
chmod 755 "$bin_dir"/samurai-install "$bin_dir"/samurai-postinstall \
    "$bin_dir"/validate-profile.py "$bin_dir"/samurai "$bin_dir"/samurai-notifier \
    "$bin_dir"/samurai-rescue "$bin_dir"/samurai-snap "$bin_dir"/samurai-update

### Offline install cache (Genor 2026-11-20): bake the default release
### profile's full package closure + sync dbs into the ISO, so the default
### install runs WITHOUT network ("a base version installable without a lot
### of curls"). Downloaded with SigLevel Never: the ISO itself is the trust
### anchor (signatures were verified upstream when these repos built).
### This MUST run before mkarchiso: the cache is injected into the airootfs.
echo "==> downloading offline install cache (release default set)"
# stale-state rule: a reused download dir would keep two versions of one
# package and pacman -U would refuse the pair — purge first
docker run --rm -v "$out_dir":/b alpine sh -c 'rm -rf /b/offline-pkgs'
docker run --rm -v "$out_dir":/b -v "$repo_root":/src:ro "$image" bash -c '
set -euo pipefail
pacman-key --init >/dev/null 2>&1
pacman -Syu --noconfirm >/dev/null
# NOTE: no XferCommand override here — curl -C - (resume) corrupts downloads
# on the CachyOS CDN ("Unrecognized archive format" on cachyos.db, seen
# 2026-11-20); the pacman internal downloader fetched it clean in the probe.
sed -i "s|^ParallelDownloads.*|ParallelDownloads = 2|" /etc/pacman.conf
cat >> /etc/pacman.conf <<REPOS

[cachyos]
Server = https://nl.cachyos.org/repo/\$arch/\$repo
Server = https://aur.cachyos.org/repo/\$arch/\$repo
Server = https://mirror.cachyos.org/repo/\$arch/\$repo
Server = https://build.cachyos.org/repo/\$arch/\$repo

[samurai]
Server = https://github.com/GenorTG/samurai-repo/releases/download/x86_64
Server = https://repo.smart-samurai.pl/samurai/x86_64
REPOS
sed -i "s|^SigLevel.*|SigLevel = Never|" /etc/pacman.conf
pacman -Sy >/dev/null
mkdir -p /b/offline-pkgs
# NO --needed here: the -U target is empty, so the closure must include even
# packages the container itself already has (bash, glibc, ...)
pacman -Sw --noconfirm --cachedir /b/offline-pkgs \
    base linux-cachyos linux-zen linux linux-lts linux-firmware mkinitcpio \
    sudo fish zsh python pciutils snapper snap-pac \
    grub limine refind efibootmgr os-prober \
    networkmanager wpa_supplicant qemu-guest-agent archlinux-keyring cachyos-keyring \
    plasma-meta sddm konsole dolphin kwrite ark gwenview okular \
    samurai-release samurai-mirrorlist samurai-keyring
cp /var/lib/pacman/sync/*.db /b/offline-pkgs/
'
rm -rf "$repo_root/iso/airootfs/usr/share/samurai/pkg-cache"
mkdir -p "$repo_root/iso/airootfs/usr/share/samurai/pkg-cache"
find "$out_dir/offline-pkgs" -maxdepth 1 -type f ! -name '*.sig' \
    -exec cp {} "$repo_root/iso/airootfs/usr/share/samurai/pkg-cache/" \;
echo "==> offline cache: $(ls "$repo_root/iso/airootfs/usr/share/samurai/pkg-cache" | wc -l) files"
# mkarchiso skips every stamped step when the work dir holds stamps from a
# previous build — a reused work dir "succeeds" in 2 s and produces NO ISO.
# (Same bug class: stale repo db in build-repo.sh, stale qcow2 in tests/vm.)
# The container runs privileged, so leftovers are root-owned: clean via docker.
docker run --rm -v "$repo_root/build":/b alpine sh -c 'rm -rf /b/work && mkdir -p /b/work'

echo "==> building ISO in $image (container $container)"
docker run --rm --privileged \
    --name "$container" \
    -v "$repo_root":/src:ro \
    -v "$out_dir":/out \
    -v "$repo_root/build/work":/work \
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
pacman -U --noconfirm /src/build/offline-pkgs/cachyos-keyring-*.pkg.tar.zst >/dev/null
mkarchiso -v -w /work -o /out /src/iso
'

echo "==> done:"
ls -lh "$out_dir"

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

mkdir -p "$out_dir" "$repo_root/build/work"

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
pacman -Syu --noconfirm >/dev/null
pacman -S --noconfirm --needed archiso >/dev/null
mkarchiso -v -w /work -o /out /src/iso
'

echo "==> done:"
ls -lh "$out_dir"

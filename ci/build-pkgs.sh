#!/usr/bin/env bash
# Build the [samurai] packages UNSIGNED, for tests of a branch whose packages are
# not published yet (SAMURAI_LOCAL_PKGS for ci/build-iso.sh). Never publish this
# output: release builds are ci/build-repo.sh (signed).
#
# Usage: ci/build-pkgs.sh [out-dir] [package ...]   (default: build/localpkgs, all)
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out_dir="$(realpath -m "${1:-$repo_root/build/localpkgs}")"
[ $# -gt 0 ] && shift
only="$*"
srcdest="$(realpath -m "${SAMURAI_SRCDEST:-$repo_root/build/srcdest}")"
image="${SAMURAI_BUILD_IMAGE:-archlinux:base-devel}"
mkdir -p "$out_dir" "$srcdest"

docker run --rm -i --name "samurai-pkg-build-$$" \
    -v "$repo_root":/src:ro -v "$out_dir":/out -v "$srcdest":/srcdest \
    -e SAMURAI_ONLY="$only" -e OWNER="$(id -u):$(id -g)" \
    "$image" bash -es <<'EOS'
set -euo pipefail
trap 'chown -R "$OWNER" /out /srcdest' EXIT
pacman-key --init >/dev/null 2>&1
pacman -Syu --noconfirm >/dev/null      # stale-image guard (see ci/build-iso.sh)
pacman -S --noconfirm --needed namcap >/dev/null
/src/ci/check-no-shadowing /src/pkgbuilds
/src/ci/check-no-epoch /src/pkgbuilds
/src/ci/build-packages.sh /out
EOS

ls -lh "$out_dir"

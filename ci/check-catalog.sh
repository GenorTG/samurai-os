#!/usr/bin/env bash
# Every app the catalog OFFERS must resolve in the repositories an installed system
# enables (spec 08 "Catalog"). Syncs the live databases in a throwaway Arch
# container and runs `samurai apps check --strict`; hidden entries may be missing.
#
# Usage: ci/check-catalog.sh [--json]
# Env: SAMURAI_BUILD_IMAGE (default archlinux:base-devel)
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
image="${SAMURAI_BUILD_IMAGE:-archlinux:base-devel}"

docker run --rm -i --name "samurai-catalog-check-$$" -v "$repo_root":/src:ro \
    -e SAMURAI_CHECK_ARGS="$*" "$image" bash -es <<'EOS'
set -euo pipefail
pacman -Syu --noconfirm --needed python >/dev/null
# Only package NAMES are read here and nothing is installed from the three added
# repositories, so their keyrings are not set up (SigLevel Never, databases only).
conf=/etc/pacman.conf
{
    sed -n '/^\[options\]/,$p' /src/installer/pacman.conf.target | sed '/^\[cachyos\]/,$d; s/^DownloadUser.*/DisableSandbox/'
    # (inside a container pacman cannot set up its download sandbox)
    printf '%s\n' '[cachyos]' 'SigLevel = Never' \
        'Server = https://aur.cachyos.org/repo/$arch/$repo' \
        'Server = https://mirror.cachyos.org/repo/$arch/$repo' \
        '[chaotic-aur]' 'SigLevel = Never' \
        'Server = https://cdn-mirror.chaotic.cx/$repo/$arch' \
        'Server = https://geo-mirror.chaotic.cx/$repo/$arch' \
        '[samurai]' 'SigLevel = Never' \
        'Server = https://github.com/GenorTG/samurai-repo/releases/download/x86_64'
} > /tmp/pacman-catalog.conf
cp /tmp/pacman-catalog.conf "$conf"
pacman -Sy --noconfirm >/dev/null
# shellcheck disable=SC2086  # the arguments are our own flags
python -I /src/updates/samurai-apps check --strict $SAMURAI_CHECK_ARGS
EOS

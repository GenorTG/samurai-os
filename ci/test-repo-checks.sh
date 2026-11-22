#!/usr/bin/env bash
# M1 exit criterion (roadmap): the checks FAIL ON PURPOSE on crafted bad
# packages. Runs inside the archlinux container (ci/build-repo.sh env) or on
# any host: without pacman, Arch's package names are fetched from the repo dbs
# into $SAMURAI_ARCH_NAMES (default /tmp/arch-names).
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
export SAMURAI_ARCH_NAMES="${SAMURAI_ARCH_NAMES:-/tmp/arch-names}"
mirror="${SAMURAI_ARCH_MIRROR:-https://geo.mirror.pkgbuild.com}"

# arch names list: parse %NAME% records from the repo db entries (entry dir
# names carry versions, so parsing dir names alone would miss collisions)
if [ ! -s "$SAMURAI_ARCH_NAMES" ] && ! command -v pacman >/dev/null; then
    for repo in core extra multilib; do
        curl -fsSL --retry 3 "$mirror/$repo/os/x86_64/$repo.db" -o "$tmp/$repo.db"
        tar -xOf "$tmp/$repo.db" --wildcards '*/desc' | awk '/^%NAME%$/{getline; print}'
    done | sort -u > "$SAMURAI_ARCH_NAMES"
    echo "==> fetched $(wc -l < "$SAMURAI_ARCH_NAMES") Arch package names from $mirror"
fi

# good packages must pass
"$repo_root/ci/check-no-shadowing" "$repo_root/pkgbuilds"
"$repo_root/ci/check-no-epoch" "$repo_root/pkgbuilds"

# fixture 1: a package named like an Arch package -> shadowing must fail
mkdir -p "$tmp/shadow/linux" && printf 'pkgname=linux\npkgver=6.0\npkgrel=1\n' > "$tmp/shadow/linux/PKGBUILD"
if "$repo_root/ci/check-no-shadowing" "$tmp/shadow" 2>/dev/null; then
    echo "==> NEGATIVE TEST FAILED: shadowing check passed a shadowing package" >&2
    exit 1
fi
echo "==> negative test OK: shadowing check rejects 'linux'"

# fixture 1b: quoted array form of pkgname must be seen too
mkdir -p "$tmp/shadow2/x" && printf "pkgname=('pacman')\npkgver=7\npkgrel=1\n" > "$tmp/shadow2/x/PKGBUILD"
if "$repo_root/ci/check-no-shadowing" "$tmp/shadow2" 2>/dev/null; then
    echo "==> NEGATIVE TEST FAILED: shadowing check passed pkgname=('pacman')" >&2
    exit 1
fi
echo "==> negative test OK: shadowing check rejects pkgname=('pacman')"

# fixture 2: an epoch version -> epoch check must fail
mkdir -p "$tmp/epoch/bad-pkg" && printf 'pkgname=bad-pkg\npkgver=1:2.0\npkgrel=1\n' > "$tmp/epoch/bad-pkg/PKGBUILD"
if "$repo_root/ci/check-no-epoch" "$tmp/epoch" 2>/dev/null; then
    echo "==> NEGATIVE TEST FAILED: epoch check passed an epoch package" >&2
    exit 1
fi
echo "==> negative test OK: epoch check rejects '1:2.0'"

# fixture 3: an explicit epoch= line -> epoch check must fail
mkdir -p "$tmp/epoch2/bad-pkg" && printf 'pkgname=bad-pkg\npkgver=2.0\npkgrel=1\nepoch=1\n' > "$tmp/epoch2/bad-pkg/PKGBUILD"
if "$repo_root/ci/check-no-epoch" "$tmp/epoch2" 2>/dev/null; then
    echo "==> NEGATIVE TEST FAILED: epoch check passed epoch=1" >&2
    exit 1
fi
echo "==> negative test OK: epoch check rejects 'epoch=1'"

echo "==> all check tests passed"

#!/usr/bin/env bash
# M1 exit criterion (roadmap): the checks FAIL ON PURPOSE on crafted bad
# packages. Run inside the archlinux container (ci/build-repo.sh env) or on
# any host with /tmp/arch-names populated.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

# arch names list: parse %NAME% records from the repo db entries (entry dir
# names carry versions, so parsing dir names alone would miss collisions)
if [ ! -f /tmp/arch-names ]; then
    mkdir -p "$(dirname /tmp/arch-names)"
    curl -fsSL 'https://geo.mirror.pkgbuild.com/core/os/x86_64/core.db' -o /tmp/core.db
    tar -xzOf /tmp/core.db | awk '/^%NAME%$/{getline; print}' | sort -u > /tmp/arch-names
    curl -fsSL 'https://geo.mirror.pkgbuild.com/extra/os/x86_64/extra.db' -o /tmp/extra.db
    tar -xzOf /tmp/extra.db | awk '/^%NAME%$/{getline; print}' | sort -u >> /tmp/arch-names
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

# fixture 2: an epoch version -> epoch check must fail
mkdir -p "$tmp/epoch/bad-pkg" && printf 'pkgname=bad-pkg\npkgver=1:2.0\npkgrel=1\n' > "$tmp/epoch/bad-pkg/PKGBUILD"
if "$repo_root/ci/check-no-epoch" "$tmp/epoch" 2>/dev/null; then
    echo "==> NEGATIVE TEST FAILED: epoch check passed an epoch package" >&2
    exit 1
fi
echo "==> negative test OK: epoch check rejects '1:2.0'"

echo "==> all check tests passed"

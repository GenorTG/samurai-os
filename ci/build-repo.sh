#!/usr/bin/env bash
# M1: build, sign and assemble the [samurai] pacman repository locally.
# Spec: docs/specs/02-PACKAGE-REPOSITORY.md (Build pipeline steps 3, 5).
#
# Usage: ci/build-repo.sh [out-dir]   (default: build/repo)
# Requires: docker, the signing subkey in ~/samurai-keys/gnupg (runbook: docs/KEYS.md).
# Output: out-dir/x86_64/ with *.pkg.tar.zst(.sig) + samurai.db.tar.(zst|gz)(.sig)
#
# The signing subkey is exported per-run into a temp file, mounted read-only,
# and deleted when the container exits. It never lands in the image or /out.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out_dir="${1:-$repo_root/build/repo}"
out_dir="$(realpath -m "$out_dir")"
keyfile="$(mktemp /tmp/samurai-subkey.XXXXXX.asc)"
trap 'shred -u "$keyfile" 2>/dev/null || rm -f "$keyfile"' EXIT
container="samurai-repo-build-$$"

export GNUPGHOME="${GNUPGHOME:-$HOME/samurai-keys/gnupg}"
fpr_file="$HOME/samurai-keys/primary-fingerprint.txt"
read -r primary < "$fpr_file"
# signing subkey = the ed25519 [S] subkey of the primary
subkey="$(gpg --list-keys --with-colons --with-subkey-fingerprints 'build@smart-samurai.pl' \
    | awk -F: '/^sub:/{s=1} s && /^fpr:/{print $10; exit}')"
[ -n "$subkey" ] || { echo "no signing subkey found for build@smart-samurai.pl" >&2; exit 1; }
gpg --armor --export-secret-subkeys "$subkey!" > "$keyfile"
chmod 600 "$keyfile"

mkdir -p "$out_dir"

echo "==> building [samurai] packages in $container (subkey $subkey)"
docker run --rm -i --name "$container" \
    -v "$repo_root":/src:ro \
    -v "$out_dir":/out \
    -v "$keyfile":/keys/subkey.asc:ro \
    -e SAMURAI_SUBKEY="$subkey" \
    archlinux:base-devel \
    bash -es <<'EOS'
set -euo pipefail
pacman-key --init >/dev/null 2>&1
pacman -Syu --noconfirm >/dev/null      # stale-image guard (see ci/build-iso.sh)
pacman -S --noconfirm --needed namcap >/dev/null

# non-root builder, per spec 02 step 3
useradd -m builder

# trust the signing subkey in a throwaway GNUPGHOME
export GNUPGHOME=/keys/.gnupg
mkdir -m 700 /keys/.gnupg
gpg --batch --import /keys/subkey.asc
echo "$SAMURAI_SUBKEY:6:" | gpg --batch --import-ownertrust

mkdir /build && chown builder /build
mkdir -p /out/x86_64
out64=/out/x86_64
cd /build
for src in /src/pkgbuilds/*/; do
    pkg="$(basename "$src")"
    cp -a "$src" "$pkg"
    chown -R builder "$pkg"
    echo "==> makepkg $pkg"
    su builder -c "cd /build/$pkg && makepkg -sf --noconfirm"
    set -- /build/$pkg/*.pkg.tar.zst   # .sig files are created below, in the signing step
    cp -v "$@" "$out64/"
done

echo "==> signing packages + building samurai.db (SignedBy $SAMURAI_SUBKEY)"
cd "$out64"
for p in *.pkg.tar.zst; do
    [ -f "$p.sig" ] || gpg --batch --detach-sign --local-user "$SAMURAI_SUBKEY" "$p"
done
repo-add --sign samurai.db.tar.gz "$out64"/*.pkg.tar.zst
# current repo-add also builds and signs samurai.files.tar.gz itself (see output)
for f in samurai.db.tar.gz samurai.files.tar.gz; do
    [ -f "$f" ] || continue
    gpg --batch --detach-sign --local-user "$SAMURAI_SUBKEY" "$f"
done
ls -lh /out/x86_64
# hand the output back to the host user (root-owned bind-mount writes)
chown -R 1000:1000 /out
EOS

echo "==> repo built in $out_dir/x86_64"

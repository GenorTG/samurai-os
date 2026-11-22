#!/usr/bin/env bash
# M1: build, sign and assemble the [samurai] pacman repository locally.
# Spec: docs/specs/02-PACKAGE-REPOSITORY.md (Build pipeline steps 3, 4, 5).
#
# Usage: ci/build-repo.sh [out-dir]   (default: build/repo)
# Requires: docker, the signing subkey in ~/samurai-keys/gnupg (runbook: docs/KEYS.md).
# Output: out-dir/x86_64/ with *.pkg.tar.zst(.sig) + samurai.db.tar.gz(.sig) + samurai.files.tar.gz(.sig)
#
# Env:
#   GNUPGHOME                offline/CI keyring holding the signing subkey (default ~/samurai-keys/gnupg)
#   SAMURAI_PKGREL_OVERRIDE  monotonic pkgrel for samurai-tools/samurai-release (CI testing channel)
#   SAMURAI_BUILD_IMAGE      build container image (default archlinux:base-devel; pin a digest to freeze it)
#
# Key handling: the signing subkey is exported per run into a 0600 temp file,
# mounted read-only into a throwaway container, imported into a GNUPGHOME that
# lives only in the container's own filesystem, and shredded when this script
# exits. It is never written into the image or into out-dir.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out_dir="${1:-$repo_root/build/repo}"
out_dir="$(realpath -m "$out_dir")"
image="${SAMURAI_BUILD_IMAGE:-archlinux:base-devel}"
keyfile="$(mktemp /tmp/samurai-subkey.XXXXXX.asc)"   # mktemp creates it 0600
trap 'shred -u "$keyfile" 2>/dev/null || rm -f "$keyfile"' EXIT
container="samurai-repo-build-$$"

export GNUPGHOME="${GNUPGHOME:-$HOME/samurai-keys/gnupg}"
fpr_file="${SAMURAI_PRIMARY_FPR_FILE:-$HOME/samurai-keys/primary-fingerprint.txt}"
[ -f "$fpr_file" ] || { echo "error: $fpr_file missing (docs/KEYS.md)" >&2; exit 1; }
read -r primary < "$fpr_file"
case "$primary" in
    *[!0-9A-F]*|"") echo "error: $fpr_file does not hold an uppercase hex fingerprint" >&2; exit 1 ;;
esac
# The signing subkey: the first usable (not expired/revoked) subkey of the pinned
# primary that carries the sign capability (field 12 of --with-colons output).
subkey="$(gpg --batch --list-keys --with-colons --with-subkey-fingerprints "$primary" \
    | awk -F: '/^sub:/ { ok = ($2 != "e" && $2 != "r" && $12 ~ /s/); next }
               /^fpr:/ && ok { print $10; exit }')"
[ -n "$subkey" ] || { echo "error: no usable signing subkey under primary $primary in $GNUPGHOME" >&2; exit 1; }
gpg --batch --armor --export-secret-subkeys "$subkey!" > "$keyfile"
[ -s "$keyfile" ] || { echo "error: secret subkey export is empty (is the secret part in $GNUPGHOME?)" >&2; exit 1; }
chmod 600 "$keyfile"

mkdir -p "$out_dir"
# repo-add APPENDS to an existing db: a stale one would duplicate entries
# and resurrect removed packages. Always start the db from scratch.
rm -rf "$out_dir/x86_64"
mkdir -p "$out_dir/x86_64"

echo "==> building [samurai] packages in $container (image $image, primary $primary, subkey $subkey)"
docker run --rm -i --name "$container" \
    -v "$repo_root":/src:ro \
    -v "$out_dir":/out \
    -v "$keyfile":/keys/subkey.asc:ro \
    -e SAMURAI_PRIMARY="$primary" \
    -e SAMURAI_SUBKEY="$subkey" \
    -e SAMURAI_PKGREL_OVERRIDE="${SAMURAI_PKGREL_OVERRIDE:-}" \
    "$image" \
    bash -es <<'EOS'
set -euo pipefail
pacman-key --init >/dev/null 2>&1
pacman -Syu --noconfirm >/dev/null      # stale-image guard (see ci/build-iso.sh)
pacman -S --noconfirm --needed namcap >/dev/null

# non-root builder, per spec 02 step 3
useradd -m builder

# trust the signing key in a throwaway GNUPGHOME (ownertrust is keyed by the
# PRIMARY fingerprint; the subkey inherits it)
export GNUPGHOME=/keys/.gnupg
mkdir -m 700 /keys/.gnupg
gpg --batch --import /keys/subkey.asc
echo "$SAMURAI_PRIMARY:6:" | gpg --batch --import-ownertrust
gpg --batch --list-secret-keys --with-colons "$SAMURAI_PRIMARY" | grep -q "^fpr:.*:$SAMURAI_SUBKEY:" ||
    { echo "error: imported secret material does not contain subkey $SAMURAI_SUBKEY" >&2; exit 1; }

# The shipped keyring must carry the key that signs this build: a mismatch
# would publish packages no installed system can verify (docs/KEYS.md rotation).
ship_fprs="$(gpg --batch --show-keys --with-colons --with-subkey-fingerprints /src/pkgbuilds/samurai-keyring/build-key.txt | awk -F: '/^fpr:/ {print $10}')"
grep -qx "$SAMURAI_PRIMARY" <<<"$ship_fprs" || { echo "error: samurai-keyring/build-key.txt lacks primary $SAMURAI_PRIMARY" >&2; exit 1; }
grep -qx "$SAMURAI_SUBKEY" <<<"$ship_fprs" || { echo "error: samurai-keyring/build-key.txt lacks signing subkey $SAMURAI_SUBKEY" >&2; exit 1; }

# spec 02 step 4: the repo rules, inside the container where pacman knows Arch's names
/src/ci/check-no-shadowing /src/pkgbuilds
/src/ci/check-no-epoch /src/pkgbuilds

mkdir /build && chown builder /build
mkdir -p /out/x86_64
out64=/out/x86_64
cd /build
for src in /src/pkgbuilds/*/; do
    pkg="$(basename "$src")"
    cp -a "$src" "$pkg"
    cp -a /src/LICENSE "$pkg/LICENSE"
    if [ "$pkg" = samurai-tools ]; then
        for name in samurai samurai-update samurai-snap samurai-rescue samurai-apps samurai-notifier \
                    samurai-notifier.service samurai-notifier.timer; do
            cp -a "/src/updates/$name" "$pkg/"
        done
        cp -a /src/updates/interventions/0001-update-mechanism-demo.toml "$pkg/"
        cp -a /src/updates/interventions/0001-update-mechanism-demo.sh "$pkg/"
        cp -a /src/catalog/garuda-setup-wizard.toml /src/catalog/samurai.toml "$pkg/"
    fi
    if [ -n "$SAMURAI_PKGREL_OVERRIDE" ] && \
       { [ "$pkg" = samurai-tools ] || [ "$pkg" = samurai-release ]; }; then
        sed -i "s/^pkgrel=.*/pkgrel=$SAMURAI_PKGREL_OVERRIDE/" "$pkg/PKGBUILD"
    fi
    chown -R builder "$pkg"
    echo "==> makepkg $pkg"
    # --nodeps: runtime depends on our own packages (samurai-release ->
    # samurai-tools) cannot be installed from a repo this very run creates;
    # none of these packages compile anything, so build deps are not needed
    su builder -c "cd /build/$pkg && makepkg -df --noconfirm"
    set -- /build/$pkg/*.pkg.tar.zst   # .sig files are created below, in the signing step
    # spec 02 step 4: namcap lint (advisory; its exit code is not a verdict)
    su builder -c "cd /build/$pkg && namcap PKGBUILD $* 2>&1 | sed 's/^/    namcap: /'" || true
    cp -v "$@" "$out64/"
done

echo "==> signing packages + building samurai.db (SignedBy $SAMURAI_SUBKEY)"
cd "$out64"
for p in *.pkg.tar.zst; do
    [ -f "$p.sig" ] || gpg --batch --detach-sign --local-user "$SAMURAI_SUBKEY!" "$p"
done
# --verify checks the signature of an existing db before touching it (none
# here: the dir is fresh), --sign signs the new db and files archives
repo-add --sign --verify --key "$SAMURAI_SUBKEY!" samurai.db.tar.gz "$out64"/*.pkg.tar.zst || true
# current repo-add also builds and signs samurai.files.tar.gz; its trailing
# re-sign step can abort with 'File exists' after both dbs are valid, so we
# verify instead of trusting its exit code:
verify_sig() {   # $1 = file; the signature must be VALID and made by OUR subkey
    local status
    status="$(gpg --batch --status-fd 1 --verify "$1.sig" "$1" 2>/dev/null)" ||
        { echo "error: bad signature on $1" >&2; return 1; }
    grep -q "^\[GNUPG:\] VALIDSIG $SAMURAI_SUBKEY " <<<"$status" ||
        { echo "error: $1 is not signed by subkey $SAMURAI_SUBKEY" >&2; return 1; }
}
test -s samurai.db.tar.gz && test -s samurai.db.tar.gz.sig
test -s samurai.files.tar.gz && test -s samurai.files.tar.gz.sig
verify_sig samurai.db.tar.gz
verify_sig samurai.files.tar.gz
for p in *.pkg.tar.zst; do verify_sig "$p"; done
# every built package must be in the db, and nothing else may be
db_names="$(tar -xOf samurai.db.tar.gz --wildcards '*/desc' | awk '/^%FILENAME%$/ {getline; print}' | sort)"
dir_names="$(ls -- *.pkg.tar.zst | sort)"
[ "$db_names" = "$dir_names" ] || {
    echo "error: samurai.db entries differ from the package files:" >&2
    diff <(echo "$db_names") <(echo "$dir_names") >&2 || true
    exit 1
}
echo "==> verified: $(wc -l <<<"$dir_names") packages, all signed by $SAMURAI_SUBKEY, db + files signed"
EOS

echo "==> done:"
ls -lh "$out_dir/x86_64"
# ownership handback independent of the container's exit code: root-owned
# bind-mount writes are invisible to pacman on the host otherwise
# (learned: the in-container chown is skipped when set -e aborts first)
docker run --rm -v "$out_dir":/o "$image" chown -R "$(id -u):$(id -g)" /o >/dev/null

echo "==> repo built in $out_dir/x86_64"

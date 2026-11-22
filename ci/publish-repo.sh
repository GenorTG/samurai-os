#!/usr/bin/env bash
# Publish the built [samurai] repo to GitHub Releases atomically (spec 02).
#
# CRITICAL INVARIANT (learned the hard way, 2026-11-17): the db and its
# package assets must come from the SAME build. makepkg's zstd output is not
# byte-deterministic, so a db from run A + assets from run B gives
# "Maximum file size exceeded" on the client (pacman enforces the db's csize).
# This script therefore ALWAYS uploads the complete current set, in this order:
#   1. new packages + signatures (so every file the new db names exists),
#   2. the db/files archives + signatures LAST (the switch-over point),
#   3. only then assets the new db no longer names (the OLD db stopped
#      referencing them at step 2, so no client can still be told to fetch them).
# A failure before step 2 leaves the previous db and its assets intact.
#
# Usage: ci/publish-repo.sh [repo-dir] [channel]
#   repo-dir  default build/repo (ci/build-repo.sh output)
#   channel   release tag in GenorTG/samurai-repo: x86_64 (stable) or x86_64-testing
# Requires: gh authenticated with a token that can write Contents of GenorTG/samurai-repo
# (fine-grained, that repo only: docs/KEYS.md), gpg, python3.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
repo_dir="${1:-$repo_root/build/repo}"
repo_dir="$(realpath -m "$repo_dir")"
channel="${2:-x86_64}"
gh_repo="${SAMURAI_REPO_SLUG:-GenorTG/samurai-repo}"
asset_limit=$((2 * 1024 * 1024 * 1024))   # GitHub release asset cap (spec 02)
case "$channel" in
    x86_64|x86_64-testing) ;;
    *) echo "error: unknown channel '$channel' (x86_64 | x86_64-testing)" >&2; exit 2 ;;
esac

cd "$repo_dir/$channel" 2>/dev/null || cd "$repo_dir/x86_64"
shopt -s nullglob
pkgs=(*.pkg.tar.zst)
[ "${#pkgs[@]}" -gt 0 ] || { echo "error: no packages in $PWD" >&2; exit 1; }
[ -s samurai.db.tar.gz ] && [ -s samurai.files.tar.gz ] || { echo "error: db/files archives missing in $PWD" >&2; exit 1; }

# flat db/file copies under the names pacman requests (no symlinks on Releases)
for pair in "samurai.db.tar.gz samurai.db" "samurai.db.tar.gz.sig samurai.db.sig" \
            "samurai.files.tar.gz samurai.files" "samurai.files.tar.gz.sig samurai.files.sig"; do
    set -- $pair
    [ -f "$1" ] || { echo "error: $1 missing: every published db must be signed" >&2; exit 1; }
    # repo-add leaves samurai.db etc. as SYMLINKS to the archives; cp onto the
    # symlink is "the same file" and fails. Replace the link with a real copy.
    rm -f "$2"; cp -f "$1" "$2"
done

# Every signature must verify against the key the samurai-keyring PACKAGE ships:
# that is the only key an installed system trusts, so anything else would publish
# packages nobody can install. A throwaway GNUPGHOME keeps this independent of the
# build keyring.
verify_home="$(mktemp -d)"
trap 'rm -rf "$verify_home"' EXIT
chmod 700 "$verify_home"
gpg --batch --homedir "$verify_home" --import "$repo_root/pkgbuilds/samurai-keyring/build-key.txt" 2>/dev/null
for f in "${pkgs[@]}" samurai.db samurai.files; do
    [ -f "$f.sig" ] || { echo "error: $f has no signature" >&2; exit 1; }
    gpg --batch --homedir "$verify_home" --verify "$f.sig" "$f" 2>/dev/null ||
        { echo "error: $f.sig does not verify against pkgbuilds/samurai-keyring/build-key.txt" >&2; exit 1; }
done
echo "==> all signatures verify against the shipped keyring"

# sanity: db csize must equal the asset file size for every package, the db
# must name exactly the files present, and no asset may exceed GitHub's cap
python3 - "$asset_limit" <<'PY'
import glob, os, sys, tarfile
limit = int(sys.argv[1])
db = tarfile.open("samurai.db.tar.gz")
ok = True
named = set()
for m in db.getmembers():
    if not m.name.endswith("/desc"):
        continue
    txt = db.extractfile(m).read().decode()
    pkg = txt.split("%NAME%\n")[1].splitlines()[0]
    fname = txt.split("%FILENAME%\n")[1].splitlines()[0]
    csize = int(txt.split("%CSIZE%\n")[1].splitlines()[0])
    named.add(fname)
    if not os.path.isfile(fname):
        print(f"MISSING asset for {pkg}: {fname}"); ok = False; continue
    size = os.path.getsize(fname)
    if size != csize:
        print(f"CSIZE MISMATCH for {pkg}: db={csize} file={size}"); ok = False
    else:
        print(f"OK {pkg}: {size} bytes")
present = set(glob.glob("*.pkg.tar.zst"))
for extra in sorted(present - named):
    print(f"NOT IN DB: {extra}"); ok = False
for f in sorted(present | {"samurai.db", "samurai.files"}):
    if os.path.getsize(f) >= limit:
        print(f"TOO LARGE for a release asset: {f}"); ok = False
if not ok:
    raise SystemExit(1)
PY

# Bootstrap a channel release on its first publish. Later runs replace the
# same rolling release's assets package-first and database-last below.
if ! gh release view "$channel" --repo "$gh_repo" >/dev/null 2>&1; then
    extra=()
    case "$channel" in *-testing) extra=(--prerelease) ;; esac
    gh release create "$channel" --repo "$gh_repo" "${extra[@]}" --latest=false \
        --title "Samurai package repository ($channel)" \
        --notes "Rolling signed pacman repository channel. Assets are replaced by the Samurai OS build workflow; do not download them by hand, point pacman at this tag (samurai-mirrorlist)."
fi

mapfile -t existing < <(gh release view "$channel" --repo "$gh_repo" --json assets -q '.assets[].name')

# 1. packages + sigs (clobber: a re-run of the same build is harmless)
keep=("${pkgs[@]}" "${pkgs[@]/%/.sig}")
gh release upload "$channel" --repo "$gh_repo" --clobber "${keep[@]}"
# 2. the db LAST: this is the atomic switch-over
gh release upload "$channel" --repo "$gh_repo" --clobber \
    samurai.db samurai.db.sig samurai.files samurai.files.sig
# 3. stale assets: nothing published now references them
for a in "${existing[@]}"; do
    case "$a" in
        samurai.db|samurai.db.sig|samurai.files|samurai.files.sig) continue ;;
    esac
    stale=1
    for k in "${keep[@]}"; do [ "$a" = "$k" ] && { stale=0; break; }; done
    [ "$stale" -eq 1 ] || continue
    echo "==> removing stale asset $a"
    gh release delete-asset "$channel" "$a" --repo "$gh_repo" -y
done

# post-condition: the remote set is exactly the local set, byte sizes included
python3 - "$(gh release view "$channel" --repo "$gh_repo" --json assets -q '.assets[] | "\(.name) \(.size)"')" "${keep[@]}" samurai.db samurai.db.sig samurai.files samurai.files.sig <<'PY'
import os, sys
remote = dict(line.rsplit(" ", 1) for line in sys.argv[1].splitlines() if line)
local = {f: str(os.path.getsize(f)) for f in sys.argv[2:]}
if remote != local:
    for k in sorted(set(remote) | set(local)):
        if remote.get(k) != local.get(k):
            print(f"MISMATCH {k}: remote={remote.get(k)} local={local.get(k)}")
    raise SystemExit(1)
print(f"==> remote assets verified: {len(local)} files")
PY

echo "==> published $channel: consistent set, db last, stale assets removed"

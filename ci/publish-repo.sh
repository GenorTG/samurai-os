#!/usr/bin/env bash
# Publish the built [samurai] repo to GitHub Releases atomically (spec 02).
#
# CRITICAL INVARIANT (learned the hard way, 2026-11-17): the db and its
# package assets must come from the SAME build. makepkg's zstd output is not
# byte-deterministic, so a db from run A + assets from run B gives
# "Maximum file size exceeded" on the client (pacman enforces the db's csize).
# This script therefore ALWAYS uploads the complete current set: packages
# first, db/files last; assets that are no longer part of the build are removed.
#
# Usage: ci/publish-repo.sh [repo-dir]   (default: build/repo)
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
repo_dir="${1:-$repo_root/build/repo}"
repo_dir="$(realpath -m "$repo_dir")"
channel="${2:-x86_64}"
gh_repo="GenorTG/samurai-repo"

cd "$repo_dir/$channel"

# Bootstrap a channel release on its first publish. Later runs replace the
# same rolling release's assets package-first and database-last below.
if ! gh release view "$channel" --repo "$gh_repo" >/dev/null 2>&1; then
    gh release create "$channel" --repo "$gh_repo" \
        --title "Samurai package repository ($channel)" \
        --notes "Rolling signed pacman repository channel. Assets are replaced by the Samurai OS build workflow."
fi

# flat db/file copies under the names pacman requests (no symlinks on Releases)
for pair in "samurai.db.tar.gz samurai.db" "samurai.db.tar.gz.sig samurai.db.sig" \
            "samurai.files.tar.gz samurai.files" "samurai.files.tar.gz.sig samurai.files.sig"; do
    set -- $pair
    # repo-add leaves samurai.db etc. as SYMLINKS to the archives; cp onto the
    # symlink is "the same file" and fails. Replace the link with a real copy.
    if [ -f "$1" ]; then rm -f "$2"; cp -f "$1" "$2"; fi
done

# sanity: db csize must equal the asset file size for every package
python3 - <<'PY'
import glob, os, tarfile
db = tarfile.open("samurai.db.tar.gz")
ok = True
for m in db.getmembers():
    if not m.name.endswith("/desc"):
        continue
    txt = db.extractfile(m).read().decode()
    pkg = txt.split("%NAME%\n")[1].splitlines()[0]
    csize = int(txt.split("%CSIZE%\n")[1].splitlines()[0])
    actual = glob.glob(f"{pkg}-*.pkg.tar.zst")
    if len(actual) != 1:
        print(f"AMBIGUOUS/MISSING asset for {pkg}: {actual}"); ok = False; continue
    size = os.path.getsize(actual[0])
    if size != csize:
        print(f"CSIZE MISMATCH for {pkg}: db={csize} file={size}"); ok = False
    else:
        print(f"OK {pkg}: {size} bytes")
if not ok:
    raise SystemExit(1)
PY

# packages + sigs first (stale ones removed), db/files LAST
keep=(*.pkg.tar.zst *.pkg.tar.zst.sig)
existing=$(gh release view "$channel" --repo "$gh_repo" --json assets -q '.assets[].name')
for a in $existing; do
    case " ${keep[*]} " in
        *" $a "*) : ;;
        samurai.db|samurai.db.sig|samurai.files|samurai.files.sig) : ;;
        *) gh release delete-asset "$channel" "$a" --repo "$gh_repo" -y 2>&1 | grep -v "^$" || true ;;
    esac
done
gh release upload "$channel" --repo "$gh_repo" --clobber "${keep[@]}" 2>&1 | tail -1
gh release upload "$channel" --repo "$gh_repo" --clobber \
    samurai.db samurai.db.sig samurai.files samurai.files.sig 2>&1 | tail -1

echo "==> published $channel: consistent set, db last"

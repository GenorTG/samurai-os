#!/usr/bin/env bash
# Package built ISOs as a GitHub release (spec 08: installers).
#
# Usage: ci/release-iso.sh <online.iso> <offline.iso> [--sign] [--publish]
#                           [--verified FILE] [--new FILE]
#
# Writes dist/<tag>/:
#   samurai-os-<date>-online-x86_64.iso          (fits GitHub's 2 GiB asset limit)
#   samurai-os-<date>-offline-x86_64.iso.001...  (offline ISO split in 1900 MiB parts)
#   SHA256SUMS                                   (every asset AND the joined offline ISO)
#   SHA256SUMS.sig                               (--sign: detached signature by the build subkey)
#   RELEASE-NOTES.md
# --verified FILE  markdown body of the notes' "Verified" section: what was really
#           tested for THIS release, and what was not. Required by --publish: the
#           script holds no verification claim of its own.
# --new FILE       markdown body of the "New in this release" section (left out
#           when not given).
# --sign    signs SHA256SUMS with the signing subkey in $GNUPGHOME (default
#           ~/samurai-keys/gnupg, docs/KEYS.md); users verify it with the public
#           key shipped in pkgbuilds/samurai-keyring/build-key.txt.
# --publish creates the release v<VERSION> on GenorTG/samurai-os with gh and uploads
#           everything. Re-running is safe: an existing release keeps its tag and only
#           gets its notes and assets refreshed. Without --publish the script only
#           prepares and verifies dist/.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
online="${1:?online ISO path}"; offline="${2:?offline ISO path}"; shift 2
publish=0; sign=0; verified_file=""; new_file=""
while [ $# -gt 0 ]; do
    case "$1" in
        --publish) publish=1 ;;
        --sign) sign=1 ;;
        --verified) verified_file="${2:?--verified needs a file}"; shift ;;
        --new) new_file="${2:?--new needs a file}"; shift ;;
        *) echo "error: unknown argument $1 (--sign | --publish | --verified FILE | --new FILE)" >&2; exit 2 ;;
    esac
    shift
done
for f in "$verified_file" "$new_file"; do
    [ -z "$f" ] && continue
    { [ -f "$f" ] && grep -q '[^[:space:]]' "$f"; } ||
        { echo "error: release notes file $f is missing or empty" >&2; exit 2; }
done
if [ "$publish" -eq 1 ] && [ -z "$verified_file" ]; then
    echo "error: --publish needs --verified FILE (what was tested for this release)" >&2
    exit 2
fi
version="$(tr -d '[:space:]' < "$repo_root/VERSION")"
case "$version" in
    *[!0-9.]*|"") echo "error: VERSION '$version' is not dotted digits" >&2; exit 2 ;;
esac
tag="v$version"
gh_repo="${SAMURAI_OS_SLUG:-GenorTG/samurai-os}"
dist="$repo_root/dist/$tag"
part_size=$((1900 * 1024 * 1024))
asset_limit=$((2 * 1024 * 1024 * 1024))

for f in "$online" "$offline"; do
    [ -f "$f" ] || { echo "error: missing ISO $f" >&2; exit 2; }
done
case "$(basename "$online")" in *-online-x86_64.iso) ;; *) echo "error: $online is not an online ISO" >&2; exit 2 ;; esac
case "$(basename "$offline")" in *-offline-x86_64.iso) ;; *) echo "error: $offline is not an offline ISO" >&2; exit 2 ;; esac
if [ "$(stat -c %s "$online")" -ge "$asset_limit" ]; then
    echo "error: online ISO exceeds the 2 GiB release asset limit" >&2; exit 2
fi

mkdir -p "$dist"
rm -f "$dist"/*
cp --reflink=auto "$online" "$dist/"
off_name="$(basename "$offline")"
on_name="$(basename "$online")"
split -b "$part_size" -d -a 3 --numeric-suffixes=1 "$offline" "$dist/$off_name."

(
    cd "$dist"
    # the joined ISO's sum first: `sha256sum -c --ignore-missing` checks parts
    # before joining and the full ISO after
    sha256sum "$offline" | sed "s|  .*|  $off_name|" > SHA256SUMS
    sha256sum "$on_name" "$off_name".[0-9][0-9][0-9] >> SHA256SUMS
    # self-check: the parts must rejoin to the exact original
    joined="$(cat "$off_name".[0-9][0-9][0-9] | sha256sum | cut -d' ' -f1)"
    grep -q "^$joined  $off_name\$" SHA256SUMS ||
        { echo "error: split parts do not rejoin to the offline ISO" >&2; exit 1; }
    # and the checksum file must be usable the way the notes tell users to use it
    sha256sum -c --quiet --ignore-missing SHA256SUMS
)

if [ "$sign" -eq 1 ]; then
    export GNUPGHOME="${GNUPGHOME:-$HOME/samurai-keys/gnupg}"
    fpr_file="${SAMURAI_PRIMARY_FPR_FILE:-$HOME/samurai-keys/primary-fingerprint.txt}"
    read -r primary < "$fpr_file"
    subkey="$(gpg --batch --list-keys --with-colons --with-subkey-fingerprints "$primary" \
        | awk -F: '/^sub:/ { ok = ($2 != "e" && $2 != "r" && $12 ~ /s/); next }
                   /^fpr:/ && ok { print $10; exit }')"
    [ -n "$subkey" ] || { echo "error: no usable signing subkey under $primary in $GNUPGHOME" >&2; exit 1; }
    gpg --batch --yes --detach-sign --local-user "$subkey!" --output "$dist/SHA256SUMS.sig" "$dist/SHA256SUMS"
    # must verify with the PUBLIC key users get from the keyring package
    vh="$(mktemp -d)"; chmod 700 "$vh"
    gpg --batch --homedir "$vh" --import "$repo_root/pkgbuilds/samurai-keyring/build-key.txt" 2>/dev/null
    gpg --batch --homedir "$vh" --verify "$dist/SHA256SUMS.sig" "$dist/SHA256SUMS" 2>/dev/null ||
        { rm -rf "$vh"; echo "error: SHA256SUMS.sig does not verify with pkgbuilds/samurai-keyring/build-key.txt" >&2; exit 1; }
    rm -rf "$vh"
    echo "==> SHA256SUMS signed by subkey $subkey (verifies with the shipped keyring)"
fi

parts=$(cd "$dist" && ls "$off_name".[0-9][0-9][0-9] | wc -l)
if [ "$sign" -eq 1 ]; then
    verify_block="\`SHA256SUMS\` is signed with the Samurai OS build key. To check the signature, import
the public key that ships in the [\`samurai-keyring\` package](https://github.com/GenorTG/samurai-os/blob/main/pkgbuilds/samurai-keyring/build-key.txt)
(primary fingerprint \`DFE8C148 C042AA3C 199AAEA0 8436E127 31EDD3FA\`, see \`docs/KEYS.md\`):

\`\`\`sh
curl -fsSLO https://raw.githubusercontent.com/GenorTG/samurai-os/main/pkgbuilds/samurai-keyring/build-key.txt
gpg --import build-key.txt
gpg --verify SHA256SUMS.sig SHA256SUMS
\`\`\`"
else
    verify_block="This release's \`SHA256SUMS\` is not GPG-signed; checksums protect against corrupt
downloads, not against a tampered release page."
fi
cat > "$dist/RELEASE-NOTES.md" <<EOF
## Samurai OS $tag

Arch Linux underneath, a thin signed layer on top. KDE Plasma, \`linux-cachyos\` + \`linux-zen\`,
LUKS2 + btrfs, staged updates with snapshots and a rescue rollback.

> Early, personal release. What was tested is listed under "Verified"; there is no physical-hardware
> guarantee. Back up before installing on a real machine.

### Pick an installer

| Installer | File | Network during install |
|-----------|------|------------------------|
| **Online** (netinstall) | \`$on_name\` ($(du -h "$online" | cut -f1)) | required: pulls current packages |
| **Offline** (full) | \`$off_name\` in $parts parts ($(du -h "$offline" | cut -f1)) | not needed: KDE, kernels and GPU drivers are on the ISO |

GitHub limits release files to 2 GiB, so the offline ISO is split. Download every
\`$off_name.0NN\` part and \`SHA256SUMS\` into one folder, then:

\`\`\`sh
sha256sum -c --ignore-missing SHA256SUMS          # parts
cat $off_name.0* > $off_name
sha256sum -c --ignore-missing SHA256SUMS          # joined ISO
\`\`\`

$verify_block

Write either ISO to a USB stick (\`sudo dd if=<iso> of=/dev/sdX bs=4M status=progress oflag=sync\`, or
copy it onto a Ventoy stick) and boot it. The live system is a KDE Plasma desktop with icons for the
graphical installer, the terminal installer (\`samurai-install\`) and rescue.
EOF
# per-release text comes from files: inserted verbatim, never expanded by the shell
{
    if [ -n "$new_file" ]; then printf '\n### New in this release\n'; cat -- "$new_file"; fi
    if [ -n "$verified_file" ]; then
        printf '\n### Verified\n'; cat -- "$verified_file"
    else
        echo "warning: no --verified file: these notes cannot be published" >&2
    fi
    printf '\nSource, specs and VM tests: <https://github.com/GenorTG/samurai-os>. Website: <https://genortg.github.io/samurai-os/>.\n'
} >> "$dist/RELEASE-NOTES.md"

echo "==> dist ready: $dist"
ls -lh "$dist"
cat "$dist/SHA256SUMS"

if [ "$publish" -eq 1 ]; then
    git -C "$repo_root" diff --quiet && git -C "$repo_root" diff --cached --quiet ||
        { echo "error: commit the tree before publishing" >&2; exit 2; }
    head="$(git -C "$repo_root" rev-parse HEAD)"
    if git -C "$repo_root" rev-parse -q --verify "refs/tags/$tag" >/dev/null; then
        # an existing tag must be THIS commit; a stale local tag would publish the wrong tree
        [ "$(git -C "$repo_root" rev-parse "$tag^{commit}")" = "$head" ] ||
            { echo "error: tag $tag exists but does not point at HEAD ($head)" >&2; exit 2; }
    else
        git -C "$repo_root" tag -a "$tag" -m "Samurai OS $tag"
    fi
    git -C "$repo_root" push origin "refs/tags/$tag"
    if gh release view "$tag" --repo "$gh_repo" >/dev/null 2>&1; then
        echo "==> release $tag exists: refreshing notes and assets"
        gh release edit "$tag" --repo "$gh_repo" --title "Samurai OS $tag" --notes-file "$dist/RELEASE-NOTES.md"
    else
        # --verify-tag: refuse to create a release for a tag that is not on the remote
        gh release create "$tag" --repo "$gh_repo" --title "Samurai OS $tag" \
            --notes-file "$dist/RELEASE-NOTES.md" --verify-tag
    fi
    assets=("$dist"/SHA256SUMS)
    [ -f "$dist/SHA256SUMS.sig" ] && assets+=("$dist/SHA256SUMS.sig")
    assets+=("$dist"/*.iso "$dist"/*.iso.[0-9][0-9][0-9])
    # one asset per call: a failed multi-GB upload retries alone
    for f in "${assets[@]}"; do
        ok=0
        for attempt in 1 2 3 4; do
            if gh release upload "$tag" "$f" --repo "$gh_repo" --clobber; then ok=1; break; fi
            echo "upload of $(basename "$f") failed (attempt $attempt), retrying" >&2
            sleep $((attempt * 15))
        done
        [ "$ok" -eq 1 ] || { echo "error: giving up on $(basename "$f")" >&2; exit 1; }
    done
    # post-condition: every local asset is on the release with the same byte size
    python3 - "$(gh release view "$tag" --repo "$gh_repo" --json assets --jq '.assets[] | "\(.name) \(.size)"')" "${assets[@]}" <<'PY'
import os, sys
remote = dict(line.rsplit(" ", 1) for line in sys.argv[1].splitlines() if line)
bad = [f for f in sys.argv[2:] if remote.get(os.path.basename(f)) != str(os.path.getsize(f))]
for f in bad:
    print(f"MISMATCH {os.path.basename(f)}: remote={remote.get(os.path.basename(f))} local={os.path.getsize(f)}")
if bad:
    raise SystemExit(1)
print(f"==> release assets verified: {len(sys.argv) - 2} files")
PY
fi

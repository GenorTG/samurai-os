#!/usr/bin/env bash
# Package built ISOs as a GitHub release (spec 08: installers).
#
# Usage: ci/release-iso.sh <online.iso> <offline.iso> [--publish]
#
# Writes dist/<tag>/:
#   samurai-os-<date>-online-x86_64.iso          (fits GitHub's 2 GiB asset limit)
#   samurai-os-<date>-offline-x86_64.iso.001...  (offline ISO split in 1900 MiB parts)
#   SHA256SUMS                                   (every asset AND the joined offline ISO)
#   RELEASE-NOTES.md
# --publish creates the release v<VERSION> on GenorTG/samurai-os with gh and uploads
# everything. Without it the script only prepares and verifies dist/.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
online="${1:?online ISO path}"; offline="${2:?offline ISO path}"; publish="${3:-}"
version="$(cat "$repo_root/VERSION")"
tag="v$version"
dist="$repo_root/dist/$tag"
part_size=$((1900 * 1024 * 1024))

for f in "$online" "$offline"; do
    [ -f "$f" ] || { echo "error: missing ISO $f" >&2; exit 2; }
done
case "$(basename "$online")" in *-online-x86_64.iso) ;; *) echo "error: $online is not an online ISO" >&2; exit 2 ;; esac
case "$(basename "$offline")" in *-offline-x86_64.iso) ;; *) echo "error: $offline is not an offline ISO" >&2; exit 2 ;; esac
if [ "$(stat -c %s "$online")" -ge $((2 * 1024 * 1024 * 1024)) ]; then
    echo "error: online ISO exceeds the 2 GiB release asset limit" >&2; exit 2
fi

mkdir -p "$dist"
rm -f "$dist"/*
cp --reflink=auto "$online" "$dist/"
off_name="$(basename "$offline")"
split -b "$part_size" -d -a 3 --numeric-suffixes=1 "$offline" "$dist/$off_name."

(
    cd "$dist"
    # the joined ISO's sum first: `sha256sum -c --ignore-missing` checks parts
    # before joining and the full ISO after
    sha256sum "$offline" | sed "s|  .*|  $off_name|" > SHA256SUMS
    sha256sum "$(basename "$online")" "$off_name".[0-9][0-9][0-9] >> SHA256SUMS
    # self-check: the parts must rejoin to the exact original
    cat "$off_name".[0-9][0-9][0-9] | sha256sum | cut -d' ' -f1 > /tmp/samurai-joined.sum
    grep -q "^$(cat /tmp/samurai-joined.sum)  $off_name$" SHA256SUMS ||
        { echo "error: split parts do not rejoin to the offline ISO" >&2; exit 1; }
)

parts=$(cd "$dist" && ls "$off_name".[0-9][0-9][0-9] | wc -l)
cat > "$dist/RELEASE-NOTES.md" <<EOF
## Samurai OS $tag

Arch Linux underneath, a thin signed layer on top. KDE Plasma, \`linux-cachyos\` + \`linux-zen\`,
LUKS2 + btrfs, staged updates with snapshots and a rescue rollback.

> Early, personal release. Every feature listed here passes its VM scenario on QEMU/KVM; no
> physical-hardware guarantee yet. Back up before installing on a real machine.

### Pick an installer

| Installer | File | Network during install |
|-----------|------|------------------------|
| **Online** (netinstall) | \`$(basename "$online")\` ($(du -h "$online" | cut -f1)) | required: pulls current packages |
| **Offline** (full) | \`$off_name\` in $parts parts ($(du -h "$offline" | cut -f1)) | not needed: KDE, kernels and GPU drivers are on the ISO |

GitHub limits release files to 2 GiB, so the offline ISO is split. Download every
\`$off_name.0NN\` part and \`SHA256SUMS\` into one folder, then:

\`\`\`sh
sha256sum -c --ignore-missing SHA256SUMS          # parts
cat $off_name.0* > $off_name
sha256sum -c --ignore-missing SHA256SUMS          # joined ISO
\`\`\`

Write either ISO to a USB stick (\`sudo dd if=<iso> of=/dev/sdX bs=4M status=progress oflag=sync\`, or
copy it onto a Ventoy stick), boot it and run \`samurai-install\` for the guided wizard.

### Verified in this release (QEMU/KVM, OVMF)
- Offline install of the full KDE desktop with a dead mirror pinned: no network used.
- LUKS2 + btrfs subvolumes, multi-kernel boot entries (linux-cachyos, linux-zen), SDDM up, no failed units.
- GPU stacks installed and checked by package: Mesa/RADV/ANV/NVK, Intel media, NVIDIA open modules per kernel.
- Staged updates from the signed \`[samurai]\` repo, snapshot + rescue rollback, attended wizard install.

Source, specs and VM tests: <https://github.com/GenorTG/samurai-os>. Website: <https://genortg.github.io/samurai-os/>.
EOF

echo "==> dist ready: $dist"
ls -lh "$dist"
cat "$dist/SHA256SUMS"

if [ "$publish" = --publish ]; then
    git -C "$repo_root" diff --quiet && git -C "$repo_root" diff --cached --quiet ||
        { echo "error: commit the tree before publishing" >&2; exit 2; }
    git -C "$repo_root" tag -a "$tag" -m "Samurai OS $tag" 2>/dev/null || true
    git -C "$repo_root" push origin "$tag"
    gh release create "$tag" --repo GenorTG/samurai-os --title "Samurai OS $tag" \
        --notes-file "$dist/RELEASE-NOTES.md" --verify-tag
    # one asset per call: a failed multi-GB upload retries alone
    for f in "$dist"/SHA256SUMS "$dist"/*.iso "$dist"/*.iso.[0-9][0-9][0-9]; do
        for attempt in 1 2 3; do
            gh release upload "$tag" "$f" --repo GenorTG/samurai-os --clobber && break
            echo "upload of $(basename "$f") failed (attempt $attempt), retrying" >&2
            sleep 10
        done
    done
    gh release view "$tag" --repo GenorTG/samurai-os --json assets --jq '.assets[] | "\(.name) \(.size)"'
fi

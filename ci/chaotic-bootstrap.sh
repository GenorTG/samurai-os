#!/usr/bin/env bash
# Trust Chaotic-AUR inside a throwaway build container (ADR-0007) and install
# its keyring + mirrorlist. Runs as root after `pacman-key --init`; the
# repository is mounted at /src. The caller adds the [chaotic-aur] section.
#
# Trust chain: the primary public key is vendored in this repository and pinned
# by fingerprint; both bootstrap packages must carry a valid signature made
# under that primary before pacman installs them.
set -euo pipefail

CHAOTIC_FPR=EF925EA60F33D0CB85C44AD13056513887B78AEB
key="${1:-/src/iso/airootfs/usr/share/samurai/chaotic-key.asc}"
got="$(gpg --batch --show-keys --with-colons "$key" \
    | awk -F: '/^pub:/ {p=1; n++; next} p && /^fpr:/ {print $10; p=0} END {if (n != 1) exit 1}')" ||
    { echo "error: $key must hold exactly one primary key" >&2; exit 1; }
[ "$got" = "$CHAOTIC_FPR" ] || { echo "error: $key primary is $got, pinned $CHAOTIC_FPR" >&2; exit 1; }
pacman-key --add "$key" >/dev/null
pacman-key --lsign-key "$CHAOTIC_FPR" >/dev/null

tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT
for name in chaotic-keyring chaotic-mirrorlist; do
    file="$tmp/$name.pkg.tar.zst"
    for host in cdn-mirror geo-mirror; do
        curl -fsSL --retry 4 -o "$file" "https://$host.chaotic.cx/chaotic-aur/$name.pkg.tar.zst" &&
            curl -fsSL --retry 4 -o "$file.sig" "https://$host.chaotic.cx/chaotic-aur/$name.pkg.tar.zst.sig" &&
            break
    done
    status="$(gpg --homedir /etc/pacman.d/gnupg --batch --status-fd 1 --verify "$file.sig" "$file" 2>/dev/null)" ||
        { echo "error: bad signature on $name" >&2; exit 1; }
    # the last VALIDSIG field is the primary key of whoever signed
    awk -v fpr="$CHAOTIC_FPR" '$2 == "VALIDSIG" && $NF == fpr {ok = 1} END {exit !ok}' <<<"$status" ||
        { echo "error: $name is not signed under $CHAOTIC_FPR" >&2; exit 1; }
done
pacman -U --noconfirm "$tmp"/*.pkg.tar.zst >/dev/null
pacman-key --populate chaotic >/dev/null
echo "==> Chaotic-AUR trusted (pinned $CHAOTIC_FPR)"

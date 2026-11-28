#!/usr/bin/env bash
# Spec 07 "Testing": the REAL chwd binary with the Samurai profiles, and the real
# `samurai hw`, against recorded/synthetic machines (tests/hw-fixtures/*.hw). Each
# machine is a fake /sys/bus/pci and DMI directory mounted read-only into an
# UNPRIVILEGED container: nothing here can reach the host's devices or modules.
#
# Usage: ci/test-hw.sh [--e2e]
#   --e2e  also run `samurai hw apply` for real inside a throwaway container
#          (downloads ~1.5 GB of driver packages; tests/hw/e2e.sh)
# Env: SAMURAI_LOCAL_PKGS  dir with samurai-chwd + samurai-hardware packages
#                          (default build/localpkgs, built when missing)
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
pkgs="$(realpath -m "${SAMURAI_LOCAL_PKGS:-$repo_root/build/localpkgs}")"
work="$repo_root/build/hwtest"
image=samurai-hw-test:local
e2e=0
[ "${1:-}" = --e2e ] && e2e=1

compgen -G "$pkgs/samurai-hardware-*.pkg.tar.zst" >/dev/null &&
    compgen -G "$pkgs/samurai-chwd-*.pkg.tar.zst" >/dev/null ||
    "$repo_root/ci/build-pkgs.sh" "$pkgs" samurai-chwd samurai-hardware

rm -rf "$work"
mkdir -p "$work/ctx" "$work/out" "$work/bin"
cp "$pkgs"/samurai-chwd-*.pkg.tar.zst "$pkgs"/samurai-hardware-*.pkg.tar.zst "$work/ctx/"

echo "==> building samurai (release)"
docker run --rm -i -v "$repo_root/toolbox":/toolbox:ro -v "$work/bin":/bin-out \
    -e OWNER="$(id -u):$(id -g)" archlinux:base-devel bash -es <<'EOS'
pacman -Syu --noconfirm --needed rust >/dev/null
CARGO_TARGET_DIR=/tmp/target cargo build --quiet --release --locked --offline \
    --manifest-path /toolbox/Cargo.toml -p samurai-cli
install -m755 /tmp/target/release/samurai /bin-out/samurai
chown "$OWNER" /bin-out/samurai
EOS

echo "==> building the test image"
cat > "$work/ctx/Dockerfile" <<'EOF'
FROM archlinux:base-devel
RUN printf '\n[multilib]\nInclude = /etc/pacman.d/mirrorlist\n' >> /etc/pacman.conf \
 && pacman -Syu --noconfirm --needed lua pciutils libusb hwdata >/dev/null
COPY *.pkg.tar.zst /pk/
RUN pacman -U --noconfirm /pk/*.pkg.tar.zst >/dev/null
EOF
docker build -q -t "$image" "$work/ctx" >/dev/null

status=0
for fixture in "$repo_root"/tests/hw-fixtures/*.hw; do
    name="$(basename "$fixture" .hw)"
    machine="$work/machines/$name"
    python3 "$repo_root/tests/hw/fake_machine.py" "$fixture" "$machine"
    mkdir -p "$work/out/$name"
    docker run --rm -i \
        -v "$machine/pci":/sys/bus/pci:ro -v "$machine/dmi":/sys/devices/virtual/dmi/id:ro \
        -v "$machine":/machine:ro -v "$work/bin/samurai":/usr/local/bin/samurai:ro \
        -v "$work/out/$name":/out -e OWNER="$(id -u):$(id -g)" \
        "$image" bash -es <<'EOS' || { echo "error: machine run failed: $name" >&2; status=1; }
trap 'chown -R "$OWNER" /out' EXIT
i=0
for k in $(cat /machine/kernels); do
    i=$((i + 1))
    mkdir -p "/usr/lib/modules/6.99.$i-$k"
    echo "$k" > "/usr/lib/modules/6.99.$i-$k/pkgbase"
done
printf '#!/bin/sh\ncat /machine/virt\n' > /usr/local/bin/systemd-detect-virt
chmod 755 /usr/local/bin/systemd-detect-virt
lspci -Dnnmm > /out/lspci.txt
LC_ALL=C chwd --list > /out/chwd-list.txt
samurai hw detect --json > /out/detect.json
samurai hw detect > /out/detect.txt
samurai hw list --all --json > /out/list.json
samurai health --category drivers --json > /out/health.json || [ $? -eq 1 ]
EOS
done

python3 "$repo_root/tests/hw/check_machines.py" "$work/out" || status=1
if [ "$e2e" = 1 ]; then
    SAMURAI_HW_IMAGE="$image" SAMURAI_HW_WORK="$work" "$repo_root/tests/hw/e2e.sh" || status=1
fi
exit "$status"

#!/usr/bin/env bash
# M0 exit test (roadmap spec 12): boot the ISO in QEMU with OVMF firmware and
# prove it reaches a root shell on the serial console.
#
# Usage: ci/boot-vm.sh <iso> [timeout-seconds]   (default timeout: 180)
# Requires: qemu-system-x86_64, OVMF (/usr/share/OVMF), /dev/kvm (falls back to TCG).
# Exit 0 when "root@samurai" shows on the serial console.
set -euo pipefail

iso="${1:?usage: boot-vm.sh <iso> [timeout-seconds]}"
timeout_s="${2:-180}"
[ -f "$iso" ] || { echo "error: no such ISO: $iso" >&2; exit 2; }
work_dir="$(dirname "$iso")"
vars="$work_dir/OVMF_VARS.fd"
serial="$work_dir/serial.log"

ovmf_code=""
for c in /usr/share/OVMF/OVMF_CODE_4M.fd /usr/share/OVMF/OVMF_CODE.fd /usr/share/ovmf/OVMF_CODE.fd; do
    [ -f "$c" ] && ovmf_code="$c" && break
done
[ -n "$ovmf_code" ] || { echo "no OVMF firmware found" >&2; exit 1; }
ovmf_vars="${ovmf_code/OVMF_CODE/OVMF_VARS}"
cp "$ovmf_vars" "$vars"

# -cpu host needs KVM; TCG gets the richest emulated CPU instead
if [ -w /dev/kvm ]; then
    accel_args=(-machine "q35,accel=kvm" -cpu host)
else
    accel_args=(-machine "q35,accel=tcg" -cpu max)
fi
rm -f "$serial"

echo "==> booting $iso (OVMF: $ovmf_code, accel: ${accel_args[1]#q35,accel=}, console: serial)"
set +e
timeout --foreground "$timeout_s" \
qemu-system-x86_64 \
    "${accel_args[@]}" \
    -m 4096 -smp 4 \
    -drive "if=pflash,format=raw,readonly=on,file=$ovmf_code" \
    -drive "if=pflash,format=raw,file=$vars" \
    -drive "file=$iso,media=cdrom,read-only=on" \
    -display none \
    -serial "file:$serial" \
    -monitor none
qemu_rc=$?
set -e

if grep -q "root@samurai" "$serial" 2>/dev/null; then
    echo "==> PASS: root shell reached on serial console"
    tail -5 "$serial"
    exit 0
fi

echo "==> FAIL: no root@samurai on serial (qemu rc=$qemu_rc); serial log tail:" >&2
tail -30 "$serial" 2>/dev/null || echo "(no serial output)" >&2
exit 1

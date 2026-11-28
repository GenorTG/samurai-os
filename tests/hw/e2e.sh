#!/usr/bin/env bash
# Real `samurai hw apply` with real chwd, pacman, DKMS and the real driver packages
# on a fake GPU, inside an UNPRIVILEGED throwaway container (ci/test-hw.sh --e2e).
# No module is ever loaded: this proves the package transactions, the safety
# rules and the way back, not that a GPU renders.
#
#   GTX 1070 -> apply (580xx, DKMS) -> card swapped to RTX 3090 -> fingerprint
#   notice -> refused without --switch-driver -> switch to the open modules ->
#   swap back with a failing chwd -> previous driver restored
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
work="${SAMURAI_HW_WORK:-$repo_root/build/hwtest}"
image="${SAMURAI_HW_IMAGE:-samurai-hw-test:local}"
for m in desktop-gtx1070 desktop-rtx3090; do
    rm -rf "$work/e2e-$m"
    python3 "$repo_root/tests/hw/fake_machine.py" "$repo_root/tests/hw-fixtures/$m.hw" "$work/e2e-$m"
done
rm -rf "$work/e2e-live"
cp -r "$work/e2e-desktop-gtx1070" "$work/e2e-live"

docker run --rm -i --name "samurai-hw-e2e-$$" \
    -v "$work/e2e-live/pci":/sys/bus/pci:ro -v "$work/e2e-live/dmi":/sys/devices/virtual/dmi/id:ro \
    -v "$work/e2e-live":/live -v "$work/e2e-desktop-rtx3090":/rtx3090:ro \
    -v "$work/e2e-desktop-gtx1070":/gtx1070:ro \
    -v "$work/bin/samurai":/usr/local/bin/samurai:ro \
    -e OWNER="$(id -u):$(id -g)" "$image" bash -es <<'EOS'
set -euo pipefail
trap 'chown -R "$OWNER" /live' EXIT
say() { printf '\n==> %s\n' "$*"; }
fail() { echo "E2E FAIL: $*" >&2; tail -40 /tmp/step.log >&2 || true; exit 1; }
card() { cp -r "/$1/pci/devices/0000:01:00.0/." /live/pci/devices/0000:01:00.0/; }
has() { pacman -Qq "$1" >/dev/null 2>&1; }

# [cachyos] carries the 580xx branch; throwaway container, so no key ceremony
printf '\n[cachyos]\nSigLevel = Never\nServer = https://mirror.cachyos.org/repo/$arch/$repo\n' >> /etc/pacman.conf
pacman -Syu --noconfirm --needed linux linux-headers mkinitcpio >/tmp/step.log 2>&1 || fail "base kernel"
mkdir -p /var/lib/samurai

say "1. GTX 1070: first fingerprint and apply"
samurai hw fingerprint --check
samurai hw apply --yes --no-snapshot >/tmp/step.log 2>&1 || fail "apply on the 1070"
grep -E '^(done|warning|Reboot)' /tmp/step.log
has nvidia-580xx-dkms && has nvidia-580xx-utils || fail "580xx packages missing"
rel=$(ls /usr/lib/modules | head -1)
modinfo -k "$rel" -F version nvidia || fail "580xx module not built"
grep -q '^\[nvidia-dkms-580xx\]' /var/lib/chwd/local/pci/graphic_drivers/profiles.toml || fail "no chwd record"
samurai hw detect | grep -q 'Drivers match the hardware' || fail "detect still wants changes"
samurai health --category drivers || true

say "2. card swapped to RTX 3090"
card rtx3090
samurai hw fingerprint --check
grep -q 'RTX 3090.*was.*GTX 1070' /var/lib/samurai/hw-changed.json || fail "no hardware change notice"
if samurai hw apply --yes --no-snapshot >/tmp/step.log 2>&1; then fail "switch ran without --switch-driver"; fi
grep '^refused' /tmp/step.log
has nvidia-580xx-utils || fail "the refused run changed packages"

say "3. switch to the open modules"
samurai hw apply --yes --no-snapshot --switch-driver >/tmp/step.log 2>&1 || fail "switch"
grep -E '^(done|Reboot)' /tmp/step.log
has nvidia-utils && has nvidia-open || fail "open driver missing"
! has nvidia-580xx-utils && ! has nvidia-580xx-dkms || fail "580xx packages left behind"
modinfo -k "$rel" -F version nvidia || fail "open module missing"
! grep -q '580xx' /var/lib/chwd/local/pci/graphic_drivers/profiles.toml || fail "stale chwd record"
[ ! -e /var/lib/samurai/hw-changed.json ] || fail "notice not cleared"
grep -q 'nvidia_drm' /etc/mkinitcpio.conf.d/11-samurai-nvidia.conf || fail "no early KMS drop-in"

say "4. switch back with a failing chwd step: the open driver must come back"
card gtx1070
cp /var/lib/chwd/scripts/chwd /tmp/chwd.script
printf '#!/bin/sh\necho "simulated pacman failure"\nexit 1\n' > /var/lib/chwd/scripts/chwd
if samurai hw apply --yes --no-snapshot --switch-driver >/tmp/step.log 2>&1; then fail "failing chwd reported success"; fi
cp /tmp/chwd.script /var/lib/chwd/scripts/chwd
grep -A1 '^failed' /tmp/step.log
has nvidia-utils && has nvidia-open || fail "previous driver not restored"
! has nvidia-580xx-utils || fail "new branch left behind"
modinfo -k "$rel" -F version nvidia || fail "no module after the restore"
grep -q '^\[nvidia-open-dkms\]' /var/lib/chwd/local/pci/graphic_drivers/profiles.toml || fail "record not restored"

say "5. removal of a present card's profile is refused"
card rtx3090
if samurai hw remove nvidia-open-dkms --yes --no-snapshot >/tmp/step.log 2>&1; then fail "removed the live driver"; fi
cat /tmp/step.log

say "6. no database sync happened during any apply"
! grep -E 'pacman.*-S[a-z]*y' /var/log/pacman.log | grep -v -- '-Syu' || fail "a sync without upgrade ran"
grep "Running 'pacman" /var/log/pacman.log | sed 's/.*Running/  Running/' | cut -c1-150
echo "E2E PASS"
EOS

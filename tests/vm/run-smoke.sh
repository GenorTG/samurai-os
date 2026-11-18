#!/usr/bin/env bash
# M2 smoke suite: green from one command (roadmap M2 exit criterion).
# Usage: tests/vm/run-smoke.sh [iso]
set -euo pipefail
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
iso="${1:-$(ls -t "$repo_root"/build/samurai-os-*.iso 2>/dev/null | head -1)}"
[ -n "${iso:-}" ] && [ -f "$iso" ] || { echo "no ISO found; run ci/build-iso.sh first" >&2; exit 1; }

cd "$repo_root/tests/vm"
echo "==> smoke suite against $iso"
python3 -m pytest -x -q test_smoke.py test_update.py "$@"

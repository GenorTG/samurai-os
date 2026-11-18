#!/usr/bin/env bash
# M3 mechanism test (clearly labelled): proves the intervention runner can
# execute a signed, idempotent post-phase script and record it in .done.
# Real interventions ship with samurai-update as packages and carry real
# Arch-news fixes; this one only writes a marker file.
set -euo pipefail
mkdir -p /var/lib/samurai
printf '%s\n' "$(date -Is)" > /var/lib/samurai/intervention-0001-demo

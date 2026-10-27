#!/usr/bin/env bash
# Point this clone at the versioned hooks in scripts/git-hooks.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
chmod +x scripts/git-hooks/*
git config core.hooksPath scripts/git-hooks
echo "hooks installed: core.hooksPath=scripts/git-hooks"

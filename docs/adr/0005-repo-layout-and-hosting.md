# ADR-0005 — One source monorepo, one data-only package repo

**Status:** proposed

## Decision
- `GenorTG/samurai-os` (public): specs, PKGBUILDs, toolbox source, settings, ISO profile, tests, CI.
  ISO releases are GitHub Releases here.
- `GenorTG/samurai-repo` (public, data only): no code, just GitHub Releases `x86_64` and
  `x86_64-testing` holding the signed packages and databases (spec 02).
- Mirror on genorbox1 at `repo.smart-samurai.pl`.

## Why
- One person maintains this. Changing a PKGBUILD, the toolbox and its VM test in one commit is worth
  more than per-package repos.
- Keeping package blobs out of the source repo's release list keeps ISO releases readable, and keeps
  the "rolling" releases (which we overwrite constantly) apart from versioned ones.
- Package files are never stored in git history, so the repos stay small.

## Consequences
- Publishing needs a token that can write to `samurai-repo` releases; it lives in the same protected
  CI environment as the signing subkey (spec 09).
- Creating both repos was approved by Genor (2026-11-17, "public now"): they go up at M1.

#!/usr/bin/env python3
"""Validate intervention definitions (spec 03). Allowlist + strict types:
an intervention ships inside a signed package; this validator is the second
gate after the signature (no free-form shell in the TOML itself; scripts are
separate reviewed files)."""
from __future__ import annotations

import pathlib
import re
import sys
import tomllib

ALLOWED = {"id", "title", "news_url", "phase", "applies_if", "script", "pacman_args", "idempotent"}
PHASES = {"pre", "pacman-args", "post"}
ID_RE = re.compile(r"^\d{4}-[a-z0-9-]+$")
URL_RE = re.compile(r"^https://[a-zA-Z0-9./_-]+$")


def validate(path: str) -> None:
    raw = pathlib.Path(path).read_bytes()
    try:
        it = tomllib.loads(raw.decode("utf-8"))
    except tomllib.TOMLDecodeError as e:
        fail(f"{path}: invalid TOML: {e}")
    for key in it:
        if key not in ALLOWED:
            fail(f"{path}: unknown key {key!r}")
    if not ID_RE.match(it.get("id", "")):
        fail(f"{path}: bad id (want NNNN-slug)")
    if it.get("phase") not in PHASES:
        fail(f"{path}: bad phase")
    if it.get("news_url") and not URL_RE.match(it["news_url"]):
        fail(f"{path}: bad news_url")
    if not it.get("applies_if"):
        fail(f"{path}: missing applies_if")
    if not it.get("idempotent", False):
        fail(f"{path}: must be idempotent=true (runs at most once, but never trust it)")
    if "pacman_args" in it and "script" in it:
        fail(f"{path}: either script or pacman_args, not both")
    if it.get("script"):
        s = pathlib.Path(path).parent / it["script"]
        if not s.exists():
            fail(f"{path}: script {it['script']} missing beside the TOML")


def fail(msg: str) -> None:
    print(f"VALIDATION ERROR: {msg}", file=sys.stderr)
    raise SystemExit(1)


if __name__ == "__main__":
    targets = sys.argv[1:] or sorted(str(p) for p in pathlib.Path("updates/interventions").glob("*.toml"))
    for t in targets:
        validate(t)
    print(f"interventions OK ({len(targets)})")

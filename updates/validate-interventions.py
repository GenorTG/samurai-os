#!/usr/bin/env python3
"""Validate update interventions (spec 03) and answer the updater's queries.

An intervention is `<NNNN>-<slug>.toml` plus an optional `<NNNN>-<slug>.sh`
beside it, shipped inside the signed samurai-tools package. Two gates stand
between a TOML file and `samurai update` running something:

1. the package signature (pacman verifies it on install);
2. this validator: a closed allowlist of keys, a closed `applies_if` grammar,
   a closed `pacman_args` allowlist, and the rule that a script must be the
   file named after the intervention (never a path, never a command).

Usage:
    validate-interventions.py [FILE...]      validate (default: updates/interventions/*.toml)
    validate-interventions.py --query FILE   validate one file, then print
                                             `key<TAB>value` lines for the updater
    validate-interventions.py --applies FILE evaluate applies_if on this machine
                                             (exit 0 = applies, 1 = does not, 2 = invalid)

Only stdlib; runs with `python3 -I` (isolated) from the updater.
"""
from __future__ import annotations

import pathlib
import re
import shutil
import subprocess
import sys
import tomllib

ALLOWED = {"id", "title", "news_url", "phase", "applies_if", "script", "pacman_args", "idempotent"}
PHASES = {"pre", "pacman-args", "post"}
ID_RE = re.compile(r"^\d{4}-[a-z0-9]+(?:-[a-z0-9]+)*$")
URL_RE = re.compile(r"^https://[a-zA-Z0-9.-]+(?:/[a-zA-Z0-9._~%/-]*)?$")
PKG_RE = re.compile(r"^[a-z0-9][a-z0-9@._+-]*$")
VERSION_RE = re.compile(r"^[0-9][A-Za-z0-9._+:~-]*$")
ABS_PATH_RE = re.compile(r"^/[A-Za-z0-9._/+@-]*$")
# pacman arguments an intervention may contribute to the transaction (spec 03
# stage 6). Every value is one token: no spaces, no shell characters.
PACMAN_ARG_RES = (
    re.compile(r"^--overwrite=/[A-Za-z0-9._/+@*?-]+$"),
    re.compile(r"^--ignore=[a-z0-9][a-z0-9@._+-]*$"),
    re.compile(r"^--assume-installed=[a-z0-9][a-z0-9@._+-]*=[0-9][A-Za-z0-9._+:~-]*$"),
)
# applies_if grammar: `always`, or one or more clauses joined by ` and `.
#   installed <pkg>            not-installed <pkg>
#   version-lt <pkg> <version> (installed and older than <version>)
#   path-exists <abs path>     path-missing <abs path>
#   file-contains <abs path> <literal>   (literal: no spaces)
CLAUSE_SHAPES = {
    "installed": (PKG_RE,),
    "not-installed": (PKG_RE,),
    "version-lt": (PKG_RE, VERSION_RE),
    "path-exists": (ABS_PATH_RE,),
    "path-missing": (ABS_PATH_RE,),
    "file-contains": (ABS_PATH_RE, re.compile(r"^\S+$")),
}


class Invalid(Exception):
    pass


def parse_applies_if(expr: str) -> list[list[str]]:
    if not isinstance(expr, str) or not expr.strip():
        raise Invalid("missing applies_if")
    expr = expr.strip()
    if expr == "always":
        return []
    clauses = []
    for raw in expr.split(" and "):
        parts = raw.split()
        if not parts or parts[0] not in CLAUSE_SHAPES:
            raise Invalid(f"applies_if: unknown clause {raw!r} "
                          f"(known: always, {', '.join(sorted(CLAUSE_SHAPES))})")
        shapes = CLAUSE_SHAPES[parts[0]]
        if len(parts) != len(shapes) + 1:
            raise Invalid(f"applies_if: clause {raw!r} needs {len(shapes)} argument(s)")
        for value, shape in zip(parts[1:], shapes):
            if not shape.match(value):
                raise Invalid(f"applies_if: bad value {value!r} in {raw!r}")
        clauses.append(parts)
    return clauses


def load(path: pathlib.Path) -> dict:
    try:
        it = tomllib.loads(path.read_bytes().decode("utf-8"))
    except (OSError, UnicodeDecodeError, tomllib.TOMLDecodeError) as e:
        raise Invalid(f"invalid TOML: {e}") from e
    for key, value in it.items():
        if key not in ALLOWED:
            raise Invalid(f"unknown key {key!r}")
        if key == "pacman_args":
            if not isinstance(value, list) or not all(isinstance(v, str) for v in value):
                raise Invalid("pacman_args must be a list of strings")
        elif key == "idempotent":
            if not isinstance(value, bool):
                raise Invalid("idempotent must be true/false")
        elif not isinstance(value, str):
            raise Invalid(f"{key} must be a string")
    if not ID_RE.match(it.get("id", "")):
        raise Invalid("bad id (want NNNN-slug)")
    if it["id"] != path.stem:
        raise Invalid(f"id {it['id']!r} must equal the file name {path.stem!r}")
    if not it.get("title"):
        raise Invalid("missing title")
    if it.get("phase") not in PHASES:
        raise Invalid(f"bad phase (want one of {sorted(PHASES)})")
    if it.get("news_url") and not URL_RE.match(it["news_url"]):
        raise Invalid("bad news_url (https only)")
    parse_applies_if(it.get("applies_if", ""))
    if it.get("idempotent") is not True:
        raise Invalid("must declare idempotent = true (runs at most once, but never trust it)")
    if "pacman_args" in it and "script" in it:
        raise Invalid("either script or pacman_args, not both")
    if it["phase"] == "pacman-args":
        if not it.get("pacman_args"):
            raise Invalid("phase pacman-args needs pacman_args")
        for arg in it["pacman_args"]:
            if not any(r.match(arg) for r in PACMAN_ARG_RES):
                raise Invalid(f"pacman_args: {arg!r} is not an allowed argument")
    else:
        if "pacman_args" in it:
            raise Invalid("pacman_args only with phase pacman-args")
        if it.get("script") != f"{it['id']}.sh":
            raise Invalid(f"script must be exactly {it['id']}.sh beside the TOML")
        script = path.parent / it["script"]
        if not script.is_file() or script.is_symlink():
            raise Invalid(f"script {it['script']} missing beside the TOML (regular file required)")
        head = script.read_text(encoding="utf-8", errors="replace").splitlines()[:40]
        if not head or head[0] not in ("#!/bin/bash", "#!/usr/bin/bash", "#!/usr/bin/env bash"):
            raise Invalid(f"{it['script']}: first line must be a bash shebang")
        if not any(line.strip().startswith("set -") and "e" in line.split()[1] for line in head
                   if line.strip().startswith("set -")):
            raise Invalid(f"{it['script']}: must `set -e` (fail closed)")
    return it


def applies(expr: str) -> bool:
    for clause in parse_applies_if(expr):
        kind, args = clause[0], clause[1:]
        if kind == "installed" and not _installed(args[0]):
            return False
        if kind == "not-installed" and _installed(args[0]):
            return False
        if kind == "version-lt":
            ver = _installed_version(args[0])
            if ver is None or _vercmp(ver, args[1]) >= 0:
                return False
        if kind == "path-exists" and not pathlib.Path(args[0]).exists():
            return False
        if kind == "path-missing" and pathlib.Path(args[0]).exists():
            return False
        if kind == "file-contains":
            try:
                text = pathlib.Path(args[0]).read_text(encoding="utf-8", errors="replace")
            except OSError:
                return False
            if args[1] not in text:
                return False
    return True


def _installed_version(pkg: str) -> str | None:
    if not shutil.which("pacman"):
        return None
    r = subprocess.run(["pacman", "-Q", "--", pkg], capture_output=True, text=True,
                       env={"LC_ALL": "C", "PATH": "/usr/bin:/bin"})
    if r.returncode != 0:
        return None
    parts = r.stdout.split()
    return parts[1] if len(parts) == 2 else None


def _installed(pkg: str) -> bool:
    return _installed_version(pkg) is not None


def _vercmp(a: str, b: str) -> int:
    r = subprocess.run(["vercmp", "--", a, b], capture_output=True, text=True,
                       env={"LC_ALL": "C", "PATH": "/usr/bin:/bin"})
    try:
        return int(r.stdout.strip())
    except ValueError:
        return 0


def fail(msg: str) -> None:
    print(f"VALIDATION ERROR: {msg}", file=sys.stderr)
    raise SystemExit(2)


def main(argv: list[str]) -> int:
    if argv[:1] == ["--query"]:
        if len(argv) != 2:
            fail("--query takes exactly one file")
        path = pathlib.Path(argv[1])
        try:
            it = load(path)
        except Invalid as e:
            fail(f"{path}: {e}")
        print(f"id\t{it['id']}")
        print(f"phase\t{it['phase']}")
        print(f"title\t{it['title']}")
        print(f"news_url\t{it.get('news_url', '')}")
        print(f"script\t{it.get('script', '')}")
        print("pacman_args\t" + " ".join(it.get("pacman_args", [])))
        return 0
    if argv[:1] == ["--applies"]:
        if len(argv) != 2:
            fail("--applies takes exactly one file")
        path = pathlib.Path(argv[1])
        try:
            it = load(path)
            return 0 if applies(it["applies_if"]) else 1
        except Invalid as e:
            fail(f"{path}: {e}")
    if argv[:1] in (["-h"], ["--help"]):
        print(__doc__.strip())
        return 0
    targets = argv or sorted(str(p) for p in pathlib.Path("updates/interventions").glob("*.toml"))
    for t in targets:
        try:
            load(pathlib.Path(t))
        except Invalid as e:
            fail(f"{t}: {e}")
    print(f"interventions OK ({len(targets)})")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

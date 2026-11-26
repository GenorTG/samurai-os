#!/usr/bin/env python3
"""Keep the checksums of LOCAL PKGBUILD sources current.

  ci/update-pkg-sums.py [--check] [pkgbuild-dir ...]

A local source is a file that sits beside the PKGBUILD (LICENSE: the repository's
root file, which the build copies in). Checksums of downloaded sources are
upstream pins and are never touched. --check changes nothing and exits 1 when a
local checksum is stale. samurai-tools is skipped: its sources are staged from
updates/ and catalog/ (tests/unit/test_installer_apps.py covers it).
"""
from __future__ import annotations

import hashlib
import pathlib
import re
import sys

REPO = pathlib.Path(__file__).resolve().parents[1]
STAGED_ELSEWHERE = {"samurai-tools"}
ARRAY_RE = re.compile(r"^(source|sha256sums|sha512sums)=\((.*?)\)", re.M | re.S)
TOKEN_RE = re.compile(r"""'([^']*)'|"([^"]*)"|([^\s'"]+)""")


def tokens(body: str) -> list[str]:
    lines = [line for line in body.splitlines() if not line.lstrip().startswith("#")]
    return ["".join(m) for m in TOKEN_RE.findall("\n".join(lines))]


def local_file(pkgdir: pathlib.Path, source: str) -> pathlib.Path | None:
    if "::" in source or "://" in source:
        return None
    if source == "LICENSE":
        return REPO / "LICENSE"
    path = pkgdir / source
    return path if path.is_file() else None


def refresh(pkgdir: pathlib.Path) -> tuple[str, str]:
    """(current text, text with correct local checksums)"""
    pkgbuild = pkgdir / "PKGBUILD"
    text = pkgbuild.read_text(encoding="utf-8")
    arrays = {m.group(1): m for m in ARRAY_RE.finditer(text)}
    algo = "sha512sums" if "sha512sums" in arrays else "sha256sums"
    if "source" not in arrays or algo not in arrays:
        return text, text
    sources = tokens(arrays["source"].group(2))
    sums = tokens(arrays[algo].group(2))
    new = []
    for index, source in enumerate(sources):
        path = local_file(pkgdir, source)
        if path is not None:
            new.append(hashlib.new(algo.removesuffix("sums"), path.read_bytes()).hexdigest())
        elif index < len(sums):
            new.append(sums[index])
        else:
            raise SystemExit(f"error: {pkgbuild}: no checksum pinned for remote source {source}")
    if new == sums:
        return text, text
    block = f"{algo}=(" + "".join(f"\n    '{value}'" for value in new) + "\n)"
    match = arrays[algo]
    return text, text[:match.start()] + block + text[match.end():]


def main(argv: list[str]) -> int:
    check = "--check" in argv
    dirs = [pathlib.Path(a) for a in argv if a != "--check"] or sorted(
        p.parent for p in (REPO / "pkgbuilds").glob("*/PKGBUILD"))
    stale = []
    for pkgdir in dirs:
        if pkgdir.name in STAGED_ELSEWHERE:
            continue
        old, new = refresh(pkgdir)
        if old != new:
            stale.append(pkgdir.name)
            if not check:
                (pkgdir / "PKGBUILD").write_text(new, encoding="utf-8")
    if stale:
        print(("stale checksums: " if check else "updated: ") + ", ".join(stale))
    return 1 if check and stale else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

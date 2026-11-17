#!/usr/bin/env python3
"""Take a screenshot of a running test VM via QMP screendump.

Usage: screenshot.py <vm-name> [out.png]
Saves PPM via QMP, converts to PNG next to the VM logs.
"""
from __future__ import annotations

import json
import pathlib
import socket
import sys

import vm  # noqa: F401  (sets REPO/WORK paths)


def qmp_screendump(sock_path: str, ppm_path: str) -> None:
    s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    s.settimeout(30)
    s.connect(sock_path)
    f = s.makefile("rw")
    f.readline()  # greeting
    f.write('{"execute": "qmp_capabilities"}\n'); f.flush(); f.readline()
    f.write(json.dumps({"execute": "screendump", "arguments": {"filename": ppm_path}}) + "\n")
    f.flush()
    resp = json.loads(f.readline())
    s.close()
    if "error" in resp:
        raise RuntimeError(f"screendump failed: {resp['error']}")


def ppm_to_png(ppm: pathlib.Path, png: pathlib.Path) -> None:
    try:
        from PIL import Image  # type: ignore
        Image.open(ppm).save(png)
    except ImportError:
        subprocess_run = subprocess.run  # noqa: F841
        import subprocess
        subprocess.run(["convert", str(ppm), str(png)], check=True)
    ppm.unlink(missing_ok=True)


def main() -> None:
    name = sys.argv[1] if len(sys.argv) > 1 else "smoke-install"
    work = pathlib.Path(vm.WORK)
    out = pathlib.Path(sys.argv[2]) if len(sys.argv) > 2 else work / f"{name}-screen.png"
    sock = work / f"{name}-qmp.sock"
    if not sock.exists():
        print(f"no QMP socket for VM {name!r} — is it running?", file=sys.stderr)
        raise SystemExit(1)
    ppm = work / f"{name}-screen.ppm"
    qmp_screendump(str(sock), str(ppm))
    ppm_to_png(ppm, out)
    print(f"saved {out}")


if __name__ == "__main__":
    main()

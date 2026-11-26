"""`samurai snap list --json` (schema 1), the toolbox GUI's snapshot source."""
from __future__ import annotations

import json
import os
import subprocess

from conftest import REPO

SNAP = REPO / "updates" / "samurai-snap"

ROWS = {"root": [
    {"number": 0, "type": "single", "date": "", "description": "current", "cleanup": ""},
    {"number": 1, "type": "single", "date": "2026-11-26 08:00:00", "description": 'odd "<b>" label',
     "cleanup": ""},
    {"number": 2, "type": "pre", "date": "2026-11-26 09:00:00", "description": "pacman -S htop",
     "cleanup": "number"},
]}


def run(tmp_path, *args: str, rc: int = 0):
    fake = tmp_path / "snapper"
    fake.write_text(
        "#!/bin/sh\n"
        f"echo \"$*\" >> '{tmp_path}/calls'\n"
        f"cat <<'EOF'\n{json.dumps(ROWS)}\nEOF\n"
        f"exit {rc}\n")
    fake.chmod(0o755)
    env = {**os.environ, "PATH": f"{tmp_path}:{os.environ['PATH']}"}
    return subprocess.run(["bash", str(SNAP), *args], capture_output=True, text=True, env=env)


def test_list_json_skips_the_live_system_and_keeps_text_verbatim(tmp_path):
    out = run(tmp_path, "list", "--json")
    assert out.returncode == 0, out.stderr
    data = json.loads(out.stdout)
    assert data["schema"] == 1 and data["config"] == "root"
    assert [s["id"] for s in data["snapshots"]] == [1, 2]
    assert data["snapshots"][0]["description"] == 'odd "<b>" label'
    assert "--jsonout -c root list" in (tmp_path / "calls").read_text()


def test_list_json_reports_a_snapper_failure(tmp_path):
    out = run(tmp_path, "list", "--json", rc=1)
    assert out.returncode == 1
    assert out.stdout == ""
    assert "cannot list" in out.stderr


def test_list_rejects_unknown_arguments(tmp_path):
    out = run(tmp_path, "list", "--bogus")
    assert out.returncode == 2

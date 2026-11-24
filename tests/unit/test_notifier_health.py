"""samurai-notifier counts warning-or-worse health issues straight from health.json (no jq)."""
from __future__ import annotations

import re
import subprocess

from conftest import REPO

NOTIFIER = REPO / "updates" / "samurai-notifier"


def _count(report: str, tmp_path) -> int:
    """Run the notifier's own grep line against `report`."""
    line = re.search(r"HEALTH=\$\((grep -o .*?) \"\$HEALTH_REPORT\" \| grep -c \. \|\| true\)", NOTIFIER.read_text())
    assert line, "health counting line missing from samurai-notifier"
    f = tmp_path / "health.json"
    f.write_text(report)
    script = f'set -o pipefail; HEALTH_REPORT="{f}"; {line.group(1)} "$HEALTH_REPORT" | grep -c . || true'
    return int(subprocess.run(["bash", "-c", script], capture_output=True, text=True, check=True).stdout.strip())


def _row(sev: str, status: str) -> str:
    return (f'{{"id":"x.y","category":"boot","severity":"{sev}","fix_risk":"safe",'
            f'"status":"{status}","summary":"","details":"","evidence":[]}}')


def test_counts_only_warning_or_worse_issues(tmp_path):
    rows = [_row("info", "issue"), _row("warning", "issue"), _row("error", "issue"),
            _row("critical", "issue"), _row("warning", "ok"), _row("error", "skipped")]
    report = '{"schema":1,"checked_epoch":1,"issues":4,"results":[' + ",".join(rows) + "]}\n"
    assert _count(report, tmp_path) == 3


def test_clean_report_counts_zero(tmp_path):
    report = '{"schema":1,"checked_epoch":1,"issues":0,"results":[' + _row("warning", "ok") + "]}\n"
    assert _count(report, tmp_path) == 0


def test_state_files_carry_health_key():
    text = NOTIFIER.read_text()
    assert "health=%s" in text and '"health": %s' in text
    assert "HEALTH=0" in text

"""publish-repo.sh must upload the database under the name each pacman section requests."""
from __future__ import annotations

import os
import shutil
import subprocess

import pytest

from conftest import REPO


@pytest.mark.skipif(shutil.which("gpg") is None, reason="gpg not available")
@pytest.mark.parametrize("channel,names", [
    ("x86_64", {"samurai"}),
    ("x86_64-testing", {"samurai", "samurai-testing"}),
])
def test_db_asset_names_follow_the_channel(channel, names):
    script = (REPO / "ci" / "publish-repo.sh").read_text()
    start = script.index("db_assets=(")
    end = script.index("# 1. packages + sigs")
    snippet = script[start:end]
    probe = f'set -euo pipefail; cd "$(mktemp -d)"; channel={channel}; ' \
            'touch samurai.db samurai.db.sig samurai.files samurai.files.sig; ' \
            f'{snippet}\nfor f in "${{db_assets[@]}}"; do test -f "$f" && echo "$f"; done'
    out = subprocess.run(["bash", "-c", probe], capture_output=True, text=True, check=True, env=os.environ).stdout.split()
    assert {o.split(".")[0] for o in out} == names
    assert len(out) == 4 * len(names)
    # the uploaded, protected and verified sets all use the same list
    assert script.count('"${db_assets[@]}"') >= 4

"""tests/vm/vm.py QMP client: an event before the reply must not end the exchange."""
from __future__ import annotations

import io
import sys

import pytest

from conftest import REPO

sys.path.insert(0, str(REPO / "tests" / "vm"))
from vm import VM  # noqa: E402


def test_events_are_skipped_until_the_reply():
    stream = io.StringIO('{"event": "DEVICE_TRAY_MOVED", "data": {}}\n{"event": "RESET"}\n{"return": {}}\n')
    assert VM._qmp_reply(stream) == {"return": {}}


def test_error_reply_is_returned_and_eof_raises():
    assert "error" in VM._qmp_reply(io.StringIO('{"error": {"desc": "no"}}\n'))
    with pytest.raises(ConnectionError):
        VM._qmp_reply(io.StringIO('{"event": "STOP"}\n'))

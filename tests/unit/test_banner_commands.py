"""Welcome banners may only advertise commands the `samurai` CLI really has."""
from __future__ import annotations

import re

import pytest

from conftest import REPO

BANNERS = ["iso/airootfs/etc/motd", "installer/samurai-postinstall"]


def _commands() -> set[str]:
    usage = (REPO / "toolbox/crates/samurai-core/src/lib.rs").read_text().split('pub const USAGE: &str = "', 1)[1].split('";', 1)[0]
    return set(re.findall(r"(?m)^  ([a-z]+)\s{2,}", usage))


@pytest.mark.parametrize("banner", BANNERS)
def test_banner_advertises_only_existing_commands(banner):
    commands = _commands()
    assert {"update", "health", "fix"} <= commands, commands
    advertised = re.findall(r"(?m)^  samurai ([a-z]+)\s{2,}", (REPO / banner).read_text())
    assert advertised, f"no commands found in {banner}"
    assert not set(advertised) - commands, f"{banner} advertises missing commands"

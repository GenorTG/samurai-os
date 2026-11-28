"""The installer's GPU decisions: which machines get the open modules, which can run CUDA."""
from __future__ import annotations

import re
import subprocess
import sys

import pytest

from conftest import REPO

INSTALL = (REPO / "installer" / "samurai-install").read_text()
POSTINSTALL = (REPO / "installer" / "samurai-postinstall").read_text()
sys.path.insert(0, str(REPO / "installer" / "gui"))
from samurai_installer import system  # noqa: E402


def gpu(chip: str, vendor: str = "10de", cls: str = "0300") -> str:
    return (f'0000:01:00.0 "VGA compatible controller [{cls}]" "Vendor [{vendor}]" '
            f'"{chip} [Some Card] [2204]" -r00 "x" "y"')


def awk_programs(script: str) -> list[str]:
    return re.findall(r"lspci -mm -nn 2>/dev/null \| awk -F'\"' '(.*?)'", script, re.S)


def program(script: str, kind: str) -> str:
    """The awk program that decides `auto` (counts old cards) or CUDA."""
    (found,) = [p for p in awk_programs(script) if ("old++" in p) == (kind == "auto")]
    return found


def run(program: str, *lines: str) -> bool:
    return subprocess.run(["awk", "-F", '"', program], input="\n".join(lines) + "\n",
                          text=True, check=False).returncode == 0


@pytest.mark.parametrize("cards,wanted", [
    (["GA102"], True),
    (["TU117M"], True),
    (["GB203"], True),
    (["Device"], True),                  # newer than pci.ids: the open modules
    (["GP104"], False),                  # Pascal keeps nouveau (a working display)
    (["GM107"], False),
    (["GK104"], False),
    (["GT218"], False),
    (["G94"], False),
    (["GA102", "GP104"], False),         # one old card is enough
    ([], False),
])
def test_auto_installs_the_open_modules_only_when_every_card_can_use_them(cards, wanted):
    detect = program(INSTALL, "auto")
    assert run(detect, *[gpu(c) for c in cards]) is wanted


def test_auto_ignores_other_vendors():
    detect = program(INSTALL, "auto")
    assert run(detect, gpu("Navi", "1002"), gpu("GA102")) is True
    assert run(detect, gpu("Navi", "1002")) is False


@pytest.mark.parametrize("chip,cuda", [("TU102", True), ("GA102", True), ("AD104", True),
                                       ("GB202", True), ("GH100", True), ("GV100", False),
                                       ("GP104", False), ("GM204", False), ("GT218", False),
                                       ("Device", False)])
def test_cuda_detection_agrees_everywhere(chip, cuda):
    for detect in (program(INSTALL, "cuda"), program(POSTINSTALL, "cuda")):
        assert run(detect, gpu(chip)) is cuda, detect
        assert run(detect, gpu(chip, "1002")) is False
    assert system.parse_gpus(gpu(chip))[0]["cuda"] is cuda

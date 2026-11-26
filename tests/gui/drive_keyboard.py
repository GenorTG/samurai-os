"""Drive the wizard by keyboard without a display, the same keys the VM test sends
(tests/vm/test_installer_gui.py), against a stand-in helper. Run in an Arch container:

    QT_QPA_PLATFORM=offscreen python3 tests/gui/drive_keyboard.py

Checks that the keys reach the choices, that nothing starts without the erase
confirmation, and that the helper receives the profile and passphrase on stdin.
"""
from __future__ import annotations

import json
import pathlib
import sys
import tempfile
import tomllib

sys.dont_write_bytecode = True
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from render_pages import GUI, REPO, facts, pause  # noqa: E402  (also sets sys.path)

from PySide6.QtCore import Qt, QUrl, qInstallMessageHandler  # noqa: E402
from PySide6.QtQml import QQmlApplicationEngine  # noqa: E402
from PySide6.QtQuickControls2 import QQuickStyle  # noqa: E402
from PySide6.QtTest import QTest  # noqa: E402
from PySide6.QtWidgets import QApplication  # noqa: E402

from samurai_installer import engine  # noqa: E402
from samurai_installer.backend import Backend  # noqa: E402

PASSPHRASE, PASSWORD = "gui-disk-2026", "gui-user-2026"


def main() -> int:
    warnings: list[str] = []
    qInstallMessageHandler(lambda _m, _c, text: warnings.append(text))
    app = QApplication(sys.argv[:1])
    QQuickStyle.setStyle("org.kde.desktop")
    QQuickStyle.setFallbackStyle("Fusion")
    tools = engine.Tools.from_checkout(REPO)
    work = pathlib.Path(tempfile.mkdtemp())
    stand_in = work / "helper"
    stand_in.write_text(f"#!/bin/sh\ncat > {work}/request.json\n"
                        "echo 'x partitioning /dev/vda'\necho 'x install complete; unmounting'\n"
                        "sleep 1\necho 'samurai-installer-helper: exit 0'\n")
    stand_in.chmod(0o755)
    machine = facts(tools)
    machine["disks"] = machine["disks"][:1]
    backend = Backend(tools, facts=machine, helper=(str(stand_in),))
    qml = QQmlApplicationEngine()
    qml.rootContext().setContextProperty("installer", backend)
    qml.load(QUrl.fromLocalFile(str(GUI / "qml" / "Main.qml")))
    window = qml.rootObjects()[0]
    window.showNormal()
    window.requestActivate()
    pause(900)

    def key(code, modifier=Qt.KeyboardModifier.NoModifier) -> None:
        QTest.keyClick(window, code, modifier)
        pause(60)

    def type_text(text: str) -> None:
        for char in text:
            QTest.keyClick(window, char)
        pause(100)

    def alt(code, expected: str, modifier=Qt.KeyboardModifier.AltModifier) -> None:
        key(code, modifier)
        pause(400)
        got = window.property("pageId")
        assert got == expected, f"expected page {expected}, on {got}: " \
                                f"{window.property('blocker')!r}"

    def ctrl_return(expected: str) -> None:
        alt(Qt.Key.Key_Return, expected, Qt.KeyboardModifier.ControlModifier)

    alt(Qt.Key.Key_Right, "region")
    alt(Qt.Key.Key_Right, "disk")
    alt(Qt.Key.Key_Right, "disk")            # blocked: no disk yet
    key(Qt.Key.Key_Space)
    key(Qt.Key.Key_Tab)
    key(Qt.Key.Key_Tab)
    type_text(PASSPHRASE)
    key(Qt.Key.Key_Tab)
    type_text(PASSPHRASE)
    assert backend._model.disk == "/dev/vda", backend._model.disk
    assert backend._model.encryption is True
    alt(Qt.Key.Key_Right, "user")
    type_text("alex")
    key(Qt.Key.Key_Tab)
    type_text(PASSWORD)
    key(Qt.Key.Key_Tab)
    type_text(PASSWORD)
    assert backend._model.username == "alex", backend._model.username
    for page in ("kernels", "bootloader", "desktop", "hardware", "gaming", "development", "apps",
                 "network", "summary"):
        alt(Qt.Key.Key_Right, page)
    ctrl_return("summary")               # not confirmed: nothing starts
    assert backend.state == "idle"
    key(Qt.Key.Key_Space)
    assert window.property("confirmed") is True, window.property("blocker")
    ctrl_return("install")
    pause(3000)
    assert backend.state == "done", (backend.state, backend.logText())
    assert window.property("pageId") == "done"
    request = json.loads((work / "request.json").read_text())
    assert request["passphrase"] == PASSPHRASE
    profile = tomllib.loads(request["profile"])
    assert profile["user"]["password_hash"].startswith("$6$")
    assert PASSWORD not in request["profile"] and PASSPHRASE not in request["profile"]
    assert profile["disk"]["device"] == "/dev/vda"

    # clicks in QML reach the choices (radio, check box, list)
    for expression, check in (
            ('installer.choices.bootloader = "grub"', lambda m: m.bootloader == "grub"),
            ('installer.choices.tailscale = false', lambda m: m.tailscale is False),
            ('installer.toggle("kernels", "linux", true)', lambda m: "linux" in m.kernels),
            ('installer.toggle("kernels", "linux-cachyos", false)',
             lambda m: m.default_kernel != "linux-cachyos")):
        from PySide6.QtQml import QQmlExpression
        QQmlExpression(qml.rootContext(), window, expression).evaluate()
        pause(60)
        assert check(backend._model), expression
    print("\n".join(warnings))
    print("keyboard drive: OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())

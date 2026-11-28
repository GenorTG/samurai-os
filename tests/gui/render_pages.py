"""Render every wizard page to PNG without a display (layout check, run in an Arch container).

    QT_QPA_PLATFORM=offscreen python3 tests/gui/render_pages.py OUT_DIR [WIDTH HEIGHT]

Uses the real validator, planner and catalog from this checkout with made-up machine facts;
nothing is installed. Exits non-zero on any QML warning or a blocked summary.
"""
from __future__ import annotations

import pathlib
import sys

REPO = pathlib.Path(__file__).resolve().parents[2]
GUI = REPO / "installer" / "gui"
sys.path.insert(0, str(GUI))
sys.dont_write_bytecode = True

from PySide6.QtCore import QEventLoop, QTimer, QUrl, qInstallMessageHandler  # noqa: E402
from PySide6.QtQml import QQmlApplicationEngine  # noqa: E402
from PySide6.QtQuickControls2 import QQuickStyle  # noqa: E402
from PySide6.QtWidgets import QApplication  # noqa: E402

from samurai_installer import engine, system  # noqa: E402
from samurai_installer.backend import Backend  # noqa: E402

LSBLK = """{"blockdevices": [
 {"name":"vda","path":"/dev/vda","size":42949672960,"type":"disk","model":null,"tran":"virtio",
  "rm":false,"ro":false,"mountpoints":[null],"children":[
   {"name":"vda1","size":536870912,"type":"part","fstype":"vfat","label":"EFI","mountpoints":[null]},
   {"name":"vda2","size":42411753472,"type":"part","fstype":"ntfs","label":"Windows <b>x</b>","mountpoints":[null]}]},
 {"name":"sdb","path":"/dev/sdb","size":15728640000,"type":"disk","model":"USB Flash","tran":"usb",
  "rm":true,"ro":false,"mountpoints":[null]}]}"""


def facts(tools: engine.Tools) -> dict:
    return {
        "flavor": "offline", "uefi": True, "memory": 7900, "online": False,
        "disks": [d.as_dict() for d in system.parse_disks(LSBLK)],
        "gpus": [{"address": "0000:00:02.0", "vendor": "Red Hat, Inc. [1af4]",
                  "device": "Virtio 1.0 GPU [1050]", "nvidia": False, "cuda": False, "legacy": False}],
        "timezones": system.timezones(), "timezone": "Europe/Warsaw",
        "locales": system.locales(), "keymaps": system.keymaps(),
        "apps": engine.catalog(tools), "catalogError": "", "offlineBrowsers": ["firefox"],
    }


def pause(ms: int = 350) -> None:
    loop = QEventLoop()
    QTimer.singleShot(ms, loop.quit)
    loop.exec()


def main() -> int:
    out = pathlib.Path(sys.argv[1])
    out.mkdir(parents=True, exist_ok=True)
    width, height = (int(v) for v in sys.argv[2:4]) if len(sys.argv) > 3 else (1280, 800)
    warnings: list[str] = []
    qInstallMessageHandler(lambda _mode, _ctx, text: warnings.append(text))
    app = QApplication(sys.argv[:1])
    QQuickStyle.setStyle("org.kde.desktop")
    QQuickStyle.setFallbackStyle("Fusion")
    tools = engine.Tools.from_checkout(REPO)
    backend = Backend(tools, facts=facts(tools), helper=("false",))
    qml = QQmlApplicationEngine()
    qml.rootContext().setContextProperty("installer", backend)
    qml.load(QUrl.fromLocalFile(str(GUI / "qml" / "Main.qml")))
    if not qml.rootObjects():
        print("\n".join(warnings))
        return 1
    window = qml.rootObjects()[0]
    window.showNormal()
    window.setWidth(width)
    window.setHeight(height)
    pause(900)

    def shot(name: str) -> None:
        pause()
        if not window.grabWindow().save(str(out / f"{name}.png")):
            raise SystemExit(f"could not save {name}")

    def go(index: int) -> None:
        window.metaObject().invokeMethod  # noqa: B018 (documented entry point is QML go())
        from PySide6.QtCore import Q_ARG, QMetaObject, Qt
        QMetaObject.invokeMethod(window, "go", Qt.ConnectionType.DirectConnection,
                                 Q_ARG("QVariant", index))

    names = ["welcome", "region", "disk", "user", "kernels", "bootloader", "desktop", "hardware",
             "gaming", "development", "apps", "network", "summary"]
    for index, name in enumerate(names):
        if name == "disk":
            backend.choices.insert("disk", "")
        go(index)
        if name == "disk":
            shot("03-disk-empty")
            set_choice(backend, "disk", "/dev/vda")
            backend.setPassphrase("correct horse", "correct horse")
        if name == "user":
            shot("04-user-empty")
            set_choice(backend, "username", "alex")
            backend.setPassword("secret-password", "secret-password")
        if name == "apps":
            backend.toggle("apps", backend.facts["apps"][-1]["id"], True)
        shot(f"{index + 1:02d}-{name}")
    blocker = window.property("blocker")
    window.setProperty("confirmed", True)
    shot("13-summary-confirmed")
    if blocker:
        print(f"summary blocked: {blocker}")
        return 1
    # progress, failure and done pages
    go(13)
    for line in ("samurai-install starting: --profile x", "partitioning /dev/vda",
                 "creating btrfs on /dev/mapper/samurai-root", "installing base system",
                 "(1/412) installing glibc ..."):
        backend._progress.feed(line)
        backend._append(f"2026-11-26T12:00:00+01:00 {line}")
    backend.progressChanged.emit()
    backend._set_state("running")
    shot("14-install")
    backend._fail("base install failed (exit 1)")
    shot("14-install-failed")
    go(14)
    shot("15-done")
    problems = [w for w in warnings if "qml" in w.lower() or "Error" in w]
    print("\n".join(warnings))
    return 1 if problems else 0


def set_choice(backend: Backend, key: str, value: object) -> None:
    backend.choices.insert(key, value)
    backend._on_value(key, value)


if __name__ == "__main__":
    sys.exit(main())

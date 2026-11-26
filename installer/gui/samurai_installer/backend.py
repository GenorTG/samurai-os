"""QObject the QML pages talk to. It holds the choices, asks the engine for validation and
the plan, and runs the root helper. No system changes happen in this process."""
from __future__ import annotations

import json
import pathlib

from PySide6.QtCore import Property, QObject, QProcess, QTimer, Signal, Slot
from PySide6.QtQml import QQmlPropertyMap

from . import engine, model, system
from .progress import Progress

HELPER_COMMAND = ("sudo", "-n", "/usr/lib/samurai-installer/samurai-installer-helper")
EXIT_MARKER = "samurai-installer-helper: exit "
RESCUE_TERMINAL = "/usr/lib/samurai-installer/rescue-terminal"
LOG_LINES_KEPT = 4000


class Backend(QObject):
    disksChanged = Signal()
    onlineChanged = Signal()
    summaryChanged = Signal()
    progressChanged = Signal()
    logAppended = Signal(str)
    stateChanged = Signal()
    choicesReset = Signal()
    revisionChanged = Signal()   # any choice or secret changed: page checks re-run

    def __init__(self, tools: engine.Tools, facts: dict | None = None,
                 helper: tuple[str, ...] = HELPER_COMMAND, parent: QObject | None = None):
        super().__init__(parent)
        self._tools = tools
        self._helper = helper
        self._facts = facts if facts is not None else gather_facts(tools)
        self._model = model.Choices(timezone=self._facts.get("timezone", "UTC"))
        self._choices = QQmlPropertyMap(self)
        self._publish()
        self._choices.valueChanged.connect(self._on_value)
        # secrets live only here, never in the property map or a file
        self._password = ["", ""]
        self._passphrase = ["", ""]
        self._summary: dict = {}
        self._revision = 0
        self._progress = Progress()
        self._state = "idle"          # idle | running | done | failed
        self._log: list[str] = []
        self._buffer = b""
        self._exit_status: int | None = None
        self._process: QProcess | None = None
        self.onlineChanged.connect(self._touch)
        self.disksChanged.connect(self._touch)
        self._refresh = QTimer(self)
        self._refresh.setInterval(5000)
        self._refresh.timeout.connect(self._poll_network)
        if facts is None:
            self._refresh.start()

    # ------------------------------------------------------------ choices
    def _publish(self) -> None:
        for key, value in self._model.as_dict().items():
            self._choices.insert(key, value)

    def _on_value(self, key: str, value: object) -> None:
        if key not in model.field_names():
            return
        current = getattr(self._model, key)
        if hasattr(value, "toVariant"):          # a QJSValue array coming from QML
            value = value.toVariant()
        if isinstance(current, list):
            value = [str(v) for v in (value or [])]
        elif not isinstance(value, type(current)):
            return
        setattr(self._model, key, value)
        self._touch()

    def _touch(self) -> None:
        self._revision += 1
        self.revisionChanged.emit()

    @Property(int, notify=revisionChanged)
    def revision(self) -> int:
        return self._revision

    @Property(QObject, constant=True)
    def choices(self) -> QQmlPropertyMap:
        return self._choices

    @Slot(str, str, bool)
    def toggle(self, key: str, item: str, on: bool) -> None:
        """Add or remove ``item`` in a list choice."""
        if key not in model.field_names() or not isinstance(getattr(self._model, key), list):
            return
        values = [v for v in getattr(self._model, key) if v != item]
        if on:
            values.append(item)
        setattr(self._model, key, values)
        if key == "kernels":
            ordered = self._model.ordered_kernels()
            if ordered and self._model.default_kernel not in ordered:
                self._model.default_kernel = ordered[0]
                self._choices.insert("default_kernel", ordered[0])
        self._choices.insert(key, values)
        self._touch()

    # ------------------------------------------------------------ facts
    @Property("QVariantMap", constant=True)
    def facts(self) -> dict:
        return {k: v for k, v in self._facts.items() if k not in ("disks", "online")}

    @Property("QVariantList", notify=disksChanged)
    def disks(self) -> list:
        return self._facts.get("disks", [])

    @Property(bool, notify=onlineChanged)
    def online(self) -> bool:
        return bool(self._facts.get("online", False))

    @Property("QVariantMap", constant=True)
    def options(self) -> dict:
        def rows(table: tuple) -> list:
            return [{"id": i, "name": n, "text": t} for i, n, t in table]
        return {"kernels": rows(model.KERNELS), "bootloaders": rows(model.BOOTLOADERS),
                "desktops": rows(model.DESKTOPS), "shells": rows(model.SHELLS),
                "gaming": rows(model.GAMING_ITEMS), "pythons": list(model.PYTHONS),
                "docker": rows(model.DOCKER_MODES), "conda": rows(model.CONDA),
                "firewalls": rows(model.FIREWALLS), "proxies": rows(model.PROXIES),
                "nvidia": rows(model.NVIDIA_MODES)}

    @Slot()
    def refreshDisks(self) -> None:
        if "live" not in self._facts:
            return
        self._facts["disks"] = [d.as_dict() for d in system.list_disks()]
        usable = {d["path"] for d in self._facts["disks"] if not d["blocked"]}
        if self._model.disk and self._model.disk not in usable:
            self._model.disk = ""
            self._choices.insert("disk", "")
        self.disksChanged.emit()

    def _poll_network(self) -> None:
        online = system.network_online()      # nmcli: milliseconds
        if online != self.online:
            self._facts["online"] = online
            self.onlineChanged.emit()

    @Property(bool, notify=revisionChanged)
    def offlineInstall(self) -> bool:
        """True when this install resolves from the medium (offline ISO, KDE or no desktop)."""
        return self._facts.get("flavor") == "offline" and self._model.desktop in ("kde", "none")

    @Slot(result=str)
    def networkRequirement(self) -> str:
        """Why the install cannot proceed without a network ("" when it can)."""
        if self.online:
            return ""
        if self._facts.get("flavor") != "offline":
            return "This is the online edition: connect to a network to install."
        if self._model.desktop not in ("kde", "none"):
            return "This desktop is downloaded during the install: connect to a network."
        return ""

    @Slot(str, result=bool)
    def hasGraphicalLayout(self, keymap: str) -> bool:
        return "live" not in self._facts or system.xkb_for(keymap) is not None

    @Slot(str)
    def applyKeyboard(self, keymap: str) -> None:
        """Switch the live session to the chosen layout, so passwords typed here match
        what the new system will understand at its first boot."""
        layout = system.xkb_for(keymap)
        if layout is None or "live" not in self._facts:
            return
        xkb, variant = layout
        # --notify: KWin watches kxkbrc through KConfig and reloads on the notification
        base = ["kwriteconfig6", "--notify", "--file", "kxkbrc", "--group", "Layout", "--key"]
        system.run([*base, "LayoutList", xkb])
        system.run([*base, "VariantList", variant])
        system.run([*base, "Use", "--type", "bool", "true"])

    # ------------------------------------------------------------ secrets
    @Slot(str, str)
    def setPassword(self, password: str, repeat: str) -> None:
        self._password = [password, repeat]
        self._touch()

    @Slot(str, str)
    def setPassphrase(self, passphrase: str, repeat: str) -> None:
        self._passphrase = [passphrase, repeat]
        self._touch()

    # ------------------------------------------------------------ page checks
    @Slot(str, result=str)
    def pageError(self, page: str) -> str:
        """Why the wizard may not leave ``page`` ("" = it may). Cheap: runs on every edit."""
        c = self._model
        if page == "welcome":
            if not self._facts.get("uefi", True):
                return "This computer booted in legacy BIOS mode. Samurai OS needs UEFI."
            memory = self._facts.get("memory", 0)
            if memory and memory < system.MIN_MEMORY_MIB:
                return "At least 4 GiB of memory is needed."
        if page == "region":
            for value, known in ((c.timezone, "timezones"), (c.locale, "locales"),
                                 (c.keyboard, "keymaps")):
                if value not in self._facts.get(known, []):
                    return f"Pick a value from the list ({known[:-1]}: “{value}”)."
        if page == "disk":
            if not c.disk:
                return "Choose the disk to install on."
            if c.encryption:
                return model.secret_error(self._passphrase[0], self._passphrase[1], "Passphrase",
                                          model.MIN_PASSPHRASE)
        if page == "user":
            return (model.username_error(c.username) or model.hostname_error(c.hostname)
                    or model.secret_error(self._password[0], self._password[1], "Password",
                                          model.MIN_PASSWORD))
        if page == "kernels":
            return model.kernel_error(c)
        return ""

    # ------------------------------------------------------------ summary and profile
    @Property("QVariantMap", notify=summaryChanged)
    def summary(self) -> dict:
        return self._summary

    @Slot()
    def buildSummary(self) -> None:
        c = self._model
        text = model.to_toml(c)
        nvidia = c.nvidia == "yes" or (c.nvidia == "auto" and
                                       any(g["nvidia"] for g in self._facts.get("gpus", [])))
        amd = any("[1002]" in g["vendor"] for g in self._facts.get("gpus", []))
        result: dict = {"error": "", "groups": [], "deferred": [], "count": 0}
        disk = next((d for d in self._facts.get("disks", []) if d["path"] == c.disk), None)
        result["disk"] = disk or {}
        try:
            result["error"] = engine.validate(self._tools, text)
            if not result["error"]:
                planned = engine.plan(self._tools, text, self.offlineInstall, nvidia, amd)
                groups: dict[str, list[str]] = {}
                for package in planned["packages"]:
                    groups.setdefault(package["group"], []).append(package["name"])
                result["groups"] = [{"name": k, "packages": ", ".join(v)} for k, v in groups.items()]
                result["count"] = len(planned["packages"])
                deferred = [{"id": d["id"], "command": d["command"], "reason": d["reason"]}
                            for d in planned["deferred"]]
                if self.offlineInstall and c.apps:
                    deferred.append({"id": ", ".join(c.apps),
                                     "command": "samurai apps install " + " ".join(c.apps),
                                     "reason": "not on the offline medium"})
                result["deferred"] = deferred
        except engine.EngineError as e:
            result["error"] = str(e)
        self._summary = result
        self.summaryChanged.emit()

    @Property(str, constant=True)
    def defaultProfilePath(self) -> str:
        return str(pathlib.Path.home() / "install-profile.toml")

    @Slot(str, result=str)
    def saveProfile(self, path: str) -> str:
        """Save without the password hash. Returns a message for the page."""
        target = pathlib.Path(path).expanduser()
        if not target.is_absolute() or target.suffix != ".toml":
            return "Give a full path ending in .toml."
        try:
            engine.write_private(target, model.to_toml(self._model))
        except OSError as e:
            return f"Could not save: {e.strerror}"
        return f"Saved to {target} (without your password)."

    @Slot(str, result=str)
    def loadProfile(self, path: str) -> str:
        source = pathlib.Path(path.removeprefix("file://")).expanduser()
        try:
            if source.stat().st_size > 256 * 1024:
                return "That file is too large to be a profile."
            text = source.read_text(encoding="utf-8")
            reason = engine.validate(self._tools, text)
            if reason:
                return f"Not a usable profile: {reason}"
            self._model = model.from_toml(text, self._model)
        except (OSError, ValueError, TypeError, engine.EngineError) as e:
            return f"Could not load: {e}"
        self._model.disk = ""     # the target disk is always confirmed on this machine
        self._publish()
        self._password, self._passphrase = ["", ""], ["", ""]
        self.choicesReset.emit()
        self._touch()
        return f"Loaded {source.name}. Choose the disk and set your passwords."

    # ------------------------------------------------------------ install
    @Property(str, notify=stateChanged)
    def state(self) -> str:
        return self._state

    @Property(int, notify=progressChanged)
    def percent(self) -> int:
        return self._progress.percent

    @Property(str, notify=progressChanged)
    def stage(self) -> str:
        return self._progress.label

    @Property(str, notify=progressChanged)
    def failure(self) -> str:
        return self._progress.error

    @Slot(result=str)
    def logText(self) -> str:
        return "\n".join(self._log)

    @Slot()
    def startInstall(self) -> None:
        if self._state in ("running", "done"):
            return
        for page in ("welcome", "region", "disk", "user", "kernels"):
            problem = self.pageError(page)
            if problem:
                self._fail(problem)
                return
        try:
            password_hash = engine.hash_password(self._tools, self._password[0])
        except engine.EngineError as e:
            self._fail(str(e))
            return
        request = json.dumps({
            "profile": model.to_toml(self._model, password_hash),
            "passphrase": self._passphrase[0] if self._model.encryption else "",
        }).encode()
        self._progress, self._log, self._buffer, self._exit_status = Progress(), [], b"", None
        process = QProcess(self)
        process.setProcessChannelMode(QProcess.ProcessChannelMode.MergedChannels)
        process.readyReadStandardOutput.connect(self._read)
        process.finished.connect(self._finished)
        process.errorOccurred.connect(self._error)
        self._process = process
        self._set_state("running")
        self.progressChanged.emit()
        process.start(self._helper[0], list(self._helper[1:]))
        process.write(request)
        process.closeWriteChannel()

    def _set_state(self, state: str) -> None:
        self._state = state
        self.stateChanged.emit()

    def _fail(self, message: str) -> None:
        self._progress.error = message
        self._append(f"FATAL: {message}")
        self.progressChanged.emit()
        self._set_state("failed")

    def _append(self, line: str) -> None:
        self._log.append(line)
        del self._log[:-LOG_LINES_KEPT]
        self.logAppended.emit(line)

    def _read(self) -> None:
        assert self._process is not None
        *lines, self._buffer = (self._buffer +
                                bytes(self._process.readAllStandardOutput().data())).split(b"\n")
        changed = False
        for raw in lines:
            line = raw.decode("utf-8", "replace").rstrip("\r")
            if line.startswith(EXIT_MARKER):
                status = line.removeprefix(EXIT_MARKER).strip()
                self._exit_status = int(status) if status.isdigit() else 1
                continue
            changed = self._progress.feed(line) or changed
            self._append(line)
        if changed:
            self.progressChanged.emit()

    def _error(self, error: QProcess.ProcessError) -> None:
        if error == QProcess.ProcessError.FailedToStart:
            self._fail("Could not start the installer helper.")

    def _finished(self, code: int, _status: QProcess.ExitStatus) -> None:
        self._read()
        success = code == 0 and self._exit_status == 0
        if success:
            self._progress.percent, self._progress.label = 100, "Installation complete"
            self._password, self._passphrase = ["", ""], ["", ""]
        elif not self._progress.error:
            self._progress.error = f"The installer stopped (exit {self._exit_status or code})."
        self.progressChanged.emit()
        self._set_state("done" if success else "failed")

    @Slot(result=str)
    def saveLog(self) -> str:
        target = pathlib.Path.home() / "Desktop" / "samurai-install.log"
        try:
            target.parent.mkdir(exist_ok=True)
            engine.write_private(target, self.logText() + "\n")
        except OSError as e:
            return f"Could not save the log: {e.strerror}"
        return f"Log saved to {target}"

    @Slot()
    def openTerminal(self) -> None:
        QProcess.startDetached("konsole", ["-e", RESCUE_TERMINAL])

    @Slot()
    def reboot(self) -> None:
        QProcess.startDetached("systemctl", ["reboot"])


def gather_facts(tools: engine.Tools) -> dict:
    try:
        apps, catalog_error = engine.catalog(tools), ""
    except engine.EngineError as e:
        apps, catalog_error = [], str(e)
    return {
        "live": True,
        "flavor": system.iso_flavor(),
        "uefi": system.uefi(),
        "memory": system.memory_mib(),
        "online": system.network_online(),
        "disks": [d.as_dict() for d in system.list_disks()],
        "gpus": system.list_gpus(),
        "timezones": system.timezones(),
        "timezone": system.current_timezone(),
        "locales": system.locales(),
        "keymaps": system.keymaps(),
        "apps": apps,
        "catalogError": catalog_error,
        "offlineBrowsers": ["firefox"],
    }

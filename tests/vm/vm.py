"""QEMU VM orchestration for the Samurai OS test harness (spec 11).

Boots QEMU directly (no libvirt): q35, OVMF, virtio disk/net, serial chardev on a
unix socket (logged + scriptable), QMP for power control, guest-agent via
virtio-serial (no guest network port). Everything is reproducible from one host.
"""
from __future__ import annotations

import json
import os
import pathlib
import re
import socket
import subprocess
import time

REPO = pathlib.Path(__file__).resolve().parents[2]
WORK = REPO / ".vm"

# Serial streams are full of ANSI/OSC noise (zsh colours split words like
# 'root@samurai', boot menu box art). Matching happens on the cleaned text;
# the raw stream stays on disk in the serial log for forensics.
# Covers: CSI sequences, OSC strings, two-char charset designations (ESC ( B —
# that one sits between 'root' and '@samurai' in the live prompt!), and other
# two-character escape forms.
ANSI_RE = re.compile(
    "\x1b(?:"
    r"\[[0-9;?=!/<>*\"']*[ -/]*[@-~]"
    r"|\][^\x07\x1b]*(?:\x07|\x1b\\)"
    r"|[()][0-9A-Za-z]"
    r"|[@-Z\\-_]"
    ")"
)
OVMF_CODE_CANDIDATES = [
    "/usr/share/OVMF/OVMF_CODE_4M.fd",
    "/usr/share/OVMF/OVMF_CODE.fd",
    "/usr/share/ovmf/OVMF_CODE.fd",
]


def _find_ovmf() -> tuple[str, str]:
    for code in OVMF_CODE_CANDIDATES:
        if os.path.exists(code):
            return code, code.replace("OVMF_CODE", "OVMF_VARS")
    raise RuntimeError("no OVMF firmware found")


class VM:
    def __init__(self, name: str, iso: str | None = None, disk: str | None = None,
                 disk_size: str = "40G", ram: str = "4096", smp: str = "4"):
        self.name = name
        WORK.mkdir(parents=True, exist_ok=True)
        self.iso = iso
        self.disk_path = disk or str(WORK / f"{name}.qcow2")
        self.fresh = True
        # tests need a fresh disk every run: a reused qcow2 would boot the
        # PREVIOUS test's installed system (same bug class as stale build
        # work dirs / stale repo dbs)
        if disk is None and os.path.exists(self.disk_path):
            os.unlink(self.disk_path)
        self.ram, self.smp = ram, smp
        self.code, self.vars = _find_ovmf()
        self.vars_rw = str(WORK / f"{name}-VARS.fd")
        self.serial_sock = str(WORK / f"{name}-serial.sock")
        self.qmp_sock = str(WORK / f"{name}-qmp.sock")
        self.qga_sock = str(WORK / f"{name}-qga.sock")
        self.serial_log = str(WORK / f"{name}-serial.log")
        self.proc: subprocess.Popen | None = None
        self._serial: socket.socket | None = None
        self._serial_buf = b""
        self._seen_serial = 0

        if not disk and not os.path.exists(self.disk_path):
            subprocess.run(["qemu-img", "create", "-f", "qcow2", self.disk_path, disk_size], check=True)
        if os.path.exists(self.serial_log):
            os.unlink(self.serial_log)
        for s in (self.serial_sock, self.qmp_sock, self.qga_sock):
            if os.path.exists(s):
                os.unlink(s)

    def start(self) -> None:
        code, _ = self.code, self.vars
        subprocess.run(["cp", self.vars, self.vars_rw], check=True)
        cmd = [
            "qemu-system-x86_64", "-machine", "q35,accel=kvm", "-cpu", "host",
            "-m", self.ram, "-smp", self.smp,
            "-drive", f"if=pflash,format=raw,readonly=on,file={code}",
            "-drive", f"if=pflash,format=raw,file={self.vars_rw}",
            "-drive", f"file={self.disk_path},if=virtio,format=qcow2",
            "-nic", "user,model=virtio-net-pci",
            "-display", "none",
            "-serial", f"unix:{self.serial_sock},server=on,wait=off",
            "-qmp", f"unix:{self.qmp_sock},server=on,wait=off",
            "-chardev", f"socket,path={self.qga_sock},server=on,wait=off,id=qga0",
            "-device", "virtio-serial",
            "-device", "virtserialport,chardev=qga0,name=org.qemu.guest_agent.0",
            "-monitor", "none",
        ]
        if self.iso:
            cmd += ["-drive", f"file={self.iso},media=cdrom,read-only=on"]
        # pylint: disable=consider-using-with
        self.proc = subprocess.Popen(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.STDOUT)

    def wait_socket(self, path: str, timeout: float = 60.0) -> None:
        deadline = time.time() + timeout
        while time.time() < deadline:
            if os.path.exists(path):
                try:
                    socket.socket(socket.AF_UNIX).connect(path)  # probe
                    return
                except (ConnectionRefusedError, FileNotFoundError):
                    pass
            time.sleep(0.3)
        raise TimeoutError(f"socket never appeared: {path}")

    # --- serial ---
    def serial_connect(self) -> None:
        self.wait_socket(self.serial_sock)
        self._serial = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self._serial.settimeout(0.5)
        self._serial.connect(self.serial_sock)

    def serial_read(self, max_wait: float = 1.0) -> str:
        if self._serial is None:
            self.serial_connect()
        got = b""
        deadline = time.time() + max_wait
        while time.time() < deadline:
            try:
                data = self._serial.recv(65536)  # type: ignore[union-attr]
                if data:
                    got += data
            except socket.timeout:
                break
        if got:
            with open(self.serial_log, "ab") as f:
                f.write(got)
        return ANSI_RE.sub("", got.decode("utf-8", "replace"))

    def wait_for(self, pattern: str, timeout: float = 120.0) -> str:
        """Read the serial stream until `pattern` appears; returns the full new text."""
        deadline = time.time() + timeout
        acc = ""
        while time.time() < deadline:
            acc += self.serial_read(max_wait=1.5)
            if pattern in acc:
                return acc
            if "FATAL" in acc:
                raise RuntimeError(f"serial FATAL while waiting for {pattern!r}:\n{acc[-800:]}")
        raise TimeoutError(f"serial never showed {pattern!r}; last:\n{acc[-800:]}")

    def wait_for_any(self, patterns: list[str], timeout: float = 120.0,
                     required: bool = True) -> tuple[str, str] | tuple[None, str]:
        """Wait until any pattern appears; returns (matched_pattern, acc).

        required=False returns (None, acc) on timeout instead of raising —
        used for short opportunistic waits (e.g. autologin may beat us).
        """
        deadline = time.time() + timeout
        acc = ""
        while time.time() < deadline:
            acc += self.serial_read(max_wait=1.5)
            for p in patterns:
                if p in acc:
                    return p, acc
            if "FATAL" in acc:
                raise RuntimeError(f"serial FATAL while waiting for {patterns}:\n{acc[-800:]}")
        if required:
            raise TimeoutError(f"serial never showed any of {patterns}; last:\n{acc[-800:]}")
        return None, acc

    def send(self, text: str) -> None:
        if self._serial is None:
            self.serial_connect()
        self._serial.sendall(text.encode())  # type: ignore[union-attr]

    def login_root(self, timeout: float = 300.0) -> None:
        """Get a root shell on the serial console, autologin-aware.

        The live ISO autologins instantly (serial-getty drop-in): the getty
        prompt prints a beat before the autologin completes, so sending
        'root' blindly would just be an unknown command in the shell. Wait
        briefly for autologin; only if it does not happen, log in by hand.
        """
        matched, _ = self.wait_for_any(["root@samurai", " login:"], timeout=timeout)
        if matched != "root@samurai":
            # autologin may still land on its own (spawn under boot load is
            # slow); give it a real chance before logging in by hand
            m, _ = self.wait_for_any(["root@samurai"], timeout=15, required=False)
            if m is None:
                self.send("root\n")
                self.wait_for("root@samurai", timeout=120)

    def serial_cmd(self, cmd: str, expect: str, timeout: float = 180.0) -> str:
        # Prompt-race guard: a send during prompt rendering gets swallowed.
        # Nudge the shell with a bare newline, let it settle, discard noise,
        # then send the real command.
        self.send("\n")
        time.sleep(1.0)
        self.serial_read(max_wait=1.0)
        self.send(cmd + "\n")
        return self.wait_for(expect, timeout=timeout)

    # --- guest agent ---
    def qga(self, timeout: float = 120.0):
        from qga import GuestAgent  # noqa: local import for sys.path simplicity
        self.wait_socket(self.qga_sock, timeout=60)
        return GuestAgent(self.qga_sock, timeout=timeout)

    def ensure_qga(self) -> "GuestAgent":
        """Start the agent in the guest if the socket isn't live yet."""
        try:
            return self.qga(timeout=10)
        except Exception:
            # via serial: log in if needed, start the agent, reconnect
            self.login_root(timeout=240)
            self.send("systemctl start qemu-guest-agent.service\n")
            time.sleep(2)
            return self.qga()

    # --- power ---
    def qmp_command(self, cmd: str, **args) -> dict:
        self.wait_socket(self.qmp_sock)
        s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        s.settimeout(30)
        s.connect(self.qmp_sock)
        f = s.makefile("rw")
        f.readline()  # greeting
        f.write('{"execute": "qmp_capabilities"}\n'); f.flush(); f.readline()
        req = {"execute": cmd}
        if args:
            req["arguments"] = args
        f.write(json.dumps(req) + "\n"); f.flush()
        resp = f.readline()
        s.close()
        return json.loads(resp)

    def qmp_query_block(self) -> list:
        self.wait_socket(self.qmp_sock)
        s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        s.settimeout(30)
        s.connect(self.qmp_sock)
        f = s.makefile("rw")
        f.readline()
        f.write('{"execute": "qmp_capabilities"}\n'); f.flush(); f.readline()
        f.write('{"execute": "query-block"}\n'); f.flush()
        resp = json.loads(f.readline())
        s.close()
        return resp.get("return", [])

    def eject_iso(self) -> None:
        """Remove the attached ISO so the reboot lands on the installed disk
        (OVMF prefers the CD when both are present)."""
        for b in self.qmp_query_block():
            if b.get("removable") and b.get("inserted"):
                device = b["device"]
                self.qmp_command("eject", device=device, force=True)
                return

    def reboot(self) -> None:
        self.qmp_command("system_reset")

    def stop(self) -> None:
        if self.proc:
            try:
                self.qmp_command("quit")
            except Exception:
                self.proc.terminate()
            try:
                self.proc.wait(timeout=10)
            except subprocess.TimeoutExpired:
                self.proc.kill()
            self.proc = None

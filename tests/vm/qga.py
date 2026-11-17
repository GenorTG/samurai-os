"""Minimal QEMU guest-agent client over a unix socket (spec 11 control channel).

The guest agent opens no network port (virtio-serial only) — that is the point:
command execution inside the guest without exposing any attack surface.
"""
from __future__ import annotations

import base64
import json
import socket
import time


class GuestAgent:
    def __init__(self, socket_path: str, timeout: float = 120.0):
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.settimeout(timeout)
        self.sock.connect(socket_path)
        self._buf = b""

    def _recv_json(self) -> dict:
        while True:
            if b"\n" in self._buf:
                line, self._buf = self._buf.split(b"\n", 1)
                if line.strip():
                    return json.loads(line)
            data = self.sock.recv(65536)
            if not data:
                raise ConnectionError("guest agent socket closed")
            self._buf += data

    def _call(self, cmd: str, **args) -> dict:
        self.sock.sendall(json.dumps({"execute": cmd, "arguments": args}).encode() + b"\n")
        return self._recv_json()

    def exec(self, cmdline: str, timeout: float = 120.0, check: bool = True) -> tuple[int, str]:
        """Run /bin/sh -c 'cmdline' in the guest; returns (exit_code, stdout)."""
        r = self._call("guest-exec", path="/bin/sh", arg=["-c", cmdline], capture_output=True)
        if "error" in r:
            desc = r["error"].get("desc", "")
            if "capture-output" in desc or "unexpected" in desc:
                # older qemu-ga: no capture-output; capture through a guest file
                return self._exec_file_capture(cmdline, timeout, check)
            raise RuntimeError(f"guest-exec rejected: {r['error']}")
        pid = r["return"]["pid"]
        return self._wait_pid(pid, cmdline, timeout, check)

    def _exec_file_capture(self, cmdline: str, timeout: float, check: bool) -> tuple[int, str]:
        import uuid
        out_path = f"/tmp/.samurai-qga-{uuid.uuid4().hex[:8]}.out"
        r = self._call("guest-exec", path="/bin/sh",
                       arg=["-c", f"({cmdline}) >{out_path} 2>&1"])
        if "error" in r:
            raise RuntimeError(f"guest-exec rejected: {r['error']}")
        code, _ = self._wait_pid(r["return"]["pid"], cmdline, timeout, check=False)
        text = self._read_file(out_path)
        self._call("guest-exec", path="/bin/sh", arg=["-c", f"rm -f {out_path}"])
        if check and code != 0:
            raise RuntimeError(f"guest command failed ({code}): {cmdline}\n{text}")
        return code, text

    def _read_file(self, path: str) -> str:
        r = self._call("guest-file-open", path=path, mode="r")
        if "error" in r:
            raise RuntimeError(f"guest-file-open rejected: {r['error']}")
        fh = r["return"]
        chunks = []
        while True:
            r = self._call("guest-file-read", handle=fh, count=65536)
            if "error" in r:
                raise RuntimeError(f"guest-file-read rejected: {r['error']}")
            chunks.append(base64.b64decode(
                r["return"].get("data") or r["return"].get("buf-b64") or b""
            ).decode("utf-8", "replace"))
            if r["return"].get("eof") or not r["return"].get("data"):
                break
        self._call("guest-file-close", handle=fh)
        return "".join(chunks)

    def _wait_pid(self, pid: int, cmdline: str, timeout: float, check: bool) -> tuple[int, str]:
        deadline = time.time() + timeout
        while True:
            s = self._call("guest-exec-status", pid=pid)
            st = s["return"]
            if st.get("exited"):
                out = base64.b64decode(st.get("out-data", "") or b"").decode("utf-8", "replace")
                err = base64.b64decode(st.get("err-data", "") or b"").decode("utf-8", "replace")
                code = st.get("exitcode", -1)
                if check and code != 0:
                    raise RuntimeError(f"guest command failed ({code}): {cmdline}\n{err}{out}")
                return code, out + err
            if time.time() > deadline:
                raise TimeoutError(f"guest command timed out: {cmdline}")
            time.sleep(0.4)

    def close(self) -> None:
        self.sock.close()

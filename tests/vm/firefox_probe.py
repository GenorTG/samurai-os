"""Runs inside the guest: ask a real headless Firefox which policies and sandbox it ended up with.

Speaks Firefox's Marionette protocol (length-prefixed JSON) directly, so the guest needs nothing but
Firefox and the standard library. Test-only: remote system access is enabled on a throwaway profile.
"""
import json
import shutil
import socket
import subprocess
import tempfile
import time

SCRIPT = """
const sys = name => { try { return Services.sysinfo.getProperty(name); } catch (e) { return null; } };
const policies = Services.policies.getActivePolicies() || {};
return {
  active: Services.policies.status,
  DisableTelemetry: policies.DisableTelemetry === true,
  uBlock: !!(policies.ExtensionSettings || {})["uBlock0@raymondhill.net"],
  telemetry_upload: Services.prefs.getBoolPref("datareporting.healthreport.uploadEnabled", true),
  hasSeccompBPF: sys("hasSeccompBPF"),
  hasUserNamespaces: sys("hasUserNamespaces"),
  canSandboxContent: sys("canSandboxContent"),
};
"""


def receive(sock):
    head = b""
    while not head.endswith(b":"):
        byte = sock.recv(1)
        if not byte:
            raise ConnectionError("marionette closed")
        head += byte
    size, body = int(head[:-1]), b""
    while len(body) < size:
        body += sock.recv(size - len(body))
    return json.loads(body)


def call(sock, number, name, params):
    data = json.dumps([0, number, name, params]).encode()
    sock.sendall(str(len(data)).encode() + b":" + data)
    reply = receive(sock)
    if reply[2]:
        raise RuntimeError(reply[2])
    return reply[3]


def main():
    profile = tempfile.mkdtemp(prefix="ffprobe-")
    firefox = subprocess.Popen(
        ["firefox", "--headless", "--marionette", "--remote-allow-system-access", "--no-remote",
         "--profile", profile, "about:blank"],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        for _ in range(90):
            try:
                sock = socket.create_connection(("127.0.0.1", 2828), timeout=60)
                break
            except OSError:
                time.sleep(1)
        else:
            raise SystemExit("marionette never listened")
        with sock:
            receive(sock)  # greeting
            call(sock, 1, "WebDriver:NewSession", {})
            call(sock, 2, "Marionette:SetContext", {"value": "chrome"})
            result = call(sock, 3, "WebDriver:ExecuteScript", {"script": SCRIPT, "args": []})
            print(json.dumps(result.get("value", result), indent=1))
            call(sock, 4, "Marionette:SetContext", {"value": "content"})
            call(sock, 5, "WebDriver:Navigate", {"url": "data:text/html,<title>sandboxed</title>ok"})
            print("TITLE", call(sock, 6, "WebDriver:GetTitle", {}))
            print("FIREFOX-PROBE-OK")
    finally:
        firefox.terminate()
        try:
            firefox.wait(timeout=30)
        except subprocess.TimeoutExpired:
            firefox.kill()
        shutil.rmtree(profile, ignore_errors=True)


if __name__ == "__main__":
    main()

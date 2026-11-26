"""The toolbox's privilege boundary: `samurai-gui-helper` and its polkit policy.

The helper is standard-library C++, so it is compiled here with the host compiler against a fake
CLI (the root check and CLI path are compile-time test overrides).
"""
from __future__ import annotations

import os
import pathlib
import re
import shutil
import subprocess
import time
import xml.etree.ElementTree as ET

import pytest

from conftest import REPO, fake_index_data

GUI = REPO / "toolbox" / "gui"
POLICY = GUI / "data" / "org.samurai.toolbox.policy"
HELPER_PATH = "/usr/lib/samurai/gui-helper"
CXX = shutil.which(os.environ.get("CXX", "g++"))

pytestmark = pytest.mark.skipif(CXX is None, reason="no C++ compiler")

FAKE_CLI = """#!/bin/bash
printf 'ARG=%s\\n' "$@"
if [ "$1" = health ] && [ "$2" = --json ]; then env | sort; echo "CWD=$PWD"; fi
echo to-stderr >&2
if [ "$1" = fix ] && [ "$2" = dkms ]; then exit 3; fi
if [ "$1" = update ]; then
    # long output, then a marker file: proves the run finished although nobody was reading
    for i in $(seq 20000); do echo "line $i ................................................"; done
    echo done > "$(dirname "$0")/finished"
fi
if [ "$1" = mirrors ]; then
    sleep 30 &    # a daemon that keeps the pipes open after the command exits
fi
"""


def compile_helper(out: pathlib.Path, *defines: str) -> None:
    subprocess.run(
        [CXX, "-std=c++20", "-Wall", "-Wextra", "-Werror", "-O1", *defines,
         str(GUI / "common" / "operations.cpp"), str(GUI / "helper" / "main.cpp"), "-o", str(out)],
        check=True)


@pytest.fixture(scope="module")
def helper(tmp_path_factory):
    d = tmp_path_factory.mktemp("helper")
    cli = d / "samurai"
    cli.write_text(FAKE_CLI)
    cli.chmod(0o755)
    exe = d / "gui-helper"
    compile_helper(exe, f'-DSAMURAI_CLI="{cli}"', "-DSAMURAI_REQUIRE_ROOT=0", "-DSAMURAI_STALL_MS=500")
    return exe


def run(helper, *args, **kw):
    return subprocess.run([str(helper), *args], capture_output=True, text=True, timeout=60, **kw)


def cli_args(stdout: str) -> list[str]:
    return [line[4:] for line in stdout.splitlines() if line.startswith("ARG=")]


@pytest.mark.parametrize("request_, expected", [
    (["health-read"], ["health", "--json"]),
    (["health-fix", "security.firewall"], ["health", "--fix", "--yes", "--id", "security.firewall"]),
    (["health-fix-safe"], ["health", "--fix"]),
    (["apps-install", "firefox", "@gaming"],
     ["apps", "install", "--yes", "--no-user-scripts", "--", "firefox", "@gaming"]),
    (["apps-install-upgrade", "htop"],
     ["apps", "install", "--yes", "--no-user-scripts", "--with-upgrade", "--", "htop"]),
    (["apps-remove", "htop"], ["apps", "remove", "--yes", "--", "htop"]),
    (["clean", "journal", "orphans"], ["clean", "journal", "orphans", "--yes"]),
    (["kernel-install", "linux-cachyos-lts"], ["kernel", "install", "linux-cachyos-lts"]),
    (["kernel-install-anyway", "linux-zen"], ["kernel", "install", "linux-zen", "--yes"]),
    (["kernel-remove", "linux-lts"], ["kernel", "remove", "linux-lts"]),
    (["kernel-default", "linux"], ["kernel", "default", "linux"]),
    (["boot-timeout", "60"], ["boot", "set", "timeout", "60"]),
    (["boot-param-add", "mitigations=off"], ["boot", "set", "param", "add", "mitigations=off"]),
    (["boot-param-add", "systemd.show_status=false"],
     ["boot", "set", "param", "add", "systemd.show_status=false"]),
    (["boot-param-remove", "quiet"], ["boot", "set", "param", "remove", "quiet"]),
    (["snap-create", 'before "big" change; $(id) zażółć'],
     ["snap", "create", 'before "big" change; $(id) zażółć']),
    (["snap-delete", "12"], ["snap", "delete", "12"]),
    (["fix-keyring-reset"], ["fix", "keyring", "--reset", "--yes"]),
    (["diag-bundle"], ["diag", "bundle", "--out", "-"]),
])
def test_allowed_requests_become_fixed_argv(helper, request_, expected):
    out = run(helper, *request_)
    assert out.returncode == 0, out.stderr
    assert cli_args(out.stdout) == expected


@pytest.mark.parametrize("request_", [
    [],
    ["shell"],
    ["rescue"],
    ["health"],                                   # CLI names are not verbs
    ["health-read", "--fix"],                     # no extra arguments
    ["update", "--skip-interventions"],
    ["health-fix"],
    ["health-fix", "--yes"],
    ["health-fix", "security.firewall;reboot"],
    ["health-fix", "a.b", "c.d"],
    ["apps-install"],
    ["apps-install", "--with-upgrade"],
    ["apps-install", "-y"],
    ["apps-install", "firefox", "a b"],
    ["apps-install", "../x"],
    ["apps-install", "$(id)"],
    ["apps-install", "@"],
    ["apps-install", ""],
    ["apps-install", *["a"] * 33],
    ["clean"],
    ["clean", "cache"],                           # user data is never cleaned as root
    ["clean", "thumbnails"],
    ["clean", "journal", "--keep", "0"],
    ["kernel-install", "bash"],
    ["kernel-install", "linux --overwrite"],
    ["kernel-install", "linux-"],
    ["kernel-remove", "--noconfirm"],
    ["boot-timeout", "61"],
    ["boot-timeout", "-1"],
    ["boot-timeout", "1e1"],
    ["boot-param-add", "init=/bin/sh"],
    ["boot-param-add", "rdinit=/bin/sh"],
    ["boot-param-add", "systemd.run=/tmp/x"],
    ["boot-param-add", "systemd.unit=emergency.target"],
    ["boot-param-add", "systemd.debug-shell=1"],
    ["boot-param-add", "systemd.debug_shell"],
    ["boot-param-add", "systemd.wants=debug-shell.service"],
    ["boot-param-add", "rd.break"],
    ["boot-param-add", "rd.systemd.debug_shell"],
    ["boot-param-add", "single"],
    ["boot-param-add", "1"],
    ["boot-param-add", "a b"],
    ["boot-param-add", "x\nmodule_path: evil"],
    ["boot-param-remove", "-x"],
    ["snap-create", "-d"],
    ["snap-create", "two\nlines"],
    ["snap-create", "x" * 101],
    ["snap-delete", "0"],
    ["snap-delete", "1-5"],
    ["snap-delete", "--sync"],
    ["diag-bundle", "--no-redact"],
])
def test_everything_else_is_refused_before_the_cli_runs(helper, request_):
    out = run(helper, *request_)
    assert out.returncode == 64, (request_, out.stdout, out.stderr)
    assert cli_args(out.stdout) == []


def test_environment_is_rebuilt_and_cwd_is_root(helper):
    env = {"PATH": "/tmp/evil", "LD_LIBRARY_PATH": "/tmp/evil", "SAMURAI_CATALOG_DIR": "/tmp/evil",
           "PYTHONPATH": "/tmp/evil", "BASH_ENV": "/tmp/evil", "PKEXEC_UID": "1000", "HOME": "/home/x"}
    out = run(helper, "health-read", env=env)
    assert out.returncode == 0, out.stderr
    assert "evil" not in out.stdout
    assert "PATH=/usr/local/sbin:/usr/local/bin:/usr/bin\n" in out.stdout
    assert "HOME=/root\n" in out.stdout and "PKEXEC_UID=1000\n" in out.stdout
    assert "CWD=/\n" in out.stdout
    bad = run(helper, "health-read", env={**env, "PKEXEC_UID": "0; id"})
    assert "PKEXEC_UID" not in bad.stdout


def test_exit_code_and_streams_pass_through(helper):
    out = run(helper, "fix-dkms")
    assert out.returncode == 3
    assert "ARG=fix" in out.stdout and "to-stderr" in out.stderr and "to-stderr" not in out.stdout
    merged = subprocess.run([str(helper), "fix-dkms"], stdout=subprocess.PIPE,
                            stderr=subprocess.STDOUT, text=True, timeout=60)
    assert merged.stdout == "ARG=fix\nARG=dkms\nto-stderr\n", "merged streams keep their order"


@pytest.mark.parametrize("reader", ["closes", "stalls"])
def test_operation_finishes_when_the_reader_goes_away(helper, reader):
    marker = helper.parent / "finished"
    marker.unlink(missing_ok=True)
    proc = subprocess.Popen([str(helper), "update"], stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
    try:
        assert proc.stdout.read(100)
        if reader == "closes":
            proc.stdout.close()        # the toolbox crashed or was closed
        assert proc.wait(timeout=30) == 0
        assert marker.read_text() == "done\n"
    finally:
        if proc.poll() is None:
            proc.kill()
        if proc.stdout and not proc.stdout.closed:
            proc.stdout.close()


def test_daemons_holding_the_pipes_do_not_hang_the_helper(helper):
    start = time.monotonic()
    out = run(helper, "mirrors-refresh")
    assert out.returncode == 0
    assert time.monotonic() - start < 10


def test_release_build_refuses_without_root(tmp_path):
    if os.geteuid() == 0:
        pytest.skip("running as root")
    exe = tmp_path / "gui-helper"
    compile_helper(exe)
    out = run(exe, "health-read")
    assert out.returncode == 64 and "pkexec" in out.stderr


def test_cli_accepts_the_argument_vectors(apps, catalog, tmp_path):
    """`--` before app ids must parse in samurai-apps, as the helper sends it."""
    import json
    index = tmp_path / "index.json"
    index.write_text(json.dumps(fake_index_data(catalog)))
    for argv in (["install", "--yes", "--no-user-scripts", "--", "htop", "@dev"],
                 ["install", "--yes", "--no-user-scripts", "--with-upgrade", "--", "htop"],
                 ["remove", "--yes", "--", "htop"]):
        args = apps.build_parser().parse_args(argv)
        assert args.ids == argv[argv.index("--") + 1:]


# ------------------------------------------------------------------ polkit policy
READ_ONLY = {"health-read", "kernel-list", "boot-get", "snap-list"}


def operation_verbs() -> list[str]:
    source = (GUI / "common" / "operations.cpp").read_text()
    return re.findall(r'\{"([a-z-]+)", Arity::', source)


def policy_actions() -> dict[str, ET.Element]:
    root = ET.parse(POLICY).getroot()
    return {a.get("id"): a for a in root.findall("action")}


def test_every_operation_has_exactly_one_polkit_action():
    verbs = operation_verbs()
    assert len(verbs) == len(set(verbs)) and len(verbs) > 20
    actions = policy_actions()
    by_verb = {}
    for action_id, action in actions.items():
        notes = {n.get("key"): n.text for n in action.findall("annotate")}
        # without argv1 an action would match every operation of the helper
        assert notes["org.freedesktop.policykit.exec.path"] == HELPER_PATH, action_id
        verb = notes["org.freedesktop.policykit.exec.argv1"]
        assert verb not in by_verb, verb
        by_verb[verb] = action_id
        assert action_id == f"org.samurai.toolbox.{verb}"
        assert "org.freedesktop.policykit.exec.allow_gui" not in notes
        assert action.findtext("description") and action.findtext("message")
    assert set(by_verb) == set(verbs)


def test_only_read_only_operations_skip_authentication():
    for action_id, action in policy_actions().items():
        verb = action_id.removeprefix("org.samurai.toolbox.")
        defaults = {d.tag: d.text for d in action.find("defaults")}
        assert defaults["allow_any"] == "no", action_id
        assert defaults["allow_inactive"] == "no", action_id
        # never *_keep: a retained authorisation could be reused by another request
        assert defaults["allow_active"] == ("yes" if verb in READ_ONLY else "auth_admin"), action_id

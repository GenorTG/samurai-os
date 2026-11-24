"""M5 `samurai health` on a real install (spec 05 + spec 11 `health-*`).

Installs once, upgrades samurai-tools to the locally built package (the repo does not carry it yet),
then: baseline run, break the system on purpose, detect, repair, detect again.

  SAMURAI_TEST_TOOLS_PKG=/abs/samurai-tools-<ver>-x86_64.pkg.tar.zst pytest -q -s test_health.py
"""
from __future__ import annotations

import json
import os
import pathlib
import shutil

import pytest

from conftest import INSTALL_TIMEOUT, find_iso
from vm import VM


def _need_pkg() -> pathlib.Path:
    env = os.environ.get("SAMURAI_TEST_TOOLS_PKG", "")
    if not env or not pathlib.Path(env).is_file():
        pytest.skip("set SAMURAI_TEST_TOOLS_PKG to a built samurai-tools x86_64 package")
    return pathlib.Path(env)


@pytest.fixture(scope="module")
def installed_vm(mirror):
    pkg = _need_pkg()
    shutil.copy2(pkg, mirror.dir / pkg.name)
    vm = VM("health-inst", iso=str(find_iso()), disk_size="20G")
    vm.start()
    try:
        vm.serial_connect()
        vm.login_root(timeout=280)
        ga = vm.ensure_qga()
        _, acc = ga.exec(
            "curl -fsSL http://10.0.2.2:8966/profile-test.toml -o /tmp/profile-test.toml "
            "&& SAMURAI_TEST_AUTOKEY=1 SAMURAI_SERIAL_CONSOLE=1 samurai-install --profile /tmp/profile-test.toml --disk /dev/vda --yes "
            ">/dev/null 2>&1; rc=$?; echo INSTALL-RC=$rc; [ $rc -eq 0 ] || tail -n 60 /var/log/samurai/install.log",
            timeout=INSTALL_TIMEOUT, check=False,
        )
        assert "INSTALL-RC=0" in acc, acc[-2500:]
        ga.close()
        vm.eject_iso()
        vm.reboot()
        vm.wait_for("login:", timeout=300)
        ga = vm.qga()
        _, out = ga.exec(
            f"curl -fsSL http://10.0.2.2:8966/{pkg.name} -o /tmp/st.pkg.tar.zst && "
            "pacman -U --noconfirm /tmp/st.pkg.tar.zst >/tmp/pacman-U.log 2>&1; echo U-RC=$?; tail -5 /tmp/pacman-U.log",
            timeout=300, check=False)
        assert "U-RC=0" in out, out
        yield vm, ga
    finally:
        vm.stop()


def _health(ga, *args: str) -> tuple[int, dict]:
    _, out = ga.exec(f"samurai health --json {' '.join(args)}; echo HEALTH-RC=$?", check=False)
    body, _, rc = out.rpartition("HEALTH-RC=")
    return int(rc.strip()), json.loads(body)


def _status(report: dict) -> dict[str, str]:
    return {r["id"]: r["status"] for r in report["results"]}


def test_health_baseline_break_detect_repair(installed_vm):
    _, ga = installed_vm
    _, out = ga.exec("samurai version")
    assert "0.4." in out, out

    # baseline: a fresh install has no error-level issue and answers every check
    rc, rep = _health(ga)
    assert rep["schema"] == 1
    bad = [r for r in rep["results"] if r["status"] == "issue" and r["severity"] in ("error", "critical")]
    assert not bad, f"fresh install reports errors: {bad}"
    st = _status(rep)
    for must in ("services.failed", "security.firewall", "pacman.conf", "packages.core"):
        assert st[must] == "ok", f"{must}: {st[must]} ({rep})"
    assert rc in (0, 1)

    # break three things on purpose
    ga.exec(
        "systemctl stop firewalld; "
        "systemd-run --unit=samurai-health-fail /bin/false >/dev/null 2>&1; sleep 2; "
        "touch /etc/samurai-health-test.conf.pacnew",
        check=False)
    rc, rep = _health(ga)
    st = _status(rep)
    assert st["security.firewall"] == "issue", rep
    assert st["services.failed"] == "issue", rep
    assert st["pacman.pacnew"] == "issue", rep
    assert rc == 1
    failed = next(r for r in rep["results"] if r["id"] == "services.failed")
    assert any("samurai-health-fail" in e for e in failed["evidence"]), failed
    # filters work on the real system
    rc, rep = _health(ga, "--id", "security.firewall")
    assert [r["id"] for r in rep["results"]] == ["security.firewall"]

    # repair and re-check: the issues are gone
    ga.exec(
        "systemctl start firewalld; systemctl reset-failed; rm -f /etc/samurai-health-test.conf.pacnew; sleep 2",
        check=False)
    rc, rep = _health(ga)
    st = _status(rep)
    assert st["security.firewall"] == "ok", rep
    assert st["services.failed"] == "ok", rep
    assert st["pacman.pacnew"] == "ok", rep

    # daily timer + sandboxed service write /var/lib/samurai/health.json (spec 05)
    _, out = ga.exec("systemctl is-enabled samurai-health.timer", check=False)
    assert out.strip() == "enabled", out
    ga.exec("systemctl stop firewalld", check=False)
    _, out = ga.exec(
        "systemctl start samurai-health.service; echo START-RC=$?; systemctl is-failed samurai-health.service; "
        "stat -c '%a' /var/lib/samurai/health.json; cat /var/lib/samurai/health.json",
        check=False)
    assert "START-RC=0" in out, f"issues found must not fail the unit: {out}"
    head, _, body = out.partition("\n644\n")
    assert body, f"report missing or wrong mode: {out}"
    assert "failed" not in head, head
    stored = json.loads(body)
    st = _status(stored)
    assert stored["schema"] == 1 and stored["issues"] >= 1
    assert st["security.firewall"] == "issue", stored
    # the sandbox must not blind the checks: these answer ok/issue, never skipped
    for must in ("packages.core", "services.failed", "pacman.conf"):
        assert st[must] in ("ok", "issue"), f"{must} skipped under the unit sandbox: {stored}"
    ga.exec("systemctl start firewalld", check=False)

    # human output + usage error exit code
    _, out = ga.exec("samurai health --category services; echo rc=$?", check=False)
    assert "services.failed" in out, out
    _, out = ga.exec("samurai health --bogus >/dev/null 2>&1; echo rc=$?", check=False)
    assert out.strip().endswith("rc=2"), out


def test_health_fix_is_snapshot_first_and_verified(installed_vm):
    _, ga = installed_vm
    # not root: refused, nothing touched
    _, out = ga.exec("su samurai -c 'samurai health --fix --id security.firewall' 2>&1; echo rc=$?", check=False)
    assert "needs root" in out and out.strip().endswith("rc=1"), out

    # break the firewall completely (stopped AND disabled), then repair it
    ga.exec("systemctl disable --now firewalld", check=False)
    _, out = ga.exec("systemctl is-active firewalld; systemctl is-enabled firewalld", check=False)
    assert "inactive" in out and "disabled" in out, out
    _, before = ga.exec("snapper -c root list --columns number | wc -l", check=False)
    _, out = ga.exec("samurai health --fix --id security.firewall; echo rc=$?", timeout=300, check=False)
    assert "snapshot: created" in out, out
    assert "fixed    security.firewall" in out and out.strip().endswith("rc=0"), out
    _, out = ga.exec("systemctl is-active firewalld; systemctl is-enabled firewalld", check=False)
    assert out.split() == ["active", "enabled"], out
    _, after = ga.exec("snapper -c root list --columns number | wc -l", check=False)
    assert int(after.strip()) == int(before.strip()) + 1, "exactly one pre-fix snapshot expected"
    _, out = ga.exec("snapper -c root list --columns description | grep -c 'samurai health --fix'", check=False)
    assert out.strip() == "1", out

    # nothing left to fix: no snapshot, clean exit
    _, out = ga.exec("samurai health --fix --id security.firewall; echo rc=$?", check=False)
    assert "nothing to fix automatically" in out and out.strip().endswith("rc=0"), out
    _, again = ga.exec("snapper -c root list --columns number | wc -l", check=False)
    assert again.strip() == after.strip(), "no snapshot when there is nothing to fix"

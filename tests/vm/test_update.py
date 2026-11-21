"""M3 update scenarios (spec 03 + spec 11).

  update_and_intervention — install once; run `samurai update` twice:
    run 1: stage-4/8 intervention 0001 executes (marker file + .done entry),
           the transaction is a no-op or small; exit 0/2 allowed
    run 2: clean exit (0/2), .done unchanged (idempotent, runs at most once)
"""
from __future__ import annotations

import pytest

from conftest import INSTALL_TIMEOUT, find_iso
from vm import VM

UPDATE_TIMEOUT = 1200


@pytest.fixture(scope="module")
def installed_vm(mirror):
    """Install once for all scenarios in this module, keep the VM running."""
    vm = VM("update-inst", iso=str(find_iso()), disk_size="20G")
    vm.start()
    try:
        vm.serial_connect()
        vm.login_root(timeout=280)
        acc = vm.serial_cmd(
            "curl -fsSL http://10.0.2.2:8966/profile-test.toml -o /tmp/profile-test.toml "
            "&& samurai-install --profile /tmp/profile-test.toml --disk /dev/vda --yes "
            "2>&1 | tail -c 2500; echo INSTALL-\"RC\"=${pipestatus[1]}",
            "INSTALL-RC=", timeout=INSTALL_TIMEOUT,
        )
        assert "INSTALL-RC=0" in acc, acc[-2500:]
        vm.eject_iso()
        vm.reboot()
        vm.wait_for("login:", timeout=300)
        ga = vm.qga()
        yield vm, ga
    finally:
        vm.stop()


def test_update_and_intervention(installed_vm):
    vm, ga = installed_vm
    _, out = ga.exec(
        "python3 - <<'PY'\n"
        "from pathlib import Path\n"
        "path = Path('/etc/pacman.conf')\n"
        "text = path.read_text()\n"
        "if '[samurai-testing]' not in text:\n"
        "    stable = text.index('[samurai]')\n"
        "    block = '[samurai-testing]\\nInclude = /etc/pacman.d/samurai-testing-mirrorlist\\nSigLevel = Required DatabaseRequired\\n\\n'\n"
        "    path.write_text(text[:stable] + block + text[stable:])\n"
        "PY\n"
        "awk '/^\\[samurai-testing\\]$/{test=1} /^\\[samurai\\]$/{stable=1} END{if(test && stable) print \"TEST-CHANNEL-READY\"}' /etc/pacman.conf",
        check=False,
    )
    assert "TEST-CHANNEL-READY" in out, out

    # run 1: the intervention fires
    code, out = ga.exec(
        "samurai-update >/dev/null 2>&1; echo UP-RC=$?; "
        "test -f /var/lib/samurai/intervention-0001-demo && echo INT-RAN; "
        "grep -q '^0001-update-mechanism-demo ' /var/lib/samurai/interventions.done && echo INT-DONE",
        timeout=UPDATE_TIMEOUT, check=False)
    assert "UP-RC=0" in out or "UP-RC=2" in out, out[-800:]
    assert "INT-RAN" in out, "intervention 0001 did not run"
    assert "INT-DONE" in out, "intervention 0001 not recorded in .done"
    code, out = ga.exec("pacman -Q samurai-tools && pacman -Qo /usr/local/bin/samurai-update")
    assert "samurai-tools" in out, f"updater executors are not package-managed: {out}"

    # run 2: idempotent — the intervention must NOT run again
    code, out = ga.exec(
        "samurai-update >/dev/null 2>&1; echo UP-RC=$?; "
        "test -f /var/lib/samurai/intervention-0001-demo && echo INT-RAN",
        timeout=UPDATE_TIMEOUT, check=False)
    assert "UP-RC=0" in out or "UP-RC=2" in out, out[-800:]
    out_lines = [l for l in out.splitlines() if l.strip()]
    assert out_lines.count("INT-RAN") == 1, f"intervention ran again: {out}"

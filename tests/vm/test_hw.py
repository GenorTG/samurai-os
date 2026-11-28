"""`samurai hw` on a real install in QEMU (spec 07 + spec 11): the QEMU guest profile through real
chwd/pacman/snapper, the `drivers.*` checks, and the hardware-change flow (a virtio GPU added and
removed between boots). No real GPU is involved: NVIDIA paths are covered by ci/test-hw.sh.

  SAMURAI_TEST_ISO=<iso> SAMURAI_TEST_TOOLS_PKG=<samurai-tools> SAMURAI_TEST_HW_PKGS=<chwd>:<hardware> \
      pytest -q -s test_hw.py
"""
from __future__ import annotations

import json
import os
import pathlib
import shutil

import pytest

from conftest import INSTALL_TIMEOUT, find_iso
from vm import VM

OVERWRITE = ("--overwrite '/usr/local/bin/samurai*' --overwrite '/usr/share/samurai/*' "
             "--overwrite '/usr/lib/samurai/*'")
SECOND_GPU = ["-device", "virtio-gpu-pci"]


def _packages() -> list[pathlib.Path]:
    names = [os.environ.get("SAMURAI_TEST_TOOLS_PKG", ""),
             *os.environ.get("SAMURAI_TEST_HW_PKGS", "").split(":")]
    paths = [pathlib.Path(n) for n in names if n]
    if len(paths) != 3 or not all(p.is_file() for p in paths):
        pytest.skip("set SAMURAI_TEST_TOOLS_PKG and SAMURAI_TEST_HW_PKGS=<chwd>:<hardware>")
    return paths


def run(ga, cmd: str, timeout: float = 300, ok: tuple[int, ...] = (0,)) -> str:
    code, out = ga.exec(f"{{ {cmd}; }} 2>&1", timeout=timeout, check=False)
    print(f"$ {cmd}\n{out[-2500:]}[exit {code}]", flush=True)
    assert code in ok, f"{cmd}: exit {code}\n{out[-3000:]}"
    return out


def detect(ga) -> dict:
    code, out = ga.exec("samurai hw detect --json", timeout=120, check=False)
    assert code == 0, out
    return json.loads(out)


def drivers(ga) -> dict[str, str]:
    _, out = ga.exec("samurai health --category drivers --json", timeout=120, check=False)
    return {r["id"]: r["status"] for r in json.loads(out)["results"]}


def boot(disk: str, name: str, extra: list[str]):
    vm = VM(name, disk=disk)
    vm.extra_args = extra
    vm.start()
    try:
        vm.serial_connect()
        vm.wait_for("login:", timeout=300)
        ga = vm.qga(timeout=120)
        # the boot service has finished before the assertions look at its result
        run(ga, "for i in $(seq 60); do systemctl is-active -q multi-user.target && exit 0; "
                "sleep 2; done; exit 1", timeout=180)
    except BaseException:
        vm.stop()
        raise
    return vm, ga


@pytest.fixture(scope="module")
def installed(mirror):
    """Default-base install without a desktop, upgraded to this branch's packages. Powered off."""
    pkgs = _packages()
    for p in pkgs:
        shutil.copy2(p, mirror.dir / p.name)
    vm = VM("hw-inst", iso=str(find_iso()), disk_size="24G")
    vm.start()
    try:
        vm.serial_connect()
        vm.login_root(timeout=280)
        ga = vm.ensure_qga()
        out = run(ga, "curl -fsSL http://10.0.2.2:8966/profile-test.toml -o /tmp/p.toml && "
                      "{ SAMURAI_TEST_AUTOKEY=1 SAMURAI_SERIAL_CONSOLE=1 samurai-install --profile /tmp/p.toml "
                      "--disk /dev/vda --yes >/dev/null 2>&1; rc=$?; echo INSTALL-RC=$rc; "
                      "[ $rc -eq 0 ] || tail -n 60 /var/log/samurai/install.log; }",
                  timeout=INSTALL_TIMEOUT)
        assert "INSTALL-RC=0" in out
        ga.close()
        vm.eject_iso()
        vm.reboot()
        vm.wait_for("login:", timeout=300)
        ga = vm.qga(timeout=120)
        # an offline install has no sync databases: a full update first, never a bare -Sy
        run(ga, "pacman -Syu --noconfirm >/tmp/syu.log || { tail -20 /tmp/syu.log; exit 1; }",
            timeout=2400)
        for p in pkgs:
            run(ga, f"curl -fsSL http://10.0.2.2:8966/{p.name} -o /tmp/{p.name}")
        run(ga, f"pacman -U --noconfirm {OVERWRITE} "
                + " ".join(f"/tmp/{p.name}" for p in pkgs) + " | tail -15", timeout=900)
        assert run(ga, "systemctl is-enabled samurai-hw-fingerprint.service").strip() == "enabled"
        run(ga, "sync")
        ga.close()
        disk = vm.disk_path
    finally:
        vm.stop()
    return disk


def test_detect_apply_and_health_on_the_qemu_profile(installed):
    vm, ga = boot(installed, "hw-apply", [])
    try:
        # the boot service replaced the older installer's listing with a baseline
        out = run(ga, "systemctl show -p Result,ExecMainStatus samurai-hw-fingerprint.service; "
                      "head -3 /var/lib/samurai/hw-fingerprint; ls /var/lib/samurai/")
        assert "Result=success" in out and "# samurai hw fingerprint v1" in out
        assert "hw-changed.json" not in out
        run(ga, "samurai hw detect")
        plan = detect(ga)
        assert [s["profile"] for s in plan["steps"]] == ["qemu"], plan
        assert plan["steps"][0]["status"] != "ok" and plan["allowed"], plan
        assert "qemu" in run(ga, "samurai hw list")
        # detection needs no administrator rights (the toolbox page runs it as the user)
        out = run(ga, "runuser -u samurai -- samurai hw detect --json")
        assert json.loads(out)["steps"][0]["profile"] == "qemu"
        run(ga, "runuser -u samurai -- samurai hw apply --yes", ok=(1,))
        assert drivers(ga)["drivers.nvidia"] == "skipped"
        # preview only: no terminal and no --yes changes nothing
        out = run(ga, "samurai hw apply </dev/null", ok=(1,))
        assert "Preview only" in out
        run(ga, "! pacman -Qq spice-vdagent")
        out = run(ga, "samurai hw apply --yes", timeout=1200)
        assert "snapshot" in out and "applied qemu" in out, out
        out = run(ga, "snapper -c root list; pacman -Q spice-vdagent vulkan-virtio; "
                      "samurai hw list --installed; grep -c -- '-Sy ' /var/lib/chwd/scripts/chwd || true")
        assert "samurai hw" in out and "spice-vdagent" in out and "qemu" in out
        assert out.strip().endswith("0"), "chwd's helper must not sync the databases"
        plan = detect(ga)
        assert plan["steps"][0]["status"] == "ok" and not plan["changes"], plan
        assert drivers(ga)["drivers.profile"] == "ok"
        # a present device's profile cannot be removed
        out = run(ga, "samurai hw remove qemu --yes", ok=(1,))
        assert "present device" in out and "nothing was changed" in out
        # breaking it is detected and health --fix repairs it
        run(ga, "pacman -Rdd --noconfirm vulkan-virtio >/dev/null")
        assert drivers(ga)["drivers.profile"] == "ok", "optional packages are not an issue"
        assert detect(ga)["steps"][0]["status"] == "extras"
        run(ga, "pacman -Rdd --noconfirm mesa >/dev/null")
        assert drivers(ga)["drivers.profile"] in ("ok", "issue")
        run(ga, "samurai hw apply --yes", timeout=1200)
        run(ga, "pacman -Q mesa vulkan-virtio")
        run(ga, "sync")
        ga.close()
    finally:
        vm.stop()


def test_a_new_graphics_card_raises_a_notice_until_accepted(installed):
    vm, ga = boot(installed, "hw-added", SECOND_GPU)
    try:
        out = run(ga, "cat /var/lib/samurai/hw-changed.json; stat -c %a /var/lib/samurai/hw-changed.json")
        assert "New hardware:" in out and "Virtio" in out and out.strip().endswith("644")
        assert "New hardware:" in run(ga, "samurai hw detect")
        assert detect(ga)["hardware_changed"]
        # the user-session notifier says it without root
        out = run(ga, "runuser -u samurai -- env HOME=/home/samurai samurai-notifier", timeout=900,
                  ok=(0, 1))
        assert "New hardware:" in out
        # an unprivileged user cannot accept it
        run(ga, "runuser -u samurai -- samurai hw fingerprint --accept", ok=(1,))
        run(ga, "samurai hw fingerprint --accept && test ! -e /var/lib/samurai/hw-changed.json")
        assert not detect(ga)["hardware_changed"]
        run(ga, "sync")
        ga.close()
    finally:
        vm.stop()
    vm, ga = boot(installed, "hw-removed", [])
    try:
        out = run(ga, "cat /var/lib/samurai/hw-changed.json")
        assert "Hardware removed:" in out and "Virtio" in out
        # a successful apply is an accepted state
        run(ga, "samurai hw apply --yes", timeout=1200)
        run(ga, "test ! -e /var/lib/samurai/hw-changed.json && samurai hw fingerprint")
        ga.close()
    finally:
        vm.stop()

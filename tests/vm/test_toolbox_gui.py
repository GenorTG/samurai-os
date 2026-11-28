"""Samurai Toolbox GUI on a real Plasma (Wayland) install, driven by screen, mouse and keyboard.

Installs the default KDE profile, adds the locally built samurai-tools and samurai-toolbox-gui,
then does what a person would: opens every page, repairs a health finding, cancels and answers the
polkit password dialog, creates and deletes a snapshot, updates, installs an app, cleans, writes a
support bundle, installs a kernel while the toolbox is killed, and tries to get around the policy.
Every effect is checked through the guest agent; screenshots land in
~/.openclaw/workspace/samurai-shots/toolbox/.

  SAMURAI_TEST_TOOLS_PKG=/abs/samurai-tools-<ver>-x86_64.pkg.tar.zst \\
  SAMURAI_TEST_GUI_PKG=/abs/samurai-toolbox-gui-<ver>-x86_64.pkg.tar.zst \\
  SAMURAI_TEST_HW_PKGS=/abs/samurai-chwd-...:/abs/samurai-hardware-... \\
  SAMURAI_TEST_ISO=/abs/offline.iso pytest -q -s test_toolbox_gui.py

SAMURAI_TEST_GUI_BASE=/abs/disk.qcow2 reuses a disk made by `install_base` (an overlay is booted;
the disk itself is not changed). The tests share one machine and run in order. Needs tesseract,
ffmpeg and the network (update, app and kernel downloads).
"""
from __future__ import annotations

import os
import pathlib
import subprocess
import time

import pytest

from conftest import find_iso
from toolbox_gui import (USER, USER_ENV, Desktop, install_base, install_packages, run,
                         wait_for_plasma, write_profile)
from vm import VM, WORK

PAGES = {   # page id -> text that shows its content arrived
    "overview": "This system", "health": "Repairs", "apps": "Bundles", "cleanup": "Package cache",
    "kernels": "Boot settings", "updates": "Arch mirrors", "snapshots": "New snapshot",
    "diagnostics": "Support bundle", "hardware": "Driver profiles",
}


def _package(var: str) -> pathlib.Path:
    path = os.environ.get(var, "")
    if not path or not pathlib.Path(path).is_file():
        pytest.skip(f"set {var} to a built package")
    return pathlib.Path(path)


@pytest.fixture(scope="module")
def desktop(mirror):
    packages = [_package("SAMURAI_TEST_TOOLS_PKG"), _package("SAMURAI_TEST_GUI_PKG")]
    # the Hardware page needs chwd and our profiles (an ISO of this branch already has them)
    packages += [pathlib.Path(p) for p in os.environ.get("SAMURAI_TEST_HW_PKGS", "").split(":") if p]
    base = os.environ.get("SAMURAI_TEST_GUI_BASE", "")
    if not base:
        write_profile(mirror.dir)
        installer = VM("gui-test-install", iso=str(find_iso()), disk_size="24G", ram="6144", smp="4")
        install_base(installer)
        base = installer.disk_path
    WORK.mkdir(parents=True, exist_ok=True)
    overlay = WORK / "gui-test-overlay.qcow2"
    overlay.unlink(missing_ok=True)
    subprocess.run(["qemu-img", "create", "-q", "-f", "qcow2", "-b", str(pathlib.Path(base).resolve()),
                    "-F", "qcow2", str(overlay)], check=True)
    vm = VM("gui-test", disk=str(overlay), ram="6144", smp="4", pointer=True)
    vm.start()
    try:
        vm.serial_connect()
        vm.wait_for("login:", timeout=300)
        ga = vm.qga(timeout=900)
        wait_for_plasma(ga)
        print(install_packages(ga, mirror.dir, packages)[-800:])
        yield Desktop(vm, ga)
        ga.close()
    finally:
        vm.stop()
        overlay.unlink(missing_ok=True)


def helper_log(d: Desktop) -> str:
    return run(d.ga, "journalctl -q -t samurai-gui-helper -o cat", check=False)


def finished(d: Desktop, verb: str, timeout: float = 300) -> None:
    d.wait_guest(f"journalctl -q -t samurai-gui-helper -o cat | grep -qx 'samurai {verb} finished with exit code 0'",
                 timeout)


def snapshots(d: Desktop) -> str:
    return run(d.ga, "snapper -c root list --columns number,description")


def test_package_installs_policy_helper_and_launcher(desktop):
    out = run(desktop.ga, "pacman -Qlq samurai-toolbox-gui; pkaction | grep -c '^org.samurai.toolbox.'; "
                          "stat -c '%U %a' /usr/lib/samurai/gui-helper; "
                          "grep -E '^(Name|Exec)=' /usr/share/applications/samurai-toolbox.desktop")
    for must in ("/usr/bin/samurai-toolbox", "/usr/lib/samurai/gui-helper", "\n30\n", "root 755",
                 "Name=Samurai Toolbox", "Exec=samurai-toolbox"):
        assert must in out, out


def test_fresh_offline_install_says_why_apps_are_unavailable(desktop):
    if "none" not in run(desktop.ga, "ls /var/lib/pacman/sync/*.db >/dev/null 2>&1 && echo some || echo none"):
        pytest.skip("this install has package databases (online ISO)")
    desktop.open_toolbox("apps")
    desktop.wait_text("package databases", 90)
    desktop.shot("apps-no-databases")


def test_every_page_opens_without_qml_errors(desktop):
    for page, text in PAGES.items():
        desktop.open_toolbox(page)
        desktop.wait_text(text, 90)
        time.sleep(3)
        desktop.shot(f"page-{page}")
        log = desktop.toolbox_log()
        bad = [l for l in log.splitlines() if ("qrc:" in l or "TypeError" in l or "ReferenceError" in l)
               and "Created graphical object was not placed" not in l]
        assert not bad, f"{page}: {bad}"
    # reads that need root were answered without a password prompt (active local session)
    assert "samurai health --json" in helper_log(desktop)


def test_health_repair_asks_for_the_password_and_snapshots_first(desktop):
    run(desktop.ga, "systemctl disable --now firewalld")
    before = snapshots(desktop)
    desktop.open_toolbox("health")
    desktop.click("Repair", 0)
    desktop.shot("health-confirm")
    desktop.confirm("Repair")
    desktop.authenticate("health-password")
    finished(desktop, "health-fix")
    assert run(desktop.ga, "systemctl is-active firewalld; systemctl is-enabled firewalld").split() \
        == ["active", "enabled"]
    after = snapshots(desktop)
    assert "samurai health --fix" in after and after != before
    assert "uid 1000 runs: samurai health --fix --yes --id security.firewall" in helper_log(desktop)
    desktop.wait_text("Finished")
    desktop.shot("health-repaired")


def test_cancelled_password_changes_nothing_then_snapshot_create_and_delete(desktop):
    desktop.open_toolbox("snapshots")
    desktop.click("Description")
    desktop.vm.type_text("made in the toolbox")
    before = snapshots(desktop)
    desktop.vm.key("ret")
    desktop.wait_text("Authentication is required")
    desktop.vm.key("esc")
    desktop.wait_text("nothing was changed")
    desktop.shot("auth-cancelled")
    assert snapshots(desktop) == before

    desktop.click("made in the toolbox", 0)   # the field, above the log console
    desktop.vm.key("ret")
    desktop.authenticate()
    finished(desktop, "snap-create")
    assert "made in the toolbox" in snapshots(desktop)
    desktop.wait_text("made in the toolbox")
    desktop.shot("snapshot-created")
    desktop.click("Delete", 0)           # newest first
    desktop.shot("snapshot-delete-confirm")
    desktop.confirm("Delete")
    desktop.authenticate()
    finished(desktop, "snap-delete")
    assert "made in the toolbox" not in snapshots(desktop)


def test_update_streams_and_the_window_stays_usable(desktop):
    desktop.open_toolbox("updates")
    desktop.click("Update system")
    desktop.confirm("Update")
    desktop.authenticate()
    desktop.wait_text("System update", 60)
    desktop.page("Overview", PAGES["overview"])   # navigate while the update runs
    finished(desktop, "update", 2400)
    desktop.wait_text("Finished", 60)
    desktop.shot("update-finished")
    # what the update changed, for the test log (a removal on a fresh install is worth reading)
    print(run(desktop.ga, "awk '/starting full system upgrade/ {n = NR} {line[NR] = $0} "
                          "END {for (i = n; i <= NR; i++) if (line[i] ~ /(installed|removed|upgraded|replac)/) "
                          "print line[i]}' /var/log/pacman.log", check=False))
    out = run(desktop.ga, "cat /var/lib/samurai/update-report.json")
    assert '"ok": true' in out, out


def test_search_install_and_remove_an_app(desktop):
    desktop.open_toolbox("apps")
    desktop.wait_text("Bundles", 90)
    desktop.vm.key("ctrl", "f")
    desktop.vm.type_text("htop")
    desktop.wait_text("process viewer", 90)
    desktop.click("Install", 0)
    desktop.wait_text("pacman packages", 90)
    desktop.shot("app-install-plan")
    desktop.confirm("Install")
    desktop.authenticate()
    finished(desktop, "apps-install", 600)
    assert "htop " in run(desktop.ga, "pacman -Q htop")
    desktop.click("Remove", 0, timeout=90)
    desktop.wait_text("Cancel", 90)
    desktop.confirm("Remove")
    desktop.authenticate()
    finished(desktop, "apps-remove", 600)
    run(desktop.ga, "! pacman -Q htop 2>/dev/null")


def test_cleanup_previews_then_deletes_as_user_and_as_root(desktop):
    run(desktop.ga, "head -c 200000 /dev/urandom > /var/lib/systemd/coredump/core.vmtest.zst && "
                    f"runuser -u {USER} -- sh -c 'mkdir -p ~/.cache/thumbnails/large && "
                    "head -c 300000 /dev/urandom > ~/.cache/thumbnails/large/t.png'")
    desktop.open_toolbox("cleanup")
    desktop.wait_text("Crash dumps", 90)
    desktop.shot("cleanup-preview")
    desktop.click("Clean selected")
    desktop.wait_text("cannot be undone")
    desktop.shot("cleanup-confirm")
    # nothing is deleted before the confirmation
    run(desktop.ga, "test -s /var/lib/systemd/coredump/core.vmtest.zst")
    desktop.confirm("Delete")
    desktop.authenticate()
    finished(desktop, "clean")
    run(desktop.ga, "test ! -e /var/lib/systemd/coredump/core.vmtest.zst && "
                    f"test ! -e /home/{USER}/.cache/thumbnails/large/t.png")
    desktop.wait_text("Finished")
    desktop.shot("cleanup-done")


def test_support_bundle_is_redacted_and_owned_by_the_user(desktop):
    desktop.open_toolbox("diagnostics")
    desktop.click("Create bundle")
    desktop.confirm("Create")
    desktop.authenticate()
    finished(desktop, "diag-bundle")
    desktop.wait_text("Saved")
    desktop.shot("bundle-saved")
    out = run(desktop.ga, f"f=$(ls /home/{USER}/samurai-diag-*.txt) && stat -c '%U %a' $f && "
                          "grep -c '== boot config /boot/limine/limine.conf ==' $f && "
                          "echo machine-id-lines=$(grep -ci -- $(cat /etc/machine-id) $f); "
                          "ls /root/samurai-diag-* 2>/dev/null | wc -l")
    assert out.split() == [USER, "600", "1", "machine-id-lines=0", "0"], out


def test_kernel_install_survives_a_killed_toolbox(desktop):
    desktop.open_toolbox("kernels")
    row = desktop.wait_text("Arch long-term support kernel", 90)[0]
    button = min(desktop.wait_text("Install"), key=lambda p: abs(p[1] - row[1] + 10))
    desktop.click_at(*button)
    desktop.confirm("Install")
    desktop.authenticate()
    desktop.wait_guest("pgrep -x pacman >/dev/null", 90)
    run(desktop.ga, "pkill -9 -x samurai-toolbox")
    finished(desktop, "kernel-install", 900)
    out = run(desktop.ga, "pacman -Q linux-lts && grep -c '^/Samurai OS (linux-lts)' /boot/limine/limine.conf && "
                          "test ! -e /var/lib/pacman/db.lck && echo NO-LOCK")
    assert "NO-LOCK" in out, out


def test_policy_cannot_be_walked_around(desktop):
    helper = "pkexec --disable-internal-agent /usr/lib/samurai/gui-helper"
    # no active local session (as over ssh): even a read is refused
    out = run(desktop.ga, f"runuser -u {USER} -- {helper} health-read 2>&1; echo rc=$?", check=False)
    assert "rc=127" in out and '"schema"' not in out, out
    session = f"{USER_ENV} systemd-run --user --wait --pipe --collect -q {helper}"
    out = run(desktop.ga, f"{session} boot-get --json 2>&1; echo rc=$?", check=False)
    assert "wrong number of arguments" in out and "rc=64" in out, out
    # an unknown operation is not covered by any of our actions: generic administrator prompt
    run(desktop.ga, f"{USER_ENV} systemd-run --user --collect --unit=bypass {helper} shell")
    desktop.wait_text("super user", 60)
    desktop.shot("unknown-verb-prompt")
    desktop.vm.key("esc")
    # a parameter that would give a root shell at boot is refused before any prompt
    desktop.open_toolbox("kernels")
    desktop.wait_text("Boot settings", 90)
    desktop.click("New parameter")
    desktop.vm.type_text("init=/bin/sh")
    desktop.vm.key("ret")
    desktop.wait_text("Not possible from the toolbox")
    desktop.shot("parameter-refused")
    assert "init=/bin/sh" not in run(desktop.ga, "cat /boot/limine/limine.conf")
    desktop.close_toolbox()

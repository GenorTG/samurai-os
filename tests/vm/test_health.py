"""M5 `samurai health` on a real install (spec 05 + spec 11 `health-*`).

Installs once, upgrades samurai-tools to the locally built package (the repo does not carry it yet),
then: baseline run, break the system on purpose, detect, repair, detect again.

  SAMURAI_TEST_TOOLS_PKG=/abs/samurai-tools-<ver>-x86_64.pkg.tar.zst pytest -q -s test_health.py
"""
from __future__ import annotations

import json
import os
import pathlib
import re
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
    for must in ("services.failed", "security.firewall", "pacman.conf", "packages.core",
                 "boot.initramfs", "boot.kernels-present", "config.shell", "storage.btrfs-errors",
                 "snapshots.services", "storage.btrfs-timers"):
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
    # the notifier (user session, no root) picks the issue up from health.json
    _, out = ga.exec("su samurai -c 'samurai-notifier --check --json' 2>/dev/null", timeout=300, check=False)
    notif = json.loads(out[out.index("{"):out.rindex("}") + 1])
    assert notif["health"] >= 1, f"notifier ignored health.json: {notif}"
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


def test_health_fix_rebuilds_missing_initramfs_and_enables_btrfs_timers(installed_vm):
    _, ga = installed_vm
    # a missing initramfs is critical: the kernel would not boot
    _, out = ga.exec("test -s /boot/initramfs-linux.img && mv /boot/initramfs-linux.img /root/initramfs-linux.img.bak && echo MOVED", check=False)
    assert "MOVED" in out, out
    _, rep = _health(ga, "--id", "boot.initramfs")
    r = rep["results"][0]
    assert r["status"] == "issue" and r["severity"] == "critical", r
    assert "initramfs-linux.img" in r["evidence"][0], r
    _, out = ga.exec("samurai health --fix --id boot.initramfs; echo rc=$?", timeout=600, check=False)
    assert "fixed    boot.initramfs" in out and out.strip().endswith("rc=0"), out
    _, out = ga.exec("test -s /boot/initramfs-linux.img && echo REBUILT", check=False)
    assert "REBUILT" in out, out
    _, rep = _health(ga, "--id", "boot.initramfs")
    assert rep["results"][0]["status"] == "ok", rep
    ga.exec("rm -f /root/initramfs-linux.img.bak", check=False)

    # spec 06: scrub + fstrim timers on. Break one on purpose, fix, verify.
    ga.exec("systemctl disable --now btrfs-scrub@-.timer", check=False)
    _, rep = _health(ga, "--id", "storage.btrfs-timers")
    r = rep["results"][0]
    assert r["status"] == "issue", r
    assert any("btrfs-scrub@-.timer" in e for e in r["evidence"]), r
    _, out = ga.exec("samurai health --fix --id storage.btrfs-timers; echo rc=$?", timeout=300, check=False)
    assert "fixed    storage.btrfs-timers" in out and out.strip().endswith("rc=0"), out
    _, out = ga.exec("systemctl is-enabled fstrim.timer btrfs-scrub@-.timer", check=False)
    assert out.split() == ["enabled", "enabled"], out


def test_diag_commands_and_redacted_private_bundle(installed_vm):
    _, ga = installed_vm
    # plain commands answer and carry real content
    _, out = ga.exec("samurai diag info; echo rc=$?", timeout=120, check=False)
    assert "== os-release ==" in out and "Samurai OS" in out and out.strip().endswith("rc=0"), out[-600:]
    assert "firmware: UEFI" in out, out[-900:]
    _, out = ga.exec("samurai diag services; echo rc=$?", timeout=120, check=False)
    assert "== failed units ==" in out and out.strip().endswith("rc=0"), out[-600:]
    _, out = ga.exec("samurai diag disks; echo rc=$?", timeout=120, check=False)
    assert "== btrfs usage (/) ==" in out and out.strip().endswith("rc=0"), out[-600:]
    _, out = ga.exec("samurai diag logs --pacman --lines 20; echo rc=$?", timeout=60, check=False)
    assert "== pacman log (tail) ==" in out and out.strip().endswith("rc=0"), out[-600:]
    _, out = ga.exec("samurai diag logs --lines 0 >/dev/null 2>&1; echo rc=$?", check=False)
    assert out.strip().endswith("rc=2"), out

    # plant private data where the bundle will find it: a user, a journal error with an address,
    # a MAC, an e-mail address and a password
    ga.exec("id genortest >/dev/null 2>&1 || useradd -m genortest", check=False)
    ga.exec(
        "logger -p user.err 'bundle-test user genortest from 10.9.8.7 mac AA:BB:CC:DD:EE:FF "
        "mail alice@example.org password=hunter2 end'; sleep 2", check=False)
    # the minimal guest has no `ip`: read addresses from /proc and /sys, and keep only real-looking values
    _, facts = ga.exec(
        "cat /etc/hostname; echo ---; cat /sys/class/net/*/address; echo ---; "
        "awk '/32 host/ { print f } { f = $2 }' /proc/net/fib_trie | sort -u", check=False)
    host_raw, macs_raw, ips_raw = (p.split() for p in facts.split("---"))
    mac_re = re.compile(r"^[0-9a-f]{2}(:[0-9a-f]{2}){5}$", re.I)
    ip_re = re.compile(r"^\d{1,3}(\.\d{1,3}){3}$")
    host = host_raw
    macs = [m for m in macs_raw if mac_re.match(m) and m != "00:00:00:00:00:00"]
    ips = [i for i in ips_raw if ip_re.match(i) and not i.startswith("127.")]
    assert host and macs and ips, f"could not learn the guest's identity: {facts!r}"

    _, out = ga.exec("rm -f /root/bundle.txt; samurai diag bundle --out /root/bundle.txt; echo rc=$?", timeout=300, check=False)
    assert "(redacted)" in out and out.strip().endswith("rc=0"), out
    _, mode = ga.exec("stat -c %a /root/bundle.txt", check=False)
    assert mode.strip() == "600", mode
    _, text = ga.exec("cat /root/bundle.txt", timeout=120, check=False)
    for must in ("== kernel ==", "== boot ==", "== samurai health (json) ==", "== pacman.conf ==", "[samurai]", "Samurai OS"):
        assert must in text, f"bundle lacks {must!r}"
    assert "bundle-test user <user" in text, "the planted journal line should survive, redacted"
    low = text.lower()
    leaks = [x for x in ["genortest", "10.9.8.7", "aa:bb:cc:dd:ee:ff", "alice@example.org", "hunter2", *host, *ips,
                         *[m.lower() for m in macs]] if x.lower() in low]
    assert not leaks, f"private data leaked into the bundle: {leaks}"

    # overwriting is refused; unredacted needs a terminal (the harness has none)
    _, out = ga.exec("samurai diag bundle --out /root/bundle.txt >/dev/null 2>&1; echo rc=$?", check=False)
    assert out.strip().endswith("rc=1"), out
    _, out = ga.exec("samurai diag bundle --no-redact --out /root/raw.txt 2>&1; echo rc=$?; test -e /root/raw.txt && echo EXISTS", check=False)
    assert "needs a terminal" in out and "rc=2" in out and "EXISTS" not in out, out
    ga.exec("rm -f /root/bundle.txt; userdel -r genortest", check=False)


def _clean_preview(ga, *args: str) -> dict:
    _, out = ga.exec(f"samurai clean --preview --json {' '.join(args)}", timeout=120, check=False)
    return {t["id"]: t for t in json.loads(out)["targets"]}


def test_clean_previews_refuses_non_root_and_frees_space(installed_vm):
    _, ga = installed_vm
    # preview: read-only, every default target listed, orphans/cache not part of `all`
    prev = _clean_preview(ga)
    assert list(prev) == ["pkgcache", "journal", "coredumps", "thumbnails"], list(prev)
    _, out = ga.exec("samurai clean --preview; echo rc=$?", timeout=120, check=False)
    assert "total" in out and out.strip().endswith("rc=0"), out

    # a normal user may preview, but system targets are refused (and report exit 1)
    _, out = ga.exec("su samurai -c 'samurai clean --preview' >/dev/null 2>&1; echo rc=$?", timeout=120, check=False)
    assert out.strip().endswith("rc=0"), out
    _, out = ga.exec("su samurai -c 'samurai clean pkgcache' 2>&1; echo rc=$?", check=False)
    assert "needs root" in out and out.strip().endswith("rc=1"), out

    # coredumps: plant 5 MiB, see it in the preview, clean it, see it gone
    ga.exec("mkdir -p /var/lib/systemd/coredump && head -c 5242880 /dev/urandom > /var/lib/systemd/coredump/core.vmtest.zst", check=False)
    assert _clean_preview(ga, "coredumps")["coredumps"]["bytes"] >= 5 * 1024 * 1024
    _, out = ga.exec("samurai clean coredumps; echo rc=$?; ls /var/lib/systemd/coredump | wc -l", timeout=120, check=False)
    assert "cleaned  coredumps" in out and "rc=0" in out and out.strip().endswith("0"), out

    # thumbnails of the invoking user, run as that user
    ga.exec("su samurai -c 'mkdir -p ~/.cache/thumbnails/large && head -c 200000 /dev/urandom > ~/.cache/thumbnails/large/t.png'", check=False)
    _, out = ga.exec("su samurai -c 'samurai clean thumbnails; echo rc=$?; ls -A ~/.cache/thumbnails | wc -l'", timeout=120, check=False)
    assert "cleaned  thumbnails" in out and "rc=0" in out and out.strip().endswith("0"), out

    # journal: a fresh VM is under the cap, so this is a clean no-op that must still succeed
    _, out = ga.exec("samurai clean journal; echo rc=$?", timeout=120, check=False)
    assert "cleaned  journal" in out and out.strip().endswith("rc=0"), out

    # package cache: trim everything, then nothing for installed packages remains
    _, before = ga.exec("ls /var/cache/pacman/pkg/*.pkg.tar.zst 2>/dev/null | wc -l", check=False)
    _, out = ga.exec("samurai clean pkgcache --keep 0; echo rc=$?", timeout=300, check=False)
    assert "cleaned  pkgcache" in out and out.strip().endswith("rc=0"), out
    _, after = ga.exec("ls /var/cache/pacman/pkg/*.pkg.tar.zst 2>/dev/null | wc -l", check=False)
    assert int(after.strip()) == 0, f"cache not emptied: before={before.strip()} after={after.strip()}"

    # orphans: never automatic. Make a real one, check it is listed, declined without --yes, removed with it
    _, out = ga.exec("pacman -S --asdeps --noconfirm tree >/tmp/tree.log 2>&1; echo rc=$?; pacman -Qdtq | grep -x tree", timeout=300, check=False)
    if "rc=0" not in out:
        pytest.skip(f"could not install a test orphan (repo sync db unavailable?): {out[-300:]}")
    assert "tree" in _clean_preview(ga, "orphans")["orphans"]["items"]
    _, out = ga.exec("samurai clean orphans; echo rc=$?; pacman -Q tree", check=False)
    assert "declined orphans" in out and "rc=1" in out and "tree " in out, f"orphans must need confirmation: {out}"
    _, out = ga.exec("samurai clean orphans --yes; echo rc=$?; pacman -Q tree 2>&1", timeout=300, check=False)
    assert "cleaned  orphans" in out and "rc=0" in out and "was not found" in out, out
    # `all` never touches orphans
    _, out = ga.exec("pacman -S --asdeps --noconfirm tree >/dev/null 2>&1; samurai clean all >/dev/null 2>&1; pacman -Q tree; pacman -Rns --noconfirm tree >/dev/null 2>&1", timeout=300, check=False)
    assert "tree " in out, f"`clean all` removed an orphan: {out}"


def test_fix_lock_keyring_dkms_and_age_checks(installed_vm):
    _, ga = installed_vm
    # every v1 check answers on a real system (ok / issue / skipped, never missing)
    _, rep = _health(ga)
    st = _status(rep)
    assert len(st) >= 24, sorted(st)
    for cid in ("pacman.mirrors", "packages.outdated", "packages.partial-upgrade", "security.ssh", "boot.esp-space"):
        assert st[cid] in ("ok", "issue", "skipped"), (cid, st[cid])

    # age checks: break, detect, repair, re-detect
    ga.exec("touch -d '40 days ago' /etc/pacman.d/mirrorlist", check=False)
    _, rep = _health(ga, "--id", "pacman.mirrors")
    assert rep["results"][0]["status"] == "issue", rep
    ga.exec("touch /etc/pacman.d/mirrorlist", check=False)
    _, rep = _health(ga, "--id", "pacman.mirrors")
    assert rep["results"][0]["status"] == "ok", rep
    ga.exec("mkdir -p /var/lib/samurai && touch -d '20 days ago' /var/lib/samurai/last-update", check=False)
    _, rep = _health(ga, "--id", "packages.outdated")
    assert rep["results"][0]["status"] == "issue" and "20 days" in rep["results"][0]["summary"], rep
    ga.exec("touch /var/lib/samurai/last-update", check=False)
    _, rep = _health(ga, "--id", "packages.outdated")
    assert rep["results"][0]["status"] == "ok", rep

    # fix lock: nothing to do, then refuse while a package manager runs, then remove a stale lock
    _, out = ga.exec("rm -f /var/lib/pacman/db.lck; samurai fix lock; echo rc=$?", check=False)
    assert "nothing to do" in out and out.strip().endswith("rc=0"), out
    ga.exec("touch /var/lib/pacman/db.lck; cp /usr/bin/sleep /tmp/pacman; (/tmp/pacman 300 >/dev/null 2>&1 &) ; sleep 1", check=False)
    _, out = ga.exec("samurai fix lock; echo rc=$?; test -e /var/lib/pacman/db.lck && echo LOCK-STILL-THERE", check=False)
    assert "a package manager is running" in out and "rc=1" in out and "LOCK-STILL-THERE" in out, out
    ga.exec("pkill -x pacman; rm -f /tmp/pacman; sleep 1", check=False)
    _, out = ga.exec("samurai fix lock; echo rc=$?; test -e /var/lib/pacman/db.lck && echo LOCK-STILL-THERE", check=False)
    assert "removed stale" in out and "rc=0" in out and "LOCK-STILL-THERE" not in out, out

    # fix keyring: populate is harmless; reset needs --yes (no terminal here) and takes a snapshot first
    _, out = ga.exec("samurai fix keyring; echo rc=$?", timeout=300, check=False)
    assert "fixed    keyring" in out and out.strip().endswith("rc=0"), out
    ga.exec("touch /etc/pacman.d/gnupg/reset-marker", check=False)
    _, out = ga.exec("samurai fix keyring --reset; echo rc=$?; test -e /etc/pacman.d/gnupg/reset-marker && echo MARKER-KEPT", timeout=120, check=False)
    assert "declined keyring" in out and "rc=1" in out and "MARKER-KEPT" in out, f"reset must need confirmation: {out}"
    _, before = ga.exec("snapper -c root list --columns number | wc -l", check=False)
    _, out = ga.exec("samurai fix keyring --reset --yes; echo rc=$?; test -e /etc/pacman.d/gnupg/reset-marker && echo MARKER-KEPT", timeout=600, check=False)
    assert "snapshot: created" in out and "fixed    keyring" in out and "rc=0" in out and "MARKER-KEPT" not in out, out
    _, after = ga.exec("snapper -c root list --columns number | wc -l", check=False)
    assert int(after.strip()) == int(before.strip()) + 1, (before, after)
    # the rebuilt keyring really verifies signatures: install a package through it
    _, out = ga.exec("pacman -S --noconfirm --needed tree >/tmp/tree2.log 2>&1; echo rc=$?; tail -2 /tmp/tree2.log", timeout=300, check=False)
    assert "rc=0" in out, f"signature-checked install failed after the keyring reset: {out}"
    ga.exec("pacman -Rns --noconfirm tree >/dev/null 2>&1", check=False)

    # fix dkms: no dkms in the test profile, so this is a clean no-op
    _, out = ga.exec("samurai fix dkms; echo rc=$?", timeout=300, check=False)
    assert "fixed    dkms" in out and "rc=0" in out, out
    # not root: refused
    _, out = ga.exec("su samurai -c 'samurai fix lock' 2>&1; echo rc=$?", check=False)
    assert "needs root" in out and out.strip().endswith("rc=1"), out


def test_sudo_hardening_path_confinement_and_unprivileged_user_cleanup(installed_vm):
    _, ga = installed_vm
    sudo = "SUDO_USER=samurai"   # what sudo sets; the harness shell is already root
    # 1. a hostile $PATH cannot substitute the tools root runs
    ga.exec("mkdir -p /tmp/evil && printf '#!/bin/sh\\ntouch /tmp/evil/RAN\\necho inactive\\n' > /tmp/evil/systemctl && chmod 755 /tmp/evil/systemctl", check=False)
    _, out = ga.exec("PATH=/tmp/evil:$PATH /usr/local/bin/samurai health --id security.firewall; echo rc=$?; test -e /tmp/evil/RAN && echo HIJACKED", check=False)
    assert "HIJACKED" not in out and "ok       security.firewall" in out, out

    # 2. under sudo, --out may not leave the invoking user's home (plain, `..`, and through a symlink)
    ga.exec("su samurai -c 'ln -sfn /etc ~/escape'", check=False)
    for target in ("/etc/samurai-planted.txt", "/home/samurai/../../etc/samurai-planted.txt", "/home/samurai/escape/samurai-planted.txt"):
        _, out = ga.exec(f"{sudo} samurai diag bundle --out {target} 2>&1; echo rc=$?; test -e /etc/samurai-planted.txt && echo PLANTED", timeout=300, check=False)
        assert "rc=2" in out and "inside your home" in out and "PLANTED" not in out, (target, out)
    _, out = ga.exec(f"rm -f /home/samurai/b.txt; {sudo} samurai diag bundle --out /home/samurai/b.txt; echo rc=$?; stat -c '%U %a' /home/samurai/b.txt", timeout=300, check=False)
    assert "rc=0" in out and "samurai 600" in out, out

    # 3. user targets under sudo run as that user: a symlinked cache cannot make root delete for them
    ga.exec("mkdir -p /root/victim/thumbnails && echo keep > /root/victim/thumbnails/keep && chmod 700 /root/victim", check=False)
    ga.exec("su samurai -c 'rm -rf ~/.cache && ln -s /root/victim ~/.cache'", check=False)
    _, out = ga.exec(f"{sudo} samurai clean thumbnails; echo rc=$?; cat /root/victim/thumbnails/keep", timeout=120, check=False)
    assert out.strip().endswith("keep"), f"root deleted through a user symlink: {out}"
    ga.exec("su samurai -c 'rm -f ~/.cache ~/escape'; rm -rf /root/victim /tmp/evil /home/samurai/b.txt", check=False)

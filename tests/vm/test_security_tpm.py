"""TPM2 + PIN unlock and its way back (spec 09, 11), with an emulated TPM (swtpm).

An installed system is switched from the test key file to a typed passphrase, enrols the TPM with
`samurai security tpm enroll` on its serial console, unlocks with the PIN, then loses the TPM (a new,
empty one: what a mainboard change or firmware reset looks like) and must still boot with the
passphrase. Removing the enrolment returns to the passphrase prompt.

  SAMURAI_TEST_SEC_PKGS=<dir>    packages built by ci/build-pkgs.sh (samurai-tools)
  SAMURAI_TEST_BASE_DISK=<qcow2> reuse an installed base (tests/vm/base_disk.py); else install one
"""
from __future__ import annotations

import os
import pathlib
import shutil

import pytest

import base_disk
from conftest import TEST_USER_PASSWORD
from vm import VM, WORK

USER = base_disk.USER
PASSPHRASE = "vm-only-passphrase-2026"   # throwaway
PIN = "271828"
DEVICE = "/dev/vda2"


def run(vm: VM, command: str, timeout: float = 240) -> str:
    ga = vm.qga(timeout=timeout)
    try:
        _, out = ga.exec(f"{{ {command} ; }} 2>&1", timeout=timeout, check=False)
    finally:
        ga.close()
    print(f"--- {command[:100]}\n{out[-3000:]}", flush=True)
    return out


def unlock(vm: VM, prompts: dict[str, str], timeout: float = 300) -> list[str]:
    """Answer boot prompts until the login prompt; returns the prompts seen, in order."""
    seen: list[str] = []
    for _ in range(8):
        matched, text = vm.wait_for_any([*prompts, " login:"], timeout=timeout)
        print(f"--- boot console\n{text[-700:]}", flush=True)
        if matched == " login:":
            return seen
        seen.append(matched)
        vm.serial_read(max_wait=1.0)
        vm.send(prompts[matched] + "\n")
    raise AssertionError(f"still prompting after {seen}")


def restart(vm: VM) -> None:
    run(vm, "sync")
    vm.reboot()
    vm.serial_read(max_wait=1.0)


def shell_login(vm: VM) -> None:
    vm.send("\n")
    vm.wait_for("login:", timeout=60)
    vm.send(USER + "\n")
    vm.wait_for("Password:", timeout=60)
    vm.send(TEST_USER_PASSWORD + "\n")
    vm.serial_read(max_wait=4.0)
    vm.send("exec bash --norc\n")
    vm.serial_read(max_wait=3.0)


def typed(vm: VM, command: str, answers: list[tuple[str, str]], timeout: float = 180) -> str:
    """Run a command in the serial shell and answer its prompts the way a person would."""
    vm.serial_read(max_wait=1.0)
    vm.send(command + '; echo TYPED-"RC"=$?\n')
    log = ""
    for prompt, answer in answers:
        log += vm.wait_for(prompt, timeout=timeout)
        vm.serial_read(max_wait=1.0)
        vm.send(answer + "\n")
    log += vm.wait_for("TYPED-RC=", timeout=timeout)
    log += vm.serial_read(max_wait=2.0)
    print(f"--- typed: {command}\n{log[-3000:]}", flush=True)
    return log


def test_tpm_pin_unlock_and_recovery(mirror):
    pkg_dir = os.environ.get("SAMURAI_TEST_SEC_PKGS")
    if not pkg_dir:
        pytest.skip("SAMURAI_TEST_SEC_PKGS not set")
    tools = sorted(pathlib.Path(pkg_dir).glob("samurai-tools-*.pkg.tar.zst"))[-1]
    shutil.copy(tools, mirror.dir / tools.name)
    given = os.environ.get("SAMURAI_TEST_BASE_DISK")
    base = pathlib.Path(given) if given else WORK / "security-base.qcow2"
    if not base.is_file():
        base_disk.install(mirror, base, "security-base")
    disk = base_disk.overlay(base, "tpm")
    vm = VM("tpm", disk=str(disk), ram="6144", smp="4", tpm=True)
    shutil.rmtree(vm.tpm_state, ignore_errors=True)
    vm.start()
    try:
        # --- from the test key file to a typed passphrase --------------------------
        vm.serial_connect()
        vm.wait_for("login:", timeout=420)
        out = run(vm, f"curl -fsS -o /tmp/t.pkg.tar.zst http://10.0.2.2:8966/{tools.name} && "
                  "pacman -U --noconfirm /tmp/t.pkg.tar.zst && "
                  f"printf %s {PASSPHRASE} | cryptsetup luksAddKey --key-file /etc/cryptsetup-keys.d/samurai-root.key {DEVICE} - && "
                  "samurai security tpm enroll --dry-run; echo KEYFILE-RC=$?")
        assert "key file" in out and "KEYFILE-RC=1" in out, "a key-file system must be refused"
        out = run(vm, "sed -i -E 's/ rd\\.luks\\.key=[^ ]+//' /boot/limine.conf && "
                  "! grep -q rd.luks.key /boot/limine.conf && echo PREP-OK")
        assert "PREP-OK" in out
        restart(vm)
        seen = unlock(vm, {"passphrase": PASSPHRASE})
        assert seen == ["passphrase"], seen

        # --- enrol on the console ----------------------------------------------------
        out = run(vm, "cat /sys/class/tpm/tpm0/tpm_version_major; samurai security tpm enroll --dry-run")
        assert "dry run: nothing changed" in out and "--tpm2-with-pin=yes" in out, out
        out = run(vm, f"samurai security tpm enroll </dev/null; echo RC=$?; cryptsetup luksDump {DEVICE} | grep -c systemd-tpm2")
        assert "needs a terminal" in out and out.strip().endswith("0"), out
        shell_login(vm)
        log = typed(vm, "sudo samurai security tpm enroll",
                    [("password for", TEST_USER_PASSWORD), ("[y/N]", "y"), ("passphrase", PASSPHRASE),
                     ("PIN", PIN), ("PIN", PIN)])
        assert "TYPED-RC=0" in log and "enrolled" in log, log[-1500:]
        out = run(vm, f"cryptsetup luksDump {DEVICE}; samurai security status | grep disk.")
        assert "systemd-tpm2" in out and "TPM unlock with PIN is enrolled" in out, out
        vm.send("exit\n")

        # --- the PIN unlocks ---------------------------------------------------------
        restart(vm)
        seen = unlock(vm, {"PIN": PIN, "passphrase": PASSPHRASE})
        assert seen == ["PIN"], f"expected one PIN prompt, saw {seen}"
        assert "UNLOCKED" in run(vm, "findmnt -no SOURCE / | grep -q mapper && echo UNLOCKED")

        # --- a different TPM: the passphrase still gets in ---------------------------
        run(vm, "sync")
        vm.stop()
        shutil.rmtree(vm.tpm_state)
        vm.start()
        vm.serial_connect()
        seen = unlock(vm, {"PIN": PIN, "passphrase": PASSPHRASE})
        assert seen and seen[-1] == "passphrase", f"no way back without the TPM: {seen}"
        print("RECOVERY prompts:", seen)

        # --- removal -----------------------------------------------------------------
        shell_login(vm)
        log = typed(vm, "sudo samurai security tpm remove",
                    [("password for", TEST_USER_PASSWORD), ("[y/N]", "y")])
        assert "TYPED-RC=0" in log and "TPM unlock removed" in log, log[-1500:]
        log = typed(vm, "sudo samurai security recovery-key",
                    [("[y/N]", "y"), ("passphrase", PASSPHRASE)])
        assert "TYPED-RC=0" in log and "recovery key added" in log, log[-1500:]
        vm.send("exit\n")
        restart(vm)
        seen = unlock(vm, {"PIN": PIN, "passphrase": PASSPHRASE})
        assert seen == ["passphrase"], seen
    finally:
        vm.stop()
        disk.unlink(missing_ok=True)
        if vm.tpm_state is not None:
            shutil.rmtree(vm.tpm_state, ignore_errors=True)

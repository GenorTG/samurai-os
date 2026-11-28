"""An install made by an older release adopts the current defaults (spec 11).

  SAMURAI_TEST_ISO=<online ISO of the old release> SAMURAI_TEST_PROFILE_REF=v0.0.1.75 \
  SAMURAI_TEST_TOOLS_PKG=<new samurai-tools package> pytest -q -s test_adopt.py

Flow: install KDE from the old ISO -> the user removes an app and changes settings ->
`samurai update` brings the new samurai-tools (from a local repo while it is
unpublished; every other [samurai] package comes from the published channel) ->
preview changes nothing -> a failing transaction leaves the system as it was ->
adopt -> second run does nothing -> reboot -> rollback -> adopt again with nothing
removed, which installs the whole desktop meta.
"""
from __future__ import annotations

import os
import pathlib
import re
import shutil
import subprocess

import pytest

from conftest import INSTALL_TIMEOUT, REPO, find_iso, password_hash, with_test_autokey
from vm import VM

DEFAULTS = ("samurai-base samurai-settings-system samurai-settings-shell samurai-settings-dev "
            "samurai-desktop-base samurai-settings-kde samurai-toolbox-gui yay pipewire-jack")
USER_FILES = ("/home/samurai/.config/kdeglobals /etc/sddm.conf.d/10-samurai-kde.conf "
              "/etc/uv-user-note")
LOCAL_REPO = r"[samurai-local]\nSigLevel = Never\nServer = file:///var/lib/adopt-repo\n"


def _old_profile(ref: str) -> str:
    """The default KDE profile of release `ref`, made headless."""
    text = subprocess.run(["git", "-C", str(REPO), "show", f"{ref}:installer/profile-default.toml"],
                          check=True, capture_output=True, text=True).stdout
    user = ('[user]\nname = "samurai"\nshell = "bash"\ngroups = ["wheel"]\n'
            f'password_hash = "{password_hash()}"\n')
    text = re.sub(r"\[user\].*?(?=\n\[|\Z)", user, text, flags=re.S)
    text = text.replace('kernels = ["linux-cachyos", "linux-zen"]', 'kernels = ["linux"]')
    assert 'desktop = "kde"' in text and 'kernels = ["linux"]' in text, "release profile changed shape"
    return with_test_autokey(text)


class Guest:
    def __init__(self, vm: VM):
        self.vm = vm
        self.ga = vm.qga()

    def sh(self, cmd: str, timeout: float = 180) -> tuple[int, str]:
        rc, out = self.ga.exec(cmd, timeout=timeout, check=False)
        print(f"\n$ {cmd}\n{out.strip()[-5000:]}\n[rc={rc}]", flush=True)
        return rc, out

    def ok(self, cmd: str, timeout: float = 180) -> str:
        rc, out = self.sh(cmd, timeout)
        assert rc == 0, f"{cmd}: rc={rc}\n{out[-3000:]}"
        return out

    def reboot(self) -> "Guest":
        self.ga.exec("sync", check=False)
        self.ga.close()
        self.vm.reboot()
        self.vm.wait_for("login:", timeout=400)
        return Guest(self.vm)

    def state(self) -> str:
        """Everything a read-only step must leave alone."""
        return self.ok("md5sum /etc/pacman.conf; pacman -Qq | md5sum; "
                       "snapper -c root list --columns number | md5sum; ls /var/lib/pacman/sync | md5sum")


def test_adopt_current_defaults(mirror):
    ref = os.environ.get("SAMURAI_TEST_PROFILE_REF", "")
    package = os.environ.get("SAMURAI_TEST_TOOLS_PKG", "")
    if not ref or not package:
        pytest.skip("set SAMURAI_TEST_PROFILE_REF=<old tag> and SAMURAI_TEST_TOOLS_PKG=<new package>")
    package_path = pathlib.Path(package)
    assert package_path.is_file(), package
    shutil.copy(package_path, mirror.dir / package_path.name)
    (mirror.dir / "profile-adopt.toml").write_text(_old_profile(ref))

    vm = VM("adopt", iso=str(find_iso()), disk_size="30G", ram="4096")
    vm.start()
    try:
        vm.serial_connect()
        vm.login_root(timeout=280)
        ga = vm.ensure_qga()
        _, acc = ga.exec(
            "curl -fsSL http://10.0.2.2:8966/profile-adopt.toml -o /tmp/p.toml "
            "&& SAMURAI_TEST_AUTOKEY=1 SAMURAI_SERIAL_CONSOLE=1 samurai-install --profile /tmp/p.toml --disk /dev/vda --yes "
            ">/dev/null 2>&1; rc=$?; echo INSTALL-RC=$rc; [ $rc -eq 0 ] || tail -n 60 /var/log/samurai/install.log",
            timeout=INSTALL_TIMEOUT, check=False)
        assert "INSTALL-RC=0" in acc, acc[-2500:]
        ga.close()
        vm.eject_iso()
        vm.reboot()
        vm.wait_for("login:", timeout=400)
        g = Guest(vm)
        _scenario(g, package_path.name)
    finally:
        vm.stop()


def _scenario(g: Guest, package_name: str) -> None:
    # the old release: no Chaotic-AUR, no metas, no adopt command
    old = g.ok("pacman -Q samurai-tools; grep -c '^\\[chaotic-aur\\]' /etc/pacman.conf; "
               "pacman -Qq | grep -E '^samurai-' | tr '\\n' ' '")
    assert "samurai-base" not in old and "\n0\n" in old, old

    # the user's own choices
    g.ok("pacman -R --noconfirm kate")
    g.ok("systemctl disable bluetooth.service")
    g.ok("install -d -o samurai -g samurai /home/samurai/.config && "
         "printf '[General]\\nColorScheme=BreezeLight\\n' > /home/samurai/.config/kdeglobals && "
         "chown samurai:samurai /home/samurai/.config/kdeglobals && echo mine > /etc/uv-user-note && "
         "echo '# my own bashrc line' >> /etc/bash.bashrc")
    user_files = g.ok(f"md5sum {USER_FILES}")

    # the new samurai-tools arrives with a normal update
    g.ok(f"install -d -m 755 /var/lib/adopt-repo && cd /var/lib/adopt-repo && "
         f"curl -fsSLO 'http://10.0.2.2:8966/{package_name}' && repo-add -q samurai-local.db.tar.gz *.pkg.tar.zst && "
         f"chmod 644 * && sed -i '/^\\[samurai\\]$/i " + LOCAL_REPO + "' /etc/pacman.conf")
    rc, out = g.sh("samurai-update --noconfirm >/tmp/up.log 2>&1; rc=$?; tail -15 /tmp/up.log; exit $rc", 1500)
    assert rc in (0, 2), out
    out = g.ok("pacman -Q samurai-tools; samurai help | grep adopt; pacman -Qo /usr/lib/samurai/samurai-profile-plan")
    assert "0.4.0-18" in out and "owned by samurai-tools" in out, out

    # preview: shows the plan, changes nothing
    before = g.state()
    preview = g.ok("samurai adopt --preview", 600)
    assert "enable [chaotic-aur]" in preview and "samurai-base" in preview
    assert "kate: you removed it" in preview and "samurai-desktop-kde: it needs kate" in preview
    assert "gaming: no" in preview
    assert g.state() == before
    gaming = g.ok("samurai adopt --preview --with gaming", 600)
    assert "steam" in gaming and "samurai-wine" in gaming and g.state() == before
    # no terminal and no --yes: nothing happens
    rc, out = g.sh("samurai adopt --with-upgrade </dev/null", 600)
    assert rc == 2 and "--yes" in out and g.state() == before

    # a failing transaction: an unowned file in the way is not overwritten, nothing is installed
    g.ok("install -d /usr/lib/sysctl.d && echo '# mine' > /usr/lib/sysctl.d/70-samurai-performance.conf")
    conf = g.ok("md5sum /etc/pacman.conf")
    rc, out = g.sh("samurai adopt --yes --with-upgrade", 1500)
    assert rc == 1 and "FAILED" in out and "pacman.conf restored" in out and "samurai snap rollback" in out, out
    assert "exists in filesystem" in out, out
    assert g.ok("md5sum /etc/pacman.conf") == conf
    assert g.ok("cat /usr/lib/sysctl.d/70-samurai-performance.conf").strip() == "# mine"
    assert g.sh("pacman -Qq samurai-base")[0] != 0
    g.ok("rm /usr/lib/sysctl.d/70-samurai-performance.conf")

    # adopt
    rc, out = g.sh("samurai adopt --yes --with-upgrade", 2400)
    assert rc == 0, out
    snapshot = re.search(r"snapshot #(\d+) created", out).group(1)
    _assert_adopted(g, user_files)
    assert g.sh("pacman -Qq kate")[0] != 0 and g.sh("pacman -Qq samurai-desktop-kde")[0] != 0
    assert g.sh("pacman -Qq steam")[0] != 0

    # second run: nothing to do, no new snapshot
    before = g.state()
    rc, out = g.sh("samurai adopt --yes", 600)
    assert rc == 0 and "nothing to do" in out, out
    after = g.state()
    assert before.splitlines()[:3] == after.splitlines()[:3], (before, after)

    g = g.reboot()
    _assert_adopted(g, user_files)
    g.ok("for i in $(seq 60); do systemctl is-active -q sddm && break; sleep 2; done; systemctl is-active sddm")
    assert g.ok("sysctl -n vm.max_map_count").strip() == "2147483642"
    assert g.ok("systemctl --failed --no-legend --plain").strip() == ""
    g.ok("swapon --show --noheadings | grep zram")
    rc, out = g.sh("samurai-update --noconfirm >/tmp/up2.log 2>&1; rc=$?; tail -8 /tmp/up2.log; exit $rc", 1200)
    assert rc in (0, 2), out
    assert g.sh("pacman -Qq kate")[0] != 0, "an update after adopting brought the removed app back"

    # the printed way back works
    g.ok(f"samurai snap rollback {snapshot}", 900)
    g = g.reboot()
    out = g.ok("pacman -Qq samurai-base samurai-toolbox-gui 2>&1; grep -c '^\\[chaotic-aur\\]' /etc/pacman.conf; true")
    assert out.count("was not found") == 2 and out.strip().endswith("0"), out

    # nothing removed any more: the desktop meta itself is installed
    g.ok("pacman -Syu --noconfirm kate", 1200)
    rc, out = g.sh("samurai adopt --yes --with-upgrade", 2400)
    assert rc == 0 and "you removed it" not in out, out
    g.ok("pacman -Q samurai-desktop-kde samurai-base kate")
    g = g.reboot()
    assert g.ok("systemctl --failed --no-legend --plain").strip() == ""
    assert "nothing to do" in g.ok("samurai adopt --preview", 600)
    g.ga.close()


def _assert_adopted(g: Guest, user_files: str) -> None:
    order = g.ok("grep -E '^\\[' /etc/pacman.conf | tr '\\n' ' '")
    assert order.index("[multilib]") < order.index("[cachyos]") < order.index("[chaotic-aur]") \
        < order.index("[samurai"), order
    g.ok("pacman-key --list-keys EF925EA60F33D0CB85C44AD13056513887B78AEB >/dev/null")
    g.ok(f"pacman -Q {DEFAULTS}")
    g.ok("pacman -Dk")
    assert "realtime" in g.ok("id -nG samurai").split()
    assert g.ok(f"md5sum {USER_FILES}") == user_files
    g.ok("grep -qx '# my own bashrc line' /etc/bash.bashrc && grep -q 'samurai-settings-shell$' /etc/bash.bashrc")
    assert g.sh("systemctl is-enabled bluetooth.service")[1].strip() == "disabled"
    out = g.ok("snapper -c root list; cat /var/lib/samurai/adopt.json")
    assert "samurai-adopt" in out and '"chaotic_enabled": true' in out, out

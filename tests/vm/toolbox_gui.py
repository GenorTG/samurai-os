"""Drive Samurai Toolbox in a Plasma VM the way a person does: look at the screen (host-side
screendump + tesseract), click, type, and answer the polkit password dialog. Asserts go through the
guest agent.
"""
from __future__ import annotations

import os
import pathlib
import re
import shlex
import shutil
import subprocess
import tempfile
import time

from conftest import INSTALL_TIMEOUT, REPO, TEST_USER_PASSWORD, password_hash, with_test_autokey
from vm import VM

USER = "samurai"
SHOTS = pathlib.Path.home() / ".openclaw/workspace/samurai-shots/toolbox"
USER_ENV = f"runuser -u {USER} -- env XDG_RUNTIME_DIR=/run/user/1000 DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus"
UNIT = "samurai-toolbox-test"


def write_profile(mirror_dir: pathlib.Path) -> None:
    default = (REPO / "installer" / "profile-default.toml").read_text()
    user = (f'[user]\nname = "{USER}"\nshell = "bash"\ngroups = ["wheel"]\n'
            f'password_hash = "{password_hash()}"\n')
    profile = re.sub(r"\[user\].*?(?=\n\[|\Z)", user, default, flags=re.S)
    (mirror_dir / "profile-gui.toml").write_text(with_test_autokey(profile))


def run(ga, cmd: str, timeout: float = 300, check: bool = True) -> str:
    code, out = ga.exec(cmd, timeout=timeout, check=False)
    if check and code != 0:
        raise AssertionError(f"guest command failed ({code}): {cmd}\n{out[-3000:]}")
    return out


def install_base(vm: VM) -> None:
    """Default KDE install; afterwards the user is logged straight into Plasma. Leaves the VM off."""
    vm.start()
    try:
        vm.serial_connect()
        vm.login_root(timeout=280)
        ga = vm.ensure_qga()
        out = run(ga,
                  "curl -fsSL http://10.0.2.2:8966/profile-gui.toml -o /tmp/p.toml && "
                  "SAMURAI_TEST_AUTOKEY=1 SAMURAI_SERIAL_CONSOLE=1 samurai-install --profile /tmp/p.toml "
                  "--disk /dev/vda --yes >/dev/null 2>&1; rc=$?; echo INSTALL-RC=$rc; "
                  "[ $rc -eq 0 ] || tail -n 60 /var/log/samurai/install.log", timeout=INSTALL_TIMEOUT)
        assert "INSTALL-RC=0" in out, out[-4000:]
        ga.close()
        vm.eject_iso()
        vm.reboot()
        vm.wait_for("login:", timeout=300)
        ga = vm.qga(timeout=120)
        home = f"/home/{USER}/.config"
        run(ga, " && ".join([
            "mkdir -p /etc/sddm.conf.d",
            f"printf '[Autologin]\\nUser={USER}\\nSession=plasma\\n' > /etc/sddm.conf.d/zz-vm-autologin.conf",
            # no screen lock, blanking or welcome window in the way of the screenshots
            f"install -d -o {USER} -g {USER} {home}",
            f"printf '[Daemon]\\nAutolock=false\\nLockOnResume=false\\n' > {home}/kscreenlockerrc",
            f"printf '[AC][Display]\\nTurnOffDisplayWhenIdle=false\\nDimDisplayWhenIdle=false\\n"
            f"[AC][SuspendAndShutdown]\\nAutoSuspendAction=0\\n' > {home}/powerdevilrc",
            f"printf '[General]\\nLastSeenVersion=99.0.0\\nShowUpdatePage=false\\n' > {home}/plasma-welcomerc",
            f"chown -R {USER}:{USER} {home}",
            "sync",
        ]))
        ga.close()
    finally:
        vm.stop()


def wait_for_plasma(ga) -> None:
    run(ga, f"for i in $(seq 120); do pgrep -u {USER} -x plasmashell >/dev/null && "
            "pgrep -u samurai -f polkit-kde-authentication-agent >/dev/null && exit 0; sleep 2; done; "
            "exit 1", timeout=300)
    time.sleep(20)  # panel and wallpaper


def install_packages(ga, mirror_dir: pathlib.Path, packages: list[pathlib.Path]) -> str:
    names = []
    for pkg in packages:
        shutil.copy2(pkg, mirror_dir / pkg.name)
        run(ga, f"curl -fsSL http://10.0.2.2:8966/{shlex.quote(pkg.name)} -o /tmp/{shlex.quote(pkg.name)}")
        names.append(f"/tmp/{pkg.name}")
    # samurai-tools replaces files the installer wrote on first install
    return run(ga, "pacman -U --noconfirm --overwrite '/usr/local/bin/samurai*' "
                   "--overwrite '/usr/share/samurai/*' --overwrite '/usr/lib/samurai/*' "
                   + " ".join(shlex.quote(n) for n in names) + " 2>&1", timeout=600)


class Desktop:
    def __init__(self, vm: VM, ga):
        self.vm, self.ga = vm, ga

    # ------------------------------------------------------------ seeing
    def shot(self, name: str) -> pathlib.Path:
        return pathlib.Path(self.vm.screendump(name, shots_dir=str(SHOTS)))

    @staticmethod
    def size(png: pathlib.Path) -> tuple[int, int]:
        head = png.read_bytes()[:24]
        return int.from_bytes(head[16:20], "big"), int.from_bytes(head[20:24], "big")

    @staticmethod
    def lines(png: pathlib.Path, mode: str) -> list[list[tuple[str, int, int, int, int]]]:
        """Text on a screenshot as lines of (word, x, y, w, h); mode ",negate" suits dark themes."""
        with tempfile.TemporaryDirectory() as tmp:
            big = pathlib.Path(tmp) / "big.png"
            subprocess.run(["ffmpeg", "-y", "-loglevel", "error", "-i", str(png), "-vf",
                            f"scale=iw*2:ih*2:flags=lanczos,format=gray{mode}", str(big)], check=True)
            # one thread: tesseract's OpenMP threads spin for minutes beside a busy VM
            tsv = subprocess.run(["tesseract", str(big), "stdout", "--psm", "11", "tsv"],
                                 capture_output=True, text=True, check=True, timeout=120,
                                 env={**os.environ, "OMP_THREAD_LIMIT": "1"}).stdout
        rows: dict[tuple, list] = {}
        for line in tsv.splitlines()[1:]:
            f = line.split("\t")
            if len(f) == 12 and f[0] == "5" and f[11].strip():
                x, y, w, h = (int(v) // 2 for v in f[6:10])
                rows.setdefault(tuple(f[1:5]), []).append((f[11].strip(), x, y, w, h))
        return list(rows.values())

    def find(self, phrase: str) -> list[tuple[int, int]]:
        """Centres of every place `phrase` is shown, top to bottom."""
        def norm(s: str) -> str:
            return re.sub(r"[^a-z0-9]", "", s.lower())

        want = norm(phrase)
        png = self.shot("ocr")
        hits: list[tuple[int, int]] = []
        for mode in ("", ",negate"):
            for words in self.lines(png, mode):
                for i in range(len(words)):
                    text = ""
                    for j in range(i, len(words)):
                        text += norm(words[j][0])
                        if text == want:
                            left, right = words[i][1], words[j][1] + words[j][3]
                            hits.append(((left + right) // 2, words[i][2] + words[i][4] // 2))
                        if not want.startswith(text):
                            break
            if hits:
                break
        return sorted(hits, key=lambda p: (p[1], p[0]))

    def wait_text(self, phrase: str, timeout: float = 60) -> list[tuple[int, int]]:
        deadline = time.time() + timeout
        while True:
            hits = self.find(phrase)
            if hits:
                return hits
            if time.time() > deadline:
                self.shot("missing-text")
                raise AssertionError(f"the screen never showed {phrase!r}")
            time.sleep(2)

    # ------------------------------------------------------------ doing
    def click_at(self, x: int, y: int) -> None:
        png = self.shot("ocr")
        self.vm.click(x, y, self.size(png))
        time.sleep(1)

    def click(self, phrase: str, index: int = 0, timeout: float = 60) -> None:
        """Click the `index`-th occurrence of `phrase` (negative: from the bottom)."""
        self.click_at(*self.wait_text(phrase, timeout)[index])

    def page(self, name: str) -> None:
        """Choose a page in the sidebar and wait for its header. The first click after another
        window closed (a password dialog) can be spent on focusing, so try again."""
        for _ in range(3):
            left = min(self.wait_text("Overview"), key=lambda p: p[0])[0]
            entry = [p for p in self.wait_text(name) if p[0] < left + 150]
            assert entry, f"no sidebar entry {name!r}"
            self.click_at(*entry[0])
            time.sleep(3)
            if any(x > left + 150 and y < entry[0][1] and y < 170 for x, y in self.find(name)):
                return
        raise AssertionError(f"page {name!r} did not open")

    def confirm(self, label: str) -> None:
        """Press the button `label` of the open confirmation dialog: the one beside Cancel."""
        cancel = self.wait_text("Cancel")[-1]
        beside = [p for p in self.wait_text(label) if abs(p[1] - cancel[1]) < 12]
        assert beside, f"no {label!r} button beside Cancel"
        self.click_at(*beside[-1])

    def authenticate(self, name: str = "auth") -> None:
        """Answer the Plasma polkit dialog."""
        self.wait_text("Authentication is required", timeout=60)  # the bold message
        time.sleep(1.5)
        self.shot(name)
        self.vm.type_text(TEST_USER_PASSWORD)
        self.vm.key("ret")

    def open_toolbox(self, page: str = "overview") -> None:
        self.close_toolbox()
        run(self.ga, f"{USER_ENV} systemd-run --user --collect --unit={UNIT} "
                     f"/usr/bin/samurai-toolbox --page {shlex.quote(page)}")
        run(self.ga, f"sleep 3; {USER_ENV} systemctl --user is-active {UNIT}")
        time.sleep(4)

    def close_toolbox(self) -> None:
        run(self.ga, f"{USER_ENV} systemctl --user stop {UNIT} 2>/dev/null; sleep 1", check=False)

    def toolbox_log(self) -> str:
        return run(self.ga, f"journalctl -q -o cat _SYSTEMD_USER_UNIT={UNIT}.service", check=False)

    def wait_guest(self, test: str, timeout: float = 120) -> None:
        """Wait until the shell condition `test` holds in the guest."""
        run(self.ga, f"for i in $(seq {int(timeout // 2)}); do if {test}; then exit 0; fi; sleep 2; "
                     "done; exit 1", timeout=timeout + 60)

"""Default KDE system (spec 10, 13, 14; spec 11: a feature counts when a VM proves it).

Install the release default profile (desktop=kde with every default software
choice) from the OFFLINE ISO behind a dead mirror, boot it, log the user into a
real Plasma session and assert that the promised defaults are installed and
configured: packages, services, performance settings, shells, developer tools,
gaming set, repositories and the desktop layout.

Every check prints its output; all failures are collected and reported together.
SAMURAI_TEST_UPGRADE_PKGS=<dir> installs the package files in that directory after
the first boot (verifies package fixes without rebuilding the ISO).
"""
from __future__ import annotations

import os
import pathlib
import re
import shutil

from conftest import INSTALL_TIMEOUT, REPO, find_iso, password_hash, with_test_autokey
from vm import VM

USER = "samurai"
DEAD_MIRROR = "https://offline.invalid/no-such-mirror"

PACKAGES = """
samurai-release samurai-base samurai-settings-system samurai-settings-shell samurai-settings-dev
samurai-desktop-base samurai-desktop-kde samurai-settings-kde samurai-wine samurai-proton-ge samurai-miniforge
linux-firmware amd-ucode intel-ucode sof-firmware fwupd
git github-cli base-devel micro nano btop nvtop fastfetch eza bat ripgrep fd fzf zoxide tldr rsync wget curl
unzip 7zip man-db yay snapper snap-pac
pipewire wireplumber pipewire-pulse pipewire-jack realtime-privileges
noto-fonts noto-fonts-cjk noto-fonts-emoji ttf-jetbrains-mono-nerd ttf-liberation
plasma-meta sddm konsole dolphin kate ark spectacle gwenview okular kcalc partitionmanager kdeconnect haruna elisa
discover flatpak firefox bluez plasma-firewall firewalld
steam heroic-games-launcher-bin gamemode lib32-gamemode mangohud lib32-mangohud goverlay gamescope
proton-cachyos-slr umu-launcher protontricks wine-staging winetricks wine-mono wine-gecko
lib32-mesa lib32-vulkan-radeon lib32-vulkan-intel
docker docker-compose docker-buildx uv python tailscale
fish zsh starship ananicy-cpp cachyos-ananicy-rules irqbalance power-profiles-daemon scx-scheds zram-generator
cachyos-keyring chaotic-keyring chaotic-mirrorlist
""".split()

ENABLED = ("sddm", "NetworkManager", "systemd-resolved", "firewalld", "bluetooth", "power-profiles-daemon",
           "ananicy-cpp", "irqbalance", "systemd-oomd", "docker.socket", "tailscaled",
           "fstrim.timer", "paccache.timer", "snapper-cleanup.timer", "samurai-health.timer")
ACTIVE = ("sddm", "NetworkManager", "firewalld", "ananicy-cpp", "irqbalance", "systemd-oomd",
          "docker.socket", "tailscaled", "dev-zram0.swap")


def _profile(mirror) -> None:
    default = (REPO / "installer" / "profile-default.toml").read_text()
    new_user = (f'[user]\nname = "{USER}"\nshell = "fish"\ngroups = ["wheel"]\n'
                f'password_hash = "{password_hash()}"\n')
    profile = re.sub(r"\[user\].*?(?=\n\[|\Z)", new_user, default, flags=re.S)
    profile = profile.replace('arch_mirror = "auto"', f'arch_mirror = "{DEAD_MIRROR}"')
    assert DEAD_MIRROR in profile and 'desktop = "kde"' in profile
    (mirror.dir / "profile-defaults.toml").write_text(with_test_autokey(profile))


class Checks:
    def __init__(self, vm: VM):
        self.vm = vm
        self.ga = vm.qga(timeout=180)
        self.failures: list[str] = []

    def run(self, name: str, cmd: str, expect: list[str] = (), absent: list[str] = (),
            timeout: float = 180) -> str:
        _, out = self.ga.exec(f"{{ {cmd} ; }} 2>&1", timeout=timeout, check=False)
        print(f"--- {name}\n{out[-3000:]}", flush=True)
        for text in expect:
            if text not in out:
                self.failures.append(f"{name}: missing {text!r}")
        for text in absent:
            if text in out:
                self.failures.append(f"{name}: unexpected {text!r}")
        return out


def test_default_kde_system(mirror):
    _profile(mirror)
    vm = VM("defaults", iso=str(find_iso()), disk_size="48G", ram="8192", smp="4")
    vm.start()
    try:
        vm.serial_connect()
        vm.login_root(timeout=280)
        ga = vm.ensure_qga()
        _, out = ga.exec(
            "curl -fsSL http://10.0.2.2:8966/profile-defaults.toml -o /tmp/p.toml "
            "&& SAMURAI_TEST_AUTOKEY=1 SAMURAI_SERIAL_CONSOLE=1 samurai-install --profile /tmp/p.toml --disk /dev/vda --yes "
            ">/dev/null 2>&1; rc=$?; echo DEFAULTS-INSTALL-RC=$rc; "
            "grep -E 'offline install|deferred|FATAL|WARNING' /var/log/samurai/install.log; "
            "[ $rc -eq 0 ] || tail -n 80 /var/log/samurai/install.log",
            timeout=INSTALL_TIMEOUT,
        )
        print(out[-6000:])
        assert "DEFAULTS-INSTALL-RC=0" in out, out[-6000:]
        assert "offline install: closure installed" in out, out[-3000:]
        assert "deferred to first boot" not in out, "the default install must be complete offline"
        ga.close()

        vm.eject_iso()
        vm.reboot()
        try:
            vm.wait_for("login:", timeout=600)
        except TimeoutError:
            vm.screendump("defaults-boot-failed")
            raise
        c = Checks(vm)
        upgrade = os.environ.get("SAMURAI_TEST_UPGRADE_PKGS")
        if upgrade:
            files = sorted(pathlib.Path(upgrade).glob("*.pkg.tar.zst"))
            assert files, f"no packages in {upgrade}"
            for f in files:
                shutil.copy(f, mirror.dir / f.name)
            fetch = " && ".join(f"curl -fsS -o /tmp/{f.name} http://10.0.2.2:8966/{f.name}" for f in files)
            c.run("upgrade", f"{fetch} && pacman -U --noconfirm "
                  f"{' '.join('/tmp/' + f.name for f in files)} && echo UPGRADE-OK", ["UPGRADE-OK"], timeout=600)

        c.run("packages", f"pacman -Q {' '.join(PACKAGES)} >/tmp/q.txt; rc=$?; grep -c . /tmp/q.txt; "
              "grep error /tmp/q.txt; echo PKGS-RC=$rc", ["PKGS-RC=0"])
        c.run("no-packagekit", "pacman -Qq packagekit-qt6 || echo NO-PACKAGEKIT", ["NO-PACKAGEKIT"])
        c.run("settled", "systemctl is-system-running --wait; systemctl --failed --no-legend", ["running"],
              timeout=240)
        c.run("enabled", "for u in " + " ".join(ENABLED) + "; do echo \"$u=$(systemctl is-enabled $u)\"; done",
              [f"{u}=enabled" for u in ENABLED])
        c.run("active", "for u in " + " ".join(ACTIVE) + "; do echo \"$u=$(systemctl is-active $u)\"; done",
              [f"{u}=active" for u in ACTIVE])

        # performance settings (spec 10) as the running kernel sees them
        c.run("sysctl", "sysctl vm.swappiness vm.vfs_cache_pressure vm.max_map_count vm.dirty_bytes",
              ["vm.swappiness = 100", "vm.vfs_cache_pressure = 50", "vm.max_map_count = 2147483642",
               "vm.dirty_bytes = 268435456"])
        c.run("thp-zram", "cat /sys/kernel/mm/transparent_hugepage/enabled; zramctl --noheadings -o NAME,ALGORITHM,DISKSIZE; "
              "free -g | head -2", ["[madvise]", "zstd"])
        c.run("journald-timeout", "systemd-analyze cat-config systemd/journald.conf | grep SystemMaxUse; "
              "systemctl show -p DefaultTimeoutStopUSec", ["SystemMaxUse=500M", "DefaultTimeoutStopUSec=15s"])
        c.run("iosched", "udevadm cat 60-samurai-ioschedulers.rules 2>/dev/null || cat /usr/lib/udev/rules.d/60-samurai-ioschedulers.rules; "
              "udevadm verify /usr/lib/udev/rules.d/60-samurai-ioschedulers.rules && echo RULES-OK",
              ["RULES-OK"])
        c.run("scx-off", "systemctl is-active scx_loader scx 2>&1; cat /sys/kernel/sched_ext/state 2>/dev/null",
              absent=["enabled"])

        # user, shells, groups (spec 10, 13)
        c.run("user", f"getent passwd {USER} | cut -d: -f7; echo GROUPS $(id -nG {USER})",
              ["/bin/fish", "docker", "gamemode", "realtime", "wheel"])
        c.run("fish", f"runuser -l {USER} -c \"fish -i -c 'type ls; type conda | head -1; echo PATH=\\$PATH; "
              "echo AUTOUPDATER=\\$DISABLE_AUTOUPDATER'\"",
              ["eza", "conda", "AUTOUPDATER=1", f"/home/{USER}/.local/bin"])
        c.run("bash", f"runuser -u {USER} -- env HOME=/home/{USER} TERM=xterm bash -ic "
              "'type ll; type conda | head -1; type pip | head -1; echo PY=$(command -v python)'",
              ["eza", "conda is a function", "pip is a function", "PY=/usr/bin/python"])
        c.run("python-conda", "python --version; uv --version; grep -v '^#' /etc/uv/uv.toml; "
              "/opt/miniforge/bin/conda --version; /opt/miniforge/bin/conda config --show auto_activate_base envs_dirs",
              ["Python 3.", 'python-preference = "system"', "conda ", "False", ".conda/envs"])
        c.run("conda-env", f"runuser -l {USER} -c '/opt/miniforge/bin/conda create -y -q --offline -n probe >/dev/null && "
              "test -d ~/.conda/envs/probe && echo CONDA-ENV-OK'", ["CONDA-ENV-OK"], timeout=300)
        c.run("docker", f"runuser -l {USER} -c 'docker version --format {{{{.Server.Version}}}} && docker compose version && "
              "docker buildx version' && echo DOCKER-OK", ["DOCKER-OK", "Docker Compose"], timeout=240)

        # gaming (spec 10)
        c.run("proton", "ls /usr/share/steam/compatibilitytools.d/; "
              "ls /usr/share/steam/compatibilitytools.d/*/compatibilitytool.vdf", ["GE-Proton", "cachyos"])
        c.run("wine", f"runuser -l {USER} -c 'WINEDEBUG=-all wine --version; printf MZ > /tmp/setup.exe; "
              "t=$(xdg-mime query filetype /tmp/setup.exe); echo TYPE=$t; echo EXE=$(xdg-mime query default $t); "
              "echo MSI=$(xdg-mime query default application/x-msi)'",
              ["wine-", "EXE=samurai-wine.desktop", "MSI=samurai-wine.desktop"])

        # repositories (spec 02, ADR-0007)
        out = c.run("repos", "grep -E '^\\[' /etc/pacman.conf | tr '\\n' ' '; echo; "
                    "pacman-conf --repo=chaotic-aur SigLevel; "
                    "pacman-key --list-keys EF925EA60F33D0CB85C44AD13056513887B78AEB | head -4",
                    ["PackageRequired", "full"])
        order = re.findall(r"\[([a-z-]+)\]", out.splitlines()[0])
        if order != ["options", "core", "extra", "multilib", "cachyos", "chaotic-aur", "samurai"]:
            c.failures.append(f"repos: unexpected order {order}")
        c.run("firewall", "firewall-cmd --state; firewall-cmd --get-default-zone; echo SVC $(firewall-cmd --list-services)",
              ["running", "public", "SVC dhcpv6-client"])
        c.run("notes", "cat /var/lib/samurai/firstboot-notes 2>/dev/null || echo NO-NOTES", ["NO-NOTES"])

        # KDE system defaults (spec 14)
        c.run("kde-files", "samurai-kde-defaults status; cat /etc/xdg/kwinrc /etc/xdg/kdeglobals /etc/xdg/baloofilerc "
              "/etc/xdg/konsolerc /etc/skel/.config/kdeglobals; ls /usr/lib/sddm/sddm.conf.d/",
              ["blurEnabled=false", "AnimationDurationFactor=0", "org.kde.breezedark.desktop",
               "only basic indexing=true", "DefaultProfile=Samurai.profile", "10-samurai-kde.conf"])

        # a real Plasma session (VM-only autologin): first-login layout runs
        env = (f"runuser -u {USER} -- env HOME=/home/{USER} XDG_RUNTIME_DIR=/run/user/1000 "
               "DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus")
        c.run("session", "mkdir -p /etc/sddm.conf.d && "
              f"printf '[Autologin]\\nUser={USER}\\nSession=plasma\\n' > /etc/sddm.conf.d/zz-vm-autologin.conf && "
              "systemctl restart sddm; "
              "for i in $(seq 90); do pgrep -u samurai -x plasmashell >/dev/null && break; sleep 2; done; "
              "pgrep -u samurai -x plasmashell >/dev/null && echo SESSION-OK", ["SESSION-OK"], timeout=240)
        c.run("layout-ran", f"for i in $(seq 90); do [ -e /home/{USER}/.config/autostart/samurai-plasma-layout.desktop ] "
              "|| break; sleep 2; done; "
              f"ls /home/{USER}/.config/autostart/; "
              f"[ ! -e /home/{USER}/.config/autostart/samurai-plasma-layout.desktop ] && echo LAYOUT-DONE; "
              "journalctl -b --no-pager | grep -i samurai-plasma | tail -5", ["LAYOUT-DONE"], timeout=240)
        panels = c.run(
            "panels",
            "sleep 8; " + env + " qdbus6 org.kde.plasmashell /PlasmaShell org.kde.PlasmaShell.evaluateScript '"
            "var p = panels(); for (var i = 0; i < p.length; i++) { var w = p[i].widgets(); var n = []; "
            "for (var j = 0; j < w.length; j++) { var s = w[j].type; "
            "if (s == \"org.kde.plasma.systemmonitor\") { w[j].currentConfigGroup = [\"Sensors\"]; "
            "s += \"{\" + w[j].readConfig(\"highPrioritySensorIds\") + \"}\"; } "
            "if (s == \"org.kde.plasma.taskmanager\") { w[j].currentConfigGroup = [\"General\"]; "
            "s += \"{group=\" + w[j].readConfig(\"groupingStrategy\") + \"}\"; } n.push(s); } "
            "print(\"PANEL \" + p[i].location + \" \" + n.join(\",\") + \"\\n\"); }'",
            ["PANEL top", "PANEL bottom", "cpu/all/usage"], timeout=90)
        lines = {l.split()[1]: l for l in panels.splitlines() if l.startswith("PANEL ")}
        if len(lines) != 2:
            c.failures.append(f"panels: expected a top and a bottom panel:\n{panels}")
        else:
            for widget in ("kickoff", "pager", "systemmonitor", "systemtray", "digitalclock", "showdesktop"):
                if widget not in lines["top"]:
                    c.failures.append(f"panels: top panel lacks {widget}")
            if "taskmanager" in lines["top"] or not re.fullmatch(
                    r"PANEL bottom org\.kde\.plasma\.taskmanager\{group=0\}", lines["bottom"].strip()):
                c.failures.append(f"panels: bottom panel is wrong: {lines['bottom']}")
        c.run("sensors", env + " timeout 60 kstatsviewer --list | grep -E '^cpu/all/(usage|averageFrequency|averageTemperature)|^gpu/' | head -20",
              ["cpu/all/usage", "cpu/all/averageFrequency"], timeout=90)
        c.run("theme", f"grep -rh -E 'ColorScheme|LookAndFeelPackage' /home/{USER}/.config/kdeglobals "
              f"/home/{USER}/.config/kdedefaults/ 2>/dev/null; "
              + env + " kreadconfig6 --file kdeglobals --group General --key ColorScheme", ["BreezeDark"])
        c.run("effects-live", env + " qdbus6 org.kde.KWin /Effects org.kde.kwin.Effects.loadedEffects",
              ["overview"], absent=["blur", "slidingpopups", "squash"])
        c.run("session-health", "systemctl --failed --no-legend | wc -l; "
              f"pgrep -u {USER} -x kwin_wayland >/dev/null && echo KWIN-OK; "
              "wpctl --version >/dev/null; echo", ["KWIN-OK"])
        c.ga.exec("sleep 10", timeout=40)
        vm.screendump("defaults-01-desktop")
        # started by the user's systemd manager: the real session environment
        c.run("terminal", env + " systemd-run --user --collect --quiet konsole; sleep 10; "
              f"pgrep -u {USER} -x konsole >/dev/null && echo KONSOLE-OK", ["KONSOLE-OK"], timeout=60)
        vm.screendump("defaults-02-konsole")
        c.run("disk-use", "df -h / | tail -1; du -sh /var/cache/pacman/pkg 2>/dev/null")
        c.ga.close()
        assert not c.failures, "\n".join(c.failures)
        vm.stop()
    except BaseException:
        vm.stop()
        raise

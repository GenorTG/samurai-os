//! The v1 check set (spec 05). Every `detect` is read-only and goes through [`System`].

use super::system::System;
use super::{Category, Check, FixRisk, Issue, Outcome, Severity};

fn issue(summary: impl Into<String>, details: impl Into<String>, evidence: Vec<String>) -> Outcome {
    Outcome::Issue(Issue {
        summary: summary.into(),
        details: details.into(),
        evidence,
    })
}

fn lines(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect()
}

/// `pacman -Qq` names, or the outcome to return when pacman cannot answer.
fn installed(sys: &dyn System) -> Result<Vec<String>, Outcome> {
    match sys.run("pacman", &["-Qq"]) {
        Some(o) if o.code == 0 => Ok(lines(&o.stdout)),
        Some(_) => Err(Outcome::Skipped(
            "pacman -Qq failed (database locked?)".into(),
        )),
        None => Err(Outcome::Skipped("pacman not available".into())),
    }
}

fn services_failed(sys: &dyn System) -> Outcome {
    let Some(o) = sys.run("systemctl", &["--failed", "--no-legend", "--plain"]) else {
        return Outcome::Skipped("systemctl not available".into());
    };
    let units: Vec<String> = lines(&o.stdout)
        .iter()
        .map(|l| l.split_whitespace().next().unwrap_or("").to_string())
        .collect();
    if units.is_empty() {
        return Outcome::Ok;
    }
    issue(
        format!("{} failed systemd unit(s)", units.len()),
        "Inspect with: systemctl status <unit>; journalctl -u <unit> -b",
        units,
    )
}

fn services_time_sync(sys: &dyn System) -> Outcome {
    match sys.run("timedatectl", &["show", "-p", "NTPSynchronized", "--value"]) {
        Some(o) if o.code == 0 && o.stdout.trim() == "yes" => Outcome::Ok,
        Some(o) if o.code == 0 => issue(
            "system clock is not synchronized",
            "Enable time sync: systemctl enable --now systemd-timesyncd",
            vec![format!("NTPSynchronized={}", o.stdout.trim())],
        ),
        _ => Outcome::Skipped("timedatectl not available".into()),
    }
}

fn security_firewall(sys: &dyn System) -> Outcome {
    let Some(o) = sys.run("systemctl", &["is-active", "firewalld"]) else {
        return Outcome::Skipped("systemctl not available".into());
    };
    let state = o.stdout.trim();
    if state == "active" {
        return Outcome::Ok;
    }
    issue(
        "firewalld is not active",
        "Enable it: systemctl enable --now firewalld",
        vec![format!(
            "systemctl is-active firewalld: {}",
            if state.is_empty() { "unknown" } else { state }
        )],
    )
}

fn snapshots_services(sys: &dyn System) -> Outcome {
    // spec 06: timeline snapshots are off for root (pre/post pairs cover changes); only cleanup runs
    if !sys.exists("/usr/bin/snapper") {
        return Outcome::Skipped("snapper is not installed".into());
    }
    let Some(o) = sys.run("systemctl", &["is-enabled", "snapper-cleanup.timer"]) else {
        return Outcome::Skipped("systemctl not available".into());
    };
    let state = o.stdout.trim();
    if state == "enabled" {
        return Outcome::Ok;
    }
    issue(
        "snapper-cleanup.timer is not enabled",
        "Old snapshots pile up without it. Enable: systemctl enable --now snapper-cleanup.timer",
        vec![format!(
            "snapper-cleanup.timer: {}",
            if state.is_empty() { "unknown" } else { state }
        )],
    )
}

fn boot_pending_reboot(sys: &dyn System) -> Outcome {
    let Some(rel) = sys.read("/proc/sys/kernel/osrelease") else {
        return Outcome::Skipped("kernel release unreadable".into());
    };
    let rel = rel.trim();
    if sys.exists(&format!("/usr/lib/modules/{rel}")) {
        return Outcome::Ok;
    }
    issue(
        "reboot pending: the running kernel's modules are gone",
        "A kernel update replaced the files of the running kernel. Reboot to load the new one.",
        vec![format!("/usr/lib/modules/{rel} is missing")],
    )
}

fn storage_disk_free(sys: &dyn System) -> Outcome {
    let args = [
        "-P", "-x", "tmpfs", "-x", "devtmpfs", "-x", "squashfs", "-x", "overlay",
    ];
    let Some(o) = sys.run("df", &args) else {
        return Outcome::Skipped("df not available".into());
    };
    let full: Vec<String> = o
        .stdout
        .lines()
        .skip(1)
        .filter_map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            let pct: u32 = f.get(4)?.trim_end_matches('%').parse().ok()?;
            (pct >= 90).then(|| format!("{} {pct}% full", f.get(5).unwrap_or(&"?")))
        })
        .collect();
    if full.is_empty() {
        return Outcome::Ok;
    }
    issue(
        format!("{} filesystem(s) above 90% full", full.len()),
        "Free space: samurai clean all --preview",
        full,
    )
}

fn packages_core(sys: &dyn System) -> Outcome {
    let have = match installed(sys) {
        Ok(h) => h,
        Err(skip) => return skip,
    };
    let missing: Vec<String> = ["base", "linux-firmware", "samurai-tools"]
        .iter()
        .filter(|p| !have.iter().any(|h| h == *p))
        .map(|p| (*p).to_string())
        .collect();
    if missing.is_empty() {
        return Outcome::Ok;
    }
    issue(
        "core packages are missing",
        "Reinstall them: pacman -S <package>",
        missing,
    )
}

fn packages_packagekit(sys: &dyn System) -> Outcome {
    let have = match installed(sys) {
        Ok(h) => h,
        Err(skip) => return skip,
    };
    if !have.iter().any(|h| h == "packagekit") {
        return Outcome::Ok;
    }
    issue(
        "packagekit is installed",
        "Discover can apply partial upgrades through packagekit. Remove it: pacman -Rns packagekit",
        vec!["packagekit".into()],
    )
}

fn pacman_pacnew(sys: &dyn System) -> Outcome {
    let args = [
        "/etc",
        "-xdev",
        "(",
        "-name",
        "*.pacnew",
        "-o",
        "-name",
        "*.pacsave",
        ")",
    ];
    let Some(o) = sys.run("find", &args) else {
        return Outcome::Skipped("find not available".into());
    };
    let files = lines(&o.stdout);
    if files.is_empty() {
        return Outcome::Ok;
    }
    issue(
        format!("{} .pacnew/.pacsave file(s) to merge", files.len()),
        "Merge them: samurai pacdiff (or pacdiff)",
        files,
    )
}

fn pacman_conf(sys: &dyn System) -> Outcome {
    let Some(conf) = sys.read("/etc/pacman.conf") else {
        return Outcome::Skipped("/etc/pacman.conf unreadable".into());
    };
    let active: Vec<&str> = conf
        .lines()
        .map(|l| l.split('#').next().unwrap_or("").trim())
        .filter(|l| !l.is_empty())
        .collect();
    let mut problems = Vec::new();
    for repo in ["core", "extra", "samurai"] {
        if !active.iter().any(|l| *l == format!("[{repo}]")) {
            problems.push(format!("repository [{repo}] is missing or commented out"));
        }
    }
    for l in active
        .iter()
        .filter(|l| l.starts_with("SigLevel") && l.contains("Never"))
    {
        problems.push(format!("signature checking disabled: {l}"));
    }
    if problems.is_empty() {
        return Outcome::Ok;
    }
    issue(
        "pacman.conf deviates from the distro defaults",
        "Restore the defaults: samurai fix pacman",
        problems,
    )
}

fn hardware_ram(sys: &dyn System) -> Outcome {
    if sys.exists("/sys/block/zram0") {
        return Outcome::Ok;
    }
    match sys.read("/proc/swaps") {
        Some(s) if s.contains("zram") => Outcome::Ok,
        Some(_) => issue(
            "no zram swap is active",
            "Compressed RAM swap helps small machines: install zram-generator",
            Vec::new(),
        ),
        None => Outcome::Skipped("/proc/swaps unreadable".into()),
    }
}

/// `systemctl enable --now <units>`; `what` names the change in the report.
fn enable_now(sys: &dyn System, units: &[&str], what: &str) -> Result<Vec<String>, String> {
    let mut args = vec!["enable", "--now"];
    args.extend_from_slice(units);
    match sys.run("systemctl", &args) {
        Some(o) if o.code == 0 => Ok(vec![what.to_string()]),
        Some(o) => Err(format!(
            "systemctl enable --now {} exited {}",
            units.join(" "),
            o.code
        )),
        None => Err("systemctl not available".into()),
    }
}

fn fix_firewall(sys: &dyn System) -> Result<Vec<String>, String> {
    enable_now(sys, &["firewalld"], "enabled and started firewalld")
}

fn fix_time_sync(sys: &dyn System) -> Result<Vec<String>, String> {
    enable_now(
        sys,
        &["systemd-timesyncd"],
        "enabled and started systemd-timesyncd",
    )
}

fn fix_snapper_cleanup(sys: &dyn System) -> Result<Vec<String>, String> {
    enable_now(
        sys,
        &["snapper-cleanup.timer"],
        "enabled snapper-cleanup.timer",
    )
}

/// Kernel package bases installed under /usr/lib/modules (`pkgbase` files, as mkinitcpio uses).
fn kernel_bases(sys: &dyn System) -> Vec<String> {
    sys.list_dir("/usr/lib/modules")
        .iter()
        .filter_map(|d| sys.read(&format!("/usr/lib/modules/{d}/pkgbase")))
        .map(|b| b.trim().to_string())
        .filter(|b| !b.is_empty())
        .collect()
}

fn boot_kernels_present(sys: &dyn System) -> Outcome {
    let bases = kernel_bases(sys);
    if bases.is_empty() {
        return Outcome::Skipped("no installed kernel found".into());
    }
    let missing: Vec<String> = bases
        .iter()
        .filter(|b| !sys.exists(&format!("/boot/vmlinuz-{b}")))
        .map(|b| format!("/boot/vmlinuz-{b} is missing"))
        .collect();
    if missing.is_empty() {
        return Outcome::Ok;
    }
    issue(
        "an installed kernel has no image in /boot",
        "The system may not boot with it. Reinstall the kernel package: pacman -S <kernel package>",
        missing,
    )
}

fn missing_initramfs(sys: &dyn System) -> Vec<String> {
    kernel_bases(sys)
        .into_iter()
        .filter(|b| {
            sys.exists(&format!("/boot/vmlinuz-{b}"))
                && !sys.exists(&format!("/boot/initramfs-{b}.img"))
        })
        .collect()
}

fn boot_initramfs(sys: &dyn System) -> Outcome {
    if kernel_bases(sys).is_empty() {
        return Outcome::Skipped("no installed kernel found".into());
    }
    let missing = missing_initramfs(sys);
    if missing.is_empty() {
        return Outcome::Ok;
    }
    issue(
        "an installed kernel has no initramfs",
        "The system will not boot that kernel. Rebuild: mkinitcpio -p <kernel>",
        missing
            .iter()
            .map(|b| format!("/boot/initramfs-{b}.img is missing"))
            .collect(),
    )
}

fn fix_initramfs(sys: &dyn System) -> Result<Vec<String>, String> {
    let mut done = Vec::new();
    for b in missing_initramfs(sys) {
        match sys.run("mkinitcpio", &["-p", &b]) {
            Some(o) if o.code == 0 => done.push(format!("rebuilt initramfs for {b}")),
            Some(o) => return Err(format!("mkinitcpio -p {b} exited {}", o.code)),
            None => return Err("mkinitcpio not available".into()),
        }
    }
    Ok(done)
}

fn pacman_keyring(sys: &dyn System) -> Outcome {
    let Some(o) = sys.run("pacman-key", &["--list-keys"]) else {
        return Outcome::Skipped("pacman-key not available".into());
    };
    if o.code != 0 {
        return Outcome::Skipped("pacman-key could not read the keyring".into());
    }
    let expired: Vec<String> = lines(&o.stdout)
        .into_iter()
        .filter(|l| l.contains("[expired]"))
        .collect();
    if expired.is_empty() {
        return Outcome::Ok;
    }
    issue(
        format!("{} expired key(s) in the pacman keyring", expired.len()),
        "Refresh: pacman -S archlinux-keyring samurai-keyring (or: pacman-key --refresh-keys)",
        expired.into_iter().take(10).collect(),
    )
}

fn security_listening(sys: &dyn System) -> Outcome {
    let Some(o) = sys.run("ss", &["-tlnH"]) else {
        return Outcome::Skipped("ss not available".into());
    };
    let open: Vec<String> = lines(&o.stdout)
        .iter()
        .filter_map(|l| l.split_whitespace().nth(3).map(str::to_string))
        .filter(|a| !(a.starts_with("127.") || a.starts_with("[::1]") || a.starts_with("::1")))
        .collect();
    if open.is_empty() {
        return Outcome::Ok;
    }
    issue(
        format!("{} TCP listener(s) reachable beyond loopback", open.len()),
        "Expected only for services you run on purpose (e.g. ssh). Find the owner: ss -tlnp",
        open,
    )
}

fn drivers_microcode(sys: &dyn System) -> Outcome {
    let Some(cpu) = sys.read("/proc/cpuinfo") else {
        return Outcome::Skipped("/proc/cpuinfo unreadable".into());
    };
    let pkg = if cpu.contains("GenuineIntel") {
        "intel-ucode"
    } else if cpu.contains("AuthenticAMD") {
        "amd-ucode"
    } else {
        return Outcome::Skipped("CPU vendor has no microcode package here".into());
    };
    let have = match installed(sys) {
        Ok(h) => h,
        Err(skip) => return skip,
    };
    if have.iter().any(|h| h == pkg) {
        return Outcome::Ok;
    }
    issue(
        format!("{pkg} is not installed"),
        format!("CPU microcode fixes need it. Install: pacman -S {pkg}"),
        vec![pkg.to_string()],
    )
}

fn config_shell(sys: &dyn System) -> Outcome {
    let (Some(passwd), Some(shells)) = (sys.read("/etc/passwd"), sys.read("/etc/shells")) else {
        return Outcome::Skipped("/etc/passwd or /etc/shells unreadable".into());
    };
    let allowed = lines(&shells);
    let bad: Vec<String> = passwd
        .lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split(':').collect();
            let (name, uid, shell) = (f.first()?, f.get(2)?.parse::<u32>().ok()?, f.get(6)?);
            let human = uid == 0 || uid >= 1000;
            let nologin =
                shell.ends_with("/nologin") || shell.ends_with("/false") || shell.is_empty();
            let ok = nologin || (allowed.iter().any(|a| a == shell) && sys.exists(shell));
            (human && !ok).then(|| format!("{name}: {shell}"))
        })
        .collect();
    if bad.is_empty() {
        return Outcome::Ok;
    }
    issue(
        "a login shell is missing or not listed in /etc/shells",
        "Reset it: chsh -s /bin/bash <user>",
        bad,
    )
}

fn root_is_btrfs(sys: &dyn System) -> Option<bool> {
    sys.run("findmnt", &["-no", "FSTYPE", "/"])
        .map(|o| o.stdout.trim() == "btrfs")
}

fn storage_btrfs_errors(sys: &dyn System) -> Outcome {
    match root_is_btrfs(sys) {
        None => return Outcome::Skipped("findmnt not available".into()),
        Some(false) => return Outcome::Skipped("root is not btrfs".into()),
        Some(true) => {}
    }
    let Some(o) = sys.run("btrfs", &["device", "stats", "/"]) else {
        return Outcome::Skipped("btrfs-progs not installed".into());
    };
    if o.code != 0 {
        return Outcome::Skipped("btrfs device stats needs root".into());
    }
    let bad: Vec<String> = lines(&o.stdout)
        .into_iter()
        .filter(|l| l.split_whitespace().last().is_some_and(|n| n != "0"))
        .collect();
    if bad.is_empty() {
        return Outcome::Ok;
    }
    issue(
        "btrfs reports device errors",
        "The disk may be failing: back up now, then check SMART (smartctl -a)",
        bad,
    )
}

fn storage_btrfs_timers(sys: &dyn System) -> Outcome {
    match root_is_btrfs(sys) {
        None => return Outcome::Skipped("findmnt not available".into()),
        Some(false) => return Outcome::Skipped("root is not btrfs".into()),
        Some(true) => {}
    }
    let timers = ["fstrim.timer", "btrfs-scrub@-.timer"];
    let Some(o) = sys.run("systemctl", &["is-enabled", timers[0], timers[1]]) else {
        return Outcome::Skipped("systemctl not available".into());
    };
    let states = lines(&o.stdout);
    let off: Vec<String> = timers
        .iter()
        .enumerate()
        .filter(|(i, _)| states.get(*i).map(String::as_str) != Some("enabled"))
        .map(|(i, t)| format!("{t}: {}", states.get(i).map_or("unknown", String::as_str)))
        .collect();
    if off.is_empty() {
        return Outcome::Ok;
    }
    issue(
        "btrfs maintenance timers are not enabled",
        "Enable: systemctl enable --now fstrim.timer btrfs-scrub@-.timer",
        off,
    )
}

fn fix_btrfs_timers(sys: &dyn System) -> Result<Vec<String>, String> {
    enable_now(
        sys,
        &["fstrim.timer", "btrfs-scrub@-.timer"],
        "enabled fstrim and btrfs scrub timers",
    )
}

macro_rules! check {
    ($id:literal, $cat:ident, $sev:ident, $risk:ident, $f:ident) => {
        Check {
            id: $id,
            category: Category::$cat,
            severity: Severity::$sev,
            fix_risk: FixRisk::$risk,
            detect: $f,
            fix: None,
        }
    };
    ($id:literal, $cat:ident, $sev:ident, $risk:ident, $f:ident, fix = $fix:ident) => {
        Check {
            id: $id,
            category: Category::$cat,
            severity: Severity::$sev,
            fix_risk: FixRisk::$risk,
            detect: $f,
            fix: Some($fix),
        }
    };
}

pub const ALL: &[Check] = &[
    check!("packages.core", Packages, Error, Safe, packages_core),
    check!(
        "packages.packagekit",
        Packages,
        Warning,
        NeedsConfirm,
        packages_packagekit
    ),
    check!("pacman.conf", Packages, Error, NeedsConfirm, pacman_conf),
    check!("pacman.pacnew", Packages, Info, ManualOnly, pacman_pacnew),
    check!(
        "boot.pending-reboot",
        Boot,
        Warning,
        ManualOnly,
        boot_pending_reboot
    ),
    check!(
        "storage.disk-free",
        Storage,
        Warning,
        ManualOnly,
        storage_disk_free
    ),
    check!(
        "snapshots.services",
        Snapshots,
        Warning,
        Safe,
        snapshots_services,
        fix = fix_snapper_cleanup
    ),
    check!(
        "services.failed",
        Services,
        Error,
        NeedsConfirm,
        services_failed
    ),
    check!(
        "services.time-sync",
        Services,
        Warning,
        Safe,
        services_time_sync,
        fix = fix_time_sync
    ),
    check!(
        "security.firewall",
        Security,
        Warning,
        Safe,
        security_firewall,
        fix = fix_firewall
    ),
    check!("hardware.ram", Hardware, Info, Safe, hardware_ram),
    check!(
        "boot.kernels-present",
        Boot,
        Critical,
        ManualOnly,
        boot_kernels_present
    ),
    check!(
        "boot.initramfs",
        Boot,
        Critical,
        Safe,
        boot_initramfs,
        fix = fix_initramfs
    ),
    check!(
        "pacman.keyring",
        Packages,
        Warning,
        ManualOnly,
        pacman_keyring
    ),
    check!(
        "security.listening",
        Security,
        Info,
        ManualOnly,
        security_listening
    ),
    check!(
        "drivers.microcode",
        Drivers,
        Warning,
        ManualOnly,
        drivers_microcode
    ),
    check!("config.shell", Config, Warning, ManualOnly, config_shell),
    check!(
        "storage.btrfs-errors",
        Storage,
        Error,
        ManualOnly,
        storage_btrfs_errors
    ),
    check!(
        "storage.btrfs-timers",
        Storage,
        Warning,
        Safe,
        storage_btrfs_timers,
        fix = fix_btrfs_timers
    ),
];

#[cfg(test)]
mod tests {
    use super::super::system::fake::Fake;
    use super::*;

    fn is_issue(o: &Outcome) -> &Issue {
        match o {
            Outcome::Issue(i) => i,
            other => panic!("expected issue, got {other:?}"),
        }
    }

    #[test]
    fn failed_units() {
        let ok = Fake::default().cmd("systemctl --failed --no-legend --plain", 0, "");
        assert_eq!(services_failed(&ok), Outcome::Ok);
        let bad = Fake::default().cmd(
            "systemctl --failed --no-legend --plain",
            0,
            "foo.service loaded failed failed Foo\nbar.mount loaded failed failed Bar\n",
        );
        let i = is_issue(&services_failed(&bad)).clone();
        assert_eq!(i.evidence, vec!["foo.service", "bar.mount"]);
        assert!(matches!(
            services_failed(&Fake::default()),
            Outcome::Skipped(_)
        ));
    }

    #[test]
    fn time_sync() {
        let c = "timedatectl show -p NTPSynchronized --value";
        assert_eq!(
            services_time_sync(&Fake::default().cmd(c, 0, "yes\n")),
            Outcome::Ok
        );
        is_issue(&services_time_sync(&Fake::default().cmd(c, 0, "no\n")));
        assert!(matches!(
            services_time_sync(&Fake::default()),
            Outcome::Skipped(_)
        ));
        assert!(matches!(
            services_time_sync(&Fake::default().cmd(c, 1, "")),
            Outcome::Skipped(_)
        ));
    }

    #[test]
    fn firewall() {
        let c = "systemctl is-active firewalld";
        assert_eq!(
            security_firewall(&Fake::default().cmd(c, 0, "active\n")),
            Outcome::Ok
        );
        let i = is_issue(&security_firewall(&Fake::default().cmd(c, 3, "inactive\n"))).clone();
        assert!(i.evidence[0].contains("inactive"));
        assert!(matches!(
            security_firewall(&Fake::default()),
            Outcome::Skipped(_)
        ));
    }

    #[test]
    fn snapper_cleanup_timer() {
        let c = "systemctl is-enabled snapper-cleanup.timer";
        assert!(matches!(
            snapshots_services(&Fake::default()),
            Outcome::Skipped(_)
        ));
        let base = || Fake::default().path("/usr/bin/snapper");
        assert_eq!(
            snapshots_services(&base().cmd(c, 0, "enabled\n")),
            Outcome::Ok
        );
        let i = is_issue(&snapshots_services(&base().cmd(c, 1, "disabled\n"))).clone();
        assert_eq!(i.evidence, vec!["snapper-cleanup.timer: disabled"]);
        // the timeline timer is deliberately off (spec 06): never required
        assert!(
            !format!("{:?}", snapshots_services(&base().cmd(c, 0, "enabled\n")))
                .contains("timeline")
        );
    }

    #[test]
    fn pending_reboot() {
        let base = || Fake::default().file("/proc/sys/kernel/osrelease", "6.12.1-arch1-1\n");
        assert_eq!(
            boot_pending_reboot(&base().path("/usr/lib/modules/6.12.1-arch1-1")),
            Outcome::Ok
        );
        let i = is_issue(&boot_pending_reboot(&base())).clone();
        assert!(i.evidence[0].contains("6.12.1-arch1-1"));
        assert!(matches!(
            boot_pending_reboot(&Fake::default()),
            Outcome::Skipped(_)
        ));
    }

    #[test]
    fn disk_free() {
        let c = "df -P -x tmpfs -x devtmpfs -x squashfs -x overlay";
        let hdr = "Filesystem 1024-blocks Used Available Capacity Mounted on\n";
        let ok = format!("{hdr}/dev/vda2 100 50 50 50% /\n");
        assert_eq!(
            storage_disk_free(&Fake::default().cmd(c, 0, &ok)),
            Outcome::Ok
        );
        let bad = format!("{hdr}/dev/vda2 100 95 5 95% /\n/dev/vda1 100 10 90 10% /boot\n");
        let i = is_issue(&storage_disk_free(&Fake::default().cmd(c, 0, &bad))).clone();
        assert_eq!(i.evidence, vec!["/ 95% full"]);
    }

    #[test]
    fn core_and_packagekit() {
        let all = Fake::default().cmd(
            "pacman -Qq",
            0,
            "base\nlinux-firmware\nsamurai-tools\nbash\n",
        );
        assert_eq!(packages_core(&all), Outcome::Ok);
        assert_eq!(packages_packagekit(&all), Outcome::Ok);
        let some = Fake::default().cmd("pacman -Qq", 0, "base\npackagekit\n");
        assert_eq!(
            is_issue(&packages_core(&some)).evidence,
            vec!["linux-firmware", "samurai-tools"]
        );
        assert_eq!(
            is_issue(&packages_packagekit(&some)).evidence,
            vec!["packagekit"]
        );
        assert!(matches!(
            packages_core(&Fake::default()),
            Outcome::Skipped(_)
        ));
        assert!(matches!(
            packages_core(&Fake::default().cmd("pacman -Qq", 1, "")),
            Outcome::Skipped(_)
        ));
    }

    #[test]
    fn pacnew() {
        let c = "find /etc -xdev ( -name *.pacnew -o -name *.pacsave )";
        assert_eq!(pacman_pacnew(&Fake::default().cmd(c, 0, "")), Outcome::Ok);
        let i = is_issue(&pacman_pacnew(&Fake::default().cmd(
            c,
            0,
            "/etc/foo.pacnew\n",
        )))
        .clone();
        assert_eq!(i.evidence, vec!["/etc/foo.pacnew"]);
    }

    #[test]
    fn pacman_conf_rules() {
        let good = "[options]\nSigLevel = Required DatabaseOptional\n[core]\nInclude = x\n[extra]\nInclude = x\n[samurai]\nSigLevel = Required DatabaseRequired\n";
        assert_eq!(
            pacman_conf(&Fake::default().file("/etc/pacman.conf", good)),
            Outcome::Ok
        );
        let bad = "[options]\nSigLevel = Never\n[core]\n#[extra]\n";
        let i = is_issue(&pacman_conf(&Fake::default().file("/etc/pacman.conf", bad))).clone();
        assert_eq!(i.evidence.len(), 3, "{:?}", i.evidence);
        assert!(i.evidence.iter().any(|e| e.contains("[extra]")));
        assert!(i.evidence.iter().any(|e| e.contains("[samurai]")));
        assert!(i.evidence.iter().any(|e| e.contains("SigLevel = Never")));
    }

    #[test]
    fn zram() {
        assert_eq!(
            hardware_ram(&Fake::default().path("/sys/block/zram0")),
            Outcome::Ok
        );
        assert_eq!(
            hardware_ram(&Fake::default().file("/proc/swaps", "Filename\n/dev/zram0 partition\n")),
            Outcome::Ok
        );
        is_issue(&hardware_ram(
            &Fake::default().file("/proc/swaps", "Filename\n"),
        ));
        assert!(matches!(
            hardware_ram(&Fake::default()),
            Outcome::Skipped(_)
        ));
    }

    #[test]
    fn fixes_enable_units_and_report_errors() {
        let ok = Fake::default().cmd("systemctl enable --now firewalld", 0, "");
        assert_eq!(
            fix_firewall(&ok).unwrap(),
            vec!["enabled and started firewalld"]
        );
        let bad = Fake::default().cmd("systemctl enable --now firewalld", 1, "");
        assert!(fix_firewall(&bad).unwrap_err().contains("exited 1"));
        assert!(fix_firewall(&Fake::default()).is_err());
        let t = Fake::default().cmd("systemctl enable --now snapper-cleanup.timer", 0, "");
        assert!(fix_snapper_cleanup(&t).is_ok());
    }

    fn kernels(extra: Fake) -> Fake {
        extra
            .dir("/usr/lib/modules", &["6.12.1-arch1-1", "6.12.2-zen1-1"])
            .file("/usr/lib/modules/6.12.1-arch1-1/pkgbase", "linux\n")
            .file("/usr/lib/modules/6.12.2-zen1-1/pkgbase", "linux-zen\n")
    }

    #[test]
    fn kernel_images_and_initramfs() {
        let all = kernels(Fake::default())
            .path("/boot/vmlinuz-linux")
            .path("/boot/vmlinuz-linux-zen")
            .path("/boot/initramfs-linux.img")
            .path("/boot/initramfs-linux-zen.img");
        assert_eq!(boot_kernels_present(&all), Outcome::Ok);
        assert_eq!(boot_initramfs(&all), Outcome::Ok);
        assert!(matches!(
            boot_kernels_present(&Fake::default()),
            Outcome::Skipped(_)
        ));

        let no_initrd = kernels(Fake::default())
            .path("/boot/vmlinuz-linux")
            .path("/boot/vmlinuz-linux-zen")
            .path("/boot/initramfs-linux.img");
        assert_eq!(
            is_issue(&boot_initramfs(&no_initrd)).evidence,
            vec!["/boot/initramfs-linux-zen.img is missing"]
        );
        assert_eq!(boot_kernels_present(&no_initrd), Outcome::Ok);

        let no_kernel = kernels(Fake::default()).path("/boot/vmlinuz-linux");
        assert_eq!(
            is_issue(&boot_kernels_present(&no_kernel)).evidence,
            vec!["/boot/vmlinuz-linux-zen is missing"]
        );
        // a kernel without an image is the kernels-present check's job, not an initramfs one
        assert_eq!(boot_initramfs(&no_kernel), is_initrd_issue_for(&["linux"]));
    }

    fn is_initrd_issue_for(bases: &[&str]) -> Outcome {
        issue(
            "an installed kernel has no initramfs",
            "The system will not boot that kernel. Rebuild: mkinitcpio -p <kernel>",
            bases
                .iter()
                .map(|b| format!("/boot/initramfs-{b}.img is missing"))
                .collect(),
        )
    }

    #[test]
    fn initramfs_fix_rebuilds_missing_only() {
        let sys = kernels(Fake::default())
            .path("/boot/vmlinuz-linux")
            .path("/boot/vmlinuz-linux-zen")
            .path("/boot/initramfs-linux.img")
            .cmd("mkinitcpio -p linux-zen", 0, "");
        assert_eq!(
            fix_initramfs(&sys).unwrap(),
            vec!["rebuilt initramfs for linux-zen"]
        );
        assert_eq!(sys.calls(), vec!["mkinitcpio -p linux-zen"]);
        let failing = kernels(Fake::default())
            .path("/boot/vmlinuz-linux")
            .path("/boot/vmlinuz-linux-zen")
            .cmd("mkinitcpio -p linux", 1, "");
        assert!(fix_initramfs(&failing).unwrap_err().contains("exited 1"));
    }

    #[test]
    fn keyring_expired_keys() {
        let ok = Fake::default().cmd(
            "pacman-key --list-keys",
            0,
            "pub   rsa4096 2020 [SC]\nuid [ full ] Foo\n",
        );
        assert_eq!(pacman_keyring(&ok), Outcome::Ok);
        let bad = Fake::default().cmd(
            "pacman-key --list-keys",
            0,
            "uid [expired] Old Key <o@x>\nuid [ full ] Foo\n",
        );
        assert_eq!(is_issue(&pacman_keyring(&bad)).evidence.len(), 1);
        assert!(matches!(
            pacman_keyring(&Fake::default()),
            Outcome::Skipped(_)
        ));
        assert!(matches!(
            pacman_keyring(&Fake::default().cmd("pacman-key --list-keys", 2, "")),
            Outcome::Skipped(_)
        ));
    }

    #[test]
    fn listening_ports_ignore_loopback() {
        let ok = Fake::default().cmd(
            "ss -tlnH",
            0,
            "LISTEN 0 4096 127.0.0.1:631 0.0.0.0:*\nLISTEN 0 4096 [::1]:631 [::]:*\nLISTEN 0 128 127.0.0.53%lo:53 0.0.0.0:*\n",
        );
        assert_eq!(security_listening(&ok), Outcome::Ok);
        let bad = Fake::default().cmd("ss -tlnH", 0, "LISTEN 0 128 0.0.0.0:22 0.0.0.0:*\nLISTEN 0 128 [::]:22 [::]:*\nLISTEN 0 1 127.0.0.1:1 0.0.0.0:*\n");
        assert_eq!(
            is_issue(&security_listening(&bad)).evidence,
            vec!["0.0.0.0:22", "[::]:22"]
        );
        assert!(matches!(
            security_listening(&Fake::default()),
            Outcome::Skipped(_)
        ));
    }

    #[test]
    fn microcode_matches_cpu_vendor() {
        let intel = || Fake::default().file("/proc/cpuinfo", "vendor_id : GenuineIntel\n");
        assert_eq!(
            drivers_microcode(&intel().cmd("pacman -Qq", 0, "base\nintel-ucode\n")),
            Outcome::Ok
        );
        assert_eq!(
            is_issue(&drivers_microcode(&intel().cmd(
                "pacman -Qq",
                0,
                "base\namd-ucode\n"
            )))
            .evidence,
            vec!["intel-ucode"]
        );
        let amd = Fake::default()
            .file("/proc/cpuinfo", "vendor_id : AuthenticAMD\n")
            .cmd("pacman -Qq", 0, "amd-ucode\n");
        assert_eq!(drivers_microcode(&amd), Outcome::Ok);
        assert!(matches!(
            drivers_microcode(&Fake::default().file("/proc/cpuinfo", "vendor_id : Other\n")),
            Outcome::Skipped(_)
        ));
    }

    #[test]
    fn login_shells_must_be_listed_and_exist() {
        let shells = "# comment\n/bin/bash\n/usr/bin/fish\n";
        let ok = Fake::default()
            .file("/etc/shells", shells)
            .path("/bin/bash")
            .path("/usr/bin/fish")
            .file("/etc/passwd", "root:x:0:0::/root:/bin/bash\nbob:x:1000:1000::/home/bob:/usr/bin/fish\ndaemon:x:2:2::/:/usr/bin/nologin\nsvc:x:5:5::/:/bin/zsh\n");
        assert_eq!(
            config_shell(&ok),
            Outcome::Ok,
            "system users (uid 1..999) are not checked"
        );
        let bad = Fake::default()
            .file("/etc/shells", shells)
            .path("/bin/bash")
            .file("/etc/passwd", "root:x:0:0::/root:/bin/bash\nbob:x:1000:1000::/home/bob:/usr/bin/fish\nann:x:1001:1001::/home/ann:/bin/zsh\n");
        assert_eq!(
            is_issue(&config_shell(&bad)).evidence,
            vec!["bob: /usr/bin/fish", "ann: /bin/zsh"]
        );
    }

    #[test]
    fn btrfs_checks_need_btrfs_root() {
        let fs = |t: &str| Fake::default().cmd("findmnt -no FSTYPE /", 0, t);
        assert!(matches!(
            storage_btrfs_errors(&fs("ext4\n")),
            Outcome::Skipped(_)
        ));
        assert!(matches!(
            storage_btrfs_timers(&fs("ext4\n")),
            Outcome::Skipped(_)
        ));
        assert!(matches!(
            storage_btrfs_errors(&Fake::default()),
            Outcome::Skipped(_)
        ));

        let clean = "[/dev/dm-0].write_io_errs    0\n[/dev/dm-0].read_io_errs     0\n";
        assert_eq!(
            storage_btrfs_errors(&fs("btrfs\n").cmd("btrfs device stats /", 0, clean)),
            Outcome::Ok
        );
        let dirty = "[/dev/dm-0].write_io_errs    0\n[/dev/dm-0].corruption_errs  3\n";
        assert_eq!(
            is_issue(&storage_btrfs_errors(&fs("btrfs\n").cmd(
                "btrfs device stats /",
                0,
                dirty
            )))
            .evidence,
            vec!["[/dev/dm-0].corruption_errs  3"]
        );
        assert!(matches!(
            storage_btrfs_errors(&fs("btrfs\n").cmd("btrfs device stats /", 1, "")),
            Outcome::Skipped(_)
        ));

        let c = "systemctl is-enabled fstrim.timer btrfs-scrub@-.timer";
        assert_eq!(
            storage_btrfs_timers(&fs("btrfs\n").cmd(c, 0, "enabled\nenabled\n")),
            Outcome::Ok
        );
        let i = is_issue(&storage_btrfs_timers(&fs("btrfs\n").cmd(
            c,
            1,
            "enabled\ndisabled\n",
        )))
        .clone();
        assert_eq!(i.evidence, vec!["btrfs-scrub@-.timer: disabled"]);
        let f = Fake::default().cmd(
            "systemctl enable --now fstrim.timer btrfs-scrub@-.timer",
            0,
            "",
        );
        assert!(fix_btrfs_timers(&f).is_ok());
    }
}

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
    if !sys.exists("/usr/bin/snapper") {
        return Outcome::Skipped("snapper is not installed".into());
    }
    let timers = ["snapper-timeline.timer", "snapper-cleanup.timer"];
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
        "snapper timers are not enabled",
        "Enable them: systemctl enable --now snapper-timeline.timer snapper-cleanup.timer",
        off,
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

fn fix_snapper_timers(sys: &dyn System) -> Result<Vec<String>, String> {
    enable_now(
        sys,
        &["snapper-timeline.timer", "snapper-cleanup.timer"],
        "enabled snapper timeline and cleanup timers",
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
        fix = fix_snapper_timers
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
    fn snapper_timers() {
        let c = "systemctl is-enabled snapper-timeline.timer snapper-cleanup.timer";
        assert!(matches!(
            snapshots_services(&Fake::default()),
            Outcome::Skipped(_)
        ));
        let base = || Fake::default().path("/usr/bin/snapper");
        assert_eq!(
            snapshots_services(&base().cmd(c, 0, "enabled\nenabled\n")),
            Outcome::Ok
        );
        let i = is_issue(&snapshots_services(&base().cmd(
            c,
            1,
            "enabled\ndisabled\n",
        )))
        .clone();
        assert_eq!(i.evidence, vec!["snapper-cleanup.timer: disabled"]);
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
        let t = Fake::default().cmd(
            "systemctl enable --now snapper-timeline.timer snapper-cleanup.timer",
            0,
            "",
        );
        assert!(fix_snapper_timers(&t).is_ok());
    }
}

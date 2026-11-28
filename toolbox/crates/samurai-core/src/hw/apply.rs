//! Carrying out a [`Plan`]: snapshot, download everything first, change, verify, and put the old
//! driver back when anything fails.
//!
//! chwd is only ever called as `chwd -i <profile>`. Its `--force` and `-r` paths remove packages
//! with `pacman -Rdd` (including DKMS and kernel headers) before anything replaces them, so a
//! reinstall here drops chwd's record instead, and a branch switch removes exactly the NVIDIA
//! packages after the replacements are in the cache.

use crate::fix::DB_LOCK;
use crate::health::system::System;

use super::plan::{self, Branch, Plan, Status, Step};
use super::profiles;

/// Files the NVIDIA profiles create; saved before a change and restored with the old driver.
pub const DROPINS: [&str; 3] = [
    "/etc/mkinitcpio.conf.d/10-chwd-kms.conf",
    "/etc/mkinitcpio.conf.d/11-samurai-nvidia.conf",
    "/etc/modprobe.d/chwd-nvidia.conf",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotPolicy {
    /// Abort when no snapshot can be taken (the default).
    Require,
    /// `--no-snapshot`, or a caller that already took one.
    Skip,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub done: Vec<String>,
    pub snapshot: Option<String>,
    /// A driver changed: the running system still uses the old one.
    pub reboot: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Failure {
    pub reason: String,
    /// What happened to the machine afterwards.
    pub outcome: String,
}

impl Failure {
    fn untouched(reason: impl Into<String>) -> Failure {
        Failure {
            reason: reason.into(),
            outcome: "nothing was changed".into(),
        }
    }
}

fn pacman(sys: &dyn System, op: &[&str], pkgs: &[String]) -> Result<(), String> {
    let mut args: Vec<&str> = op.to_vec();
    args.extend(pkgs.iter().map(String::as_str));
    match sys.run_visible("pacman", &args) {
        Some(0) => Ok(()),
        Some(c) => Err(format!("pacman {} exited {c}", op.join(" "))),
        None => Err("pacman is not available".into()),
    }
}

fn snapshot(sys: &dyn System) -> Result<String, String> {
    if !sys.exists("/usr/bin/snapper") {
        return Err("snapper is not installed".into());
    }
    let args = [
        "-c",
        "root",
        "create",
        "--print-number",
        "--cleanup-algorithm",
        "number",
        "--description",
        "samurai hw",
    ];
    match sys.run("snapper", &args) {
        Some(o) if o.code == 0 => Ok(o.stdout.trim().to_string()),
        Some(o) => Err(format!("snapper exited {}", o.code)),
        None => Err("snapper could not be started".into()),
    }
}

fn union<'a>(lists: impl Iterator<Item = &'a Vec<String>>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for name in lists.flatten() {
        if !out.contains(name) {
            out.push(name.clone());
        }
    }
    out
}

/// Records and drop-ins as they were, to put back on failure.
struct Saved(Vec<(String, Option<String>)>);

impl Saved {
    fn take(sys: &dyn System, records: &[(String, String)]) -> Saved {
        let mut paths: Vec<String> = DROPINS.iter().map(|s| (*s).to_string()).collect();
        for (_, path) in records {
            if !paths.contains(path) {
                paths.push(path.clone());
            }
        }
        Saved(
            paths
                .into_iter()
                .map(|p| (p.clone(), sys.read(&p)))
                .collect(),
        )
    }

    fn restore(&self, sys: &dyn System) -> Vec<String> {
        let mut errors = Vec::new();
        for (path, content) in &self.0 {
            let result = match content {
                Some(c) => sys.write(path, c),
                None if sys.exists(path) => sys.remove(path),
                None => Ok(()),
            };
            errors.extend(result.err());
        }
        errors
    }
}

/// Forget that chwd installed `name`, so `chwd -i` runs again without removing anything.
fn drop_record(sys: &dyn System, name: &str) -> Result<(), String> {
    for (record, path) in profiles::records(sys) {
        if record == name {
            let text = sys.read(&path).ok_or(format!("cannot read {path}"))?;
            sys.write(&path, &profiles::without_record(&text, name))?;
        }
    }
    Ok(())
}

fn verify(sys: &dyn System, step: &Step) -> Result<(), String> {
    let missing = plan::unsatisfied(sys, &step.packages)?;
    let core: Vec<&String> = missing
        .iter()
        .filter(|p| !p.ends_with("-headers"))
        .collect();
    // an unchanged provider satisfies the module part under another package name
    let relevant: Vec<&str> = core
        .iter()
        .filter(|p| plan::module_branch(p).is_none() && p.as_str() != "dkms")
        .map(|p| p.as_str())
        .collect();
    if !relevant.is_empty() {
        return Err(format!("still not installed: {}", relevant.join(", ")));
    }
    if step.branch.is_some() {
        let bare = plan::kernels_without_module(sys);
        if !bare.is_empty() {
            return Err(format!(
                "no nvidia module for kernel(s): {}",
                bare.join(", ")
            ));
        }
        if plan::kernel_releases(sys).is_empty() {
            return Err("no installed kernel found to check the nvidia module".into());
        }
    }
    Ok(())
}

fn run_step(sys: &dyn System, step: &Step) -> Result<(), String> {
    if !step.remove.is_empty() {
        pacman(sys, &["-Rdd", "--noconfirm"], &step.remove)?;
    }
    if step.status == Status::Switch {
        // the old branch's record and module list must not survive the switch
        for (name, _) in profiles::records(sys) {
            if name.starts_with("nvidia-") || name == "nouveau" {
                drop_record(sys, &name)?;
            }
        }
        if step.branch.is_none() {
            for path in DROPINS.iter().filter(|p| sys.exists(p)) {
                sys.remove(path)?;
            }
            for param in ["nvidia_drm.modeset", "nvidia_drm.fbdev"] {
                let _ = sys.run("samurai", &["boot", "set", "param", "remove", param]);
            }
        }
    }
    drop_record(sys, &step.profile)?;
    match sys.run_visible("chwd", &["-i", &step.profile]) {
        Some(0) => verify(sys, step),
        Some(c) => Err(format!("chwd -i {} exited {c}", step.profile)),
        None => Err("chwd is not installed".into()),
    }
}

/// Put the previous NVIDIA state back. Returns what the machine is left with.
fn restore(sys: &dyn System, before: &[String], saved: &Saved, snapshot: Option<&str>) -> String {
    let rollback = match snapshot {
        Some(n) => format!("roll back with: samurai snap rollback {n}"),
        None => "no snapshot was taken".into(),
    };
    let previous = plan::userspace_and_modules(before);
    let mut errors = Vec::new();
    match plan::installed_packages(sys) {
        Ok(now) => {
            let added: Vec<String> = plan::userspace_and_modules(&now)
                .into_iter()
                .filter(|p| !before.contains(p))
                .collect();
            // nvidia-utils blacklists nouveau: leftovers without a module mean no driver at all
            if !added.is_empty() {
                errors.extend(pacman(sys, &["-Rdd", "--noconfirm"], &added).err());
            }
            let gone: Vec<String> = previous
                .iter()
                .filter(|p| !now.contains(p) || !added.is_empty())
                .cloned()
                .collect();
            if !gone.is_empty() {
                errors.extend(pacman(sys, &["-S", "--noconfirm"], &gone).err());
            }
        }
        Err(e) => errors.push(e),
    }
    errors.extend(saved.restore(sys));
    if sys.run_visible("mkinitcpio", &["-P"]) != Some(0) {
        errors.push("mkinitcpio -P failed".into());
    }
    if !previous.is_empty() {
        for param in ["nvidia_drm.modeset=1", "nvidia_drm.fbdev=1"] {
            let _ = sys.run("samurai", &["boot", "set", "param", "add", param]);
        }
    }
    if errors.is_empty() {
        let what = if previous.is_empty() {
            "the new driver packages were removed again; the open drivers stay in use"
        } else {
            "the previous driver packages were restored"
        };
        format!("{what} (snapshot: {})", snapshot.unwrap_or("none"))
    } else {
        format!(
            "RESTORE FAILED, do not reboot: {}; {rollback}",
            errors.join("; ")
        )
    }
}

pub fn apply(sys: &dyn System, plan: &Plan, policy: SnapshotPolicy) -> Result<Report, Failure> {
    if !plan.refusals.is_empty() {
        return Err(Failure::untouched(plan.refusals.join("; ")));
    }
    let steps: Vec<&Step> = plan.steps.iter().filter(|s| s.acts()).collect();
    let mut report = Report::default();
    if steps.is_empty() {
        return Ok(report);
    }
    if sys.exists(DB_LOCK) {
        return Err(Failure::untouched(
            "pacman is locked: another package operation runs (or: samurai fix lock)",
        ));
    }
    let before = plan::installed_packages(sys).map_err(Failure::untouched)?;
    if policy == SnapshotPolicy::Require {
        let number = snapshot(sys).map_err(|e| {
            Failure::untouched(format!(
                "cannot take a snapshot ({e}); --no-snapshot proceeds without one"
            ))
        })?;
        report.done.push(format!("snapshot {number} taken"));
        report.snapshot = Some(number);
    }
    // everything must be in the cache before anything is removed: no -y, never a partial upgrade
    let wanted = union(steps.iter().map(|s| &s.install));
    if !wanted.is_empty() {
        pacman(sys, &["-Sw", "--noconfirm"], &wanted).map_err(|e| {
            Failure::untouched(format!(
                "cannot download the new packages ({e}); update first: samurai update"
            ))
        })?;
    }
    let outgoing = union(steps.iter().map(|s| &s.remove));
    if !outgoing.is_empty() {
        pacman(sys, &["-Sw", "--noconfirm"], &outgoing).map_err(|e| {
            Failure::untouched(format!(
                "cannot download the current driver for a way back ({e})"
            ))
        })?;
    }
    let saved = Saved::take(sys, &profiles::records(sys));
    for step in steps {
        if let Err(reason) = run_step(sys, step) {
            let outcome = if step.branch.is_some() || step.status == Status::Switch {
                restore(sys, &before, &saved, report.snapshot.as_deref())
            } else {
                saved.restore(sys);
                "the other drivers are unchanged".into()
            };
            return Err(Failure {
                reason: format!("{}: {reason}", step.profile),
                outcome,
            });
        }
        report.done.push(format!("applied {}", step.profile));
        report.reboot |= step.graphics
            && (step.status == Status::Switch
                || step.status == Status::Missing
                || step.branch == Some(Branch::Open)
                || step.branch == Some(Branch::Legacy580));
    }
    Ok(report)
}

/// Remove a profile whose hardware is gone. A profile that matches a present device is refused:
/// removing it would take that device's driver away.
pub fn remove(sys: &dyn System, name: &str, policy: SnapshotPolicy) -> Result<Report, Failure> {
    let db = profiles::database(sys);
    let Some(profile) = db.iter().find(|p| p.name == name) else {
        return Err(Failure::untouched(format!("unknown profile: {name}")));
    };
    let plan = plan::build(sys, &plan::Request::default()).map_err(Failure::untouched)?;
    if let Some(offer) = plan
        .offers
        .iter()
        .find(|o| o.profiles.iter().any(|p| p == name))
    {
        return Err(Failure::untouched(format!(
            "{name} matches the present device {}; switch to another profile with `samurai hw apply --profile <name>` instead",
            offer.addr
        )));
    }
    let installed = plan::installed_packages(sys).map_err(Failure::untouched)?;
    let needed = union(plan.steps.iter().map(|s| &s.packages));
    let mut candidates = profile.packages.clone();
    if let Some(branch) = plan::profile_branch(profile) {
        if plan
            .devices
            .iter()
            .any(super::devices::Device::is_nvidia_display)
        {
            return Err(Failure::untouched(format!(
                "an NVIDIA card is present; not removing {name}"
            )));
        }
        candidates.extend(
            installed
                .iter()
                .filter(|p| plan::module_branch(p) == Some(branch))
                .cloned(),
        );
    }
    let outgoing: Vec<String> = candidates
        .into_iter()
        .filter(|p| installed.contains(p) && !needed.contains(p))
        // never take the shared open stack away
        .filter(|p| {
            !matches!(
                p.as_str(),
                "mesa" | "lib32-mesa" | "vulkan-icd-loader" | "lib32-vulkan-icd-loader"
            )
        })
        .collect();
    let recorded = profiles::records(sys).iter().any(|(n, _)| n == name);
    let mut report = Report::default();
    if outgoing.is_empty() && !recorded {
        return Err(Failure::untouched(format!("{name} is not installed")));
    }
    if policy == SnapshotPolicy::Require {
        let number = snapshot(sys).map_err(|e| {
            Failure::untouched(format!(
                "cannot take a snapshot ({e}); --no-snapshot proceeds without one"
            ))
        })?;
        report.snapshot = Some(number);
    }
    if !outgoing.is_empty() {
        // plain -R: pacman refuses when something else depends on one of them
        pacman(sys, &["-R", "--noconfirm"], &outgoing).map_err(Failure::untouched)?;
        report.done.push(format!("removed {}", outgoing.join(" ")));
    }
    drop_record(sys, name).map_err(|e| Failure {
        reason: e,
        outcome: "packages removed, chwd's record remains".into(),
    })?;
    if plan::profile_branch(profile).is_some() {
        for path in DROPINS.iter().filter(|p| sys.exists(p)) {
            let _ = sys.remove(path);
        }
        for param in ["nvidia_drm.modeset", "nvidia_drm.fbdev"] {
            let _ = sys.run("samurai", &["boot", "set", "param", "remove", param]);
        }
        if sys.run_visible("mkinitcpio", &["-P"]) != Some(0) {
            return Err(Failure {
                reason: "mkinitcpio -P failed".into(),
                outcome: "do not reboot; run mkinitcpio -P".into(),
            });
        }
        report.reboot = true;
    }
    report.done.push(format!("profile {name} removed"));
    Ok(report)
}

//! The `drivers.*` checks (spec 05), built on the same plan `samurai hw` applies.
//! `drivers.microcode` lives with the other package checks.

use super::system::System;
use super::{Issue, Outcome};
use crate::boot;
use crate::hw::apply::{self, SnapshotPolicy};
use crate::hw::devices::{self, Device, Generation};
use crate::hw::fingerprint;
use crate::hw::plan::{self, Branch, Status};

fn issue(summary: impl Into<String>, details: impl Into<String>, evidence: Vec<String>) -> Outcome {
    Outcome::Issue(Issue {
        summary: summary.into(),
        details: details.into(),
        evidence,
    })
}

pub fn profile(sys: &dyn System) -> Outcome {
    let plan = match plan::build(sys, &plan::Request::default()) {
        Ok(p) => p,
        Err(why) => return Outcome::Skipped(why),
    };
    if plan.nvidia.branch == Some(Branch::Foreign) {
        return Outcome::Skipped("the NVIDIA driver is managed by hand".into());
    }
    let wrong: Vec<String> = plan
        .steps
        .iter()
        .filter(|s| matches!(s.status, Status::Missing | Status::Switch))
        .map(|s| match s.status {
            Status::Switch => format!(
                "{}: replaces {}",
                s.profile,
                plan.nvidia.branch.map_or("the driver", Branch::as_str)
            ),
            _ => format!("{}: missing {}", s.profile, s.install.join(" ")),
        })
        .collect();
    if wrong.is_empty() {
        return Outcome::Ok;
    }
    let mut evidence = wrong;
    if let Ok(fingerprint::State::Changed { added, removed }) = fingerprint::compare(sys) {
        evidence.push(fingerprint::message(&added, &removed));
    }
    evidence.extend(plan.refusals.iter().map(|r| format!("refused: {r}")));
    issue(
        "the installed drivers differ from the recommended profile",
        "Preview: samurai hw detect. Apply: sudo samurai hw apply (a driver branch change needs --switch-driver)",
        evidence,
    )
}

/// Installs what is missing; a branch switch stays a deliberate `samurai hw apply --switch-driver`.
pub fn fix_profile(sys: &dyn System) -> Result<Vec<String>, String> {
    let plan = plan::build(sys, &plan::Request::default())?;
    // `health --fix` has taken its snapshot already
    apply::apply(sys, &plan, SnapshotPolicy::Skip)
        .map(|r| r.done)
        .map_err(|f| format!("{}; {}", f.reason, f.outcome))
}

/// NVIDIA cards and the installed driver branch, or why a check does not apply.
fn nvidia(sys: &dyn System) -> Result<(Vec<Device>, plan::NvidiaState), Outcome> {
    let cards: Vec<Device> = devices::scan(sys)
        .map_err(Outcome::Skipped)?
        .into_iter()
        .filter(Device::is_nvidia_display)
        .collect();
    if cards.is_empty() {
        return Err(Outcome::Skipped("no NVIDIA graphics card".into()));
    }
    let installed = plan::installed_packages(sys).map_err(Outcome::Skipped)?;
    let state = plan::nvidia_state(&installed);
    if state.branch == Some(Branch::Foreign) {
        return Err(Outcome::Skipped(
            "the NVIDIA driver is managed by hand".into(),
        ));
    }
    Ok((cards, state))
}

pub fn driver_family(sys: &dyn System) -> Outcome {
    let (cards, state) = match nvidia(sys) {
        Ok(v) => v,
        Err(skip) => return skip,
    };
    let wrong: Vec<String> = cards
        .iter()
        .filter_map(|d| {
            let g = Generation::of(&d.name);
            let problem = match state.branch {
                Some(Branch::Open) if !g.open_modules() && g != Generation::Unknown => {
                    "the open modules do not support this card"
                }
                Some(Branch::Legacy580) if g == Generation::Blackwell => {
                    "this card needs the open modules, the 580xx branch is installed"
                }
                None if g.open_modules() => "no NVIDIA driver is installed",
                _ => return None,
            };
            Some(format!("{} {} ({}): {problem}", d.addr, d.name, g.as_str()))
        })
        .collect();
    if wrong.is_empty() {
        return Outcome::Ok;
    }
    issue(
        "an NVIDIA card has no matching driver",
        "Preview: samurai hw detect. Apply: sudo samurai hw apply",
        wrong,
    )
}

pub fn module(sys: &dyn System) -> Outcome {
    let (_, state) = match nvidia(sys) {
        Ok(v) => v,
        Err(skip) => return skip,
    };
    if state.branch.is_none() {
        return Outcome::Skipped("no NVIDIA driver is installed".into());
    }
    if plan::kernel_releases(sys).is_empty() {
        return Outcome::Skipped("no installed kernel found".into());
    }
    let bare = plan::kernels_without_module(sys);
    if !bare.is_empty() {
        return issue(
            "an installed kernel has no nvidia module: do not reboot into it",
            "Rebuild: sudo samurai fix dkms",
            bare,
        );
    }
    if !sys.exists("/sys/module/nvidia") {
        return issue(
            "the nvidia module is built but not loaded",
            "Reboot. If it stays unloaded, check: samurai diag gpu",
            vec!["/sys/module/nvidia is missing".into()],
        );
    }
    Outcome::Ok
}

pub fn fix_module(sys: &dyn System) -> Result<Vec<String>, String> {
    if plan::kernels_without_module(sys).is_empty() {
        return Err("every kernel has the module; a reboot is what is left".into());
    }
    crate::fix::dkms(sys)
}

const PARAMS: [&str; 2] = ["modeset", "fbdev"];

/// Parameters that are off: the loaded module's value wins over the kernel command line.
fn disabled_params(sys: &dyn System) -> Vec<&'static str> {
    let cmdline = sys.read("/proc/cmdline").unwrap_or_default();
    PARAMS
        .into_iter()
        .filter(|p| {
            let loaded = sys.read(&format!("/sys/module/nvidia_drm/parameters/{p}"));
            match loaded {
                Some(v) => !matches!(v.trim(), "Y" | "1"),
                None => {
                    let key = format!("nvidia_drm.{p}=");
                    let last = cmdline
                        .split_whitespace()
                        .filter_map(|w| w.strip_prefix(&key))
                        .next_back();
                    !matches!(last, Some("1" | "Y" | "y"))
                }
            }
        })
        .collect()
}

pub fn params(sys: &dyn System) -> Outcome {
    let (_, state) = match nvidia(sys) {
        Ok(v) => v,
        Err(skip) => return skip,
    };
    if state.branch.is_none() {
        return Outcome::Skipped("no NVIDIA driver is installed".into());
    }
    let off = disabled_params(sys);
    if off.is_empty() {
        return Outcome::Ok;
    }
    issue(
        "NVIDIA kernel mode setting is not enabled (Wayland needs it)",
        "Add: sudo samurai boot set param add nvidia_drm.<name>=1, then reboot",
        off.iter()
            .map(|p| format!("nvidia_drm.{p} is not enabled"))
            .collect(),
    )
}

pub fn fix_params(sys: &dyn System) -> Result<Vec<String>, String> {
    disabled_params(sys)
        .into_iter()
        .map(|p| {
            let change = boot::settings::Change::Add(format!("nvidia_drm.{p}=1"));
            boot::settings::set(sys, &change)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::health::system::fake::Fake;

    const RELEASE: &str = "6.99.1-linux";

    fn card(name: &str, installed: &str) -> Fake {
        let line = format!(
            r#"0000:01:00.0 "VGA compatible controller [0300]" "NVIDIA Corporation [10de]" "{name} [2204]" -p00 "" """#
        );
        Fake::default()
            .cmd("lspci -Dnnmm", 0, &line)
            .cmd("pacman -Qq", 0, installed)
            .dir("/usr/lib/modules", &[RELEASE])
            .path(&format!("/usr/lib/modules/{RELEASE}/pkgbase"))
    }

    fn modinfo(sys: Fake, code: i32) -> Fake {
        sys.cmd(&format!("modinfo -k {RELEASE} -F version nvidia"), code, "")
    }

    fn is_issue(o: &Outcome) -> bool {
        matches!(o, Outcome::Issue(_))
    }

    #[test]
    fn driver_family_follows_the_card_generation() {
        for (name, installed, bad) in [
            ("GA102 [RTX 3090]", "nvidia-open nvidia-utils", false),
            ("GA102 [RTX 3090]", "nvidia-580xx-dkms", false),
            ("GA102 [RTX 3090]", "base", true),
            ("GP104 [GTX 1070]", "nvidia-open nvidia-utils", true),
            ("GP104 [GTX 1070]", "base", false),
            ("GB203 [RTX 5080]", "nvidia-580xx-dkms", true),
            ("Device", "nvidia-open", false),
        ] {
            let o = driver_family(&card(name, installed));
            assert_eq!(is_issue(&o), bad, "{name} with {installed}: {o:?}");
        }
        let hand = driver_family(&card("GA102 [RTX 3090]", "gpu-unlock-dkms"));
        assert!(matches!(hand, Outcome::Skipped(_)), "{hand:?}");
        let none = driver_family(&Fake::default());
        assert!(matches!(none, Outcome::Skipped(_)), "{none:?}");
    }

    #[test]
    fn module_check_covers_every_kernel_and_the_running_system() {
        let sys = modinfo(card("GA102 [RTX 3090]", "nvidia-open-dkms"), 1);
        assert!(is_issue(&module(&sys)));
        let unloaded = modinfo(card("GA102 [RTX 3090]", "nvidia-open"), 0);
        assert!(is_issue(&module(&unloaded)));
        assert!(fix_module(&unloaded).is_err());
        let fine = modinfo(card("GA102 [RTX 3090]", "nvidia-open"), 0).path("/sys/module/nvidia");
        assert_eq!(module(&fine), Outcome::Ok);
    }

    #[test]
    fn kernel_mode_setting_reads_the_effective_value() {
        let base = || card("GA102 [RTX 3090]", "nvidia-open nvidia-utils");
        let cmdline = base().file(
            "/proc/cmdline",
            "quiet nvidia_drm.modeset=1 nvidia_drm.fbdev=1\n",
        );
        assert_eq!(params(&cmdline), Outcome::Ok);
        // the loaded module says no, whatever the command line says
        let off = base()
            .file("/proc/cmdline", "nvidia_drm.modeset=1 nvidia_drm.fbdev=1")
            .file("/sys/module/nvidia_drm/parameters/modeset", "N\n")
            .file("/sys/module/nvidia_drm/parameters/fbdev", "Y\n");
        match params(&off) {
            Outcome::Issue(i) => assert_eq!(i.evidence, ["nvidia_drm.modeset is not enabled"]),
            other => panic!("{other:?}"),
        }
        let later_wins = base().file(
            "/proc/cmdline",
            "nvidia_drm.modeset=1 nvidia_drm.modeset=0 nvidia_drm.fbdev=1",
        );
        assert!(is_issue(&params(&later_wins)));
    }

    #[test]
    fn profile_check_skips_without_chwd() {
        let sys = card("GA102 [RTX 3090]", "base");
        assert!(matches!(profile(&sys), Outcome::Skipped(_)));
    }
}

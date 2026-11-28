//! `samurai hw` (spec 07): detect hardware, show and apply driver profiles (chwd is the engine),
//! and notice hardware changes. Preview is the default; root and prompts live in the CLI.

pub mod apply;
pub mod devices;
pub mod fingerprint;
pub mod plan;
pub mod profiles;
#[cfg(test)]
mod tests;

use crate::health::json_escape;
use crate::health::system::System;

use devices::{Device, Generation};
use plan::{Plan, Request, Step};

pub const USAGE: &str = "usage: samurai hw <detect|list|apply|remove|fingerprint> [options]

Hardware and drivers (spec 07). chwd matches the devices; profiles come from samurai-hardware.

  detect [--json]            what is there, what is installed and what apply would do (read-only)
  list [--installed|--available|--all] [--json]
                             driver profiles (default: --available)
  apply [--profile <name>] [--reinstall] [--switch-driver] [--no-snapshot] [--dry-run] [--yes]
                             install the recommended profiles (root). Shows the plan and asks
                             first; --yes approves, --dry-run only shows. Takes a snapshot,
                             downloads every package before changing anything and restores the
                             old driver when a step fails.
    --profile <name>         apply this profile instead of the recommended one
    --reinstall              run the profiles' setup again even when nothing is missing
    --switch-driver          allow replacing the NVIDIA driver branch (open <-> 580xx)
    --no-snapshot            continue without a snapper snapshot
  remove <name> [--no-snapshot] [--yes]
                             remove a profile whose hardware is gone (root)
  fingerprint [--check|--accept] [--json]
                             compare the PCI devices with the last accepted state; --check also
                             updates the notice (boot service, root); --accept takes the
                             present hardware as the new state (root)

Exit codes: 0 done or nothing to do, 1 failed or refused, 2 usage error.
";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Listing {
    Installed,
    Available,
    All,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cmd {
    Help,
    Detect,
    List(Listing),
    Apply {
        request: Request,
        dry_run: bool,
        no_snapshot: bool,
    },
    Remove {
        profile: String,
        no_snapshot: bool,
    },
    Fingerprint {
        check: bool,
        accept: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    pub cmd: Cmd,
    pub json: bool,
    pub yes: bool,
}

fn profile_name(name: &str) -> Result<String, String> {
    let ok = !name.is_empty()
        && !name.starts_with('-')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-._".contains(c));
    ok.then(|| name.to_string())
        .ok_or(format!("not a profile name: {name:?}"))
}

pub fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut flags: Vec<&str> = Vec::new();
    let mut words: Vec<&str> = Vec::new();
    let mut profile = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--profile" => {
                profile = Some(profile_name(it.next().ok_or("--profile needs a name")?)?);
            }
            "-h" | "--help" => flags.push("--help"),
            "-y" => flags.push("--yes"),
            f if f.starts_with('-') => flags.push(f),
            w => words.push(w),
        }
    }
    let has = |f: &str| flags.contains(&f);
    if has("--help") || words.is_empty() {
        if words.is_empty() && !has("--help") {
            return Err("a command is required".into());
        }
        return Ok(Options {
            cmd: Cmd::Help,
            json: false,
            yes: false,
        });
    }
    let (allowed, wanted_words): (&[&str], usize) = match words[0] {
        "detect" => (&["--json"], 1),
        "list" => (&["--json", "--installed", "--available", "--all"], 1),
        "apply" => (
            &[
                "--reinstall",
                "--switch-driver",
                "--no-snapshot",
                "--dry-run",
                "--yes",
                "--json",
            ],
            1,
        ),
        "remove" => (&["--no-snapshot", "--yes"], 2),
        "fingerprint" => (&["--check", "--accept", "--json"], 1),
        other => return Err(format!("unknown command: {other}")),
    };
    if let Some(bad) = flags.iter().find(|f| !allowed.contains(f)) {
        return Err(format!("unknown option for {}: {bad}", words[0]));
    }
    if words.len() != wanted_words {
        return Err(if words.len() < wanted_words {
            "remove needs a profile name".into()
        } else {
            format!("unexpected argument: {}", words[wanted_words])
        });
    }
    if profile.is_some() && words[0] != "apply" {
        return Err("--profile only applies to apply".into());
    }
    let cmd = match words[0] {
        "detect" => Cmd::Detect,
        "list" => {
            let chosen: Vec<Listing> = [
                ("--installed", Listing::Installed),
                ("--available", Listing::Available),
                ("--all", Listing::All),
            ]
            .into_iter()
            .filter(|(f, _)| has(f))
            .map(|(_, l)| l)
            .collect();
            if chosen.len() > 1 {
                return Err("choose one of --installed, --available, --all".into());
            }
            Cmd::List(chosen.first().copied().unwrap_or(Listing::Available))
        }
        "apply" => {
            if has("--json") && !has("--dry-run") {
                return Err("apply --json needs --dry-run".into());
            }
            Cmd::Apply {
                request: Request {
                    profile,
                    reinstall: has("--reinstall"),
                    switch_driver: has("--switch-driver"),
                },
                dry_run: has("--dry-run"),
                no_snapshot: has("--no-snapshot"),
            }
        }
        "remove" => Cmd::Remove {
            profile: profile_name(words[1])?,
            no_snapshot: has("--no-snapshot"),
        },
        _ => {
            if has("--check") && has("--accept") {
                return Err("choose --check or --accept".into());
            }
            Cmd::Fingerprint {
                check: has("--check"),
                accept: has("--accept"),
            }
        }
    };
    Ok(Options {
        cmd,
        json: has("--json"),
        yes: has("--yes"),
    })
}

fn q(s: &str) -> String {
    format!("\"{}\"", json_escape(s))
}

fn list_json(items: &[String]) -> String {
    format!(
        "[{}]",
        items.iter().map(|s| q(s)).collect::<Vec<_>>().join(",")
    )
}

fn cuda_note(d: &Device) -> Option<&'static str> {
    d.is_nvidia_display()
        .then(|| Generation::of(&d.name).cuda())
        .map(|ok| {
            if ok {
                "supported"
            } else {
                "needs Turing (RTX 20) or newer"
            }
        })
}

fn device_json(plan: &Plan, d: &Device) -> String {
    let offered = plan
        .offers
        .iter()
        .find(|o| o.addr == d.addr)
        .map(|o| o.profiles.clone())
        .unwrap_or_default();
    let generation = if d.is_nvidia_display() {
        Generation::of(&d.name).as_str()
    } else {
        ""
    };
    format!(
        "{{\"addr\":{},\"class\":{},\"vendor\":{},\"device\":{},\"name\":{},\"driver\":{},\"boot_display\":{},\"generation\":{},\"cuda\":{},\"profiles\":{}}}",
        q(&d.addr),
        q(&d.class),
        q(&d.vendor),
        q(&d.device),
        q(&d.label()),
        q(d.driver.as_deref().unwrap_or("")),
        d.boot_vga,
        q(generation),
        d.is_nvidia_display() && Generation::of(&d.name).cuda(),
        list_json(&offered)
    )
}

fn step_json(s: &Step) -> String {
    format!(
        "{{\"profile\":{},\"description\":{},\"graphics\":{},\"status\":\"{}\",\"devices\":{},\"install\":{},\"remove\":{},\"recorded\":{}}}",
        q(&s.profile),
        q(&s.desc),
        s.graphics,
        s.status.as_str(),
        list_json(&s.devices),
        list_json(&s.install),
        list_json(&s.remove),
        s.recorded
    )
}

/// `{"schema":1,...}`: `samurai hw detect --json`, read by the toolbox GUI.
pub fn render_json(plan: &Plan, hardware: &fingerprint::State) -> String {
    let devices: Vec<String> = plan.devices.iter().map(|d| device_json(plan, d)).collect();
    let steps: Vec<String> = plan.steps.iter().map(step_json).collect();
    let (changed, message) = match hardware {
        fingerprint::State::Changed { added, removed } => {
            (true, fingerprint::message(added, removed))
        }
        _ => (false, String::new()),
    };
    format!(
        "{{\"schema\":1,\"changes\":{},\"allowed\":{},\"hardware_changed\":{changed},\"hardware_message\":{},\"nvidia_driver\":{},\"devices\":[{}],\"steps\":[{}],\"refusals\":{},\"warnings\":{}}}\n",
        plan.changes(),
        plan.refusals.is_empty(),
        q(&message),
        q(plan.nvidia.branch.map_or("", plan::Branch::as_str)),
        devices.join(","),
        steps.join(","),
        list_json(&plan.refusals),
        list_json(&plan.warnings)
    )
}

pub fn render_text(plan: &Plan) -> String {
    let mut out = String::from("Devices\n");
    for d in &plan.devices {
        out.push_str(&format!(
            "  {} {} [{}:{}] driver: {}{}\n",
            d.addr,
            d.label(),
            d.vendor,
            d.device,
            d.driver.as_deref().unwrap_or("none"),
            if d.boot_vga { " (boot display)" } else { "" }
        ));
        if let Some(note) = cuda_note(d) {
            out.push_str(&format!(
                "      {} generation, CUDA: {note}\n",
                Generation::of(&d.name).as_str()
            ));
        }
    }
    out.push_str("Profiles\n");
    for s in &plan.steps {
        out.push_str(&format!(
            "  {:<24} {:<9} {}\n",
            s.profile,
            s.status.as_str(),
            s.desc
        ));
        if !s.remove.is_empty() {
            out.push_str(&format!("      remove:  {}\n", s.remove.join(" ")));
        }
        if s.acts() && !s.install.is_empty() {
            out.push_str(&format!("      install: {}\n", s.install.join(" ")));
        }
    }
    if plan.steps.is_empty() {
        out.push_str("  no profile applies to this hardware\n");
    }
    for w in &plan.warnings {
        out.push_str(&format!("warning: {w}\n"));
    }
    for r in &plan.refusals {
        out.push_str(&format!("refused: {r}\n"));
    }
    out.push_str(if !plan.refusals.is_empty() {
        "Nothing will be changed.\n"
    } else if plan.changes() {
        "To apply: sudo samurai hw apply\n"
    } else {
        "Drivers match the hardware.\n"
    });
    out
}

/// One row of `samurai hw list`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub name: String,
    pub desc: String,
    pub priority: i64,
    pub available: bool,
    pub installed: bool,
}

pub fn list(sys: &dyn System, listing: Listing) -> Result<Vec<Row>, String> {
    let db = profiles::database(sys);
    if db.is_empty() {
        return Err("the chwd profile database is empty (install samurai-hardware)".into());
    }
    let offered: Vec<String> = profiles::offers(sys)?
        .into_iter()
        .flat_map(|o| o.profiles)
        .collect();
    let records = profiles::records(sys);
    let mut rows: Vec<Row> = db
        .into_iter()
        .map(|p| Row {
            available: offered.contains(&p.name),
            installed: records.iter().any(|(n, _)| *n == p.name),
            name: p.name,
            desc: p.desc,
            priority: p.priority,
        })
        .filter(|r| match listing {
            Listing::Installed => r.installed,
            Listing::Available => r.available,
            Listing::All => true,
        })
        .collect();
    rows.sort_by(|a, b| b.priority.cmp(&a.priority).then(a.name.cmp(&b.name)));
    Ok(rows)
}

pub fn render_list_text(rows: &[Row]) -> String {
    if rows.is_empty() {
        return "no profiles\n".into();
    }
    rows.iter()
        .map(|r| {
            format!(
                "{:<24} {}{} {}\n",
                r.name,
                if r.installed { "installed " } else { "" },
                if r.available { "available" } else { "-" },
                r.desc
            )
        })
        .collect()
}

pub fn render_list_json(rows: &[Row]) -> String {
    let items: Vec<String> = rows
        .iter()
        .map(|r| {
            format!(
                "{{\"name\":{},\"description\":{},\"priority\":{},\"available\":{},\"installed\":{}}}",
                q(&r.name),
                q(&r.desc),
                r.priority,
                r.available,
                r.installed
            )
        })
        .collect();
    format!("{{\"schema\":1,\"profiles\":[{}]}}\n", items.join(","))
}

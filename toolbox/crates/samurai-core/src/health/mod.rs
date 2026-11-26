//! `samurai health` (spec 05): typed checks, a runner, text/JSON output and exit codes.
//!
//! Slice 1 is detect-only. `fix()` and `fix_risk` handling arrive with the first fixable check.

pub mod checks;
pub mod system;

use std::fs;
use std::io;
use std::path::Path;

use system::System;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Info,
    Warning,
    Error,
    Critical,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Info => "info",
            Severity::Warning => "warning",
            Severity::Error => "error",
            Severity::Critical => "critical",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Packages,
    Boot,
    Drivers,
    Storage,
    Snapshots,
    Services,
    Security,
    Hardware,
    Config,
}

const CATEGORIES: [(&str, Category); 9] = [
    ("packages", Category::Packages),
    ("boot", Category::Boot),
    ("drivers", Category::Drivers),
    ("storage", Category::Storage),
    ("snapshots", Category::Snapshots),
    ("services", Category::Services),
    ("security", Category::Security),
    ("hardware", Category::Hardware),
    ("config", Category::Config),
];

impl Category {
    pub fn as_str(self) -> &'static str {
        CATEGORIES
            .iter()
            .find(|(_, c)| *c == self)
            .map_or("", |(n, _)| n)
    }

    pub fn parse(name: &str) -> Option<Category> {
        CATEGORIES.iter().find(|(n, _)| *n == name).map(|(_, c)| *c)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixRisk {
    Safe,
    NeedsConfirm,
    ManualOnly,
}

impl FixRisk {
    pub fn as_str(self) -> &'static str {
        match self {
            FixRisk::Safe => "safe",
            FixRisk::NeedsConfirm => "needs-confirm",
            FixRisk::ManualOnly => "manual-only",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    pub summary: String,
    pub details: String,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Ok,
    Issue(Issue),
    /// The check could not run here (tool missing, not applicable). Never counts as a failure.
    Skipped(String),
}

/// Repairs one check's issue; returns the changes made. Runs as root, after the snapshot.
pub type FixFn = fn(&dyn System) -> Result<Vec<String>, String>;

pub struct Check {
    pub id: &'static str,
    pub category: Category,
    pub severity: Severity,
    pub fix_risk: FixRisk,
    pub detect: fn(&dyn System) -> Outcome,
    pub fix: Option<FixFn>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub id: &'static str,
    pub category: Category,
    pub severity: Severity,
    pub fix_risk: FixRisk,
    /// `--fix` can repair this check by itself (it has a fix that is not manual-only).
    pub fixable: bool,
    pub outcome: Outcome,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Filter {
    pub category: Option<Category>,
    pub id: Option<String>,
}

pub fn run(sys: &dyn System, checks: &[Check], filter: &Filter) -> Vec<Finding> {
    checks
        .iter()
        .filter(|c| filter.category.is_none_or(|cat| cat == c.category))
        .filter(|c| filter.id.as_deref().is_none_or(|id| id == c.id))
        .map(|c| Finding {
            id: c.id,
            category: c.category,
            severity: c.severity,
            fix_risk: c.fix_risk,
            fixable: c.fix.is_some() && c.fix_risk != FixRisk::ManualOnly,
            outcome: (c.detect)(sys),
        })
        .collect()
}

/// 0 = nothing at warning level or above; 1 = at least one warning/error/critical issue.
pub fn exit_code(findings: &[Finding]) -> i32 {
    let bad = findings
        .iter()
        .any(|f| matches!(f.outcome, Outcome::Issue(_)) && f.severity >= Severity::Warning);
    i32::from(bad)
}

pub fn render_text(findings: &[Finding]) -> String {
    let mut out = String::new();
    for f in findings {
        match &f.outcome {
            Outcome::Ok => out.push_str(&format!("ok       {}\n", f.id)),
            Outcome::Skipped(why) => out.push_str(&format!("skipped  {} ({why})\n", f.id)),
            Outcome::Issue(i) => {
                out.push_str(&format!(
                    "{:<8} {}: {}\n",
                    f.severity.as_str(),
                    f.id,
                    i.summary
                ));
                if !i.details.is_empty() {
                    out.push_str(&format!("           {}\n", i.details));
                }
                for e in &i.evidence {
                    out.push_str(&format!("           - {e}\n"));
                }
            }
        }
    }
    let issues = findings
        .iter()
        .filter(|f| matches!(f.outcome, Outcome::Issue(_)))
        .count();
    out.push_str(&format!("{} check(s), {issues} issue(s)\n", findings.len()));
    out
}

pub fn json_escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    for ch in s.chars() {
        match ch {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o
}

/// Field order inside a result (`severity`, `fix_risk`, `status` adjacent) is a contract:
/// `samurai-notifier` greps it to count warning-or-worse issues without a JSON parser.
/// `{"schema":1,...}`. `checked_epoch` is injected so output is deterministic in tests.
pub fn render_json(findings: &[Finding], checked_epoch: u64) -> String {
    let rows: Vec<String> = findings
        .iter()
        .map(|f| {
            let (status, summary, details, evidence) = match &f.outcome {
                Outcome::Ok => ("ok", String::new(), String::new(), Vec::new()),
                Outcome::Skipped(why) => ("skipped", why.clone(), String::new(), Vec::new()),
                Outcome::Issue(i) => ("issue", i.summary.clone(), i.details.clone(), i.evidence.clone()),
            };
            let ev: Vec<String> = evidence.iter().map(|e| format!("\"{}\"", json_escape(e))).collect();
            format!(
                "{{\"id\":\"{}\",\"category\":\"{}\",\"severity\":\"{}\",\"fix_risk\":\"{}\",\"status\":\"{status}\",\"summary\":\"{}\",\"details\":\"{}\",\"evidence\":[{}],\"fixable\":{}}}",
                f.id,
                f.category.as_str(),
                f.severity.as_str(),
                f.fix_risk.as_str(),
                json_escape(&summary),
                json_escape(&details),
                ev.join(","),
                f.fixable
            )
        })
        .collect();
    let issues = findings
        .iter()
        .filter(|f| matches!(f.outcome, Outcome::Issue(_)))
        .count();
    format!(
        "{{\"schema\":1,\"checked_epoch\":{checked_epoch},\"issues\":{issues},\"results\":[{}]}}\n",
        rows.join(",")
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FixAction {
    /// Fix ran and the check passes again; lists what changed.
    Fixed(Vec<String>),
    /// Fix ran but the check still reports an issue.
    StillFailing(String),
    /// The fix itself returned an error.
    Failed(String),
    /// A needs-confirm fix the user did not approve.
    Declined,
    /// Nothing automatic: the issue's `details` hold the manual steps.
    Manual,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixReport {
    pub id: &'static str,
    pub action: FixAction,
}

/// True when [`apply_fixes`] would run (or ask about) at least one fix: callers snapshot only then.
pub fn has_actionable_fix(checks: &[Check], findings: &[Finding]) -> bool {
    findings.iter().any(|f| {
        matches!(f.outcome, Outcome::Issue(_))
            && f.fix_risk != FixRisk::ManualOnly
            && checks.iter().any(|c| c.id == f.id && c.fix.is_some())
    })
}

/// Run the fixes for failed checks: safe ones directly, needs-confirm ones after `confirm`,
/// manual-only ones never. Each fix is verified by detecting again.
pub fn apply_fixes(
    sys: &dyn System,
    checks: &[Check],
    findings: &[Finding],
    confirm: &mut dyn FnMut(&Finding, &Issue) -> bool,
) -> Vec<FixReport> {
    let mut reports = Vec::new();
    for f in findings {
        let Outcome::Issue(issue) = &f.outcome else {
            continue;
        };
        let Some(check) = checks.iter().find(|c| c.id == f.id) else {
            continue;
        };
        let action = match (check.fix, f.fix_risk) {
            (None, _) | (_, FixRisk::ManualOnly) => FixAction::Manual,
            (Some(_), FixRisk::NeedsConfirm) if !confirm(f, issue) => FixAction::Declined,
            (Some(fix), _) => match fix(sys) {
                Err(e) => FixAction::Failed(e),
                Ok(changes) => match (check.detect)(sys) {
                    Outcome::Issue(again) => FixAction::StillFailing(again.summary),
                    _ => FixAction::Fixed(changes),
                },
            },
        };
        reports.push(FixReport { id: f.id, action });
    }
    reports
}

pub fn render_fix_text(reports: &[FixReport]) -> String {
    let mut out = String::new();
    for r in reports {
        let line = match &r.action {
            FixAction::Fixed(c) => format!("fixed    {}: {}\n", r.id, c.join("; ")),
            FixAction::StillFailing(why) => {
                format!("still    {}: fix ran but the issue remains ({why})\n", r.id)
            }
            FixAction::Failed(e) => format!("failed   {}: {e}\n", r.id),
            FixAction::Declined => format!("declined {}\n", r.id),
            FixAction::Manual => format!("manual   {}: see the steps in the health report\n", r.id),
        };
        out.push_str(&line);
    }
    out
}

pub enum Snapshot {
    Created,
    /// No snapper here: fixes may continue, the caller says so.
    Unavailable(String),
    /// snapper exists but could not snapshot: do NOT fix (spec 05: snapshot-first).
    Failed(String),
}

pub fn pre_fix_snapshot(sys: &dyn System) -> Snapshot {
    if !sys.exists("/usr/bin/snapper") {
        return Snapshot::Unavailable("snapper is not installed".into());
    }
    match sys.run(
        "snapper",
        &[
            "-c",
            "root",
            "create",
            "--description",
            "samurai health --fix",
        ],
    ) {
        Some(o) if o.code == 0 => Snapshot::Created,
        Some(o) => Snapshot::Failed(format!("snapper exited {}", o.code)),
        None => Snapshot::Failed("snapper could not be started".into()),
    }
}

pub const REPORT_PATH: &str = "/var/lib/samurai/health.json";

/// Write `content` to `path` via a temp file + rename, so readers never see a partial report.
pub fn write_atomic(path: &Path, content: &str) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, content)?;
    fs::rename(&tmp, path)
}

pub const USAGE: &str =
    "usage: samurai health [--category <name>] [--id <check-id>] [--json] [--list] [--write]
                      [--fix [--yes]]

Runs the health checks (read-only) and reports what is wrong.

  --category <name>  only this category: packages boot drivers storage snapshots
                     services security hardware config
  --id <check-id>    only this check (see --list)
  --json             machine-readable output (schema 1)
  --list             print the check ids and exit
  --write            also write the JSON report to /var/lib/samurai/health.json (root;
                     the daily samurai-health.timer does this; the notifier and GUI read it)
  --fix              repair what has an automatic fix (root): takes a snapper snapshot first, safe
                     fixes run directly, risky ones ask; combine with --id/--category to narrow
  --yes              with --fix: approve the risky fixes without asking
  --help             this text

Exit codes: 0 no warning-level or worse issue, 1 at least one issue, 2 usage error.
";

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Options {
    pub filter: Filter,
    pub json: bool,
    pub list: bool,
    pub write: bool,
    pub fix: bool,
    pub yes: bool,
    pub help: bool,
}

pub fn parse_args(args: &[String], checks: &[Check]) -> Result<Options, String> {
    let mut o = Options::default();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--json" => o.json = true,
            "--list" => o.list = true,
            "--write" => o.write = true,
            "--fix" => o.fix = true,
            "--yes" | "-y" => o.yes = true,
            "-h" | "--help" => o.help = true,
            "--category" => {
                let v = it.next().ok_or("--category needs a value")?;
                o.filter.category =
                    Some(Category::parse(v).ok_or(format!("unknown category: {v}"))?);
            }
            "--id" => {
                let v = it.next().ok_or("--id needs a value")?;
                if !checks.iter().any(|c| c.id == v) {
                    return Err(format!(
                        "unknown check id: {v} (see: samurai health --list)"
                    ));
                }
                o.filter.id = Some(v.clone());
            }
            other => return Err(format!("unknown option: {other}")),
        }
    }
    if o.fix && (o.json || o.write || o.list) {
        return Err("--fix cannot be combined with --json, --write or --list".into());
    }
    if o.yes && !o.fix {
        return Err("--yes only applies to --fix".into());
    }
    Ok(o)
}

#[cfg(test)]
mod tests {
    use super::checks::ALL;
    use super::system::fake::Fake;
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| (*x).to_string()).collect()
    }

    fn ok_check(_: &dyn System) -> Outcome {
        Outcome::Ok
    }

    fn bad_check(_: &dyn System) -> Outcome {
        Outcome::Issue(Issue {
            summary: "bad \"thing\"".into(),
            details: "line1\nline2".into(),
            evidence: vec!["e\\1".into()],
        })
    }

    fn sample(sev: Severity) -> [Check; 2] {
        [
            Check {
                id: "a.ok",
                category: Category::Boot,
                severity: Severity::Error,
                fix_risk: FixRisk::Safe,
                detect: ok_check,
                fix: None,
            },
            Check {
                id: "b.bad",
                category: Category::Services,
                severity: sev,
                fix_risk: FixRisk::ManualOnly,
                detect: bad_check,
                fix: None,
            },
        ]
    }

    #[test]
    fn ids_are_unique_and_well_formed() {
        let mut seen = std::collections::HashSet::new();
        for c in ALL {
            assert!(seen.insert(c.id), "duplicate id {}", c.id);
            let (area, name) = c.id.split_once('.').expect("id is <area>.<name>");
            let ok =
                |s: &str| !s.is_empty() && s.chars().all(|ch| ch.is_ascii_lowercase() || ch == '-');
            assert!(ok(area) && ok(name), "malformed id {}", c.id);
        }
    }

    #[test]
    fn filter_by_category_and_id() {
        let checks = sample(Severity::Warning);
        let sys = Fake::default();
        assert_eq!(run(&sys, &checks, &Filter::default()).len(), 2);
        let f = Filter {
            category: Some(Category::Services),
            id: None,
        };
        assert_eq!(run(&sys, &checks, &f)[0].id, "b.bad");
        let f = Filter {
            category: None,
            id: Some("a.ok".into()),
        };
        assert_eq!(run(&sys, &checks, &f).len(), 1);
    }

    #[test]
    fn exit_code_ignores_info_and_skipped() {
        let sys = Fake::default();
        assert_eq!(
            exit_code(&run(&sys, &sample(Severity::Info), &Filter::default())),
            0
        );
        assert_eq!(
            exit_code(&run(&sys, &sample(Severity::Warning), &Filter::default())),
            1
        );
        let skipped = [Check {
            id: "x.s",
            category: Category::Boot,
            severity: Severity::Critical,
            fix_risk: FixRisk::Safe,
            detect: |_| Outcome::Skipped("no tool".into()),
            fix: None,
        }];
        assert_eq!(exit_code(&run(&sys, &skipped, &Filter::default())), 0);
    }

    #[test]
    fn json_is_escaped_and_stable() {
        let sys = Fake::default();
        let out = render_json(&run(&sys, &sample(Severity::Error), &Filter::default()), 42);
        assert!(out.starts_with("{\"schema\":1,\"checked_epoch\":42,\"issues\":1,\"results\":["));
        assert!(out.contains("\"summary\":\"bad \\\"thing\\\"\""));
        assert!(out.contains("\"details\":\"line1\\nline2\""));
        assert!(out.contains("\"evidence\":[\"e\\\\1\"]"));
        assert!(out.contains("\"id\":\"a.ok\",\"category\":\"boot\",\"severity\":\"error\",\"fix_risk\":\"safe\",\"status\":\"ok\""));
        assert!(out.ends_with("]}\n"));
    }

    #[test]
    fn text_output_lists_each_state() {
        let sys = Fake::default();
        let t = render_text(&run(&sys, &sample(Severity::Warning), &Filter::default()));
        assert!(t.contains("ok       a.ok\n"));
        assert!(t.contains("warning  b.bad: bad \"thing\"\n"));
        assert!(t.ends_with("2 check(s), 1 issue(s)\n"));
    }

    #[test]
    fn write_atomic_replaces_and_leaves_no_temp() {
        let d = std::env::temp_dir().join(format!("samurai-health-write-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        let path = d.join("nested/health.json");
        write_atomic(&path, "one\n").unwrap();
        write_atomic(&path, "two\n").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "two\n");
        assert!(!path.with_extension("json.tmp").exists());
    }

    #[test]
    fn args_parse_and_reject() {
        let checks = sample(Severity::Warning);
        let o = parse_args(&s(&["--json", "--category", "boot"]), &checks).unwrap();
        assert!(o.json && o.filter.category == Some(Category::Boot));
        assert_eq!(
            parse_args(&s(&["--id", "a.ok"]), &checks)
                .unwrap()
                .filter
                .id
                .as_deref(),
            Some("a.ok")
        );
        assert!(parse_args(&s(&["--id", "nope"]), &checks).is_err());
        assert!(parse_args(&s(&["--category", "nope"]), &checks).is_err());
        assert!(parse_args(&s(&["--category"]), &checks).is_err());
        assert!(parse_args(&s(&["--bogus"]), &checks).is_err());
        assert!(parse_args(&s(&["--help"]), &checks).unwrap().help);
        assert!(parse_args(&s(&["--write"]), &checks).unwrap().write);
    }

    fn noop_fix(_: &dyn System) -> Result<Vec<String>, String> {
        Ok(vec!["did it".into()])
    }

    fn failing_fix(_: &dyn System) -> Result<Vec<String>, String> {
        Err("boom".into())
    }

    fn fixable(risk: FixRisk, fix: Option<FixFn>) -> Check {
        Check {
            id: "x.fixable",
            category: Category::Services,
            severity: Severity::Warning,
            fix_risk: risk,
            detect: bad_check,
            fix,
        }
    }

    fn reports(checks: &[Check], yes: bool) -> Vec<FixReport> {
        let sys = Fake::default();
        let findings = run(&sys, checks, &Filter::default());
        apply_fixes(&sys, checks, &findings, &mut |_, _| yes)
    }

    #[test]
    fn safe_fix_runs_without_asking_and_is_verified() {
        let sys = Fake::default()
            .cmd("systemctl is-active firewalld", 3, "inactive\n")
            .cmd("systemctl is-active firewalld", 0, "active\n")
            .cmd("systemctl enable --now firewalld", 0, "");
        let f = run(
            &sys,
            ALL,
            &Filter {
                category: None,
                id: Some("security.firewall".into()),
            },
        );
        let r = apply_fixes(&sys, ALL, &f, &mut |_, _| {
            panic!("a safe fix must not prompt")
        });
        assert_eq!(
            r,
            vec![FixReport {
                id: "security.firewall",
                action: FixAction::Fixed(vec!["enabled and started firewalld".into()])
            }]
        );
        assert!(sys
            .calls()
            .contains(&"systemctl enable --now firewalld".to_string()));
    }

    #[test]
    fn fix_that_does_not_help_is_reported_still_failing() {
        let sys = Fake::default()
            .cmd("systemctl is-active firewalld", 3, "inactive\n")
            .cmd("systemctl enable --now firewalld", 0, "");
        let f = run(
            &sys,
            ALL,
            &Filter {
                category: None,
                id: Some("security.firewall".into()),
            },
        );
        let r = apply_fixes(&sys, ALL, &f, &mut |_, _| true);
        assert!(matches!(r[0].action, FixAction::StillFailing(_)), "{r:?}");
    }

    #[test]
    fn risk_levels_decide_who_runs() {
        assert!(
            matches!(
                reports(&[fixable(FixRisk::Safe, Some(noop_fix))], false)[0].action,
                FixAction::StillFailing(_)
            ),
            "bad_check keeps failing, so even a safe fix that ran is still-failing"
        );
        assert_eq!(
            reports(&[fixable(FixRisk::NeedsConfirm, Some(noop_fix))], false)[0].action,
            FixAction::Declined
        );
        assert!(matches!(
            reports(&[fixable(FixRisk::NeedsConfirm, Some(noop_fix))], true)[0].action,
            FixAction::StillFailing(_)
        ));
        assert_eq!(
            reports(&[fixable(FixRisk::ManualOnly, Some(noop_fix))], true)[0].action,
            FixAction::Manual
        );
        assert_eq!(
            reports(&[fixable(FixRisk::Safe, None)], true)[0].action,
            FixAction::Manual
        );
        assert_eq!(
            reports(&[fixable(FixRisk::Safe, Some(failing_fix))], true)[0].action,
            FixAction::Failed("boom".into())
        );
    }

    #[test]
    fn actionable_means_issue_plus_fix_plus_not_manual() {
        let sys = Fake::default();
        let run_all = |c: &[Check]| run(&sys, c, &Filter::default());
        let a = [fixable(FixRisk::Safe, Some(noop_fix))];
        assert!(has_actionable_fix(&a, &run_all(&a)));
        let b = [fixable(FixRisk::ManualOnly, Some(noop_fix))];
        assert!(!has_actionable_fix(&b, &run_all(&b)));
        let c = [fixable(FixRisk::Safe, None)];
        assert!(!has_actionable_fix(&c, &run_all(&c)));
        let ok = sample(Severity::Warning);
        assert!(!has_actionable_fix(&ok[..1], &run_all(&ok[..1])));
    }

    #[test]
    fn snapshot_first_rules() {
        let none = Fake::default();
        assert!(matches!(pre_fix_snapshot(&none), Snapshot::Unavailable(_)));
        let cmd = "snapper -c root create --description samurai health --fix";
        let ok = Fake::default().path("/usr/bin/snapper").cmd(cmd, 0, "");
        assert!(matches!(pre_fix_snapshot(&ok), Snapshot::Created));
        let bad = Fake::default().path("/usr/bin/snapper").cmd(cmd, 1, "");
        assert!(matches!(pre_fix_snapshot(&bad), Snapshot::Failed(_)));
    }

    #[test]
    fn fix_flag_rules() {
        let checks = sample(Severity::Warning);
        let o = parse_args(&s(&["--fix", "--yes", "--id", "a.ok"]), &checks).unwrap();
        assert!(o.fix && o.yes);
        for bad in [
            &["--fix", "--json"][..],
            &["--fix", "--write"],
            &["--fix", "--list"],
            &["--yes"],
        ] {
            assert!(parse_args(&s(bad), &checks).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn fix_text_lines() {
        let t = render_fix_text(&[
            FixReport {
                id: "a",
                action: FixAction::Fixed(vec!["x".into(), "y".into()]),
            },
            FixReport {
                id: "b",
                action: FixAction::Declined,
            },
            FixReport {
                id: "c",
                action: FixAction::Manual,
            },
        ]);
        assert_eq!(
            t,
            "fixed    a: x; y\ndeclined b\nmanual   c: see the steps in the health report\n"
        );
    }
}

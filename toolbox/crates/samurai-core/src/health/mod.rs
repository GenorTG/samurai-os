//! `samurai health` (spec 05): typed checks, a runner, text/JSON output and exit codes.
//!
//! Slice 1 is detect-only. `fix()` and `fix_risk` handling arrive with the first fixable check.

pub mod checks;
pub mod system;

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

pub struct Check {
    pub id: &'static str,
    pub category: Category,
    pub severity: Severity,
    pub fix_risk: FixRisk,
    pub detect: fn(&dyn System) -> Outcome,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub id: &'static str,
    pub category: Category,
    pub severity: Severity,
    pub fix_risk: FixRisk,
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
                "{{\"id\":\"{}\",\"category\":\"{}\",\"severity\":\"{}\",\"fix_risk\":\"{}\",\"status\":\"{status}\",\"summary\":\"{}\",\"details\":\"{}\",\"evidence\":[{}]}}",
                f.id,
                f.category.as_str(),
                f.severity.as_str(),
                f.fix_risk.as_str(),
                json_escape(&summary),
                json_escape(&details),
                ev.join(",")
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

pub const USAGE: &str =
    "usage: samurai health [--category <name>] [--id <check-id>] [--json] [--list]

Runs the health checks (read-only) and reports what is wrong.

  --category <name>  only this category: packages boot drivers storage snapshots
                     services security hardware config
  --id <check-id>    only this check (see --list)
  --json             machine-readable output (schema 1)
  --list             print the check ids and exit
  --help             this text

Exit codes: 0 no warning-level or worse issue, 1 at least one issue, 2 usage error.
";

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Options {
    pub filter: Filter,
    pub json: bool,
    pub list: bool,
    pub help: bool,
}

pub fn parse_args(args: &[String], checks: &[Check]) -> Result<Options, String> {
    let mut o = Options::default();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--json" => o.json = true,
            "--list" => o.list = true,
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
            },
            Check {
                id: "b.bad",
                category: Category::Services,
                severity: sev,
                fix_risk: FixRisk::ManualOnly,
                detect: bad_check,
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
    }
}

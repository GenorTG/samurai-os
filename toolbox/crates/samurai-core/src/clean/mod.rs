//! `samurai clean` (spec 04 "System cleaner"): preview how much each target frees, then clean.
//!
//! Safety rules, in code and in tests:
//! - `all` only runs targets that are safe to run unattended; orphans and the whole `~/.cache`
//!   always need an explicit target and a confirmation ("never automatic").
//! - user paths come from /etc/passwd via the caller, never from raw input, and must be
//!   absolute without `..`.
//! - every command is fixed argv (no shell), so nothing here can be steered by file names.

use crate::health::system::System;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Risk {
    /// Safe to run unattended (regenerable data).
    Safe,
    /// Needs an explicit target and a confirmation.
    NeedsConfirm,
}

impl Risk {
    pub fn as_str(self) -> &'static str {
        match self {
            Risk::Safe => "safe",
            Risk::NeedsConfirm => "needs-confirm",
        }
    }
}

pub struct Ctx {
    /// Home directory of the real user (None: user targets are skipped).
    pub home: Option<String>,
    /// How many cached versions of each package `pkgcache` keeps.
    pub keep: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Preview {
    /// Bytes that would be freed, when known.
    pub bytes: Option<u64>,
    pub items: Vec<String>,
    pub note: String,
}

pub struct Target {
    pub id: &'static str,
    pub description: &'static str,
    pub risk: Risk,
    pub needs_root: bool,
    pub preview: fn(&dyn System, &Ctx) -> Preview,
    pub apply: fn(&dyn System, &Ctx) -> Result<String, String>,
}

/// `1.5 KiB`, `12 MiB`, `1.2 GiB`, `512 B`.
pub fn human(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let (mut v, mut u) = (bytes as f64, 0);
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 {
        format!("{bytes} B")
    } else if v >= 10.0 {
        format!("{v:.0} {}", UNITS[u])
    } else {
        format!("{v:.1} {}", UNITS[u])
    }
}

/// `"12.34 MiB"` / `"1.2G"` / `"512B"` → bytes. systemd and paccache both print these.
pub fn parse_size(text: &str) -> Option<u64> {
    let t = text.trim();
    let split = t
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(t.len());
    let (num, unit) = t.split_at(split);
    let n: f64 = num.parse().ok()?;
    let mult: f64 = match unit.trim().to_ascii_lowercase().as_str() {
        "" | "b" => 1.0,
        "k" | "kb" | "kib" => 1024.0,
        "m" | "mb" | "mib" => 1024.0 * 1024.0,
        "g" | "gb" | "gib" => 1024.0 * 1024.0 * 1024.0,
        "t" | "tb" | "tib" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    Some((n * mult).round() as u64)
}

fn safe_abs(path: &str) -> bool {
    path.starts_with('/')
        && !path.split('/').any(|p| p == "..")
        && !path.contains('\0')
        && path.len() > 1
}

/// `du -sb PATH` → bytes.
fn du(sys: &dyn System, path: &str) -> Option<u64> {
    if !safe_abs(path) {
        return None;
    }
    let o = sys.run("du", &["-sb", path])?;
    (o.code == 0)
        .then(|| o.stdout.split_whitespace().next()?.parse().ok())
        .flatten()
}

fn run_ok(sys: &dyn System, prog: &str, args: &[&str], what: &str) -> Result<(), String> {
    match sys.run(prog, args) {
        Some(o) if o.code == 0 => Ok(()),
        Some(o) => Err(format!("{what}: {prog} exited {}", o.code)),
        None => Err(format!("{what}: {prog} is not installed")),
    }
}

// ---- pkgcache ----------------------------------------------------------------------------

fn pkgcache_preview(sys: &dyn System, ctx: &Ctx) -> Preview {
    let keep = ctx.keep.to_string();
    let Some(o) = sys.run("paccache", &["-d", "-k", &keep]) else {
        return Preview {
            note: "paccache is not installed (pacman-contrib)".into(),
            ..Preview::default()
        };
    };
    // "==> finished dry run: 12 candidates (disk space saved: 345.67 MiB)"
    let bytes = o.stdout.lines().find_map(|l| {
        let rest = l.split("disk space saved:").nth(1)?;
        parse_size(rest.trim().trim_end_matches(')'))
    });
    Preview {
        bytes,
        note: format!("keeps the newest {} version(s) of each package", ctx.keep),
        ..Preview::default()
    }
}

fn pkgcache_apply(sys: &dyn System, ctx: &Ctx) -> Result<String, String> {
    let keep = ctx.keep.to_string();
    run_ok(sys, "paccache", &["-r", "-k", &keep], "package cache")?;
    run_ok(
        sys,
        "paccache",
        &["-r", "-u", "-k", "0"],
        "uninstalled packages in the cache",
    )?;
    Ok(format!(
        "trimmed the package cache (kept {keep} version(s); removed uninstalled packages)"
    ))
}

// ---- orphans -----------------------------------------------------------------------------

fn orphan_list(sys: &dyn System) -> Option<Vec<String>> {
    let o = sys.run("pacman", &["-Qdtq"])?;
    // exit 1 with empty output means "no orphans"
    Some(
        o.stdout
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect(),
    )
}

fn orphans_preview(sys: &dyn System, _: &Ctx) -> Preview {
    match orphan_list(sys) {
        None => Preview {
            note: "pacman is not available".into(),
            ..Preview::default()
        },
        Some(items) => Preview {
            note: "never removed automatically".into(),
            items,
            bytes: None,
        },
    }
}

fn orphans_apply(sys: &dyn System, _: &Ctx) -> Result<String, String> {
    let items = orphan_list(sys).ok_or("pacman is not available")?;
    if items.is_empty() {
        return Ok("no orphans".into());
    }
    let mut args = vec!["-Rns", "--noconfirm"];
    args.extend(items.iter().map(String::as_str));
    run_ok(sys, "pacman", &args, "orphans")?;
    Ok(format!("removed {} orphan package(s)", items.len()))
}

// ---- journal -----------------------------------------------------------------------------

const JOURNAL_KEEP: &str = "200M";

fn journal_preview(sys: &dyn System, _: &Ctx) -> Preview {
    let Some(o) = sys.run("journalctl", &["--disk-usage"]) else {
        return Preview {
            note: "journalctl is not available".into(),
            ..Preview::default()
        };
    };
    // "Archived and active journals take up 1.2G in the file system."
    let used = o
        .stdout
        .split("take up")
        .nth(1)
        .and_then(|r| parse_size(r.split_whitespace().next()?));
    let keep = parse_size(JOURNAL_KEEP).unwrap_or(0);
    Preview {
        bytes: used.map(|u| u.saturating_sub(keep)),
        note: format!("vacuums the journal down to {JOURNAL_KEEP}"),
        ..Preview::default()
    }
}

fn journal_apply(sys: &dyn System, _: &Ctx) -> Result<String, String> {
    run_ok(
        sys,
        "journalctl",
        &[&format!("--vacuum-size={JOURNAL_KEEP}")],
        "journal",
    )?;
    Ok(format!("vacuumed the journal to {JOURNAL_KEEP}"))
}

// ---- coredumps ---------------------------------------------------------------------------

const COREDUMPS: &str = "/var/lib/systemd/coredump";

fn coredumps_preview(sys: &dyn System, _: &Ctx) -> Preview {
    match du(sys, COREDUMPS) {
        Some(b) => Preview {
            bytes: Some(b),
            ..Preview::default()
        },
        None => Preview {
            note: "no coredump directory, or needs root".into(),
            ..Preview::default()
        },
    }
}

fn coredumps_apply(sys: &dyn System, _: &Ctx) -> Result<String, String> {
    run_ok(
        sys,
        "find",
        &[COREDUMPS, "-mindepth", "1", "-type", "f", "-delete"],
        "coredumps",
    )?;
    Ok("removed stored coredumps".into())
}

// ---- user caches -------------------------------------------------------------------------

fn user_path(ctx: &Ctx, sub: &str) -> Option<String> {
    let home = ctx.home.as_deref()?;
    let p = format!("{}/{sub}", home.trim_end_matches('/'));
    (safe_abs(home) && home != "/" && safe_abs(&p)).then_some(p)
}

fn dir_preview(sys: &dyn System, ctx: &Ctx, sub: &str) -> Preview {
    match user_path(ctx, sub).and_then(|p| du(sys, &p)) {
        Some(b) => Preview {
            bytes: Some(b),
            ..Preview::default()
        },
        None => Preview {
            note: "nothing to clean (or no home directory)".into(),
            ..Preview::default()
        },
    }
}

fn dir_apply(sys: &dyn System, ctx: &Ctx, sub: &str, what: &str) -> Result<String, String> {
    let p = user_path(ctx, sub).ok_or("no usable home directory")?;
    if !sys.exists(&p) {
        return Ok(format!("{what}: nothing to remove"));
    }
    run_ok(sys, "find", &[&p, "-mindepth", "1", "-delete"], what)?;
    Ok(format!("emptied {p}"))
}

fn thumbnails_preview(sys: &dyn System, ctx: &Ctx) -> Preview {
    dir_preview(sys, ctx, ".cache/thumbnails")
}

fn thumbnails_apply(sys: &dyn System, ctx: &Ctx) -> Result<String, String> {
    dir_apply(sys, ctx, ".cache/thumbnails", "thumbnails")
}

fn cache_preview(sys: &dyn System, ctx: &Ctx) -> Preview {
    let mut p = dir_preview(sys, ctx, ".cache");
    p.note = "the whole ~/.cache: apps rebuild it, but logins and indexes restart cold".into();
    p
}

fn cache_apply(sys: &dyn System, ctx: &Ctx) -> Result<String, String> {
    dir_apply(sys, ctx, ".cache", "~/.cache")
}

pub const ALL_TARGETS: &[Target] = &[
    Target {
        id: "pkgcache",
        description: "old and uninstalled package versions in /var/cache/pacman/pkg",
        risk: Risk::Safe,
        needs_root: true,
        preview: pkgcache_preview,
        apply: pkgcache_apply,
    },
    Target {
        id: "journal",
        description: "systemd journal above 200M",
        risk: Risk::Safe,
        needs_root: true,
        preview: journal_preview,
        apply: journal_apply,
    },
    Target {
        id: "coredumps",
        description: "stored coredumps",
        risk: Risk::Safe,
        needs_root: true,
        preview: coredumps_preview,
        apply: coredumps_apply,
    },
    Target {
        id: "thumbnails",
        description: "thumbnail cache of the current user",
        risk: Risk::Safe,
        needs_root: false,
        preview: thumbnails_preview,
        apply: thumbnails_apply,
    },
    Target {
        id: "orphans",
        description: "packages installed as dependencies that nothing needs any more",
        risk: Risk::NeedsConfirm,
        needs_root: true,
        preview: orphans_preview,
        apply: orphans_apply,
    },
    Target {
        id: "cache",
        description: "the whole ~/.cache of the current user",
        risk: Risk::NeedsConfirm,
        needs_root: false,
        preview: cache_preview,
        apply: cache_apply,
    },
];

/// Targets `all` covers: the safe ones only.
pub fn all_selection() -> Vec<&'static Target> {
    ALL_TARGETS
        .iter()
        .filter(|t| t.risk == Risk::Safe)
        .collect()
}

pub fn find(id: &str) -> Option<&'static Target> {
    ALL_TARGETS.iter().find(|t| t.id == id)
}

pub fn render_preview(rows: &[(&Target, Preview)]) -> String {
    let mut out = String::new();
    let mut total = 0u64;
    for (t, p) in rows {
        let size = p.bytes.map_or("-".to_string(), human);
        total += p.bytes.unwrap_or(0);
        out.push_str(&format!("{:<11} {:>9}  {}\n", t.id, size, t.description));
        if !p.note.is_empty() {
            out.push_str(&format!("{:<11} {:>9}  {}\n", "", "", p.note));
        }
        for i in p.items.iter().take(20) {
            out.push_str(&format!("{:<11} {:>9}  - {i}\n", "", ""));
        }
        if p.items.len() > 20 {
            out.push_str(&format!(
                "{:<11} {:>9}  ... and {} more\n",
                "",
                "",
                p.items.len() - 20
            ));
        }
    }
    out.push_str(&format!("total       {:>9}\n", human(total)));
    out
}

pub fn render_json(rows: &[(&Target, Preview)]) -> String {
    use crate::health::json_escape;
    let items: Vec<String> = rows
        .iter()
        .map(|(t, p)| {
            let ev: Vec<String> = p.items.iter().map(|i| format!("\"{}\"", json_escape(i))).collect();
            format!(
                "{{\"id\":\"{}\",\"risk\":\"{}\",\"needs_root\":{},\"bytes\":{},\"note\":\"{}\",\"items\":[{}]}}",
                t.id,
                t.risk.as_str(),
                t.needs_root,
                p.bytes.map_or("null".to_string(), |b| b.to_string()),
                json_escape(&p.note),
                ev.join(",")
            )
        })
        .collect();
    format!("{{\"schema\":1,\"targets\":[{}]}}\n", items.join(","))
}

pub const USAGE: &str = "usage: samurai clean [<target>...] [--preview] [--json] [--yes] [--keep N]

Frees disk space. With no target, or `all`, only the targets that are safe to run
unattended are used. `--preview` shows how much each would free and changes nothing.

targets
  pkgcache     old and uninstalled package versions (paccache; --keep N, default 2)
  journal      systemd journal above 200M
  coredumps    stored coredumps
  thumbnails   thumbnail cache of the current user
  orphans      unneeded dependency packages (never part of `all`; asks first)
  cache        the whole ~/.cache (never part of `all`; asks first)
  all          pkgcache journal coredumps thumbnails

  --preview    show sizes, change nothing
  --json       with --preview: machine-readable
  --yes        approve the confirmation of orphans/cache
  --keep N     versions of each package pkgcache keeps (default 2)

System targets need root. Exit codes: 0 ok, 1 a target failed or was refused, 2 usage error.
";

#[derive(Debug, PartialEq, Eq)]
pub struct Options {
    pub targets: Vec<&'static str>,
    pub preview: bool,
    pub json: bool,
    pub yes: bool,
    pub keep: usize,
    pub help: bool,
}

pub fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut o = Options {
        targets: Vec::new(),
        preview: false,
        json: false,
        yes: false,
        keep: 2,
        help: false,
    };
    let mut it = args.iter();
    let mut all = false;
    while let Some(a) = it.next() {
        match a.as_str() {
            "-h" | "--help" => o.help = true,
            "--preview" => o.preview = true,
            "--json" => o.json = true,
            "--yes" | "-y" => o.yes = true,
            "--keep" => {
                let v = it.next().ok_or("--keep needs a number")?;
                o.keep = v
                    .parse()
                    .ok()
                    .filter(|n| *n <= 100)
                    .ok_or(format!("bad --keep: {v}"))?;
            }
            "all" => all = true,
            id => match find(id) {
                Some(t) => {
                    if !o.targets.contains(&t.id) {
                        o.targets.push(t.id);
                    }
                }
                None if id.starts_with('-') => return Err(format!("unknown option: {id}")),
                None => return Err(format!("unknown target: {id}")),
            },
        }
    }
    if o.json && !o.preview {
        return Err("--json only applies to --preview".into());
    }
    if all || o.targets.is_empty() {
        for t in all_selection() {
            if !o.targets.contains(&t.id) {
                o.targets.push(t.id);
            }
        }
    }
    Ok(o)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::health::system::fake::Fake;

    fn ctx() -> Ctx {
        Ctx {
            home: Some("/home/alice".into()),
            keep: 2,
        }
    }

    fn a(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| (*x).to_string()).collect()
    }

    #[test]
    fn sizes_parse_and_print() {
        assert_eq!(
            parse_size("12.34 MiB"),
            Some((12.34 * 1048576.0_f64).round() as u64)
        );
        assert_eq!(
            parse_size("1.2G"),
            Some((1.2 * 1073741824.0_f64).round() as u64)
        );
        assert_eq!(parse_size("512B"), Some(512));
        assert_eq!(parse_size("7"), Some(7));
        assert_eq!(parse_size("x"), None);
        assert_eq!(parse_size("5 parsecs"), None);
        assert_eq!(human(512), "512 B");
        assert_eq!(human(1536), "1.5 KiB");
        assert_eq!(human(25 * 1048576), "25 MiB");
    }

    #[test]
    fn pkgcache_preview_reads_paccache_dry_run() {
        let sys = Fake::default().cmd(
            "paccache -d -k 2",
            0,
            "==> finished dry run: 12 candidates (disk space saved: 345.5 MiB)\n",
        );
        let p = pkgcache_preview(&sys, &ctx());
        assert_eq!(p.bytes, Some((345.5 * 1048576.0_f64).round() as u64));
        let none = pkgcache_preview(&Fake::default(), &ctx());
        assert_eq!(none.bytes, None);
        assert!(none.note.contains("not installed"));
    }

    #[test]
    fn pkgcache_apply_trims_then_removes_uninstalled() {
        let sys = Fake::default()
            .cmd("paccache -r -k 2", 0, "")
            .cmd("paccache -r -u -k 0", 0, "");
        assert!(pkgcache_apply(&sys, &ctx()).is_ok());
        assert_eq!(sys.calls(), vec!["paccache -r -k 2", "paccache -r -u -k 0"]);
        let failing = Fake::default().cmd("paccache -r -k 2", 1, "");
        assert!(pkgcache_apply(&failing, &ctx())
            .unwrap_err()
            .contains("exited 1"));
    }

    #[test]
    fn orphans_list_and_removal() {
        let sys = Fake::default().cmd("pacman -Qdtq", 0, "tree\nfoo\n").cmd(
            "pacman -Rns --noconfirm tree foo",
            0,
            "",
        );
        assert_eq!(orphans_preview(&sys, &ctx()).items, vec!["tree", "foo"]);
        assert_eq!(
            orphans_apply(&sys, &ctx()).unwrap(),
            "removed 2 orphan package(s)"
        );
        // pacman -Qdtq exits 1 with no output when there are none
        let none = Fake::default().cmd("pacman -Qdtq", 1, "");
        assert!(orphans_preview(&none, &ctx()).items.is_empty());
        assert_eq!(orphans_apply(&none, &ctx()).unwrap(), "no orphans");
        assert!(
            !none.calls().iter().any(|c| c.contains("-Rns")),
            "nothing is removed when there are no orphans"
        );
    }

    #[test]
    fn journal_frees_everything_above_the_cap() {
        let sys = Fake::default().cmd(
            "journalctl --disk-usage",
            0,
            "Archived and active journals take up 1.2G in the file system.\n",
        );
        let p = journal_preview(&sys, &ctx());
        assert_eq!(
            p.bytes,
            Some(parse_size("1.2G").unwrap() - parse_size("200M").unwrap())
        );
        let small = Fake::default().cmd(
            "journalctl --disk-usage",
            0,
            "Archived and active journals take up 50.0M in the file system.\n",
        );
        assert_eq!(journal_preview(&small, &ctx()).bytes, Some(0));
        let ok = Fake::default().cmd("journalctl --vacuum-size=200M", 0, "");
        assert!(journal_apply(&ok, &ctx()).is_ok());
    }

    #[test]
    fn coredumps_use_a_fixed_directory() {
        let sys = Fake::default().cmd(
            "du -sb /var/lib/systemd/coredump",
            0,
            "4096\t/var/lib/systemd/coredump\n",
        );
        assert_eq!(coredumps_preview(&sys, &ctx()).bytes, Some(4096));
        let ok = Fake::default().cmd(
            "find /var/lib/systemd/coredump -mindepth 1 -type f -delete",
            0,
            "",
        );
        assert!(coredumps_apply(&ok, &ctx()).is_ok());
    }

    #[test]
    fn user_targets_are_confined_to_the_home_directory() {
        let sys = Fake::default()
            .path("/home/alice/.cache/thumbnails")
            .cmd("du -sb /home/alice/.cache/thumbnails", 0, "1000\t/x\n")
            .cmd(
                "find /home/alice/.cache/thumbnails -mindepth 1 -delete",
                0,
                "",
            );
        assert_eq!(thumbnails_preview(&sys, &ctx()).bytes, Some(1000));
        assert!(thumbnails_apply(&sys, &ctx())
            .unwrap()
            .contains("/home/alice/.cache/thumbnails"));
        // unsafe or missing homes never reach a command
        for home in [
            None,
            Some("/"),
            Some(""),
            Some("relative/dir"),
            Some("/home/../etc"),
        ] {
            let c = Ctx {
                home: home.map(str::to_string),
                keep: 2,
            };
            let s = Fake::default();
            assert_eq!(cache_preview(&s, &c).bytes, None, "{home:?}");
            assert!(cache_apply(&s, &c).is_err(), "{home:?}");
            assert!(s.calls().is_empty(), "{home:?} ran {:?}", s.calls());
        }
        // an absent directory is a no-op, not a failure and not a find call
        let none = Fake::default();
        assert!(thumbnails_apply(&none, &ctx())
            .unwrap()
            .contains("nothing to remove"));
        assert!(none.calls().is_empty());
    }

    #[test]
    fn all_means_safe_targets_only() {
        let ids: Vec<&str> = all_selection().iter().map(|t| t.id).collect();
        assert_eq!(ids, vec!["pkgcache", "journal", "coredumps", "thumbnails"]);
        assert!(find("orphans").is_some_and(|t| t.risk == Risk::NeedsConfirm));
        assert!(find("cache").is_some_and(|t| t.risk == Risk::NeedsConfirm));
    }

    #[test]
    fn argument_parsing() {
        let o = parse_args(&[]).unwrap();
        assert_eq!(
            o.targets,
            vec!["pkgcache", "journal", "coredumps", "thumbnails"]
        );
        assert_eq!(o.keep, 2);
        let o = parse_args(&a(&["orphans", "--yes"])).unwrap();
        assert_eq!(o.targets, vec!["orphans"]);
        assert!(o.yes);
        let o = parse_args(&a(&[
            "all",
            "orphans",
            "--preview",
            "--json",
            "--keep",
            "0",
        ]))
        .unwrap();
        assert_eq!(
            o.targets,
            vec!["orphans", "pkgcache", "journal", "coredumps", "thumbnails"]
        );
        assert!(o.preview && o.json && o.keep == 0);
        for bad in [
            &["nope"][..],
            &["--bogus"],
            &["--keep"],
            &["--keep", "x"],
            &["--keep", "999"],
            &["--json"],
        ] {
            assert!(parse_args(&a(bad)).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn preview_rendering_text_and_json() {
        let rows: Vec<(&Target, Preview)> = vec![
            (
                find("pkgcache").unwrap(),
                Preview {
                    bytes: Some(3 * 1048576),
                    note: "keeps 2".into(),
                    items: vec![],
                },
            ),
            (
                find("orphans").unwrap(),
                Preview {
                    bytes: None,
                    note: String::new(),
                    items: vec!["tree".into(), "a\"b".into()],
                },
            ),
        ];
        let t = render_preview(&rows);
        assert!(t.contains("pkgcache      3.0 MiB"), "{t}");
        assert!(
            t.contains("- tree") && t.contains("total         3.0 MiB"),
            "{t}"
        );
        let j = render_json(&rows);
        assert!(j.starts_with("{\"schema\":1,\"targets\":[{\"id\":\"pkgcache\",\"risk\":\"safe\",\"needs_root\":true,\"bytes\":3145728"));
        assert!(j.contains("\"bytes\":null") && j.contains("a\\\"b"), "{j}");
    }
}

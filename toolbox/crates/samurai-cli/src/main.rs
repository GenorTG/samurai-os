//! `samurai`: single entry point (spec 01/04). Replaces the shell dispatcher `updates/samurai`
//! behind the same interface; executors stay separate programs.

use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use samurai_core::health::{self, checks::ALL, system::RealSystem};
use samurai_core::status;
use samurai_core::{
    parse, parse_pacman_q, read_version_file, resolve_tool, version_line, Action,
    FALLBACK_TOOL_DIR, TOOLS_VERSION_FILE, USAGE,
};

fn pacman_version() -> Option<String> {
    let out = Command::new("pacman")
        .args(["-Q", "samurai-tools"])
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| parse_pacman_q(&String::from_utf8_lossy(&out.stdout)))
        .flatten()
}

fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .and_then(std::fs::canonicalize)
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn run_tool(tool: &str, prefix: &[&str], rest: &[String]) -> ExitCode {
    let path = resolve_tool(&exe_dir(), Path::new(FALLBACK_TOOL_DIR), tool);
    if !samurai_core::is_executable(&path) {
        eprintln!(
            "samurai: command \"{tool}\" is not installed ({} missing)",
            path.display()
        );
        return ExitCode::from(2);
    }
    let err = Command::new(&path).args(prefix).args(rest).exec();
    eprintln!("samurai: cannot run {}: {err}", path.display());
    ExitCode::from(2)
}

fn confirm_on_tty(yes: bool, id: &str, summary: &str) -> bool {
    use std::io::{BufRead, IsTerminal, Write};
    if yes {
        return true;
    }
    if !std::io::stdin().is_terminal() {
        eprintln!("samurai health: {id} needs confirmation; not a terminal, skipping (use --yes)");
        return false;
    }
    eprint!("{id}: {summary}\nApply the fix? [y/N] ");
    let _ = std::io::stderr().flush();
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line).is_ok() && matches!(line.trim(), "y" | "Y" | "yes")
}

fn run_health_fix(opts: &health::Options) -> ExitCode {
    use health::{system::System, FixAction, Snapshot};
    let sys = RealSystem;
    if sys
        .run("id", &["-u"])
        .is_none_or(|o| o.stdout.trim() != "0")
    {
        eprintln!("samurai health: --fix needs root (try: sudo samurai health --fix)");
        return ExitCode::from(1);
    }
    let findings = health::run(&sys, ALL, &opts.filter);
    if !health::has_actionable_fix(ALL, &findings) {
        print!("{}", health::render_text(&findings));
        println!("nothing to fix automatically");
        return ExitCode::from(health::exit_code(&findings) as u8);
    }
    match health::pre_fix_snapshot(&sys) {
        Snapshot::Created => println!("snapshot: created (snapper root)"),
        Snapshot::Unavailable(why) => {
            eprintln!("samurai health: no pre-fix snapshot ({why}); continuing")
        }
        Snapshot::Failed(why) => {
            eprintln!("samurai health: pre-fix snapshot failed ({why}); not fixing anything");
            return ExitCode::from(1);
        }
    }
    let yes = opts.yes;
    let reports = health::apply_fixes(&sys, ALL, &findings, &mut |f, i| {
        confirm_on_tty(yes, f.id, &i.summary)
    });
    print!("{}", health::render_fix_text(&reports));
    let unresolved = reports
        .iter()
        .any(|r| !matches!(r.action, FixAction::Fixed(_)));
    ExitCode::from(u8::from(unresolved))
}

fn run_health(args: &[String]) -> ExitCode {
    let opts = match health::parse_args(args, ALL) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("samurai health: {e}\n");
            eprint!("{}", health::USAGE);
            return ExitCode::from(2);
        }
    };
    if opts.help {
        print!("{}", health::USAGE);
        return ExitCode::SUCCESS;
    }
    if opts.list {
        for c in ALL {
            println!("{}", c.id);
        }
        return ExitCode::SUCCESS;
    }
    if opts.fix {
        return run_health_fix(&opts);
    }
    let findings = health::run(&RealSystem, ALL, &opts.filter);
    let epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    if opts.write {
        // the stored report always covers every check, whatever filter the run used
        let all = if opts.filter == health::Filter::default() {
            None
        } else {
            Some(health::run(&RealSystem, ALL, &health::Filter::default()))
        };
        let report = health::render_json(all.as_deref().unwrap_or(&findings), epoch);
        if let Err(e) = health::write_atomic(Path::new(health::REPORT_PATH), &report) {
            eprintln!("samurai health: cannot write {}: {e}", health::REPORT_PATH);
            return ExitCode::from(1);
        }
    }
    if opts.json {
        print!("{}", health::render_json(&findings, epoch));
    } else {
        print!("{}", health::render_text(&findings));
    }
    ExitCode::from(health::exit_code(&findings) as u8)
}

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let cmd = args.next().unwrap_or_else(|| "help".to_string());
    let rest: Vec<String> = args.collect();

    match parse(&cmd) {
        Action::Help => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        Action::Status if status::is_native(&rest) => {
            let dir = status::state_dir(
                std::env::var("XDG_STATE_HOME").ok().as_deref(),
                std::env::var("HOME").ok().as_deref(),
            );
            let json = rest.first().is_some_and(|a| a == "--json");
            print!(
                "{}",
                status::render(dir.as_deref(), Path::new(status::REPORT_PATH), json)
            );
            ExitCode::SUCCESS
        }
        // other flags (--check, --help, unknown): the notifier owns their semantics
        Action::Status => run_tool("notifier", &["--status"], &rest),
        Action::Health => run_health(&rest),
        Action::Version => {
            let v = read_version_file(Path::new(TOOLS_VERSION_FILE)).or_else(pacman_version);
            println!("{}", version_line(v.as_deref()));
            ExitCode::SUCCESS
        }
        Action::Unknown(c) => {
            eprintln!("samurai: unknown command: {c}\n");
            eprint!("{USAGE}");
            ExitCode::from(2)
        }
        Action::Exec { tool, prefix } => run_tool(tool, prefix, &rest),
    }
}

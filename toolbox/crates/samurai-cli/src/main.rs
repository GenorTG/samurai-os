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
    let findings = health::run(&RealSystem, ALL, &opts.filter);
    if opts.json {
        let epoch = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
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

//! `samurai`: single entry point (spec 01/04). Replaces the shell dispatcher `updates/samurai`
//! behind the same interface; executors stay separate programs.

use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use samurai_core::{
    parse, parse_pacman_q, read_version_file, resolve_tool, version_line, Action, FALLBACK_TOOL_DIR,
    TOOLS_VERSION_FILE, USAGE,
};

fn pacman_version() -> Option<String> {
    let out = Command::new("pacman").args(["-Q", "samurai-tools"]).output().ok()?;
    out.status.success().then(|| parse_pacman_q(&String::from_utf8_lossy(&out.stdout))).flatten()
}

fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .and_then(std::fs::canonicalize)
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
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
        Action::Exec { tool, prefix } => {
            let path = resolve_tool(&exe_dir(), Path::new(FALLBACK_TOOL_DIR), tool);
            if !samurai_core::is_executable(&path) {
                eprintln!("samurai: command \"{tool}\" is not installed ({} missing)", path.display());
                return ExitCode::from(2);
            }
            let err = Command::new(&path).args(prefix).args(&rest).exec();
            eprintln!("samurai: cannot run {}: {err}", path.display());
            ExitCode::from(2)
        }
    }
}

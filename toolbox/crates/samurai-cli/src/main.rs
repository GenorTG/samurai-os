//! `samurai`: single entry point (spec 01/04). Replaces the shell dispatcher `updates/samurai`
//! behind the same interface; executors stay separate programs.

use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use samurai_core::clean;
use samurai_core::diag::{self, Cmd};
use samurai_core::fix;
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

fn now_epoch() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Two typed confirmations on a terminal, or refuse: an unredacted bundle holds private data.
fn confirm_unredacted() -> bool {
    use std::io::{BufRead, IsTerminal, Write};
    if !std::io::stdin().is_terminal() {
        eprintln!("samurai diag: --no-redact needs a terminal (it asks for confirmation twice)");
        return false;
    }
    let ask = |prompt: &str, expect: &str| {
        eprint!("{prompt}");
        let _ = std::io::stderr().flush();
        let mut line = String::new();
        std::io::stdin().lock().read_line(&mut line).is_ok() && line.trim() == expect
    };
    ask("An unredacted bundle contains user names, addresses and serial numbers.\nType 'yes' to continue: ", "yes")
        && ask("Type 'unredacted' to confirm: ", "unredacted")
}

fn write_private(path: &Path, text: &str, owner: Option<(u32, u32)>) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    f.write_all(text.as_bytes())?;
    if let Some((uid, gid)) = owner {
        std::os::unix::fs::chown(path, Some(uid), Some(gid))?;
    }
    Ok(())
}

/// Home directory of the real user: the `sudo` caller's (from /etc/passwd), else `$HOME`.
fn real_home(sys: &RealSystem) -> Option<String> {
    match sudo_target(sys) {
        Some(u) => diag::home_of(sys, &u),
        None => std::env::var("HOME").ok(),
    }
}

/// The user `sudo` was invoked by, when we are root on their behalf.
fn sudo_target(sys: &RealSystem) -> Option<String> {
    use health::system::System;
    let is_root = sys
        .run("id", &["-u"])
        .is_some_and(|o| o.stdout.trim() == "0");
    let user = std::env::var("SUDO_USER")
        .ok()
        .filter(|u| !u.is_empty() && u != "root");
    user.filter(|_| is_root)
}

fn run_fix(args: &[String]) -> ExitCode {
    use health::{system::System, Snapshot};
    let opts = match fix::parse_args(args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("samurai fix: {e}\n");
            eprint!("{}", fix::USAGE);
            return ExitCode::from(2);
        }
    };
    let Some(repair) = opts.repair.filter(|_| !opts.help) else {
        print!("{}", fix::USAGE);
        return ExitCode::SUCCESS;
    };
    let sys = RealSystem;
    if sys
        .run("id", &["-u"])
        .is_none_or(|o| o.stdout.trim() != "0")
    {
        eprintln!(
            "samurai fix: needs root (try: sudo samurai fix {})",
            repair.name()
        );
        return ExitCode::from(1);
    }
    if repair.needs_confirm()
        && !confirm_on_tty(
            opts.yes,
            repair.name(),
            "deletes /etc/pacman.d/gnupg and rebuilds the keyring from scratch",
        )
    {
        println!("declined {}", repair.name());
        return ExitCode::from(1);
    }
    if repair.wants_snapshot() {
        match health::pre_fix_snapshot(&sys) {
            Snapshot::Created => println!("snapshot: created (snapper root)"),
            Snapshot::Unavailable(why) => {
                eprintln!("samurai fix: no pre-fix snapshot ({why}); continuing")
            }
            Snapshot::Failed(why) => {
                eprintln!("samurai fix: pre-fix snapshot failed ({why}); not repairing anything");
                return ExitCode::from(1);
            }
        }
    }
    match fix::run(&sys, repair) {
        Ok(changes) => {
            println!("fixed    {}: {}", repair.name(), changes.join("; "));
            ExitCode::SUCCESS
        }
        Err(e) => {
            println!("failed   {}: {e}", repair.name());
            ExitCode::from(1)
        }
    }
}

fn run_clean(args: &[String]) -> ExitCode {
    use health::system::System;
    let opts = match clean::parse_args(args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("samurai clean: {e}\n");
            eprint!("{}", clean::USAGE);
            return ExitCode::from(2);
        }
    };
    if opts.help {
        print!("{}", clean::USAGE);
        return ExitCode::SUCCESS;
    }
    let sys = RealSystem;
    let user = sudo_target(&sys).and_then(|u| diag::ids_of(&sys, &u));
    let ctx = clean::Ctx {
        home: real_home(&sys),
        keep: opts.keep,
        user,
    };
    let targets: Vec<&clean::Target> = opts
        .targets
        .iter()
        .filter_map(|id| clean::find(id))
        .collect();
    if opts.preview {
        let rows: Vec<_> = targets
            .iter()
            .map(|t| (*t, (t.preview)(&sys, &ctx)))
            .collect();
        if opts.json {
            print!("{}", clean::render_json(&rows));
        } else {
            print!("{}", clean::render_preview(&rows));
        }
        return ExitCode::SUCCESS;
    }
    let is_root = sys
        .run("id", &["-u"])
        .is_some_and(|o| o.stdout.trim() == "0");
    let mut failed = false;
    for t in targets {
        if t.needs_root && !is_root {
            println!(
                "skipped  {}: needs root (try: sudo samurai clean {})",
                t.id, t.id
            );
            failed = true;
            continue;
        }
        let before = (t.preview)(&sys, &ctx);
        if t.risk == clean::Risk::NeedsConfirm {
            let what = format!("{} ({} item(s))", t.description, before.items.len());
            if !confirm_on_tty(opts.yes, t.id, &what) {
                println!("declined {}", t.id);
                failed = true;
                continue;
            }
        }
        match (t.apply)(&sys, &ctx) {
            Ok(msg) => {
                let freed = before
                    .bytes
                    .map_or(String::new(), |b| format!(" (about {})", clean::human(b)));
                println!("cleaned  {}: {msg}{freed}", t.id);
            }
            Err(e) => {
                println!("failed   {}: {e}", t.id);
                failed = true;
            }
        }
    }
    ExitCode::from(u8::from(failed))
}

fn run_diag(args: &[String]) -> ExitCode {
    let cmd = match diag::parse_args(args) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("samurai diag: {e}\n");
            eprint!("{}", diag::USAGE);
            return ExitCode::from(2);
        }
    };
    let sys = RealSystem;
    let sections = match cmd {
        Cmd::Help => {
            print!("{}", diag::USAGE);
            return ExitCode::SUCCESS;
        }
        Cmd::Info => diag::info(&sys),
        Cmd::Services => diag::services(&sys),
        Cmd::Logs(o) => diag::logs(&sys, &o),
        Cmd::Disks => diag::disks(&sys),
        Cmd::Gpu => diag::gpu(&sys),
        Cmd::Bundle { redact, out } => return run_diag_bundle(&sys, redact, out),
    };
    print!("{}", diag::render(&sections));
    ExitCode::SUCCESS
}

fn run_diag_bundle(sys: &RealSystem, redact: bool, out: Option<String>) -> ExitCode {
    if !redact && !confirm_unredacted() {
        return ExitCode::from(2);
    }
    let epoch = now_epoch();
    // under sudo the bundle belongs in the invoking user's home, owned by them
    let user = sudo_target(sys);
    let home = match &user {
        Some(u) => diag::home_of(sys, u),
        None => std::env::var("HOME").ok(),
    };
    let path = match (out, home.clone()) {
        (Some(p), _) => std::path::PathBuf::from(p),
        (None, Some(h)) => Path::new(&h).join(format!("samurai-diag-{epoch}.txt")),
        (None, None) => {
            eprintln!("samurai diag: cannot find a home directory; pass --out FILE");
            return ExitCode::from(1);
        }
    };
    let path = match std::env::current_dir() {
        Ok(cwd) if path.is_relative() => cwd.join(path),
        _ => path,
    };
    // root writing for a sudo user creates a file and hands it over: only inside their home
    if user.is_some()
        && !home
            .as_deref()
            .is_some_and(|h| diag::confined(Path::new(h), &path))
    {
        eprintln!("samurai diag: under sudo, --out must be inside your home directory");
        return ExitCode::from(2);
    }
    let mut text = diag::render(&diag::bundle(sys, epoch));
    if redact {
        let mut extra: Vec<String> = user.iter().cloned().collect();
        extra.extend(std::env::var("USER").ok());
        text = diag::redactor(sys, &extra).redact(&text);
    }
    let owner = user.as_deref().and_then(|u| diag::ids_of(sys, u));
    if let Err(e) = write_private(&path, &text, owner) {
        eprintln!("samurai diag: cannot write {}: {e}", path.display());
        return ExitCode::from(1);
    }
    println!(
        "{} ({})",
        path.display(),
        if redact {
            "redacted"
        } else {
            "NOT redacted: contains private data"
        }
    );
    ExitCode::SUCCESS
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
        Action::Diag => run_diag(&rest),
        Action::Clean => run_clean(&rest),
        Action::Fix => run_fix(&rest),
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

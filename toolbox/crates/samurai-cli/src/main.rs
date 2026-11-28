//! `samurai`: single entry point (spec 01/04). Replaces the shell dispatcher `updates/samurai`
//! behind the same interface; executors stay separate programs.

use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use samurai_core::boot;
use samurai_core::clean;
use samurai_core::diag::{self, Cmd};
use samurai_core::firewall;
use samurai_core::fix;
use samurai_core::health::{self, checks::ALL, system::RealSystem};
use samurai_core::kernel;
use samurai_core::mirrors;
use samurai_core::security;
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
        eprintln!("samurai: {id} needs confirmation; not a terminal, skipping (use --yes)");
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

/// The user who asked through pkexec (the GUI): their name must be redacted too.
fn pkexec_user(sys: &RealSystem) -> Option<String> {
    use health::system::System;
    let uid: u32 = std::env::var("PKEXEC_UID").ok()?.parse().ok()?;
    sys.read("/etc/passwd")?.lines().find_map(|l| {
        let f: Vec<&str> = l.split(':').collect();
        let name = f.first()?;
        (f.get(2)?.parse::<u32>().ok()? == uid).then(|| (*name).to_string())
    })
}

fn run_kernel(args: &[String]) -> ExitCode {
    use health::system::System;
    let cmd = match kernel::parse_args(args) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("samurai kernel: {e}\n");
            eprint!("{}", kernel::USAGE);
            return ExitCode::from(2);
        }
    };
    let sys = RealSystem;
    if cmd == kernel::Cmd::Help {
        print!("{}", kernel::USAGE);
        return ExitCode::SUCCESS;
    }
    if !matches!(cmd, kernel::Cmd::List { .. })
        && sys
            .run("id", &["-u"])
            .is_none_or(|o| o.stdout.trim() != "0")
    {
        eprintln!("samurai kernel: needs root (try: sudo samurai kernel ...)");
        return ExitCode::from(1);
    }
    let result: Result<(), String> = match cmd {
        kernel::Cmd::Help => Ok(()),
        kernel::Cmd::List { json: false } => kernel::list(&sys).map(|t| print!("{t}")),
        kernel::Cmd::List { json: true } => kernel::list_json(&sys).map(|t| print!("{t}")),
        kernel::Cmd::Install { name, yes } => kernel::plan_install(&sys, &name).and_then(|plan| {
            for w in &plan.warnings {
                eprintln!("warning: {w}");
            }
            if !plan.warnings.is_empty() && !confirm_on_tty(yes, "kernel install", "install anyway")
            {
                return Err("declined".into());
            }
            kernel::install(&sys, &plan)?;
            // the pacman hook already did this; running it again reports the real state
            boot::sync(&sys, true).map(|_| println!("{name} installed; boot menu up to date"))
        }),
        kernel::Cmd::Remove { name } => kernel::plan_remove(&sys, &name).and_then(|packages| {
            kernel::remove(&sys, &packages)?;
            boot::sync(&sys, true).map(|_| println!("{name} removed; boot menu up to date"))
        }),
        kernel::Cmd::Default { name } => boot::set_default(&sys, &name).map(|m| println!("{m}")),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("samurai kernel: {e}");
            ExitCode::from(1)
        }
    }
}

fn run_boot(args: &[String]) -> ExitCode {
    use health::system::System;
    let cmd = match boot::parse_args(args) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("samurai boot: {e}\n");
            eprint!("{}", boot::USAGE);
            return ExitCode::from(2);
        }
    };
    let report = |r: Result<String, String>| match r {
        Ok(text) => {
            println!("{}", text.trim_end());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("samurai boot: {e}");
            ExitCode::from(1)
        }
    };
    let check = match cmd {
        boot::Cmd::Help => {
            print!("{}", boot::USAGE);
            return ExitCode::SUCCESS;
        }
        boot::Cmd::Get { json: false } => return report(boot::settings::get(&RealSystem)),
        boot::Cmd::Get { json: true } => return report(boot::settings::get_json(&RealSystem)),
        boot::Cmd::Set(change) => {
            if RealSystem
                .run("id", &["-u"])
                .is_none_or(|o| o.stdout.trim() != "0")
            {
                eprintln!("samurai boot: set needs root (try: sudo samurai boot set ...)");
                return ExitCode::from(1);
            }
            return report(boot::settings::set(&RealSystem, &change));
        }
        boot::Cmd::Sync { check } => check,
    };
    match boot::sync(&RealSystem, !check) {
        Ok(boot::Outcome::InSync) => {
            println!("boot entries match the installed kernels");
            ExitCode::SUCCESS
        }
        Ok(boot::Outcome::NotManaged(why)) => {
            println!("nothing to do: {why}");
            ExitCode::SUCCESS
        }
        Ok(boot::Outcome::Changed(lines)) => {
            for l in &lines {
                println!("{}{l}", if check { "would " } else { "" });
            }
            ExitCode::from(u8::from(check))
        }
        Err(e) => {
            eprintln!("samurai boot: {e}");
            ExitCode::from(1)
        }
    }
}

fn run_security(args: &[String]) -> ExitCode {
    let opts = match security::parse_args(args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("samurai security: {e}\n");
            eprint!("{}", security::USAGE);
            return ExitCode::from(2);
        }
    };
    if opts.help {
        print!("{}", security::USAGE);
        return ExitCode::SUCCESS;
    }
    let root = security::is_root(&RealSystem);
    let items = security::collect(&RealSystem, opts.online);
    if opts.json {
        print!("{}", security::render_json(&items, root));
    } else {
        print!("{}", security::render_text(&items, root));
    }
    ExitCode::from(security::exit_code(&items) as u8)
}

fn run_firewall(args: &[String]) -> ExitCode {
    let cmd = match firewall::parse_args(args) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("samurai firewall: {e}\n");
            eprint!("{}", firewall::USAGE);
            return ExitCode::from(2);
        }
    };
    match firewall_command(&RealSystem, cmd) {
        Ok(text) => {
            print!("{text}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("samurai firewall: {e}");
            ExitCode::from(1)
        }
    }
}

/// Preview, then (unless --dry-run) require root, ask, apply and verify.
fn firewall_command(sys: &RealSystem, cmd: firewall::Cmd) -> Result<String, String> {
    let may_change = |flags: firewall::Flags, what: &str| -> Result<(), String> {
        if !security::is_root(sys) {
            return Err("changing the firewall needs root (try: sudo samurai firewall ...)".into());
        }
        if !confirm_on_tty(flags.yes, "firewall", what) {
            return Err("declined; nothing changed".into());
        }
        Ok(())
    };
    let shown = |commands: &[Vec<String>]| -> String {
        commands
            .iter()
            .map(|c| format!("  firewall-cmd {}\n", c.join(" ")))
            .collect()
    };
    match cmd {
        firewall::Cmd::Help => Ok(firewall::USAGE.to_string()),
        firewall::Cmd::Status { json } => firewall::status(sys).map(|z| {
            if json {
                firewall::render_status_json(&z)
            } else {
                firewall::render_status(&z)
            }
        }),
        firewall::Cmd::Presets { json } => Ok(firewall::render_presets(sys, json)),
        firewall::Cmd::Change {
            open,
            target,
            zone,
            flags,
        } => {
            // firewalld answers a normal user only partially: refuse early, with the reason
            if !flags.dry_run && !security::is_root(sys) {
                return Err(
                    "changing the firewall needs root (try: sudo samurai firewall ...)".into(),
                );
            }
            let plan = firewall::plan(sys, open, &target, zone.as_deref())?;
            if plan.steps.is_empty() {
                return Ok(format!(
                    "nothing to change: already {} in zone {}\n",
                    if open { "open" } else { "closed" },
                    plan.zone
                ));
            }
            println!("{}", plan.preview.join("\n"));
            print!("{}", shown(&plan.commands()));
            if flags.dry_run {
                return Ok("dry run: nothing changed\n".into());
            }
            may_change(flags, &plan.preview[0])?;
            firewall::apply(sys, &plan)?;
            Ok(format!(
                "done: {} in zone {} (now and after reboot)\n",
                if open { "opened" } else { "closed" },
                plan.zone
            ))
        }
        firewall::Cmd::Zone {
            interface,
            zone,
            flags,
        } => {
            let commands = firewall::zone_commands(sys, &interface, &zone)?;
            print!("{}", shown(&commands));
            if zone == "trusted" {
                println!("warning: zone trusted accepts every connection on {interface}");
            }
            if flags.dry_run {
                return Ok("dry run: nothing changed\n".into());
            }
            may_change(flags, &format!("move {interface} to zone {zone}"))?;
            firewall::set_zone(sys, &interface, &zone)?;
            Ok(format!("done: {interface} is in zone {zone}\n"))
        }
    }
}

fn run_mirrors(args: &[String]) -> ExitCode {
    use health::system::System;
    let cmd = match mirrors::parse_args(args) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("samurai mirrors: {e}\n");
            eprint!("{}", mirrors::USAGE);
            return ExitCode::from(2);
        }
    };
    let sys = RealSystem;
    let result = match cmd {
        mirrors::Cmd::Help => {
            print!("{}", mirrors::USAGE);
            return ExitCode::SUCCESS;
        }
        mirrors::Cmd::Status { json: false } => mirrors::status(&sys),
        mirrors::Cmd::Status { json: true } => mirrors::status_json(&sys),
        mirrors::Cmd::Refresh => {
            if sys
                .run("id", &["-u"])
                .is_none_or(|o| o.stdout.trim() != "0")
            {
                eprintln!(
                    "samurai mirrors: refresh needs root (try: sudo samurai mirrors refresh)"
                );
                return ExitCode::from(1);
            }
            mirrors::refresh(&sys).map(|m| format!("refreshed: {m}\n"))
        }
    };
    match result {
        Ok(text) => {
            print!("{text}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("samurai mirrors: {e}");
            ExitCode::from(1)
        }
    }
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
    let to_stdout = out.as_deref() == Some(diag::STDOUT);
    if to_stdout {
        // printed, never written: always redacted (parse_args refuses --no-redact with `-`)
        let mut extra: Vec<String> = user.iter().cloned().collect();
        extra.extend(std::env::var("USER").ok());
        extra.extend(pkexec_user(sys));
        let text = diag::render(&diag::bundle(sys, epoch));
        print!("{}", diag::redactor(sys, &extra).redact(&text));
        return ExitCode::SUCCESS;
    }
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
        Action::Status if rest == ["--report"] => {
            print!("{}", status::render_report(Path::new(status::REPORT_PATH)));
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
        Action::Mirrors => run_mirrors(&rest),
        Action::Boot => run_boot(&rest),
        Action::Kernel => run_kernel(&rest),
        Action::Security => run_security(&rest),
        Action::Firewall => run_firewall(&rest),
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

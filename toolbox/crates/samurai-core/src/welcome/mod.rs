//! `samurai welcome` (spec 08 "First boot"): what is left to do on a new system.
//!
//! Read-only and unprivileged. Every step names the command that does it; the toolbox page shows the
//! same list. Steps that change the disk or the network stay commands the user runs in a terminal.

use crate::health::{json_escape, system::System};
use crate::security::{self, State};

pub const USAGE: &str = "usage: samurai welcome [--json | --text | --autostart]

First steps on a new system. In a graphical session it opens the toolbox's Welcome page.
  --text       print the list here
  --json       machine-readable (schema 1)
  --autostart  used at login: open the window once per user, never on the live medium
";

pub const NOTES: &str = "/var/lib/samurai/firstboot-notes";
pub const LIVE_MARKER: &str = "/run/archiso";
pub const RELEASE_NOTES: &str = "https://github.com/GenorTG/samurai-os/releases";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Auto,
    Text,
    Json,
    Autostart,
    Help,
}

pub fn parse_args(args: &[String]) -> Result<Mode, String> {
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        [] => Ok(Mode::Auto),
        ["--text"] => Ok(Mode::Text),
        ["--json"] => Ok(Mode::Json),
        ["--autostart"] => Ok(Mode::Autostart),
        ["--help" | "-h" | "help"] => Ok(Mode::Help),
        other => Err(format!("unexpected arguments: {}", other.join(" "))),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub id: String,
    pub title: String,
    pub done: bool,
    pub summary: String,
    /// Command for a terminal; empty when there is none.
    pub command: String,
    /// Toolbox page that does it; empty when there is none.
    pub page: &'static str,
}

fn step(id: &str, title: &str, summary: &str, command: &str, page: &'static str) -> Step {
    Step {
        id: id.into(),
        title: title.into(),
        done: false,
        summary: summary.into(),
        command: command.into(),
        page,
    }
}

fn tailscale_step(sys: &dyn System) -> Option<Step> {
    if !sys.exists("/usr/bin/tailscale") {
        return None;
    }
    let running = sys
        .run("tailscale", &["status", "--json"])
        .is_some_and(|o| {
            let compact: String = o.stdout.chars().filter(|c| !c.is_whitespace()).collect();
            compact.contains("\"BackendState\":\"Running\"")
        });
    let mut s = step(
        "tailscale",
        "Connect Tailscale",
        "Reach this machine from your other devices. Nothing is exposed until you open a service for the tailnet zone.",
        "sudo tailscale up",
        "",
    );
    if running {
        s.done = true;
        s.summary = "Tailscale is connected.".into();
        s.command = String::new();
    }
    Some(s)
}

/// Lines `command   # reason` the installer left for choices it could not finish.
fn deferred_steps(sys: &dyn System) -> Vec<Step> {
    sys.read(NOTES)
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .take(20)
        .enumerate()
        .map(|(n, line)| {
            let (command, reason) = line.split_once('#').unwrap_or((line, ""));
            step(
                &format!("deferred-{}", n + 1),
                "Finish an installer choice",
                reason.trim(),
                command.trim(),
                "",
            )
        })
        .collect()
}

pub fn collect(sys: &dyn System) -> Vec<Step> {
    let security = security::collect(sys, false);
    let state = |id: &str| security.iter().find(|i| i.id == id);
    let mut steps = vec![step(
        "update",
        "Update the system",
        "Takes a snapshot first, then upgrades everything.",
        "samurai update",
        "updates",
    )];
    steps.extend(deferred_steps(sys));
    steps.extend(tailscale_step(sys));
    if state("disk.encryption").is_some_and(|i| i.state == State::Pass) {
        steps.push(step(
            "recovery-key",
            "Make a disk recovery key",
            "A second way into the encrypted disk if you forget the passphrase. Shown once: write it down.",
            "sudo samurai security recovery-key",
            "",
        ));
        if sys.exists("/sys/class/tpm/tpm0") {
            steps.push(step(
                "tpm",
                "Unlock the disk with the TPM and a PIN",
                "Optional. A short PIN instead of the passphrase at boot; the passphrase keeps working.",
                "sudo samurai security tpm enroll",
                "",
            ));
        }
    }
    steps.push(step(
        "security",
        "Check the security state",
        "Encryption, firewall, kernel settings, listening services.",
        "sudo samurai security status",
        "",
    ));
    steps.push(step(
        "firewall",
        "Open what you need in the firewall",
        "Everything inbound is refused. One command opens KDE Connect, Steam, Sunshine and more, and closes them again.",
        "samurai firewall presets",
        "",
    ));
    steps.push(step(
        "apps",
        "Choose more software",
        "Games, creative tools, office and developer programs from the catalog.",
        "samurai apps list",
        "apps",
    ));
    steps
}

fn printable(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

pub fn render_text(steps: &[Step]) -> String {
    let mut out = format!("Welcome to Samurai OS. Release notes: {RELEASE_NOTES}\n\n");
    for s in steps {
        out.push_str(&format!(
            "[{}] {}\n    {}\n",
            if s.done { "x" } else { " " },
            s.title,
            printable(&s.summary)
        ));
        if !s.command.is_empty() {
            out.push_str(&format!("    $ {}\n", printable(&s.command)));
        }
    }
    out
}

pub fn render_json(steps: &[Step]) -> String {
    let rows: Vec<String> = steps
        .iter()
        .map(|s| {
            format!(
                "{{\"id\":\"{}\",\"title\":\"{}\",\"done\":{},\"summary\":\"{}\",\"command\":\"{}\",\"page\":\"{}\"}}",
                json_escape(&s.id),
                json_escape(&s.title),
                s.done,
                json_escape(&s.summary),
                json_escape(&s.command),
                s.page
            )
        })
        .collect();
    format!(
        "{{\"schema\":1,\"release_notes\":\"{RELEASE_NOTES}\",\"steps\":[{}]}}\n",
        rows.join(",")
    )
}

/// The per-user marker that the login window has been shown.
pub fn marker(state_home: Option<&str>, home: Option<&str>) -> Option<String> {
    let base = state_home
        .filter(|s| s.starts_with('/'))
        .map(str::to_string)
        .or_else(|| {
            home.filter(|h| h.starts_with('/'))
                .map(|h| format!("{h}/.local/state"))
        })?;
    Some(format!("{base}/samurai/welcome-shown"))
}

/// Whether the login autostart should open the window now.
pub fn should_autostart(sys: &dyn System, marker: &str) -> bool {
    !sys.exists(LIVE_MARKER) && !sys.exists(marker)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::health::system::fake::Fake;

    fn ids(steps: &[Step]) -> Vec<&str> {
        steps.iter().map(|s| s.id.as_str()).collect()
    }

    #[test]
    fn a_bare_system_gets_only_the_general_steps() {
        assert_eq!(
            ids(&collect(&Fake::default())),
            ["update", "security", "firewall", "apps"]
        );
    }

    #[test]
    fn encryption_tpm_tailscale_and_notes_add_steps() {
        let sys = Fake::default()
            .cmd("findmnt -no SOURCE /", 0, "/dev/mapper/l\n")
            .cmd(
                "lsblk -lspno NAME,TYPE /dev/mapper/l",
                0,
                "/dev/mapper/l crypt\n/dev/vda2 part\n",
            )
            .path("/sys/class/tpm/tpm0")
            .path("/usr/bin/tailscale")
            .cmd(
                "tailscale status --json",
                0,
                "{\n \"BackendState\": \"Running\"\n}",
            )
            .file(
                NOTES,
                "samurai apps install x   # needs network \"now\"\n\n",
            );
        let steps = collect(&sys);
        assert_eq!(
            ids(&steps),
            [
                "update",
                "deferred-1",
                "tailscale",
                "recovery-key",
                "tpm",
                "security",
                "firewall",
                "apps"
            ]
        );
        assert_eq!(steps[1].command, "samurai apps install x");
        assert!(steps[2].done);
        assert!(render_json(&steps).contains("needs network \\\"now\\\""));
    }

    #[test]
    fn the_window_opens_once_and_never_on_the_live_medium() {
        let m = marker(None, Some("/home/u")).unwrap();
        assert_eq!(m, "/home/u/.local/state/samurai/welcome-shown");
        assert_eq!(marker(Some("relative"), None), None);
        assert!(should_autostart(&Fake::default(), &m));
        assert!(!should_autostart(&Fake::default().path(&m), &m));
        assert!(!should_autostart(&Fake::default().path(LIVE_MARKER), &m));
    }

    #[test]
    fn arguments() {
        let a = |v: &[&str]| parse_args(&v.iter().map(|s| (*s).to_string()).collect::<Vec<_>>());
        assert_eq!(a(&[]), Ok(Mode::Auto));
        assert_eq!(a(&["--autostart"]), Ok(Mode::Autostart));
        assert!(a(&["--json", "--text"]).is_err());
    }
}

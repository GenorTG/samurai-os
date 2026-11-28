//! `samurai security status` (spec 09): one read-only report of the machine's security state.
//!
//! Every item states what is true on the running system, never what a config file promises.
//! Items that need root say `unknown` for a normal user instead of guessing.

use crate::health::{json_escape, system::System};

pub mod json;
#[cfg(test)]
mod tests;
pub mod tpm;

pub const USAGE: &str = "usage: samurai security status [--json] [--online]
       samurai security tpm enroll|remove [--dry-run]
       samurai security recovery-key [--dry-run]

  status      disk encryption, Secure Boot, TPM, AppArmor, kernel settings, firewall, listeners,
              SSH, Docker ports, accounts, browser policy. Read-only.
  --json      machine-readable (schema 1)
  --online    also ask arch-audit (network) for packages with known vulnerabilities

  tpm enroll    unlock the encrypted disk with this machine's TPM and a PIN
  tpm remove    remove that again
  recovery-key  add a recovery key, shown once
  --dry-run     show what would run, change nothing

The disk passphrase is never removed: it is what you type if the TPM refuses. These three need
root and a terminal.

Run with sudo for the items only root can read. Exit code: 0 no failed item, 1 at least one
failed item, 2 usage error.
";

pub const SYSCTL_FILE: &str = "/usr/lib/sysctl.d/80-samurai-security.conf";
const SECUREBOOT_VAR: &str =
    "/sys/firmware/efi/efivars/SecureBoot-8be4df61-93ca-11d2-aa0d-00e098032b8c";
const SETUPMODE_VAR: &str =
    "/sys/firmware/efi/efivars/SetupMode-8be4df61-93ca-11d2-aa0d-00e098032b8c";
const FIREFOX_POLICY: &str = "/etc/firefox/policies/policies.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Pass,
    Warn,
    Fail,
    /// Cannot be read with these privileges or tools.
    Unknown,
    /// Does not apply here, or an optional feature that is off.
    Skip,
}

impl State {
    pub fn as_str(self) -> &'static str {
        match self {
            State::Pass => "pass",
            State::Warn => "warn",
            State::Fail => "fail",
            State::Unknown => "unknown",
            State::Skip => "skip",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub id: &'static str,
    pub state: State,
    pub summary: String,
    pub details: Vec<String>,
    /// What to do about it; empty when there is nothing to do.
    pub hint: String,
}

fn item(id: &'static str, state: State, summary: impl Into<String>) -> Item {
    Item {
        id,
        state,
        summary: summary.into(),
        details: Vec::new(),
        hint: String::new(),
    }
}

impl Item {
    fn hint(mut self, text: impl Into<String>) -> Self {
        self.hint = text.into();
        self
    }

    fn details(mut self, lines: Vec<String>) -> Self {
        self.details = lines;
        self
    }
}

/// `Some(trimmed stdout)` of a successful command.
fn ok_out(sys: &dyn System, prog: &str, args: &[&str]) -> Option<String> {
    sys.run(prog, args)
        .filter(|o| o.code == 0)
        .map(|o| o.stdout.trim().to_string())
}

fn active(sys: &dyn System, unit: &str) -> bool {
    sys.run("systemctl", &["is-active", unit])
        .is_some_and(|o| o.stdout.trim() == "active")
}

pub fn is_root(sys: &dyn System) -> bool {
    ok_out(sys, "id", &["-u"]).as_deref() == Some("0")
}

/// Whether the root filesystem sits on an encrypted device.
pub fn root_encrypted(sys: &dyn System) -> bool {
    matches!(root_stack(sys), Some((true, _)))
}

/// The block device behind `/` and, when it sits on LUKS, the partition holding the container.
fn root_stack(sys: &dyn System) -> Option<(bool, Option<String>)> {
    let source = ok_out(sys, "findmnt", &["-no", "SOURCE", "/"])?;
    let device = source.split('[').next()?.to_string();
    // -s walks towards the disk: the mapper first, then what it sits on
    let rows = ok_out(sys, "lsblk", &["-lspno", "NAME,TYPE", &device])?;
    let rows: Vec<(&str, &str)> = rows
        .lines()
        .filter_map(|l| {
            let mut f = l.split_whitespace();
            Some((f.next()?, f.next()?))
        })
        .collect();
    let crypt = rows.iter().position(|(_, t)| *t == "crypt");
    let backing = crypt
        .and_then(|i| rows.get(i + 1))
        .map(|(n, _)| (*n).to_string());
    Some((crypt.is_some(), backing))
}

fn disk_items(sys: &dyn System, root: bool) -> Vec<Item> {
    let stack = root_stack(sys);
    let encryption = match &stack {
        Some((true, _)) => item(
            "disk.encryption",
            State::Pass,
            "the root filesystem is on LUKS",
        ),
        Some((false, _)) => item(
            "disk.encryption",
            State::Warn,
            "the root filesystem is not encrypted",
        )
        .hint("A lost or stolen machine exposes every file. Encryption is chosen at install time."),
        None => item(
            "disk.encryption",
            State::Unknown,
            "cannot inspect the root device",
        ),
    };
    let tpm = tpm_item(sys, root, stack.and_then(|(_, backing)| backing));
    vec![encryption, tpm]
}

fn tpm_item(sys: &dyn System, root: bool, backing: Option<String>) -> Item {
    if !sys.exists("/sys/class/tpm/tpm0") {
        return item("disk.tpm", State::Skip, "no TPM found");
    }
    let Some(device) = backing else {
        return item(
            "disk.tpm",
            State::Skip,
            "TPM present; no encrypted root to unlock",
        );
    };
    if !root {
        return item(
            "disk.tpm",
            State::Unknown,
            "TPM present; enrolment needs root to read",
        );
    }
    let Ok(header) = tpm::header(sys, &device) else {
        return item("disk.tpm", State::Unknown, "cannot read the LUKS header");
    };
    let Some(pin) = header.tpm() else {
        return item(
            "disk.tpm",
            State::Skip,
            "TPM unlock is not set up (optional)",
        );
    };
    if pin {
        item("disk.tpm", State::Pass, "TPM unlock with PIN is enrolled")
    } else {
        item(
            "disk.tpm",
            State::Warn,
            "TPM unlock is enrolled without a PIN",
        )
        .details(vec![
            "Anyone holding the machine reaches the login screen with the disk open".into(),
        ])
        .hint("Re-enrol with a PIN: sudo samurai security tpm enroll")
    }
}

/// Value byte of a boolean EFI variable (4 attribute bytes, then the data).
fn efi_flag(sys: &dyn System, path: &str) -> Option<bool> {
    sys.read(path)?.as_bytes().get(4).map(|b| *b == 1)
}

fn secureboot_item(sys: &dyn System) -> Item {
    if !sys.exists("/sys/firmware/efi") {
        return item("boot.secureboot", State::Skip, "not booted with UEFI");
    }
    match efi_flag(sys, SECUREBOOT_VAR) {
        Some(true) => item("boot.secureboot", State::Pass, "Secure Boot is on"),
        Some(false) => {
            let setup = efi_flag(sys, SETUPMODE_VAR) == Some(true);
            item(
                "boot.secureboot",
                State::Warn,
                if setup {
                    "Secure Boot is off (firmware is in setup mode)"
                } else {
                    "Secure Boot is off"
                },
            )
            .hint("Nothing verifies the boot loader: someone with the machine can replace it.")
        }
        None => item(
            "boot.secureboot",
            State::Unknown,
            "the firmware reports no SecureBoot variable",
        ),
    }
}

fn apparmor_item(sys: &dyn System, root: bool) -> Item {
    let enabled = sys
        .read("/sys/module/apparmor/parameters/enabled")
        .is_some_and(|v| v.trim() == "Y")
        && sys
            .read("/sys/kernel/security/lsm")
            .is_some_and(|v| v.trim().split(',').any(|m| m == "apparmor"));
    if !enabled {
        return item("apps.apparmor", State::Fail, "AppArmor is not active on this boot").hint(
            "sudo samurai boot set param add lsm=landlock,lockdown,yama,integrity,apparmor,bpf, then reboot",
        );
    }
    if !active(sys, "apparmor.service") {
        return item(
            "apps.apparmor",
            State::Fail,
            "AppArmor is on in the kernel but no profiles are loaded",
        )
        .hint("sudo systemctl enable --now apparmor.service");
    }
    let profiles = root
        .then(|| sys.read("/sys/kernel/security/apparmor/profiles"))
        .flatten();
    match profiles {
        Some(text) => {
            let enforced = text.lines().filter(|l| l.ends_with("(enforce)")).count();
            let total = text.lines().count();
            let state = if enforced == 0 {
                State::Warn
            } else {
                State::Pass
            };
            item(
                "apps.apparmor",
                state,
                format!("AppArmor is active: {enforced} of {total} loaded profiles enforce"),
            )
        }
        None => item(
            "apps.apparmor",
            State::Pass,
            "AppArmor is active and its profiles are loaded",
        ),
    }
}

/// `/proc/sys` paths a sysctl.d key names, with `*` expanded against the running system.
fn sysctl_paths(sys: &dyn System, key: &str) -> Vec<(String, String)> {
    let parts: Vec<&str> = key.split('.').collect();
    let Some(star) = parts.iter().position(|p| *p == "*") else {
        return vec![(key.to_string(), format!("/proc/sys/{}", parts.join("/")))];
    };
    let dir = format!("/proc/sys/{}", parts[..star].join("/"));
    sys.list_dir(&dir)
        .into_iter()
        .map(|name| {
            let mut p = parts.clone();
            p[star] = &name;
            // interface names may contain dots (VLANs): join the path, not the key
            (p.join("."), format!("/proc/sys/{}", p.join("/")))
        })
        .collect()
}

fn sysctl_item(sys: &dyn System) -> Item {
    let Some(conf) = sys.read(SYSCTL_FILE) else {
        return item(
            "kernel.sysctl",
            State::Fail,
            "the kernel hardening defaults are not installed",
        )
        .hint("sudo pacman -S --needed samurai-settings-security");
    };
    let (mut checked, mut unreadable) = (0usize, Vec::new());
    let mut differing: Vec<String> = Vec::new();
    for line in conf.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        let Some((key, want)) = line.split_once('=') else {
            continue;
        };
        let (key, want) = (key.trim().trim_start_matches('-'), want.trim());
        for (name, path) in sysctl_paths(sys, key) {
            match sys.read(&path) {
                Some(v) if v.trim() == want => checked += 1,
                Some(v) => {
                    checked += 1;
                    let line = format!("{name} = {} (default: {want})", v.trim());
                    if !differing.contains(&line) {
                        differing.push(line);
                    }
                }
                None if sys.exists(&path) => unreadable.push(name),
                None => {}
            }
        }
    }
    differing.sort();
    differing.dedup();
    if !differing.is_empty() {
        return item(
            "kernel.sysctl",
            State::Warn,
            format!(
                "{} kernel setting(s) differ from the Samurai defaults",
                differing.len()
            ),
        )
        .details(differing)
        .hint(
            "Expected if you changed them under /etc/sysctl.d/; otherwise: sudo sysctl --system",
        );
    }
    let mut out = item(
        "kernel.sysctl",
        State::Pass,
        format!("{checked} kernel settings match the Samurai defaults"),
    );
    if !unreadable.is_empty() {
        unreadable.dedup();
        out.details = vec![format!("readable only by root: {}", unreadable.join(", "))];
    }
    out
}

fn words(text: &str) -> Vec<String> {
    text.split_whitespace().map(str::to_string).collect()
}

/// firewalld's D-Bus answers an unprivileged caller with a password dialog (or a refusal), so a
/// read-only report asks it only as root.
fn firewall_items(sys: &dyn System, root: bool) -> Vec<Item> {
    if !active(sys, "firewalld") {
        let ufw = active(sys, "ufw");
        let fw = if ufw {
            item("net.firewall", State::Pass, "ufw is active")
                .hint("Rules: sudo ufw status verbose")
        } else {
            item("net.firewall", State::Fail, "no firewall is active")
                .hint("sudo systemctl enable --now firewalld")
        };
        return vec![fw];
    }
    let zone = root
        .then(|| ok_out(sys, "firewall-cmd", &["--get-default-zone"]))
        .flatten();
    let Some(zone) = zone else {
        return vec![item(
            "net.firewall",
            State::Unknown,
            "firewalld is active; reading its rules needs root",
        )];
    };
    let zone_arg = format!("--zone={zone}");
    let list = |what: &str| ok_out(sys, "firewall-cmd", &[&zone_arg, what]).map(|o| words(&o));
    let (Some(services), Some(ports)) = (list("--list-services"), list("--list-ports")) else {
        return vec![item(
            "net.firewall",
            State::Unknown,
            "firewalld is active; reading its rules needs root",
        )];
    };
    let details = vec![
        format!("services open in {zone}: {}", none_or(&services)),
        format!("ports open in {zone}: {}", none_or(&ports)),
    ];
    let fw = if zone == "trusted" {
        item(
            "net.firewall",
            State::Fail,
            "the default zone is trusted: everything is accepted",
        )
        .hint("sudo firewall-cmd --set-default-zone=public")
    } else if zone != "public" {
        item(
            "net.firewall",
            State::Warn,
            format!("firewalld is active; default zone is {zone}"),
        )
    } else {
        item(
            "net.firewall",
            State::Pass,
            "firewalld is active; default zone public",
        )
    };
    vec![fw.details(details), tailnet_item(sys)]
}

fn none_or(list: &[String]) -> String {
    if list.is_empty() {
        "none".into()
    } else {
        list.join(" ")
    }
}

fn tailnet_item(sys: &dyn System) -> Item {
    if !sys.exists("/sys/class/net/tailscale0") {
        return item("net.tailnet", State::Skip, "no Tailscale interface");
    }
    match sys.run("firewall-cmd", &["--get-zone-of-interface=tailscale0"]) {
        Some(o) if o.stdout.trim() == "samurai-tailnet" => item(
            "net.tailnet",
            State::Pass,
            "tailscale0 is in its own zone samurai-tailnet",
        ),
        Some(o) => {
            let zone = o.stdout.trim();
            let zone = if zone.is_empty() { "no zone" } else { zone };
            item("net.tailnet", State::Warn, format!("tailscale0 is in {zone}, not samurai-tailnet"))
                .hint("sudo firewall-cmd --reload, or: sudo samurai firewall zone tailscale0 samurai-tailnet")
        }
        None => item("net.tailnet", State::Unknown, "cannot ask firewalld"),
    }
}

fn loopback(address: &str) -> bool {
    address.starts_with("127.") || address.starts_with("[::1]") || address.starts_with("::1")
}

fn listening_item(sys: &dyn System) -> Item {
    let Some(out) = ok_out(sys, "ss", &["-tlnH"]) else {
        return item("net.listening", State::Unknown, "ss is not available");
    };
    let open: Vec<String> = out
        .lines()
        .filter_map(|l| l.split_whitespace().nth(3))
        .filter(|a| !loopback(a))
        .map(str::to_string)
        .collect();
    if open.is_empty() {
        return item(
            "net.listening",
            State::Pass,
            "no TCP service listens beyond loopback",
        );
    }
    item(
        "net.listening",
        State::Warn,
        format!("{} TCP listener(s) beyond loopback", open.len()),
    )
    .details(open)
    .hint("The firewall decides who reaches them. Find the owner: sudo ss -tlnp")
}

fn ssh_item(sys: &dyn System) -> Item {
    if !active(sys, "sshd") {
        return item("net.ssh", State::Pass, "sshd is not running");
    }
    // sshd -T prints the effective configuration, drop-ins and Match defaults included
    let Some(effective) = ok_out(sys, "sshd", &["-T"]) else {
        return item(
            "net.ssh",
            State::Unknown,
            "sshd runs; its settings need root to read",
        );
    };
    let value = |key: &str| {
        effective.lines().find_map(|l| {
            l.split_once(' ')
                .filter(|(k, _)| k.eq_ignore_ascii_case(key))
                .map(|(_, v)| v.to_string())
        })
    };
    let mut problems = Vec::new();
    if value("passwordauthentication").as_deref() != Some("no") {
        problems.push("password logins are accepted".to_string());
    }
    if value("kbdinteractiveauthentication").as_deref() != Some("no") {
        problems.push("keyboard-interactive logins are accepted".to_string());
    }
    if value("permitrootlogin").as_deref() == Some("yes") {
        problems.push("root may log in".to_string());
    }
    if problems.is_empty() {
        item(
            "net.ssh",
            State::Pass,
            "sshd runs: keys only, no root password login",
        )
    } else {
        item("net.ssh", State::Fail, "sshd accepts passwords or root logins")
            .details(problems)
            .hint("Set PasswordAuthentication no, KbdInteractiveAuthentication no, PermitRootLogin no in /etc/ssh/sshd_config.d/")
    }
}

/// Docker writes its own netfilter rules: a published port is reachable whatever the zone says.
fn docker_item(sys: &dyn System) -> Item {
    if !active(sys, "docker") {
        return item(
            "net.docker",
            State::Skip,
            "the Docker daemon is not running",
        );
    }
    let Some(out) = ok_out(sys, "docker", &["ps", "--format", "{{.Names}} {{.Ports}}"]) else {
        return item(
            "net.docker",
            State::Unknown,
            "listing published ports needs root",
        );
    };
    let exposed: Vec<String> = out
        .lines()
        .filter(|l| l.contains("0.0.0.0:") || l.contains("[::]:") || l.contains(":::"))
        .map(str::to_string)
        .collect();
    if exposed.is_empty() {
        return item(
            "net.docker",
            State::Pass,
            "no container port is published on every address",
        );
    }
    item(
        "net.docker",
        State::Warn,
        format!(
            "{} container(s) publish ports past the firewall",
            exposed.len()
        ),
    )
    .details(exposed)
    .hint("Publish on loopback instead: -p 127.0.0.1:8080:80")
}

fn account_items(sys: &dyn System, root: bool) -> Vec<Item> {
    if !root {
        return vec![
            item("accounts.root", State::Unknown, "needs root"),
            item("accounts.sudo", State::Unknown, "needs root"),
        ];
    }
    let root_item = match ok_out(sys, "passwd", &["-S", "root"]) {
        Some(o) => match o.split_whitespace().nth(1) {
            Some("L") => item("accounts.root", State::Pass, "the root password is locked"),
            Some(_) => item("accounts.root", State::Warn, "root has a usable password")
                .hint("Lock it once a wheel user can use sudo: sudo passwd -l root"),
            None => item("accounts.root", State::Unknown, "unreadable passwd status"),
        },
        None => item(
            "accounts.root",
            State::Unknown,
            "cannot read the root account status",
        ),
    };
    let mut files = vec!["/etc/sudoers".to_string()];
    files.extend(
        sys.list_dir("/etc/sudoers.d")
            .into_iter()
            // sudo itself ignores these names
            .filter(|n| !n.contains('.') && !n.ends_with('~'))
            .map(|n| format!("/etc/sudoers.d/{n}")),
    );
    let mut passwordless = Vec::new();
    for path in &files {
        for line in sys.read(path).unwrap_or_default().lines() {
            let line = line.trim();
            if !line.starts_with('#')
                && (line.contains("NOPASSWD") || line.contains("!authenticate"))
            {
                passwordless.push(format!("{path}: {line}"));
            }
        }
    }
    let sudo = if passwordless.is_empty() {
        item(
            "accounts.sudo",
            State::Pass,
            "every sudo rule asks for a password",
        )
    } else {
        item(
            "accounts.sudo",
            State::Warn,
            "some sudo rules need no password",
        )
        .details(passwordless)
    };
    vec![root_item, sudo]
}

fn firefox_item(sys: &dyn System) -> Item {
    if !sys.exists("/usr/bin/firefox") {
        return item("apps.firefox", State::Skip, "Firefox is not installed");
    }
    match sys.read(FIREFOX_POLICY) {
        Some(p) if p.contains("DisableTelemetry") && p.contains("uBlock0@raymondhill.net") => item(
            "apps.firefox",
            State::Pass,
            "Firefox policy present: telemetry off, uBlock Origin",
        ),
        Some(_) => item(
            "apps.firefox",
            State::Warn,
            "the Firefox policy was changed",
        ),
        None => item("apps.firefox", State::Warn, "no Firefox policy")
            .hint("sudo pacman -S --needed samurai-settings-security"),
    }
}

fn cve_item(sys: &dyn System, online: bool) -> Item {
    if !online {
        return item("packages.cve", State::Skip, "not checked (add --online)");
    }
    match ok_out(sys, "arch-audit", &["--upgradable", "--quiet"]) {
        Some(o) if o.is_empty() => item("packages.cve", State::Pass, "no pending security fixes"),
        Some(o) => {
            let packages: Vec<String> = o.lines().map(str::to_string).collect();
            item(
                "packages.cve",
                State::Fail,
                format!("{} package(s) have security fixes waiting", packages.len()),
            )
            .details(packages)
            .hint("samurai update")
        }
        None => item(
            "packages.cve",
            State::Unknown,
            "arch-audit failed or is not installed",
        ),
    }
}

pub fn collect(sys: &dyn System, online: bool) -> Vec<Item> {
    let root = is_root(sys);
    let mut items = disk_items(sys, root);
    items.push(secureboot_item(sys));
    items.push(apparmor_item(sys, root));
    items.push(sysctl_item(sys));
    items.extend(firewall_items(sys, root));
    items.push(listening_item(sys));
    items.push(ssh_item(sys));
    items.push(docker_item(sys));
    items.extend(account_items(sys, root));
    items.push(firefox_item(sys));
    items.push(cve_item(sys, online));
    items
}

pub fn exit_code(items: &[Item]) -> i32 {
    i32::from(items.iter().any(|i| i.state == State::Fail))
}

/// Control characters (terminal escapes in a container name, say) never reach the terminal.
fn printable(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

pub fn render_text(items: &[Item], root: bool) -> String {
    let mut out = String::new();
    for i in items {
        out.push_str(&format!(
            "{:<8} {:<17} {}\n",
            i.state.as_str().to_uppercase(),
            i.id,
            printable(&i.summary)
        ));
        for d in &i.details {
            out.push_str(&format!("{:27}{}\n", "", printable(d)));
        }
        if !i.hint.is_empty() {
            out.push_str(&format!("{:27}-> {}\n", "", printable(&i.hint)));
        }
    }
    let count = |s: State| items.iter().filter(|i| i.state == s).count();
    out.push_str(&format!(
        "\n{} pass, {} warn, {} fail, {} unknown, {} skipped\n",
        count(State::Pass),
        count(State::Warn),
        count(State::Fail),
        count(State::Unknown),
        count(State::Skip)
    ));
    if !root {
        out.push_str("Some items need root: sudo samurai security status\n");
    }
    out
}

pub fn render_json(items: &[Item], root: bool) -> String {
    let rows: Vec<String> = items
        .iter()
        .map(|i| {
            let details: Vec<String> = i
                .details
                .iter()
                .map(|d| format!("\"{}\"", json_escape(d)))
                .collect();
            format!(
                "{{\"id\":\"{}\",\"status\":\"{}\",\"summary\":\"{}\",\"details\":[{}],\"hint\":\"{}\"}}",
                i.id,
                i.state.as_str(),
                json_escape(&i.summary),
                details.join(","),
                json_escape(&i.hint)
            )
        })
        .collect();
    let count = |s: State| items.iter().filter(|i| i.state == s).count();
    format!(
        "{{\"schema\":1,\"root\":{root},\"pass\":{},\"warn\":{},\"fail\":{},\"unknown\":{},\"skip\":{},\"items\":[{}]}}\n",
        count(State::Pass),
        count(State::Warn),
        count(State::Fail),
        count(State::Unknown),
        count(State::Skip),
        rows.join(",")
    )
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Options {
    /// A change to the disk keyslots instead of the report.
    pub key_change: Option<tpm::Action>,
    pub dry_run: bool,
    pub json: bool,
    pub online: bool,
    pub help: bool,
}

pub fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut opts = Options::default();
    let mut it = args.iter().map(String::as_str);
    match it.next() {
        None | Some("help" | "--help" | "-h") => {
            opts.help = true;
            return Ok(opts);
        }
        Some("status") => {}
        Some("tpm") => {
            opts.key_change = Some(match it.next() {
                Some("enroll") => tpm::Action::Enroll,
                Some("remove") => tpm::Action::Remove,
                _ => return Err("expected: tpm enroll | tpm remove".into()),
            });
        }
        Some("recovery-key") => opts.key_change = Some(tpm::Action::RecoveryKey),
        Some(other) => return Err(format!("unknown command: {other}")),
    }
    let change = opts.key_change.is_some();
    for a in it {
        match a {
            "--json" if !change => opts.json = true,
            "--online" if !change => opts.online = true,
            "--dry-run" if change => opts.dry_run = true,
            "--help" | "-h" => opts.help = true,
            other => return Err(format!("unknown option: {other}")),
        }
    }
    Ok(opts)
}

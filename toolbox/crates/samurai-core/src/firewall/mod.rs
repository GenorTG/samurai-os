//! `samurai firewall` (spec 09): presets and ports in firewalld, previewed and reversible.
//!
//! A preset is one or more firewalld service definitions (upstream's, or `samurai-*` shipped by
//! samurai-settings-security), so overlapping presets never close each other's ports and every
//! change shows up by name in Plasma's firewall module. Changes go to the permanent and the
//! running configuration; nothing is reloaded, so other runtime rules survive.

use crate::health::{json_escape, system::System};

#[cfg(test)]
mod tests;

pub const USAGE: &str = "usage: samurai firewall <command>

  status [--json]                          zones, interfaces, open services and ports
  presets [--json]                         presets and the ports each one opens
  open  <preset|port/proto> [--zone Z]     allow inbound connections
  close <preset|port/proto> [--zone Z]     remove that allowance
  zone  <interface> <zone>                 choose how much a network is trusted

open, close and zone take --dry-run (show the commands, change nothing) and --yes (do not ask).
Without --zone the default zone is used. A port is 8080/tcp or 1714-1764/udp.
Changes need root and apply now and after a reboot. firewalld only (for ufw use `ufw`).
";

pub struct Preset {
    pub name: &'static str,
    pub title: &'static str,
    pub services: &'static [&'static str],
    /// Shown before the change is confirmed; empty when there is nothing to add.
    pub warning: &'static str,
}

pub const PRESETS: &[Preset] = &[
    Preset {
        name: "kdeconnect",
        title: "KDE Connect",
        services: &["kdeconnect"],
        warning: "",
    },
    Preset {
        name: "steam-remote-play",
        title: "Steam Remote Play",
        services: &["steam-streaming"],
        warning: "",
    },
    Preset {
        name: "steam-transfer",
        title: "Steam local game transfer",
        services: &["steam-lan-transfer"],
        warning: "",
    },
    Preset {
        name: "sunshine",
        title: "Sunshine / Moonlight",
        services: &["samurai-sunshine"],
        warning: "The web setup page (47990/tcp) stays closed: set Sunshine up on this machine.",
    },
    Preset {
        name: "localsend",
        title: "LocalSend",
        services: &["samurai-localsend"],
        warning: "",
    },
    Preset {
        name: "syncthing",
        title: "Syncthing",
        services: &["syncthing"],
        warning: "",
    },
    Preset {
        name: "samba",
        title: "Windows file sharing (Samba)",
        services: &["samba"],
        warning: "Open it only on networks you trust (--zone home).",
    },
    Preset {
        name: "ssh",
        title: "SSH server",
        services: &["ssh"],
        warning: "Anyone in the zone can try to log in. Prefer --zone samurai-tailnet.",
    },
    Preset {
        name: "minecraft",
        title: "Minecraft server",
        services: &["minecraft"],
        warning: "",
    },
    Preset {
        name: "ollama",
        title: "Ollama API",
        services: &["samurai-ollama"],
        warning: "Ollama has no authentication: everyone in the zone can use it.",
    },
    Preset {
        name: "web-dev",
        title: "Web development servers",
        services: &["samurai-web-dev"],
        warning: "Development servers often expose source code and debuggers.",
    },
    Preset {
        name: "web",
        title: "Web server (HTTP and HTTPS)",
        services: &["http", "https"],
        warning: "",
    },
];

pub fn preset(name: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|p| p.name == name)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Preset(&'static str),
    /// Canonical `8080/tcp` or `1714-1764/udp`.
    Port(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cmd {
    Help,
    Status {
        json: bool,
    },
    Presets {
        json: bool,
    },
    Change {
        open: bool,
        target: Target,
        zone: Option<String>,
        flags: Flags,
    },
    Zone {
        interface: String,
        zone: String,
        flags: Flags,
    },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Flags {
    pub dry_run: bool,
    pub yes: bool,
}

pub fn parse_port(text: &str) -> Option<String> {
    let (range, proto) = text.split_once('/')?;
    if !matches!(proto, "tcp" | "udp") {
        return None;
    }
    let number = |s: &str| {
        (!s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
            .then(|| s.parse::<u16>().ok())
            .flatten()
            .filter(|n| *n > 0)
    };
    match range.split_once('-') {
        Some((a, b)) => {
            let (a, b) = (number(a)?, number(b)?);
            (a < b).then(|| format!("{a}-{b}/{proto}"))
        }
        None => Some(format!("{}/{proto}", number(range)?)),
    }
}

/// Zone and interface names are passed to firewall-cmd inside `--opt=value`.
fn valid_name(name: &str, max: usize) -> bool {
    !name.is_empty()
        && name.len() <= max
        && !name.starts_with('-')
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

pub fn parse_args(args: &[String]) -> Result<Cmd, String> {
    let (mut flags, mut json, mut zone) = (Flags::default(), false, None);
    let mut words: Vec<&str> = Vec::new();
    let mut it = args.iter().map(String::as_str);
    while let Some(a) = it.next() {
        match a {
            "--help" | "-h" => return Ok(Cmd::Help),
            "--json" => json = true,
            "--dry-run" => flags.dry_run = true,
            "--yes" | "-y" => flags.yes = true,
            "--zone" => zone = Some(it.next().ok_or("--zone needs a name")?.to_string()),
            _ if a.starts_with("--zone=") => zone = Some(a["--zone=".len()..].to_string()),
            _ if a.starts_with('-') => return Err(format!("unknown option: {a}")),
            _ => words.push(a),
        }
    }
    if let Some(z) = &zone {
        if !valid_name(z, 17) {
            return Err(format!("invalid zone name: {z:?}"));
        }
    }
    let changes = flags != Flags::default() || zone.is_some();
    match words.as_slice() {
        [] | ["help"] => Ok(Cmd::Help),
        ["status"] if !changes => Ok(Cmd::Status { json }),
        ["presets"] if !changes => Ok(Cmd::Presets { json }),
        [verb @ ("open" | "close"), what] if !json => {
            let target = match preset(what) {
                Some(p) => Target::Preset(p.name),
                None => Target::Port(parse_port(what).ok_or_else(|| {
                    format!("{what:?} is neither a preset (samurai firewall presets) nor a port such as 8080/tcp")
                })?),
            };
            Ok(Cmd::Change {
                open: *verb == "open",
                target,
                zone,
                flags,
            })
        }
        ["zone", interface, name] if !json && zone.is_none() => {
            // 15 = IFNAMSIZ - 1; 17 = firewalld's zone name limit
            if !valid_name(interface, 15) {
                return Err(format!("invalid interface name: {interface:?}"));
            }
            if !valid_name(name, 17) {
                return Err(format!("invalid zone name: {name:?}"));
            }
            Ok(Cmd::Zone {
                interface: (*interface).to_string(),
                zone: (*name).to_string(),
                flags,
            })
        }
        _ => Err(
            "expected: status | presets | open <what> | close <what> | zone <interface> <zone>"
                .into(),
        ),
    }
}

fn query(sys: &dyn System, args: &[&str]) -> Result<String, String> {
    match sys.run("firewall-cmd", args) {
        Some(o) if o.code == 0 => Ok(o.stdout.trim().to_string()),
        Some(o) => Err(format!("firewall-cmd {} exited {}", args.join(" "), o.code)),
        None => Err("firewall-cmd is not installed".into()),
    }
}

fn require_running(sys: &dyn System) -> Result<(), String> {
    match sys.run("systemctl", &["is-active", "firewalld"]) {
        Some(o) if o.stdout.trim() == "active" => Ok(()),
        _ => Err(
            "firewalld is not running (enable it: sudo systemctl enable --now firewalld)".into(),
        ),
    }
}

fn require_zone(sys: &dyn System, zone: &str) -> Result<(), String> {
    // the permanent list also has zones installed since the last reload
    let zones = query(sys, &["--get-zones"])?;
    if zones.split_whitespace().any(|z| z == zone) {
        Ok(())
    } else {
        Err(format!(
            "firewalld has no zone {zone:?} (zones: {zones}). A newly installed zone needs: sudo firewall-cmd --reload"
        ))
    }
}

/// `ports:` of a firewalld service definition, as firewalld itself reports it.
fn service_ports(sys: &dyn System, service: &str) -> Result<String, String> {
    let info = query(sys, &[&format!("--info-service={service}")]).map_err(|_| {
        format!(
            "cannot read the firewalld service {service:?} (it needs root, or after installing samurai-settings-security: sudo firewall-cmd --reload)"
        )
    })?;
    let field = |name: &str| {
        info.lines()
            .find_map(|l| l.trim().strip_prefix(name))
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_string)
    };
    let mut parts: Vec<String> = field("ports:").into_iter().collect();
    // some upstream services only include others or name protocols
    if let Some(p) = field("protocols:") {
        parts.push(format!("protocols {p}"));
    }
    if let Some(i) = field("includes:") {
        parts.push(format!("includes {i}"));
    }
    Ok(parts.join(", "))
}

/// One reversible allowance: `kind` is `service` or `port`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub kind: &'static str,
    pub value: String,
    /// Which configurations must change: permanent, running.
    pub permanent: bool,
    pub runtime: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub open: bool,
    pub zone: String,
    pub preview: Vec<String>,
    pub steps: Vec<Step>,
}

impl Plan {
    /// The exact firewall-cmd argument lists, permanent first (if it fails nothing changed).
    pub fn commands(&self) -> Vec<Vec<String>> {
        let verb = if self.open { "add" } else { "remove" };
        let mut out = Vec::new();
        for permanent in [true, false] {
            for s in &self.steps {
                if (permanent && s.permanent) || (!permanent && s.runtime) {
                    let mut c = Vec::new();
                    if permanent {
                        c.push("--permanent".to_string());
                    }
                    c.push(format!("--zone={}", self.zone));
                    c.push(format!("--{verb}-{}={}", s.kind, s.value));
                    out.push(c);
                }
            }
        }
        out
    }
}

fn is_allowed(
    sys: &dyn System,
    permanent: bool,
    zone: &str,
    kind: &str,
    value: &str,
) -> Result<bool, String> {
    let zone = format!("--zone={zone}");
    let what = format!("--query-{kind}={value}");
    let mut args = Vec::new();
    if permanent {
        args.push("--permanent");
    }
    args.extend([zone.as_str(), what.as_str()]);
    // firewall-cmd query: 0 yes, 1 no, anything else is an error
    match sys.run("firewall-cmd", &args) {
        Some(o) if o.code == 0 => Ok(true),
        Some(o) if o.code == 1 => Ok(false),
        Some(o) => Err(format!(
            "firewall-cmd {} exited {} (changing the firewall needs root)",
            args.join(" "),
            o.code
        )),
        None => Err("firewall-cmd is not installed".into()),
    }
}

pub fn plan(
    sys: &dyn System,
    open: bool,
    target: &Target,
    zone: Option<&str>,
) -> Result<Plan, String> {
    require_running(sys)?;
    let zone = match zone {
        Some(z) => z.to_string(),
        None => query(sys, &["--get-default-zone"])?,
    };
    require_zone(sys, &zone)?;
    let verb = if open { "open" } else { "close" };
    let (mut preview, mut wanted): (Vec<String>, Vec<(&'static str, String)>) =
        (Vec::new(), Vec::new());
    match target {
        Target::Preset(name) => {
            let p = preset(name).ok_or("unknown preset")?;
            preview.push(format!("{verb} {} in zone {zone}", p.title));
            for service in p.services {
                preview.push(format!("  {service}: {}", service_ports(sys, service)?));
                wanted.push(("service", (*service).to_string()));
            }
            if open && !p.warning.is_empty() {
                preview.push(format!("  note: {}", p.warning));
            }
        }
        Target::Port(port) => {
            preview.push(format!("{verb} port {port} in zone {zone}"));
            wanted.push(("port", port.clone()));
        }
    }
    if open && zone == "public" {
        preview.push(
            "  zone public applies to every network not assigned elsewhere, public Wi-Fi included"
                .into(),
        );
    }
    let mut steps = Vec::new();
    for (kind, value) in wanted {
        // change only what differs from the goal
        let permanent = is_allowed(sys, true, &zone, kind, &value)? != open;
        let runtime = is_allowed(sys, false, &zone, kind, &value)? != open;
        if permanent || runtime {
            steps.push(Step {
                kind,
                value,
                permanent,
                runtime,
            });
        }
    }
    Ok(Plan {
        open,
        zone,
        preview,
        steps,
    })
}

fn run_all(sys: &dyn System, commands: &[Vec<String>]) -> Result<(), String> {
    for c in commands {
        let args: Vec<&str> = c.iter().map(String::as_str).collect();
        query(sys, &args)?;
    }
    Ok(())
}

/// Apply and then confirm the firewall really is in the wanted state.
pub fn apply(sys: &dyn System, plan: &Plan) -> Result<(), String> {
    run_all(sys, &plan.commands())?;
    for s in &plan.steps {
        for permanent in [true, false] {
            if is_allowed(sys, permanent, &plan.zone, s.kind, &s.value)? != plan.open {
                return Err(format!(
                    "{} {} is still {} in zone {} ({} configuration)",
                    s.kind,
                    s.value,
                    if plan.open { "closed" } else { "open" },
                    plan.zone,
                    if permanent { "permanent" } else { "running" }
                ));
            }
        }
    }
    Ok(())
}

pub fn zone_commands(
    sys: &dyn System,
    interface: &str,
    zone: &str,
) -> Result<Vec<Vec<String>>, String> {
    require_running(sys)?;
    require_zone(sys, zone)?;
    if !sys.exists(&format!("/sys/class/net/{interface}")) {
        return Err(format!("no network interface {interface:?}"));
    }
    let change = format!("--change-interface={interface}");
    let zone = format!("--zone={zone}");
    Ok(vec![
        vec!["--permanent".into(), zone.clone(), change.clone()],
        vec![zone, change],
    ])
}

pub fn set_zone(sys: &dyn System, interface: &str, zone: &str) -> Result<(), String> {
    run_all(sys, &zone_commands(sys, interface, zone)?)?;
    let now = query(sys, &[&format!("--get-zone-of-interface={interface}")])?;
    if now == zone {
        Ok(())
    } else {
        Err(format!("{interface} is in zone {now:?}, not {zone}"))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Zone {
    pub name: String,
    pub default: bool,
    pub interfaces: Vec<String>,
    pub services: Vec<String>,
    pub ports: Vec<String>,
}

/// The default zone, the tailnet zone and every zone with an interface or a source.
pub fn status(sys: &dyn System) -> Result<Vec<Zone>, String> {
    require_running(sys)?;
    let default = query(sys, &["--get-default-zone"])?;
    let mut names = vec![default.clone()];
    // active zones are printed as a name line followed by indented members
    for line in query(sys, &["--get-active-zones"])?.lines() {
        if !line.starts_with(char::is_whitespace) {
            if let Some(name) = line.split_whitespace().next() {
                names.push(name.to_string());
            }
        }
    }
    let all = query(sys, &["--get-zones"])?;
    if all.split_whitespace().any(|z| z == "samurai-tailnet") {
        names.push("samurai-tailnet".to_string());
    }
    let mut zones: Vec<Zone> = Vec::new();
    for name in names {
        if zones.iter().any(|z| z.name == name) {
            continue;
        }
        let zone = format!("--zone={name}");
        let list = |what: &str| -> Result<Vec<String>, String> {
            Ok(query(sys, &[&zone, what])?
                .split_whitespace()
                .map(str::to_string)
                .collect())
        };
        zones.push(Zone {
            default: name == default,
            interfaces: list("--list-interfaces")?,
            services: list("--list-services")?,
            ports: list("--list-ports")?,
            name,
        });
    }
    Ok(zones)
}

fn or_dash(list: &[String]) -> String {
    if list.is_empty() {
        "-".into()
    } else {
        list.join(" ")
    }
}

pub fn render_status(zones: &[Zone]) -> String {
    zones
        .iter()
        .map(|z| {
            format!(
                "zone {}{}\n  interfaces: {}\n  services:   {}\n  ports:      {}\n",
                z.name,
                if z.default { " (default)" } else { "" },
                or_dash(&z.interfaces),
                or_dash(&z.services),
                or_dash(&z.ports)
            )
        })
        .collect()
}

fn json_list(list: &[String]) -> String {
    let items: Vec<String> = list
        .iter()
        .map(|s| format!("\"{}\"", json_escape(s)))
        .collect();
    format!("[{}]", items.join(","))
}

pub fn render_status_json(zones: &[Zone]) -> String {
    let rows: Vec<String> = zones
        .iter()
        .map(|z| {
            format!(
                "{{\"name\":\"{}\",\"default\":{},\"interfaces\":{},\"services\":{},\"ports\":{}}}",
                json_escape(&z.name),
                z.default,
                json_list(&z.interfaces),
                json_list(&z.services),
                json_list(&z.ports)
            )
        })
        .collect();
    format!("{{\"schema\":1,\"zones\":[{}]}}\n", rows.join(","))
}

/// Presets with the ports firewalld reports; a service it does not know yet shows as unavailable.
pub fn render_presets(sys: &dyn System, json: bool) -> String {
    let mut rows = Vec::new();
    for p in PRESETS {
        let ports: Result<Vec<String>, String> =
            p.services.iter().map(|s| service_ports(sys, s)).collect();
        let (available, ports) = match ports {
            Ok(v) => (true, v.join(", ")),
            Err(_) => (
                false,
                "unavailable (firewalld stopped or service not loaded)".to_string(),
            ),
        };
        rows.push(if json {
            format!(
                "{{\"name\":\"{}\",\"title\":\"{}\",\"services\":{},\"available\":{available},\"ports\":\"{}\",\"warning\":\"{}\"}}",
                p.name,
                json_escape(p.title),
                json_list(&p.services.iter().map(|s| (*s).to_string()).collect::<Vec<_>>()),
                json_escape(if available { &ports } else { "" }),
                json_escape(p.warning)
            )
        } else {
            format!("{:<18} {:<30} {ports}\n", p.name, p.title)
        });
    }
    if json {
        format!("{{\"schema\":1,\"presets\":[{}]}}\n", rows.join(","))
    } else {
        rows.concat()
    }
}

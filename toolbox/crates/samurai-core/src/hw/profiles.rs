//! chwd's profile database. chwd is the engine that matches profiles to hardware; this module
//! reads its output and the few profile fields the safety rules and the preview need.
//!
//! The reader covers the TOML subset the Samurai profiles use (tests/unit/test_hw_profiles.py
//! enforces it): `[name]` tables, one-line strings and numbers, `"""` blocks without escapes.

use std::collections::BTreeMap;

use crate::health::system::System;

pub const DB_DIR: &str = "/var/lib/chwd/db";
pub const LOCAL_DIR: &str = "/var/lib/chwd/local";
pub const BUSES: [&str; 2] = ["pci", "usb"];
pub const GRAPHICS_SET: &str = "graphic_drivers";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Profile {
    pub name: String,
    pub desc: String,
    /// Directory the profile came from (`graphic_drivers`, `network_drivers`).
    pub set: String,
    pub priority: i64,
    pub conflict_group: Option<String>,
    pub packages: Vec<String>,
    /// Shell snippet printing further package names.
    pub conditional: String,
}

impl Profile {
    pub fn graphics(&self) -> bool {
        self.set == GRAPHICS_SET
    }
}

/// `[name]` header of a line, if it is one.
fn header(line: &str) -> Option<&str> {
    let name = line.trim_end().strip_prefix('[')?.strip_suffix(']')?;
    let name = name.trim_matches('"');
    (!name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-._".contains(c)))
    .then_some(name)
}

/// (name, key/value) tables in file order.
fn tables(text: &str) -> Vec<(String, BTreeMap<String, String>)> {
    let mut out: Vec<(String, BTreeMap<String, String>)> = Vec::new();
    // (key, closing delimiter, collected body)
    let mut block: Option<(String, &str, String)> = None;
    for line in text.lines() {
        if let Some((key, end, body)) = &mut block {
            if let Some(i) = line.find(*end) {
                body.push_str(&line[..i]);
                if let Some((_, t)) = out.last_mut() {
                    t.insert(key.clone(), body.clone());
                }
                block = None;
            } else {
                body.push_str(line);
                body.push('\n');
            }
            continue;
        }
        if let Some(name) = header(line) {
            out.push((name.to_string(), BTreeMap::new()));
            continue;
        }
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            continue;
        }
        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        let (key, value) = (key.trim().to_string(), value.trim());
        let delimiter = ["\"\"\"", "'''"].into_iter().find(|d| value.starts_with(d));
        if let Some(d) = delimiter {
            let rest = &value[3..];
            if let Some(i) = rest.find(d) {
                if let Some((_, t)) = out.last_mut() {
                    t.insert(key, rest[..i].to_string());
                }
            } else {
                let mut body = rest.to_string();
                if !body.is_empty() {
                    body.push('\n');
                }
                block = Some((key, d, body));
            }
            continue;
        }
        let value = match value.chars().next() {
            Some(q @ ('\'' | '"')) => value[1..].split(q).next().unwrap_or(""),
            _ => value.split('#').next().unwrap_or("").trim(),
        };
        if let Some((_, t)) = out.last_mut() {
            t.insert(key, value.to_string());
        }
    }
    out
}

/// Profiles of one file; `[a.b]` inherits every key it does not set from `[a]`.
pub fn parse(text: &str, set: &str) -> Vec<Profile> {
    let raw = tables(text);
    raw.iter()
        .map(|(name, own)| {
            let mut merged = own.clone();
            let mut parent = name.as_str();
            while let Some((p, _)) = parent.rsplit_once('.') {
                if let Some((_, t)) = raw.iter().find(|(n, _)| n == p) {
                    for (k, v) in t {
                        merged.entry(k.clone()).or_insert_with(|| v.clone());
                    }
                }
                parent = p;
            }
            let get = |k: &str| merged.get(k).cloned().unwrap_or_default();
            Profile {
                name: name.clone(),
                desc: get("desc"),
                set: set.to_string(),
                priority: get("priority").parse().unwrap_or(0),
                conflict_group: merged.get("driver_conflict_group").cloned(),
                packages: get("packages")
                    .split_whitespace()
                    .map(str::to_string)
                    .collect(),
                conditional: get("conditional_packages"),
            }
        })
        .collect()
}

fn files(sys: &dyn System, root: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for bus in BUSES {
        let dir = format!("{root}/{bus}");
        for set in sys.list_dir(&dir) {
            let path = format!("{dir}/{set}/profiles.toml");
            if sys.exists(&path) {
                out.push((set, path));
            }
        }
    }
    out
}

/// Every profile in the database.
pub fn database(sys: &dyn System) -> Vec<Profile> {
    files(sys, DB_DIR)
        .into_iter()
        .filter_map(|(set, path)| Some(parse(&sys.read(&path)?, &set)))
        .flatten()
        .collect()
}

/// Profiles chwd has recorded as installed: (name, record file).
pub fn records(sys: &dyn System) -> Vec<(String, String)> {
    files(sys, LOCAL_DIR)
        .into_iter()
        .filter_map(|(_, path)| {
            let text = sys.read(&path)?;
            Some(
                tables(&text)
                    .into_iter()
                    .map(move |(name, _)| (name, path.clone())),
            )
        })
        .flatten()
        .collect()
}

/// A record file without the table `name` (so chwd treats the profile as not installed).
pub fn without_record(text: &str, name: &str) -> String {
    let mut out = String::new();
    let mut skipping = false;
    let mut block: Option<&str> = None;
    for line in text.lines() {
        match block {
            Some(d) => {
                if line.contains(d) {
                    block = None;
                }
            }
            None => {
                if let Some(h) = header(line) {
                    skipping = h == name;
                } else if let Some((_, v)) = line.split_once('=') {
                    let v = v.trim();
                    for d in ["\"\"\"", "'''"] {
                        if v.starts_with(d) && !v[3..].contains(d) {
                            block = Some(d);
                        }
                    }
                }
            }
        }
        if !skipping {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// One device of `chwd --list` and the profiles it offers, highest priority first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offer {
    pub addr: String,
    pub profiles: Vec<String>,
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Parses `chwd --list`: a `> <addr> (class:vendor:device) ...:` line followed by a table.
pub fn parse_chwd_list(text: &str) -> Vec<Offer> {
    let mut offers: Vec<Offer> = Vec::new();
    for line in text.lines().map(strip_ansi) {
        if let Some(rest) = line.strip_prefix("> ") {
            let mut words = rest.split_whitespace();
            let addr = words.next().unwrap_or("");
            if words.next().is_some_and(|w| w.starts_with('(')) {
                offers.push(Offer {
                    addr: addr.to_string(),
                    profiles: Vec::new(),
                });
            }
            continue;
        }
        let cells: Vec<&str> = line
            .split(['│', '┆'])
            .map(str::trim)
            .filter(|c| !c.is_empty())
            .collect();
        if let ([name, priority], Some(offer)) = (cells.as_slice(), offers.last_mut()) {
            if priority.parse::<i64>().is_ok() {
                offer.profiles.push((*name).to_string());
            }
        }
    }
    offers
}

/// What chwd would offer on this machine.
pub fn offers(sys: &dyn System) -> Result<Vec<Offer>, String> {
    match sys.run("chwd", &["--list"]) {
        Some(o) if o.code == 0 => Ok(parse_chwd_list(&o.stdout)),
        Some(o) => Err(format!("chwd --list exited {}", o.code)),
        None => Err("chwd is not installed (samurai-hardware)".into()),
    }
}

//! Hardware change detection (spec 07 "When detection runs"): the PCI device list at the last
//! accepted state, compared at every boot. A change writes a world-readable notice for the
//! notifier and the toolbox; the baseline only moves when a driver apply succeeds or the user
//! accepts the change.

use crate::health::json_escape;
use crate::health::system::System;

use super::devices::{self, Device};

pub const BASELINE: &str = "/var/lib/samurai/hw-fingerprint";
pub const NOTICE: &str = "/var/lib/samurai/hw-changed.json";
const HEADER: &str = "# samurai hw fingerprint v1";

/// One device as stored: `class vendor device name`. The bus address is left out on purpose:
/// firmware renumbers buses when a card is added elsewhere.
fn entry(d: &Device) -> String {
    format!(
        "{} {} {} {}",
        d.class,
        d.vendor,
        d.device,
        d.label().replace('\n', " ")
    )
}

fn id_of(entry: &str) -> String {
    entry.split(' ').take(3).collect::<Vec<_>>().join(" ")
}

pub fn entries(devices: &[Device]) -> Vec<String> {
    let mut out: Vec<String> = devices.iter().map(entry).collect();
    out.sort();
    out
}

/// FNV-1a over the identifiers only (a pci.ids update must not look like new hardware).
pub fn hash(entries: &[String]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for e in entries {
        for b in id_of(e).bytes().chain(*b"\n") {
            h = (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    format!("{h:016x}")
}

pub fn render(entries: &[String]) -> String {
    format!(
        "{HEADER}\nhash {}\n{}",
        hash(entries),
        entries
            .iter()
            .map(|e| format!("dev {e}\n"))
            .collect::<String>()
    )
}

/// The stored entries; `None` for a missing file or another format.
pub fn parse(text: &str) -> Option<Vec<String>> {
    let mut lines = text.lines();
    (lines.next()? == HEADER).then(|| {
        lines
            .filter_map(|l| l.strip_prefix("dev ").map(str::to_string))
            .collect()
    })
}

/// Multiset difference by device id.
fn missing_from(from: &[String], other: &[String]) -> Vec<String> {
    let mut left: Vec<String> = other.iter().map(|e| id_of(e)).collect();
    from.iter()
        .filter(|e| match left.iter().position(|i| *i == id_of(e)) {
            Some(i) => {
                left.swap_remove(i);
                false
            }
            None => true,
        })
        .cloned()
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    /// No baseline yet (first boot, or an older installer's listing).
    New,
    Unchanged,
    Changed {
        added: Vec<String>,
        removed: Vec<String>,
    },
}

fn name_of(entry: &str) -> &str {
    entry.splitn(4, ' ').nth(3).unwrap_or(entry)
}

/// The notifier's sentence: display cards first, they are what needs a driver.
pub fn message(added: &[String], removed: &[String]) -> String {
    let gpus = |list: &[String]| {
        list.iter()
            .filter(|e| e.starts_with("03"))
            .map(|e| name_of(e))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let (new, old) = (gpus(added), gpus(removed));
    let what = match (new.is_empty(), old.is_empty()) {
        (false, false) => format!("New hardware: {new} (was {old})"),
        (false, true) => format!("New hardware: {new}"),
        (true, false) => format!("Hardware removed: {old}"),
        (true, true) => format!(
            "Hardware changed: {} device(s) added, {} removed",
            added.len(),
            removed.len()
        ),
    };
    format!("{what}. Check the drivers: samurai hw detect")
}

pub fn current(sys: &dyn System) -> Result<Vec<String>, String> {
    Ok(entries(&devices::scan(sys)?))
}

pub fn compare(sys: &dyn System) -> Result<State, String> {
    let now = current(sys)?;
    let Some(old) = sys.read(BASELINE).as_deref().and_then(parse) else {
        return Ok(State::New);
    };
    let added = missing_from(&now, &old);
    let removed = missing_from(&old, &now);
    Ok(if added.is_empty() && removed.is_empty() {
        State::Unchanged
    } else {
        State::Changed { added, removed }
    })
}

fn json_list(list: &[String]) -> String {
    list.iter()
        .map(|e| format!("\"{}\"", json_escape(e)))
        .collect::<Vec<_>>()
        .join(",")
}

pub fn render_state_json(state: &State) -> String {
    let (name, msg, added, removed) = match state {
        State::New => ("new", String::new(), Vec::new(), Vec::new()),
        State::Unchanged => ("unchanged", String::new(), Vec::new(), Vec::new()),
        State::Changed { added, removed } => (
            "changed",
            message(added, removed),
            added.clone(),
            removed.clone(),
        ),
    };
    format!(
        "{{\"schema\":1,\"state\":\"{name}\",\"message\":\"{}\",\"added\":[{}],\"removed\":[{}]}}\n",
        json_escape(&msg),
        json_list(&added),
        json_list(&removed)
    )
}

/// Make the present hardware the baseline and clear the notice.
pub fn accept(sys: &dyn System) -> Result<(), String> {
    sys.write(BASELINE, &render(&current(sys)?))?;
    if sys.exists(NOTICE) {
        sys.remove(NOTICE)?;
    }
    Ok(())
}

/// The boot service: write the baseline once, then keep the notice in step with reality.
pub fn check(sys: &dyn System) -> Result<State, String> {
    let state = compare(sys)?;
    match &state {
        State::New => accept(sys)?,
        State::Unchanged => {
            if sys.exists(NOTICE) {
                sys.remove(NOTICE)?;
            }
        }
        State::Changed { .. } => sys.write(NOTICE, &render_state_json(&state))?,
    }
    Ok(state)
}

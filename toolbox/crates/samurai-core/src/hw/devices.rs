//! PCI devices from `lspci -Dnnmm` plus the two sysfs facts that matter for safety: which driver
//! is bound and which display card the firmware booted on.

use crate::health::system::System;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Device {
    /// `0000:01:00.0`
    pub addr: String,
    /// Four hex digits each, lower case.
    pub class: String,
    pub vendor: String,
    pub device: String,
    pub vendor_name: String,
    pub name: String,
    /// Kernel driver bound right now.
    pub driver: Option<String>,
    /// The firmware's boot display card.
    pub boot_vga: bool,
}

pub const VENDOR_NVIDIA: &str = "10de";

impl Device {
    pub fn is_display(&self) -> bool {
        self.class.starts_with("03")
    }

    pub fn is_nvidia_display(&self) -> bool {
        self.is_display() && self.vendor == VENDOR_NVIDIA
    }

    pub fn label(&self) -> String {
        format!("{} {}", self.vendor_name, self.name)
    }
}

/// NVIDIA GPU architecture, read from the chip code name pci.ids puts first (`GA102 [...]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Generation {
    Tesla,
    Fermi,
    Kepler,
    Maxwell,
    Pascal,
    Volta,
    Turing,
    Ampere,
    Ada,
    Hopper,
    Blackwell,
    Unknown,
}

const CODENAMES: [(&str, Generation); 11] = [
    ("GT", Generation::Tesla),
    ("GF", Generation::Fermi),
    ("GK", Generation::Kepler),
    ("GM", Generation::Maxwell),
    ("GP", Generation::Pascal),
    ("GV", Generation::Volta),
    ("TU", Generation::Turing),
    ("GA", Generation::Ampere),
    ("AD", Generation::Ada),
    ("GH", Generation::Hopper),
    ("GB", Generation::Blackwell),
];

impl Generation {
    pub fn of(name: &str) -> Generation {
        let chip = name.split_whitespace().next().unwrap_or("");
        let digit_at = |i: usize| chip.as_bytes().get(i).is_some_and(u8::is_ascii_digit);
        for (prefix, generation) in CODENAMES {
            if chip.starts_with(prefix) && digit_at(prefix.len()) {
                return generation;
            }
        }
        // G80..G98
        if chip.starts_with('G') && digit_at(1) {
            return Generation::Tesla;
        }
        Generation::Unknown
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Generation::Tesla => "Tesla",
            Generation::Fermi => "Fermi",
            Generation::Kepler => "Kepler",
            Generation::Maxwell => "Maxwell",
            Generation::Pascal => "Pascal",
            Generation::Volta => "Volta",
            Generation::Turing => "Turing",
            Generation::Ampere => "Ampere",
            Generation::Ada => "Ada Lovelace",
            Generation::Hopper => "Hopper",
            Generation::Blackwell => "Blackwell",
            Generation::Unknown => "unknown",
        }
    }

    /// NVIDIA's open kernel modules: Turing and newer.
    pub fn open_modules(self) -> bool {
        matches!(
            self,
            Generation::Turing
                | Generation::Ampere
                | Generation::Ada
                | Generation::Hopper
                | Generation::Blackwell
        )
    }

    /// The 580xx legacy branch is what Maxwell, Pascal and Volta need; Turing to Ada also work
    /// with it. Blackwell does not (open modules only).
    pub fn legacy_580(self) -> bool {
        matches!(
            self,
            Generation::Maxwell
                | Generation::Pascal
                | Generation::Volta
                | Generation::Turing
                | Generation::Ampere
                | Generation::Ada
        )
    }

    /// Arch's current CUDA toolkit targets Turing and newer (spec 07).
    pub fn cuda(self) -> bool {
        self.open_modules()
    }
}

/// `"GA102 [GeForce RTX 3090] [2204]"` -> (`GA102 [GeForce RTX 3090]`, `2204`).
fn split_id(field: &str) -> Option<(String, String)> {
    let body = field.strip_suffix(']')?;
    let open = body.rfind('[')?;
    let id = &body[open + 1..];
    if id.len() != 4 || !id.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    Some((body[..open].trim().to_string(), id.to_ascii_lowercase()))
}

/// The quoted fields of one `-mm` line (`\"` and `\\` escapes resolved).
fn quoted(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current: Option<String> = None;
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match (&mut current, c) {
            (None, '"') => current = Some(String::new()),
            (Some(_), '"') => fields.extend(current.take()),
            (Some(s), '\\') => s.extend(chars.next()),
            (Some(s), c) => s.push(c),
            (None, _) => {}
        }
    }
    fields
}

pub fn parse_lspci(text: &str) -> Vec<Device> {
    text.lines()
        .filter_map(|line| {
            let addr = line.split_whitespace().next()?;
            let f = quoted(line);
            let (_, class) = split_id(f.first()?)?;
            let (vendor_name, vendor) = split_id(f.get(1)?)?;
            let (name, device) = split_id(f.get(2)?)?;
            Some(Device {
                addr: addr.to_string(),
                class,
                vendor,
                device,
                vendor_name,
                name,
                driver: None,
                boot_vga: false,
            })
        })
        .collect()
}

/// All PCI devices, or why the machine could not be read.
pub fn scan(sys: &dyn System) -> Result<Vec<Device>, String> {
    let out = match sys.run("lspci", &["-Dnnmm"]) {
        Some(o) if o.code == 0 => o.stdout,
        Some(o) => return Err(format!("lspci exited {}", o.code)),
        None => return Err("lspci is not installed (pciutils)".into()),
    };
    let mut devices = parse_lspci(&out);
    if devices.is_empty() {
        return Err("lspci reported no PCI devices".into());
    }
    for d in &mut devices {
        let base = format!("/sys/bus/pci/devices/{}", d.addr);
        d.driver = sys.read(&format!("{base}/uevent")).and_then(|u| {
            u.lines()
                .find_map(|l| l.strip_prefix("DRIVER=").map(str::to_string))
        });
        d.boot_vga = sys
            .read(&format!("{base}/boot_vga"))
            .is_some_and(|v| v.trim() == "1");
    }
    Ok(devices)
}

//! `samurai boot sync`: keep the boot menu in step with the installed kernels.
//!
//! The installer writes entries once. Without this, a kernel installed later has no entry and a
//! removed one leaves a dead (possibly default) entry. A pacman hook runs `sync` after every
//! kernel transaction.
//!
//! Safety rules, all tested:
//! - only entries this distro wrote are touched; anything else (snapshots, other systems) is
//!   preserved byte for byte;
//! - the kernel command line is copied from an existing entry, never invented;
//! - never write a menu with no bootable kernel, and never act when `/boot` cannot be read;
//! - the previous Limine config is kept as `.bak`.

use crate::health::system::System;

pub const LIMINE_CONF: &str = "/boot/limine/limine.conf";
pub const SD_LOADER: &str = "/boot/loader/loader.conf";
pub const SD_ENTRIES: &str = "/boot/loader/entries";
pub const GRUB_CFG: &str = "/boot/grub/grub.cfg";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Kernel {
    pub name: String,
    pub fallback: bool,
}

/// False when `/boot` is unreadable (not root, or the ESP is not mounted).
pub fn boot_readable(sys: &dyn System) -> bool {
    !sys.list_dir("/boot").is_empty()
}

/// Installed kernels (by `pkgbase`) that have both an image and an initramfs in `/boot`.
pub fn bootable_kernels(sys: &dyn System) -> Vec<Kernel> {
    let mut out: Vec<Kernel> = sys
        .list_dir("/usr/lib/modules")
        .iter()
        .filter_map(|d| sys.read(&format!("/usr/lib/modules/{d}/pkgbase")))
        .map(|b| b.trim().to_string())
        .filter(|b| {
            !b.is_empty()
                && b.chars()
                    .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
        })
        .filter(|b| {
            sys.exists(&format!("/boot/vmlinuz-{b}"))
                && sys.exists(&format!("/boot/initramfs-{b}.img"))
        })
        .map(|name| Kernel {
            fallback: sys.exists(&format!("/boot/initramfs-{name}-fallback.img")),
            name,
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out.dedup_by(|a, b| a.name == b.name);
    out
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Changes {
    pub added: Vec<String>,
    pub removed: Vec<String>,
}

impl Changes {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty()
    }

    pub fn lines(&self) -> Vec<String> {
        let add = self.added.iter().map(|e| format!("add entry: {e}"));
        add.chain(self.removed.iter().map(|e| format!("remove entry: {e}")))
            .collect()
    }
}

fn label(k: &str, fallback: bool) -> String {
    if fallback {
        format!("Samurai OS ({k}, fallback initramfs)")
    } else {
        format!("Samurai OS ({k})")
    }
}

fn initramfs(k: &str, fallback: bool) -> String {
    format!(
        "initramfs-{k}{}.img",
        if fallback { "-fallback" } else { "" }
    )
}

/// `(kernel, fallback)` when this Limine block is one of ours.
fn managed(block: &[&str]) -> Option<(String, bool)> {
    let title = block
        .first()?
        .trim_end()
        .strip_prefix("/Samurai OS (")?
        .strip_suffix(')')?;
    let (k, fallback) = match title.strip_suffix(", fallback initramfs") {
        Some(k) => (k, true),
        None => (title, false),
    };
    let path = format!("path: boot():/vmlinuz-{k}");
    block
        .iter()
        .any(|l| l.trim() == path)
        .then(|| (k.to_string(), fallback))
}

fn wanted(kernels: &[Kernel], k: &str, fallback: bool) -> bool {
    kernels
        .iter()
        .any(|x| x.name == k && (!fallback || x.fallback))
}

/// Lines before the first entry, and one block per line starting with `/`.
fn split_blocks(conf: &str) -> (Vec<&str>, Vec<Vec<&str>>) {
    let mut preamble = Vec::new();
    let mut blocks: Vec<Vec<&str>> = Vec::new();
    for line in conf.lines() {
        if line.starts_with('/') {
            blocks.push(vec![line]);
        } else if let Some(b) = blocks.last_mut() {
            b.push(line);
        } else {
            preamble.push(line);
        }
    }
    (preamble, blocks)
}

fn join_blocks(preamble: &[&str], blocks: impl Iterator<Item = Vec<String>>) -> String {
    let mut out: Vec<String> = preamble.iter().map(|l| (*l).to_string()).collect();
    for b in blocks {
        if out.last().is_some_and(|l| !l.is_empty()) {
            out.push(String::new());
        }
        out.extend(b);
    }
    let mut text = out.join("\n");
    text.truncate(text.trim_end().len());
    text.push('\n');
    text
}

pub fn plan_limine(conf: &str, kernels: &[Kernel]) -> Result<(String, Changes), String> {
    if kernels.is_empty() {
        return Err("no bootable kernel found; refusing to rewrite the boot menu".into());
    }
    let (preamble, blocks) = split_blocks(conf);
    let cmdline = blocks
        .iter()
        .filter(|b| managed(b).is_some_and(|(_, f)| !f))
        .find_map(|b| b.iter().find_map(|l| l.trim().strip_prefix("cmdline: ")))
        .map(str::to_string);
    // Limine boots the first entry by default: remember whether that was one of ours
    let default_was_ours = blocks.first().is_some_and(|b| managed(b).is_some());
    let mut changes = Changes::default();
    let mut present = Vec::new();
    let mut kept: Vec<Vec<String>> = Vec::new();
    // where new entries go: after our last surviving entry, else where our first removed one was
    let (mut after_ours, mut hole) = (None, None);
    for b in &blocks {
        match managed(b) {
            Some((k, f)) if !wanted(kernels, &k, f) => {
                hole.get_or_insert(kept.len());
                changes.removed.push(label(&k, f));
            }
            m => {
                kept.push(b.iter().map(|l| (*l).to_string()).collect());
                if m.is_some() {
                    after_ours = Some(kept.len());
                }
                present.extend(m);
            }
        }
    }
    let mut new_blocks: Vec<Vec<String>> = Vec::new();
    for fallback in [false, true] {
        for k in kernels.iter().filter(|k| !fallback || k.fallback) {
            if present.contains(&(k.name.clone(), fallback)) {
                continue;
            }
            let cmd = cmdline
                .as_deref()
                .ok_or("no existing Samurai OS entry to copy the kernel command line from")?;
            new_blocks.push(vec![
                format!("/{}", label(&k.name, fallback)),
                "    protocol: linux".into(),
                format!("    path: boot():/vmlinuz-{}", k.name),
                format!("    cmdline: {cmd}"),
                format!("    module_path: boot():/{}", initramfs(&k.name, fallback)),
                String::new(),
            ]);
            changes.added.push(label(&k.name, fallback));
        }
    }
    let at = after_ours.or(hole).unwrap_or(kept.len());
    kept.splice(at..at, new_blocks);
    // if we owned the default and it is gone, one of our normal entries must take its place, never
    // a snapshot or another system that merely happens to come next
    let ours = |b: &[String]| managed(&b.iter().map(String::as_str).collect::<Vec<_>>());
    if default_was_ours
        && !changes.removed.is_empty()
        && kept.first().is_some_and(|b| ours(b).is_none())
    {
        if let Some(i) = kept.iter().position(|b| ours(b).is_some_and(|(_, f)| !f)) {
            let block = kept.remove(i);
            kept.insert(0, block);
        }
    }
    if changes.is_empty() {
        return Ok((conf.to_string(), changes));
    }
    Ok((join_blocks(&preamble, kept.into_iter()), changes))
}

fn sd_file(k: &str, fallback: bool) -> String {
    format!(
        "samurai-{k}{}.conf",
        if fallback { "-fallback" } else { "" }
    )
}

/// `(files to write, files to delete)` for systemd-boot.
type SdPlan = (Vec<(String, String)>, Vec<String>, Changes);

pub fn plan_systemd_boot(sys: &dyn System, kernels: &[Kernel]) -> Result<SdPlan, String> {
    if kernels.is_empty() {
        return Err("no bootable kernel found; refusing to rewrite the boot menu".into());
    }
    let ours: Vec<String> = sys
        .list_dir(SD_ENTRIES)
        .into_iter()
        .filter(|f| f.starts_with("samurai-") && f.ends_with(".conf"))
        .collect();
    let options = ours
        .iter()
        .filter(|f| !f.ends_with("-fallback.conf"))
        .filter_map(|f| sys.read(&format!("{SD_ENTRIES}/{f}")))
        .find_map(|t| {
            t.lines()
                .find_map(|l| l.strip_prefix("options ").map(|o| o.trim().to_string()))
        });
    let want: Vec<(String, String, bool)> = kernels
        .iter()
        .flat_map(|k| {
            let fb = k
                .fallback
                .then(|| (sd_file(&k.name, true), k.name.clone(), true));
            std::iter::once((sd_file(&k.name, false), k.name.clone(), false)).chain(fb)
        })
        .collect();
    let mut changes = Changes::default();
    let mut write = Vec::new();
    for (file, k, f) in &want {
        if ours.contains(file) {
            continue;
        }
        let opts = options
            .as_deref()
            .ok_or("no existing Samurai OS entry to copy the kernel options from")?;
        let body = format!(
            "title   {}\nlinux   /vmlinuz-{k}\ninitrd  /{}\noptions {opts}\n",
            label(k, *f),
            initramfs(k, *f)
        );
        write.push((format!("{SD_ENTRIES}/{file}"), body));
        changes.added.push(label(k, *f));
    }
    let mut delete = Vec::new();
    for file in ours.iter().filter(|f| !want.iter().any(|w| &w.0 == *f)) {
        delete.push(format!("{SD_ENTRIES}/{file}"));
        changes.removed.push(file.clone());
    }
    Ok((write, delete, changes))
}

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// No bootloader this tool manages (or none configured yet, e.g. during installation).
    NotManaged(String),
    InSync,
    /// Changes needed (`check`) or made (`sync`).
    Changed(Vec<String>),
}

pub fn sync(sys: &dyn System, apply: bool) -> Result<Outcome, String> {
    if !boot_readable(sys) {
        return Err("/boot cannot be read (run as root, and check that the ESP is mounted)".into());
    }
    let kernels = bootable_kernels(sys);
    if sys.exists(LIMINE_CONF) {
        let conf = sys
            .read(LIMINE_CONF)
            .ok_or(format!("cannot read {LIMINE_CONF}"))?;
        let (new, changes) = plan_limine(&conf, &kernels)?;
        if changes.is_empty() {
            return Ok(Outcome::InSync);
        }
        if apply {
            sys.write(&format!("{LIMINE_CONF}.bak"), &conf)?;
            sys.write(LIMINE_CONF, &new)?;
        }
        return Ok(Outcome::Changed(changes.lines()));
    }
    if sys.exists(SD_LOADER) {
        let (write, delete, changes) = plan_systemd_boot(sys, &kernels)?;
        if changes.is_empty() {
            return Ok(Outcome::InSync);
        }
        if apply {
            // new entries first: an interruption must never leave fewer ways to boot
            for (path, body) in &write {
                sys.write(path, body)?;
            }
            for path in &delete {
                sys.remove(path)?;
            }
        }
        return Ok(Outcome::Changed(changes.lines()));
    }
    if sys.exists(GRUB_CFG) {
        let cfg = sys
            .read(GRUB_CFG)
            .ok_or(format!("cannot read {GRUB_CFG}"))?;
        // grub.cfg names images as /vmlinuz-<k> followed by whitespace
        let listed = |k: &str| {
            cfg.split_whitespace()
                .any(|w| w.ends_with(&format!("/vmlinuz-{k}")))
        };
        let stale = cfg
            .split_whitespace()
            .filter_map(|w| w.split_once("/vmlinuz-").map(|(_, k)| k))
            .any(|k| !kernels.iter().any(|x| x.name == k));
        if !stale && kernels.iter().all(|k| listed(&k.name)) {
            return Ok(Outcome::InSync);
        }
        if kernels.is_empty() {
            return Err("no bootable kernel found; refusing to regenerate grub.cfg".into());
        }
        if apply {
            match sys.run("grub-mkconfig", &["-o", GRUB_CFG]) {
                Some(o) if o.code == 0 => {}
                Some(o) => return Err(format!("grub-mkconfig exited {}", o.code)),
                None => return Err("grub-mkconfig is not installed".into()),
            }
        }
        return Ok(Outcome::Changed(vec!["regenerate grub.cfg".into()]));
    }
    Ok(Outcome::NotManaged(
        "no Limine, systemd-boot or GRUB configuration found".into(),
    ))
}

/// Kernel the menu boots by default, when it is one of ours.
pub fn default_kernel(sys: &dyn System) -> Option<String> {
    if sys.exists(LIMINE_CONF) {
        let conf = sys.read(LIMINE_CONF)?;
        let lines: Vec<&str> = conf.lines().skip_while(|l| !l.starts_with('/')).collect();
        let end = lines
            .iter()
            .skip(1)
            .position(|l| l.starts_with('/'))
            .map_or(lines.len(), |i| i + 1);
        return managed(&lines[..end]).map(|(k, _)| k);
    }
    let loader = sys.read(SD_LOADER)?;
    let entry = loader
        .lines()
        .find_map(|l| l.strip_prefix("default "))?
        .trim();
    let name = entry.strip_prefix("samurai-")?.strip_suffix(".conf")?;
    Some(name.trim_end_matches("-fallback").to_string())
}

/// Limine config with the normal entry of `kernel` first.
pub fn limine_with_default(conf: &str, kernel: &str) -> Result<String, String> {
    let (preamble, mut blocks) = split_blocks(conf);
    let i = blocks
        .iter()
        .position(|b| managed(b).is_some_and(|(k, f)| k == kernel && !f))
        .ok_or(format!(
            "no boot entry for {kernel}; run: samurai boot sync"
        ))?;
    let chosen = blocks.remove(i);
    blocks.insert(0, chosen);
    Ok(join_blocks(
        &preamble,
        blocks
            .iter()
            .map(|b| b.iter().map(|l| (*l).to_string()).collect()),
    ))
}

pub fn set_default(sys: &dyn System, kernel: &str) -> Result<String, String> {
    if !boot_readable(sys) {
        return Err("/boot cannot be read (run as root, and check that the ESP is mounted)".into());
    }
    if !bootable_kernels(sys).iter().any(|k| k.name == kernel) {
        return Err(format!("{kernel} is not an installed, bootable kernel"));
    }
    if sys.exists(LIMINE_CONF) {
        let conf = sys
            .read(LIMINE_CONF)
            .ok_or(format!("cannot read {LIMINE_CONF}"))?;
        let new = limine_with_default(&conf, kernel)?;
        sys.write(&format!("{LIMINE_CONF}.bak"), &conf)?;
        sys.write(LIMINE_CONF, &new)?;
        return Ok(format!("{kernel} is now the default (Limine)"));
    }
    if sys.exists(SD_LOADER) {
        let file = sd_file(kernel, false);
        if !sys.exists(&format!("{SD_ENTRIES}/{file}")) {
            return Err(format!(
                "no boot entry for {kernel}; run: samurai boot sync"
            ));
        }
        let old = sys
            .read(SD_LOADER)
            .ok_or(format!("cannot read {SD_LOADER}"))?;
        let mut lines: Vec<String> = old
            .lines()
            .filter(|l| !l.starts_with("default "))
            .map(str::to_string)
            .collect();
        lines.push(format!("default {file}"));
        sys.write(SD_LOADER, &format!("{}\n", lines.join("\n")))?;
        return Ok(format!("{kernel} is now the default (systemd-boot)"));
    }
    Err("choosing the default is only supported for Limine and systemd-boot".into())
}

pub const USAGE: &str = "usage: samurai boot sync [--check]

Make the boot menu match the installed kernels (Limine, systemd-boot or GRUB). A pacman hook
runs this after kernel transactions. Only Samurai OS entries are touched.

  --check     report what would change; exit 1 when the menu is out of sync

Exit codes: 0 in sync or updated, 1 failed or (--check) out of sync, 2 usage error.
";

#[derive(Debug, PartialEq, Eq)]
pub enum Cmd {
    Sync { check: bool },
    Help,
}

pub fn parse_args(args: &[String]) -> Result<Cmd, String> {
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    match words.as_slice() {
        ["sync"] => Ok(Cmd::Sync { check: false }),
        ["sync", "--check"] => Ok(Cmd::Sync { check: true }),
        ["-h" | "--help"] | ["sync", "-h" | "--help"] => Ok(Cmd::Help),
        [] => Err("a command is required: sync".into()),
        ["sync", other, ..] => Err(format!("unexpected argument: {other}")),
        [other, ..] => Err(format!("unknown command: {other}")),
    }
}

#[cfg(test)]
mod tests;

//! `samurai boot get/set` (spec 04): menu timeout and kernel parameters.
//!
//! Only Samurai OS entries are edited. Parameters that decide whether the machine can find and
//! unlock its root filesystem cannot be added, replaced or removed.

use super::{
    boot_readable, default_kernel, join_blocks, managed, split_blocks, GRUB_CFG, LIMINE_CONF,
    SD_ENTRIES, SD_LOADER,
};
use crate::health::system::System;

const MAX_TIMEOUT: u32 = 60;

fn key(param: &str) -> &str {
    param.split('=').next().unwrap_or(param)
}

fn protected(param: &str) -> bool {
    let k = key(param);
    [
        "root",
        "rootflags",
        "rootfstype",
        "ro",
        "rw",
        "init",
        "cryptdevice",
        "cryptkey",
        "resume",
    ]
    .contains(&k)
        || k.starts_with("rd.luks")
        || k.starts_with("luks")
}

fn check_param(p: &str) -> Result<(), String> {
    let ok = !p.is_empty()
        && p.len() <= 200
        && !p.starts_with(['-', '='])
        && p.chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-=,:/+@".contains(c));
    if !ok {
        return Err(format!("not a valid kernel parameter: {p:?}"));
    }
    if protected(p) {
        return Err(format!(
            "{} controls how the system boots its root; edit it by hand if you must",
            key(p)
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    Timeout(u32),
    Add(String),
    Remove(String),
}

/// The command line after `change`. Adding replaces an existing value of the same key, except
/// `console`, which may repeat.
fn edit_cmdline(line: &str, change: &Change) -> String {
    let mut words: Vec<&str> = line.split_whitespace().collect();
    match change {
        Change::Add(p) => {
            if !words.contains(&p.as_str()) {
                match words
                    .iter()
                    .position(|w| key(p) != "console" && key(w) == key(p))
                {
                    Some(i) => {
                        words.retain(|w| key(w) != key(p));
                        words.insert(i.min(words.len()), p);
                    }
                    None => words.push(p),
                }
            }
        }
        // `name=value` removes that exact word; a bare name removes every value of it
        Change::Remove(p) => words.retain(|w| if p.contains('=') { w != p } else { key(w) != p }),
        Change::Timeout(_) => {}
    }
    words.join(" ")
}

fn set_line(lines: &mut Vec<String>, prefix: &str, value: String) {
    match lines.iter().position(|l| l.starts_with(prefix)) {
        Some(i) => lines[i] = value,
        None => lines.insert(0, value),
    }
}

pub fn limine_set(conf: &str, change: &Change) -> Result<String, String> {
    let (preamble, blocks) = split_blocks(conf);
    let mut preamble: Vec<String> = preamble.iter().map(|l| (*l).to_string()).collect();
    if let Change::Timeout(t) = change {
        set_line(&mut preamble, "timeout:", format!("timeout: {t}"));
    }
    let mut ours = 0;
    let blocks: Vec<Vec<String>> = blocks
        .iter()
        .map(|b| {
            let mine = managed(b).is_some();
            ours += usize::from(mine);
            b.iter()
                .map(|l| match l.trim_start().strip_prefix("cmdline:") {
                    Some(cmd) if mine && !matches!(change, Change::Timeout(_)) => {
                        let indent = &l[..l.len() - l.trim_start().len()];
                        format!("{indent}cmdline: {}", edit_cmdline(cmd, change))
                    }
                    _ => (*l).to_string(),
                })
                .collect()
        })
        .collect();
    if ours == 0 && !matches!(change, Change::Timeout(_)) {
        return Err("no Samurai OS boot entry to change".into());
    }
    let pre: Vec<&str> = preamble.iter().map(String::as_str).collect();
    Ok(join_blocks(&pre, blocks.into_iter()))
}

fn map_lines(text: &str, f: impl Fn(&str) -> String) -> String {
    text.lines().map(|l| format!("{}\n", f(l))).collect()
}

/// Files `set` would rewrite: `(path, old, new)`, only those that really change.
fn plan(sys: &dyn System, change: &Change) -> Result<Vec<(String, String, String)>, String> {
    let read = |p: &str| sys.read(p).ok_or(format!("cannot read {p}"));
    let mut files = Vec::new();
    if sys.exists(LIMINE_CONF) {
        let old = read(LIMINE_CONF)?;
        let new = limine_set(&old, change)?;
        files.push((LIMINE_CONF.to_string(), old, new));
    } else if sys.exists(SD_LOADER) {
        if let Change::Timeout(t) = change {
            let old = read(SD_LOADER)?;
            let mut lines: Vec<String> = old.lines().map(str::to_string).collect();
            set_line(&mut lines, "timeout ", format!("timeout {t}"));
            files.push((
                SD_LOADER.to_string(),
                old,
                format!("{}\n", lines.join("\n")),
            ));
        } else {
            for f in sys
                .list_dir(SD_ENTRIES)
                .iter()
                .filter(|f| f.starts_with("samurai-") && f.ends_with(".conf"))
            {
                let path = format!("{SD_ENTRIES}/{f}");
                let old = read(&path)?;
                let new = map_lines(&old, |l| match l.strip_prefix("options ") {
                    Some(o) => format!("options {}", edit_cmdline(o, change)),
                    None => l.to_string(),
                });
                files.push((path, old, new));
            }
            if files.is_empty() {
                return Err("no Samurai OS boot entry to change".into());
            }
        }
    } else if sys.exists(GRUB_CFG) {
        return Err("GRUB settings are not managed yet: edit /etc/default/grub, then grub-mkconfig -o /boot/grub/grub.cfg".into());
    } else {
        return Err("no Limine, systemd-boot or GRUB configuration found".into());
    }
    files.retain(|(_, old, new)| old.trim_end() != new.trim_end());
    Ok(files)
}

pub fn set(sys: &dyn System, change: &Change) -> Result<String, String> {
    match change {
        Change::Timeout(t) if *t > MAX_TIMEOUT => {
            return Err(format!("timeout must be 0 to {MAX_TIMEOUT} seconds"))
        }
        Change::Timeout(_) => {}
        Change::Add(p) | Change::Remove(p) => check_param(p)?,
    }
    if !boot_readable(sys) {
        return Err("/boot cannot be read (run as root, and check that the ESP is mounted)".into());
    }
    let files = plan(sys, change)?;
    if files.is_empty() {
        return Ok("nothing to change".into());
    }
    for (path, old, new) in &files {
        // systemd-boot ignores files not ending in .conf, so the backup never shows in the menu
        sys.write(&format!("{path}.bak"), old)?;
        sys.write(path, new)?;
    }
    Ok(format!(
        "updated {} file(s); previous copies end in .bak; applies at the next boot",
        files.len()
    ))
}

pub fn get(sys: &dyn System) -> Result<String, String> {
    if !boot_readable(sys) {
        return Err("/boot cannot be read (run as root, and check that the ESP is mounted)".into());
    }
    let (loader, timeout, params) = if let Some(c) = sys.read(LIMINE_CONF) {
        let (pre, blocks) = split_blocks(&c);
        let t = pre
            .iter()
            .find_map(|l| l.strip_prefix("timeout:"))
            .map(|v| v.trim().to_string());
        let p = blocks
            .iter()
            .filter(|b| managed(b).is_some_and(|(_, fallback)| !fallback))
            .find_map(|b| {
                b.iter().find_map(|l| {
                    l.trim()
                        .strip_prefix("cmdline:")
                        .map(|v| v.trim().to_string())
                })
            });
        ("Limine", t, p)
    } else if let Some(c) = sys.read(SD_LOADER) {
        let t = c
            .lines()
            .find_map(|l| l.strip_prefix("timeout "))
            .map(|v| v.trim().to_string());
        let p = sys
            .list_dir(SD_ENTRIES)
            .iter()
            .filter(|f| {
                f.starts_with("samurai-") && f.ends_with(".conf") && !f.ends_with("-fallback.conf")
            })
            .filter_map(|f| sys.read(&format!("{SD_ENTRIES}/{f}")))
            .find_map(|t| {
                t.lines()
                    .find_map(|l| l.strip_prefix("options ").map(|v| v.trim().to_string()))
            });
        ("systemd-boot", t, p)
    } else if sys.exists(GRUB_CFG) {
        ("GRUB", None, None)
    } else {
        return Err("no Limine, systemd-boot or GRUB configuration found".into());
    };
    let or = |v: Option<String>| v.unwrap_or_else(|| "-".into());
    Ok(format!(
        "bootloader: {loader}\ntimeout: {}\ndefault kernel: {}\nkernel parameters: {}\n",
        or(timeout),
        or(default_kernel(sys)),
        or(params)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::health::system::fake::Fake;

    const CONF: &str = "timeout: 3\n\n/Samurai OS (linux)\n    protocol: linux\n    path: boot():/vmlinuz-linux\n    cmdline: root=UUID=a rw rd.luks.uuid=b quiet loglevel=3 console=tty1\n    module_path: boot():/initramfs-linux.img\n\n/Snapshot 1\n    protocol: linux\n    path: boot():/vmlinuz-linux\n    cmdline: root=UUID=a quiet\n";

    fn add(p: &str) -> Change {
        Change::Add(p.into())
    }

    #[test]
    fn parameters_change_only_in_our_entries() {
        let new = limine_set(CONF, &add("nowatchdog")).unwrap();
        assert!(new.contains("    cmdline: root=UUID=a rw rd.luks.uuid=b quiet loglevel=3 console=tty1 nowatchdog\n"), "{new}");
        assert!(
            new.contains("    cmdline: root=UUID=a quiet\n"),
            "snapshot entry untouched"
        );
        let new = limine_set(CONF, &Change::Remove("quiet".into())).unwrap();
        assert!(
            new.contains("rd.luks.uuid=b loglevel=3 console=tty1\n")
                && new.contains("root=UUID=a quiet\n")
        );
    }

    #[test]
    fn add_replaces_a_value_in_place_but_console_repeats() {
        assert_eq!(
            edit_cmdline("a loglevel=3 b", &add("loglevel=7")),
            "a loglevel=7 b"
        );
        assert_eq!(edit_cmdline("x=1 a x=2", &add("x=9")), "x=9 a");
        assert_eq!(
            edit_cmdline("console=tty1", &add("console=ttyS0")),
            "console=tty1 console=ttyS0"
        );
        assert_eq!(edit_cmdline("a b", &add("a")), "a b");
        assert_eq!(
            edit_cmdline(
                "console=tty1 console=ttyS0",
                &Change::Remove("console=ttyS0".into())
            ),
            "console=tty1"
        );
        assert_eq!(
            edit_cmdline("x=1 xy=2", &Change::Remove("x".into())),
            "xy=2"
        );
    }

    #[test]
    fn root_and_unlock_parameters_are_untouchable() {
        for p in [
            "root=/dev/sda",
            "rootflags=x",
            "rw",
            "ro",
            "rd.luks.uuid",
            "rd.luks.key=/x",
            "init=/bin/sh",
            "cryptdevice=a",
        ] {
            for c in [Change::Add(p.into()), Change::Remove(p.into())] {
                let sys = Fake::default()
                    .dir("/boot", &["limine"])
                    .file(LIMINE_CONF, CONF);
                assert!(set(&sys, &c).is_err(), "{p}");
                assert!(sys.writes().is_empty());
            }
        }
        for p in ["", "-x", "=y", "a b", "a\n/Evil", "$(x)", "a;b", "x\"y"] {
            assert!(check_param(p).is_err(), "{p:?}");
        }
        assert!(check_param("nvidia_drm.modeset=1").is_ok());
    }

    #[test]
    fn set_backs_up_then_writes_and_skips_no_ops() {
        let sys = Fake::default()
            .dir("/boot", &["limine"])
            .file(LIMINE_CONF, CONF);
        set(&sys, &Change::Timeout(9)).unwrap();
        let w = sys.writes();
        assert_eq!(w[0], (format!("{LIMINE_CONF}.bak"), CONF.to_string()));
        assert!(w[1].1.starts_with("timeout: 9\n"));
        assert_eq!(
            w[1].1.replace("timeout: 9", "timeout: 3"),
            CONF,
            "nothing else changes"
        );
        let same = Fake::default()
            .dir("/boot", &["limine"])
            .file(LIMINE_CONF, CONF);
        assert_eq!(
            set(&same, &Change::Timeout(3)).unwrap(),
            "nothing to change"
        );
        assert_eq!(
            set(&same, &Change::Remove("absent".into())).unwrap(),
            "nothing to change"
        );
        assert!(same.writes().is_empty());
        assert!(set(&same, &Change::Timeout(61)).is_err());
        assert!(set(&Fake::default(), &Change::Timeout(1))
            .unwrap_err()
            .contains("cannot be read"));
        let grub = Fake::default().dir("/boot", &["grub"]).path(GRUB_CFG);
        assert!(set(&grub, &Change::Timeout(1))
            .unwrap_err()
            .contains("/etc/default/grub"));
    }

    #[test]
    fn systemd_boot_timeout_and_options() {
        let entry = format!("{SD_ENTRIES}/samurai-linux.conf");
        let sys = Fake::default()
            .dir("/boot", &["loader"])
            .file(SD_LOADER, "default samurai-linux.conf\n")
            .dir(SD_ENTRIES, &["samurai-linux.conf", "other.conf"])
            .file(&entry, "title x\noptions root=a quiet\n");
        set(&sys, &Change::Timeout(5)).unwrap();
        assert_eq!(sys.writes()[1].1, "timeout 5\ndefault samurai-linux.conf\n");
        set(&sys, &add("splash")).unwrap();
        assert_eq!(
            sys.writes().last().unwrap(),
            &(entry, "title x\noptions root=a quiet splash\n".to_string())
        );
        assert!(get(&sys).unwrap().contains("bootloader: systemd-boot\ntimeout: -\ndefault kernel: linux\nkernel parameters: root=a quiet\n"));
    }

    #[test]
    fn get_shows_the_limine_state() {
        let sys = Fake::default()
            .dir("/boot", &["limine"])
            .file(LIMINE_CONF, CONF);
        let out = get(&sys).unwrap();
        assert!(
            out.contains("bootloader: Limine\ntimeout: 3\ndefault kernel: linux\n"),
            "{out}"
        );
        assert!(out.contains("loglevel=3"));
    }
}

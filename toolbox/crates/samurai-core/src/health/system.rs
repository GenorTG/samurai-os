//! The only way health checks touch the machine: files, paths and commands. Checks take
//! `&dyn System`, so every check is unit-tested against [`fake::Fake`] without root or a VM.

use std::fs;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CmdOut {
    pub code: i32,
    pub stdout: String,
}

pub trait System {
    fn read(&self, path: &str) -> Option<String>;
    fn exists(&self, path: &str) -> bool;
    /// Entry names of a directory, sorted; empty when unreadable.
    fn list_dir(&self, path: &str) -> Vec<String>;
    /// Modification time of a path in seconds since the epoch.
    fn mtime(&self, path: &str) -> Option<u64>;
    /// Current time in seconds since the epoch.
    fn now(&self) -> u64;
    /// Replace a file atomically (temp file + rename in the same directory).
    fn write(&self, path: &str, content: &str) -> Result<(), String>;
    fn remove(&self, path: &str) -> Result<(), String>;
    /// `None` when the program cannot be started (missing). A non-zero exit is still `Some`.
    fn run(&self, prog: &str, args: &[&str]) -> Option<CmdOut>;
    /// Like [`run`](System::run) but as an unprivileged user (uid, gid): used when root works inside
    /// a user's files, so a symlink in their home cannot steer root.
    fn run_as(&self, uid: u32, gid: u32, prog: &str, args: &[&str]) -> Option<CmdOut>;
}

/// Where programs are looked up. The caller's `$PATH` is never used: root commands must not be
/// hijackable by a user-writable directory early in it.
pub const TRUSTED_PATH: [&str; 6] = [
    "/usr/local/sbin",
    "/usr/local/bin",
    "/usr/sbin",
    "/usr/bin",
    "/sbin",
    "/bin",
];

/// Variables a few diagnostic tools (vulkaninfo, glxinfo) need to find the user's display.
const PASS_THROUGH: [&str; 3] = ["DISPLAY", "WAYLAND_DISPLAY", "XDG_RUNTIME_DIR"];

/// First executable `prog` in `dirs`. Names containing `/` are refused: we only run known tools.
pub fn resolve_in(prog: &str, dirs: &[&str]) -> Option<PathBuf> {
    if prog.is_empty() || prog.contains('/') {
        return None;
    }
    dirs.iter()
        .map(|d| Path::new(d).join(prog))
        .find(|p| crate::is_executable(p))
}

fn command_for(prog: &str, args: &[&str]) -> Option<Command> {
    let path = resolve_in(prog, &TRUSTED_PATH)?;
    let mut c = Command::new(path);
    c.args(args)
        .env_clear()
        .env("PATH", TRUSTED_PATH.join(":"))
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stderr(Stdio::null());
    for k in PASS_THROUGH {
        if let Ok(v) = std::env::var(k) {
            c.env(k, v);
        }
    }
    Some(c)
}

fn collect(c: &mut Command) -> Option<CmdOut> {
    let out = c.output().ok()?;
    Some(CmdOut {
        code: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
    })
}

pub struct RealSystem;

impl System for RealSystem {
    fn read(&self, path: &str) -> Option<String> {
        fs::read_to_string(path).ok()
    }

    fn exists(&self, path: &str) -> bool {
        Path::new(path).exists()
    }

    fn write(&self, path: &str, content: &str) -> Result<(), String> {
        let tmp = format!("{path}.samurai-tmp");
        let result = fs::File::create(&tmp)
            .and_then(|mut f| {
                use std::io::Write;
                f.write_all(content.as_bytes())?;
                f.sync_all()
            })
            .and_then(|()| fs::rename(&tmp, path));
        result.map_err(|e| {
            let _ = fs::remove_file(&tmp);
            format!("cannot write {path}: {e}")
        })
    }

    fn remove(&self, path: &str) -> Result<(), String> {
        fs::remove_file(path).map_err(|e| format!("cannot remove {path}: {e}"))
    }

    fn mtime(&self, path: &str) -> Option<u64> {
        let m = fs::metadata(path).ok()?.modified().ok()?;
        Some(m.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs())
    }

    fn now(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs())
    }

    fn list_dir(&self, path: &str) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(path)
            .map(|d| {
                d.filter_map(|e| e.ok()?.file_name().into_string().ok())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }

    fn run(&self, prog: &str, args: &[&str]) -> Option<CmdOut> {
        collect(&mut command_for(prog, args)?)
    }

    fn run_as(&self, uid: u32, gid: u32, prog: &str, args: &[&str]) -> Option<CmdOut> {
        let mut c = command_for(prog, args)?;
        // std clears root's supplementary groups itself when a uid is set by root
        c.uid(uid).gid(gid);
        collect(&mut c)
    }
}

#[cfg(test)]
mod real_tests {
    use super::*;

    #[test]
    fn only_trusted_directories_are_searched() {
        let evil = std::env::temp_dir().join(format!("samurai-evil-{}", std::process::id()));
        let _ = fs::remove_dir_all(&evil);
        fs::create_dir_all(&evil).unwrap();
        let fake = evil.join("id");
        fs::write(&fake, "#!/bin/sh\necho evil\n").unwrap();
        fs::set_permissions(&fake, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        // the attacker directory wins only when it is in the list we pass; RealSystem never adds it
        assert_eq!(resolve_in("id", &[evil.to_str().unwrap()]), Some(fake));
        assert!(resolve_in("id", &TRUSTED_PATH).is_some_and(|p| !p.starts_with(&evil)));
        let out = RealSystem.run("id", &["-u"]).expect("id exists");
        assert!(!out.stdout.contains("evil"), "{out:?}");
    }

    #[test]
    fn path_like_and_empty_names_are_refused() {
        assert_eq!(resolve_in("", &TRUSTED_PATH), None);
        assert_eq!(resolve_in("/bin/sh", &TRUSTED_PATH), None);
        assert_eq!(resolve_in("../bin/sh", &TRUSTED_PATH), None);
        assert!(RealSystem.run("/bin/sh", &["-c", "true"]).is_none());
        assert!(RealSystem.run("definitely-not-a-program", &[]).is_none());
    }

    #[test]
    fn environment_is_scrubbed_but_display_variables_pass() {
        let out = RealSystem.run("env", &[]).expect("env exists").stdout;
        let keys: Vec<&str> = out.lines().filter_map(|l| l.split('=').next()).collect();
        assert!(
            keys.contains(&"PATH") && keys.contains(&"LC_ALL"),
            "{keys:?}"
        );
        for k in &keys {
            assert!(
                [
                    "PATH",
                    "LC_ALL",
                    "DISPLAY",
                    "WAYLAND_DISPLAY",
                    "XDG_RUNTIME_DIR",
                    "PWD",
                    "SHLVL",
                    "_"
                ]
                .contains(k),
                "unexpected variable {k}"
            );
        }
        assert!(out.contains(&format!("PATH={}", TRUSTED_PATH.join(":"))));
    }
}

#[cfg(test)]
pub mod fake {
    use super::{CmdOut, System};
    use std::cell::RefCell;
    use std::collections::{HashMap, HashSet};

    /// Scripted system. Commands are keyed by `"prog arg arg"`; repeating `cmd` for the same key
    /// queues successive answers (the last one repeats), so a fix can flip a check's result.
    #[derive(Default)]
    pub struct Fake {
        files: HashMap<String, String>,
        paths: HashSet<String>,
        dirs: HashMap<String, Vec<String>>,
        mtimes: HashMap<String, u64>,
        now: u64,
        cmds: HashMap<String, Vec<CmdOut>>,
        seen: RefCell<HashMap<String, usize>>,
        log: RefCell<Vec<String>>,
        written: RefCell<Vec<(String, String)>>,
    }

    impl Fake {
        pub fn file(mut self, path: &str, body: &str) -> Self {
            self.files.insert(path.to_string(), body.to_string());
            self.paths.insert(path.to_string());
            self
        }

        pub fn mtime(mut self, path: &str, epoch: u64) -> Self {
            self.mtimes.insert(path.to_string(), epoch);
            self.paths.insert(path.to_string());
            self
        }

        pub fn at(mut self, now: u64) -> Self {
            self.now = now;
            self
        }

        pub fn dir(mut self, path: &str, names: &[&str]) -> Self {
            self.dirs.insert(
                path.to_string(),
                names.iter().map(|n| (*n).to_string()).collect(),
            );
            self.paths.insert(path.to_string());
            self
        }

        pub fn path(mut self, path: &str) -> Self {
            self.paths.insert(path.to_string());
            self
        }

        pub fn cmd(mut self, line: &str, code: i32, stdout: &str) -> Self {
            self.cmds.entry(line.to_string()).or_default().push(CmdOut {
                code,
                stdout: stdout.to_string(),
            });
            self
        }

        /// Every file write or removal, in order.
        pub fn writes(&self) -> Vec<(String, String)> {
            self.written.borrow().clone()
        }

        /// Every command line that was run, in order.
        pub fn calls(&self) -> Vec<String> {
            self.log.borrow().clone()
        }
    }

    impl System for Fake {
        fn read(&self, path: &str) -> Option<String> {
            self.files.get(path).cloned()
        }

        fn exists(&self, path: &str) -> bool {
            self.paths.contains(path)
        }

        fn write(&self, path: &str, content: &str) -> Result<(), String> {
            self.written
                .borrow_mut()
                .push((path.to_string(), content.to_string()));
            Ok(())
        }

        fn remove(&self, path: &str) -> Result<(), String> {
            self.written
                .borrow_mut()
                .push((path.to_string(), "<removed>".to_string()));
            Ok(())
        }

        fn mtime(&self, path: &str) -> Option<u64> {
            self.mtimes.get(path).copied()
        }

        fn now(&self) -> u64 {
            self.now
        }

        fn list_dir(&self, path: &str) -> Vec<String> {
            self.dirs.get(path).cloned().unwrap_or_default()
        }

        fn run_as(&self, uid: u32, gid: u32, prog: &str, args: &[&str]) -> Option<CmdOut> {
            // answers come from the plain command key; the log records who ran it
            self.log
                .borrow_mut()
                .push(format!("@{uid}:{gid} {prog} {}", args.join(" ")));
            let key = std::iter::once(prog)
                .chain(args.iter().copied())
                .collect::<Vec<_>>()
                .join(" ");
            let answers = self.cmds.get(&key)?;
            Some(answers[0].clone())
        }

        fn run(&self, prog: &str, args: &[&str]) -> Option<CmdOut> {
            let key = std::iter::once(prog)
                .chain(args.iter().copied())
                .collect::<Vec<_>>()
                .join(" ");
            self.log.borrow_mut().push(key.clone());
            let answers = self.cmds.get(&key)?;
            let mut seen = self.seen.borrow_mut();
            let n = seen.entry(key).or_insert(0);
            let out = answers[(*n).min(answers.len() - 1)].clone();
            *n += 1;
            Some(out)
        }
    }
}

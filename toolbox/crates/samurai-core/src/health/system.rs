//! The only way health checks touch the machine: files, paths and commands. Checks take
//! `&dyn System`, so every check is unit-tested against [`fake::Fake`] without root or a VM.

use std::fs;
use std::path::Path;
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
    /// `None` when the program cannot be started (missing). A non-zero exit is still `Some`.
    fn run(&self, prog: &str, args: &[&str]) -> Option<CmdOut>;
}

pub struct RealSystem;

impl System for RealSystem {
    fn read(&self, path: &str) -> Option<String> {
        fs::read_to_string(path).ok()
    }

    fn exists(&self, path: &str) -> bool {
        Path::new(path).exists()
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
        let out = Command::new(prog)
            .args(args)
            .env("LC_ALL", "C")
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()?;
        Some(CmdOut {
            code: out.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        })
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
        cmds: HashMap<String, Vec<CmdOut>>,
        seen: RefCell<HashMap<String, usize>>,
        log: RefCell<Vec<String>>,
    }

    impl Fake {
        pub fn file(mut self, path: &str, body: &str) -> Self {
            self.files.insert(path.to_string(), body.to_string());
            self.paths.insert(path.to_string());
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

        fn list_dir(&self, path: &str) -> Vec<String> {
            self.dirs.get(path).cloned().unwrap_or_default()
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

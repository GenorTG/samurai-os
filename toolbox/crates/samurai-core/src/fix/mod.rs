//! `samurai fix` (spec 04 "Maintenance"): targeted repairs for the pacman problems people hit.
//!
//! Each repair is a function over [`System`] with a fixed argv, so every branch is unit-tested.
//! Root, the pre-repair snapshot and the confirmation prompts live in the CLI.

use crate::health::system::System;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Repair {
    /// Remove a stale pacman database lock, only when no package manager is running.
    Lock,
    /// Re-populate the pacman keyring; `reset` rebuilds it from scratch.
    Keyring { reset: bool },
    /// Rebuild DKMS modules for every installed kernel, then the initramfs.
    Dkms,
}

impl Repair {
    pub fn name(self) -> &'static str {
        match self {
            Repair::Lock => "lock",
            Repair::Keyring { .. } => "keyring",
            Repair::Dkms => "dkms",
        }
    }

    /// A snapshot is taken first when the repair rewrites system state.
    pub fn wants_snapshot(self) -> bool {
        !matches!(self, Repair::Lock)
    }

    /// Needs a typed confirmation (destructive, hard to undo).
    pub fn needs_confirm(self) -> bool {
        matches!(self, Repair::Keyring { reset: true })
    }
}

pub const DB_LOCK: &str = "/var/lib/pacman/db.lck";
const PKG_MANAGERS: &str = "pacman|pacman-key|paru|yay|pamac|pamac-daemon|packagekitd";

fn step(sys: &dyn System, prog: &str, args: &[&str], what: &str) -> Result<(), String> {
    match sys.run(prog, args) {
        Some(o) if o.code == 0 => Ok(()),
        Some(o) => Err(format!("{what}: {prog} exited {}", o.code)),
        None => Err(format!("{what}: {prog} is not installed")),
    }
}

pub fn lock(sys: &dyn System) -> Result<Vec<String>, String> {
    if !sys.exists(DB_LOCK) {
        return Ok(vec!["no lock file present; nothing to do".into()]);
    }
    match sys.run("pgrep", &["-x", PKG_MANAGERS]) {
        Some(o) if o.code == 0 => {
            let pids = o.stdout.split_whitespace().collect::<Vec<_>>().join(", ");
            Err(format!(
                "a package manager is running (pid {pids}); not removing its lock"
            ))
        }
        Some(o) if o.code == 1 => {
            step(sys, "rm", &["-f", DB_LOCK], "removing the lock")?;
            Ok(vec![format!("removed stale {DB_LOCK}")])
        }
        Some(o) => Err(format!(
            "pgrep failed (exit {}); cannot tell whether pacman is running",
            o.code
        )),
        None => Err("pgrep is not installed; cannot tell whether pacman is running".into()),
    }
}

pub fn keyring(sys: &dyn System, reset: bool) -> Result<Vec<String>, String> {
    let mut done = Vec::new();
    if reset {
        step(
            sys,
            "rm",
            &["-rf", "/etc/pacman.d/gnupg"],
            "removing the old keyring",
        )?;
        done.push("removed /etc/pacman.d/gnupg".to_string());
        step(sys, "pacman-key", &["--init"], "creating a new keyring")?;
        done.push("created a new keyring".to_string());
    }
    step(sys, "pacman-key", &["--populate"], "populating the keyring")?;
    done.push("populated the keyring from the installed key packages".to_string());
    Ok(done)
}

pub fn dkms(sys: &dyn System) -> Result<Vec<String>, String> {
    if !sys.exists("/usr/bin/dkms") {
        return Ok(vec!["dkms is not installed; nothing to rebuild".into()]);
    }
    let releases: Vec<String> = sys
        .list_dir("/usr/lib/modules")
        .into_iter()
        .filter(|d| sys.exists(&format!("/usr/lib/modules/{d}/build")))
        .collect();
    if releases.is_empty() {
        return Ok(vec![
            "no installed kernel has headers; nothing to rebuild".into()
        ]);
    }
    let mut done = Vec::new();
    let mut failed = Vec::new();
    for r in &releases {
        match sys.run("dkms", &["autoinstall", "-k", r]) {
            Some(o) if o.code == 0 => done.push(format!("dkms modules built for {r}")),
            Some(o) => failed.push(format!("{r} (exit {})", o.code)),
            None => return Err("dkms could not be started".into()),
        }
    }
    if !failed.is_empty() {
        return Err(format!(
            "dkms autoinstall failed for: {}",
            failed.join(", ")
        ));
    }
    step(sys, "mkinitcpio", &["-P"], "rebuilding the initramfs")?;
    done.push("rebuilt the initramfs for all kernels".to_string());
    Ok(done)
}

pub fn run(sys: &dyn System, r: Repair) -> Result<Vec<String>, String> {
    match r {
        Repair::Lock => lock(sys),
        Repair::Keyring { reset } => keyring(sys, reset),
        Repair::Dkms => dkms(sys),
    }
}

pub const USAGE: &str = "usage: samurai fix <lock|keyring|dkms> [options]

Targeted repairs (root). Repairs that rewrite system state take a snapper snapshot first.

  lock                remove a stale pacman lock (/var/lib/pacman/db.lck), but only when no
                      package manager is running
  keyring [--reset]   re-populate the pacman keyring; --reset deletes /etc/pacman.d/gnupg and
                      rebuilds it (asks first; --yes approves)
  dkms                rebuild DKMS modules for every installed kernel with headers, then the
                      initramfs

  --yes               approve the confirmation of `keyring --reset`

Exit codes: 0 repaired or nothing to do, 1 failed or refused, 2 usage error.
";

#[derive(Debug, PartialEq, Eq)]
pub struct Options {
    pub repair: Option<Repair>,
    pub yes: bool,
    pub help: bool,
}

pub fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut o = Options {
        repair: None,
        yes: false,
        help: false,
    };
    let mut reset = false;
    let mut name: Option<&str> = None;
    for a in args {
        match a.as_str() {
            "-h" | "--help" => o.help = true,
            "--yes" | "-y" => o.yes = true,
            "--reset" => reset = true,
            "lock" | "keyring" | "dkms" if name.is_none() => name = Some(a),
            other if other.starts_with('-') => return Err(format!("unknown option: {other}")),
            other => return Err(format!("unexpected argument: {other}")),
        }
    }
    if reset && name != Some("keyring") {
        return Err("--reset only applies to `keyring`".into());
    }
    if o.help {
        return Ok(o);
    }
    o.repair = Some(match name {
        Some("lock") => Repair::Lock,
        Some("keyring") => Repair::Keyring { reset },
        Some("dkms") => Repair::Dkms,
        _ => return Err("a repair is required: lock, keyring or dkms".into()),
    });
    Ok(o)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::health::system::fake::Fake;

    fn a(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| (*x).to_string()).collect()
    }

    #[test]
    fn lock_is_removed_only_when_no_package_manager_runs() {
        let rm = "rm -f /var/lib/pacman/db.lck";
        let pg = "pgrep -x pacman|pacman-key|paru|yay|pamac|pamac-daemon|packagekitd";
        // no lock: nothing happens, nothing is even asked
        let none = Fake::default();
        assert!(lock(&none).unwrap()[0].contains("nothing to do"));
        assert!(none.calls().is_empty());
        // stale lock, nobody running: removed
        let stale = Fake::default().path(DB_LOCK).cmd(pg, 1, "").cmd(rm, 0, "");
        assert!(lock(&stale).unwrap()[0].contains("removed stale"));
        assert_eq!(stale.calls(), vec![pg, rm]);
        // pacman is running: refused, rm never called
        let busy = Fake::default().path(DB_LOCK).cmd(pg, 0, "4242\\n");
        let e = lock(&busy).unwrap_err();
        assert!(e.contains("4242") && e.contains("running"), "{e}");
        assert!(!busy.calls().iter().any(|c| c.starts_with("rm")));
        // cannot tell: refuse rather than guess
        let blind = Fake::default().path(DB_LOCK);
        assert!(lock(&blind).unwrap_err().contains("pgrep is not installed"));
        let broken = Fake::default().path(DB_LOCK).cmd(pg, 2, "");
        assert!(lock(&broken).unwrap_err().contains("exit 2"));
        assert!(!broken.calls().iter().any(|c| c.starts_with("rm")));
    }

    #[test]
    fn keyring_populate_and_reset_sequences() {
        let sys = Fake::default().cmd("pacman-key --populate", 0, "");
        assert_eq!(keyring(&sys, false).unwrap().len(), 1);
        assert_eq!(sys.calls(), vec!["pacman-key --populate"]);
        let sys = Fake::default()
            .cmd("rm -rf /etc/pacman.d/gnupg", 0, "")
            .cmd("pacman-key --init", 0, "")
            .cmd("pacman-key --populate", 0, "");
        assert_eq!(keyring(&sys, true).unwrap().len(), 3);
        assert_eq!(
            sys.calls(),
            vec![
                "rm -rf /etc/pacman.d/gnupg",
                "pacman-key --init",
                "pacman-key --populate"
            ]
        );
        // a failing step stops the sequence: no populate after a failed init
        let sys = Fake::default()
            .cmd("rm -rf /etc/pacman.d/gnupg", 0, "")
            .cmd("pacman-key --init", 1, "");
        assert!(keyring(&sys, true).unwrap_err().contains("exited 1"));
        assert!(!sys.calls().contains(&"pacman-key --populate".to_string()));
        assert!(keyring(&Fake::default(), false)
            .unwrap_err()
            .contains("not installed"));
    }

    #[test]
    fn dkms_rebuilds_for_kernels_with_headers_only() {
        assert!(dkms(&Fake::default()).unwrap()[0].contains("not installed"));
        let nohdr = Fake::default()
            .path("/usr/bin/dkms")
            .dir("/usr/lib/modules", &["6.12.1-arch1-1"]);
        assert!(dkms(&nohdr).unwrap()[0].contains("no installed kernel has headers"));
        let ok = Fake::default()
            .path("/usr/bin/dkms")
            .dir("/usr/lib/modules", &["6.12.1-arch1-1", "6.12.2-zen1-1"])
            .path("/usr/lib/modules/6.12.1-arch1-1/build")
            .path("/usr/lib/modules/6.12.2-zen1-1/build")
            .cmd("dkms autoinstall -k 6.12.1-arch1-1", 0, "")
            .cmd("dkms autoinstall -k 6.12.2-zen1-1", 0, "")
            .cmd("mkinitcpio -P", 0, "");
        assert_eq!(dkms(&ok).unwrap().len(), 3);
        let bad = Fake::default()
            .path("/usr/bin/dkms")
            .dir("/usr/lib/modules", &["6.12.1-arch1-1"])
            .path("/usr/lib/modules/6.12.1-arch1-1/build")
            .cmd("dkms autoinstall -k 6.12.1-arch1-1", 10, "");
        assert!(dkms(&bad).unwrap_err().contains("6.12.1-arch1-1 (exit 10)"));
        assert!(
            !bad.calls().contains(&"mkinitcpio -P".to_string()),
            "no initramfs rebuild after a failed build"
        );
    }

    #[test]
    fn policy_flags() {
        assert!(!Repair::Lock.wants_snapshot());
        assert!(Repair::Dkms.wants_snapshot() && Repair::Keyring { reset: false }.wants_snapshot());
        assert!(Repair::Keyring { reset: true }.needs_confirm());
        assert!(!Repair::Keyring { reset: false }.needs_confirm() && !Repair::Dkms.needs_confirm());
    }

    #[test]
    fn argument_parsing() {
        assert_eq!(
            parse_args(&a(&["lock"])).unwrap().repair,
            Some(Repair::Lock)
        );
        assert_eq!(
            parse_args(&a(&["keyring", "--reset", "--yes"])).unwrap(),
            Options {
                repair: Some(Repair::Keyring { reset: true }),
                yes: true,
                help: false
            }
        );
        assert_eq!(
            parse_args(&a(&["dkms"])).unwrap().repair,
            Some(Repair::Dkms)
        );
        assert!(parse_args(&a(&["--help"])).unwrap().help);
        for bad in [
            &[][..],
            &["nope"],
            &["lock", "dkms"],
            &["lock", "--reset"],
            &["dkms", "--bogus"],
            &["--yes"],
        ] {
            assert!(parse_args(&a(bad)).is_err(), "{bad:?}");
        }
    }
}

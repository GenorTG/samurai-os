//! samurai-core: the pieces of the `samurai` CLI that carry logic and need tests.
//!
//! Slice 1 (M5): dispatch table, executor lookup and version lookup, behaviourally identical to the
//! shell dispatcher in `updates/samurai`. Executors (`samurai-update`, `-apps`, ...) stay as they are.

pub mod boot;
pub mod clean;
pub mod diag;
pub mod fix;
pub mod health;
pub mod kernel;
pub mod mirrors;
pub mod status;

use std::fs;
use std::path::{Path, PathBuf};

pub const TOOLS_VERSION_FILE: &str = "/usr/share/samurai/tools-version";
pub const FALLBACK_TOOL_DIR: &str = "/usr/local/bin";

pub const USAGE: &str = "usage: samurai <command> [args]

commands
  update      staged system update (spec 03): samurai update --help
  apps        optional apps from the catalog: list/search/info/install/remove/bundles
  snap        snapshot manager (spec 06): list/create/delete/diff/restore-file/repair/rollback
  rescue      recovery tool (spec 06): mount/chroot/bootloader/initramfs/kernel/rollback
  status      last update check and report (--json, --report: report only, --check)
  health      read-only health checks (spec 05): samurai health --help
  diag        diagnostics and the redacted support bundle: samurai diag --help
  clean       free disk space (preview first): samurai clean --help
  fix         targeted repairs: lock, keyring, dkms (samurai fix --help)
  mirrors     show or refresh the Arch mirrorlist (samurai mirrors --help)
  boot        boot entries, menu timeout, kernel parameters (samurai boot --help)
  kernel      list, install, remove, choose the default (samurai kernel --help)
  version     print the samurai-tools version
  help        this text

Every command accepts --help. Exit codes follow each tool's --help; this
dispatcher exits 2 for an unknown command.
";

/// What `samurai <cmd> [args]` should do.
#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    /// exec `samurai-<tool>` with `prefix` followed by the user's arguments.
    Exec {
        tool: &'static str,
        prefix: &'static [&'static str],
    },
    /// `samurai status`: handled natively when the args are none or `--json`, else the notifier.
    Status,
    /// `samurai health`: native, read-only.
    Health,
    /// `samurai diag`: native, read-only diagnostics + redacted bundle.
    Diag,
    /// `samurai clean`: preview and free disk space.
    Clean,
    /// `samurai fix`: targeted pacman repairs.
    Fix,
    /// `samurai mirrors`: show and refresh the Arch mirrorlist.
    Mirrors,
    /// `samurai boot`: keep boot entries in step with installed kernels.
    Boot,
    /// `samurai kernel`: list, install, remove and choose kernels.
    Kernel,
    Version,
    Help,
    Unknown(String),
}

pub fn parse(cmd: &str) -> Action {
    match cmd {
        "update" => Action::Exec {
            tool: "update",
            prefix: &[],
        },
        "apps" => Action::Exec {
            tool: "apps",
            prefix: &[],
        },
        "snap" => Action::Exec {
            tool: "snap",
            prefix: &[],
        },
        "rescue" => Action::Exec {
            tool: "rescue",
            prefix: &[],
        },
        "status" => Action::Status,
        "health" => Action::Health,
        "diag" => Action::Diag,
        "clean" => Action::Clean,
        "fix" => Action::Fix,
        "mirrors" => Action::Mirrors,
        "boot" => Action::Boot,
        "kernel" => Action::Kernel,
        "version" | "--version" | "-V" => Action::Version,
        "help" | "--help" | "-h" => Action::Help,
        other => Action::Unknown(other.to_string()),
    }
}

/// Executor path: beside the running binary first, then `/usr/local/bin`.
pub fn resolve_tool(here: &Path, fallback: &Path, tool: &str) -> PathBuf {
    let name = format!("samurai-{tool}");
    let beside = here.join(&name);
    if is_executable(&beside) {
        beside
    } else {
        fallback.join(name)
    }
}

pub fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(p)
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

/// First line of `file`, trimmed. `None` when unreadable or blank.
pub fn read_version_file(file: &Path) -> Option<String> {
    let text = fs::read_to_string(file).ok()?;
    let line = text.lines().next()?.trim();
    (!line.is_empty()).then(|| line.to_string())
}

/// Version of `pacman -Q samurai-tools` output (`"samurai-tools 0.3.0-3"`).
pub fn parse_pacman_q(out: &str) -> Option<String> {
    out.split_whitespace().nth(1).map(str::to_string)
}

pub fn version_line(version: Option<&str>) -> String {
    match version {
        Some(v) => format!("samurai {v} (samurai-tools)"),
        None => "samurai (unpackaged copy, not installed from samurai-tools)".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("samurai-core-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn parse_table() {
        assert_eq!(
            parse("update"),
            Action::Exec {
                tool: "update",
                prefix: &[]
            }
        );
        assert_eq!(parse("status"), Action::Status);
        assert_eq!(parse("health"), Action::Health);
        assert_eq!(parse("diag"), Action::Diag);
        assert_eq!(parse("clean"), Action::Clean);
        assert_eq!(parse("fix"), Action::Fix);
        assert_eq!(parse("mirrors"), Action::Mirrors);
        assert_eq!(parse("boot"), Action::Boot);
        assert_eq!(parse("kernel"), Action::Kernel);
        for v in ["version", "--version", "-V"] {
            assert_eq!(parse(v), Action::Version);
        }
        for h in ["help", "--help", "-h"] {
            assert_eq!(parse(h), Action::Help);
        }
        assert_eq!(parse("bogus"), Action::Unknown("bogus".into()));
    }

    #[test]
    fn resolve_prefers_beside_then_fallback() {
        let here = tmp("here");
        let fb = tmp("fb");
        assert_eq!(resolve_tool(&here, &fb, "apps"), fb.join("samurai-apps"));
        let t = here.join("samurai-apps");
        fs::write(&t, "#!/bin/sh\n").unwrap();
        assert_eq!(
            resolve_tool(&here, &fb, "apps"),
            fb.join("samurai-apps"),
            "not executable yet"
        );
        fs::set_permissions(&t, fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(resolve_tool(&here, &fb, "apps"), t);
    }

    #[test]
    fn version_sources() {
        let d = tmp("ver");
        let f = d.join("v");
        assert_eq!(read_version_file(&f), None);
        fs::write(&f, "\n").unwrap();
        assert_eq!(read_version_file(&f), None);
        fs::write(&f, "0.3.0-3\nignored\n").unwrap();
        assert_eq!(read_version_file(&f).as_deref(), Some("0.3.0-3"));
        assert_eq!(
            parse_pacman_q("samurai-tools 0.3.0-3\n").as_deref(),
            Some("0.3.0-3")
        );
        assert_eq!(parse_pacman_q(""), None);
        assert_eq!(version_line(Some("1-2")), "samurai 1-2 (samurai-tools)");
        assert!(version_line(None).contains("unpackaged"));
    }
}

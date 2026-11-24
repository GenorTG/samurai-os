//! Black-box tests of the `samurai` binary, with stub executors beside a copy of it.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn sandbox(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("samurai-cli-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    fs::copy(env!("CARGO_BIN_EXE_samurai"), d.join("samurai")).unwrap();
    d
}

fn stub(dir: &Path, tool: &str, code: i32) {
    let p = dir.join(format!("samurai-{tool}"));
    fs::write(&p, format!("#!/bin/sh\necho \"{tool}:$*\"\nexit {code}\n")).unwrap();
    fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
}

fn run(dir: &Path, args: &[&str]) -> Output {
    Command::new(dir.join("samurai")).args(args).output().unwrap()
}

fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

#[test]
fn no_args_and_help_print_usage() {
    let d = sandbox("help");
    for a in [&[][..], &["help"], &["--help"], &["-h"]] {
        let o = run(&d, a);
        assert_eq!(o.status.code(), Some(0));
        assert!(out(&o).starts_with("usage: samurai <command> [args]"));
    }
}

#[test]
fn unknown_command_exits_2_with_usage_on_stderr() {
    let d = sandbox("unknown");
    let o = run(&d, &["frobnicate"]);
    assert_eq!(o.status.code(), Some(2));
    let e = String::from_utf8_lossy(&o.stderr);
    assert!(e.contains("samurai: unknown command: frobnicate"));
    assert!(e.contains("usage: samurai"));
}

#[test]
fn executors_get_args_and_exit_code_passes_through() {
    let d = sandbox("exec");
    stub(&d, "apps", 0);
    stub(&d, "update", 7);
    let o = run(&d, &["apps", "list", "--json"]);
    assert_eq!(out(&o), "apps:list --json\n");
    assert_eq!(o.status.code(), Some(0));
    assert_eq!(run(&d, &["update", "--check"]).status.code(), Some(7));
}

#[test]
fn status_maps_to_notifier_status() {
    let d = sandbox("status");
    stub(&d, "notifier", 0);
    assert_eq!(out(&run(&d, &["status", "--json"])), "notifier:--status --json\n");
}

#[test]
fn missing_executor_exits_2() {
    let d = sandbox("missing");
    let o = run(&d, &["snap", "list"]);
    assert_eq!(o.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&o.stderr).contains("command \"snap\" is not installed"));
}

#[test]
fn version_prints_a_samurai_line() {
    let d = sandbox("version");
    for a in ["version", "--version", "-V"] {
        let o = run(&d, &[a]);
        assert_eq!(o.status.code(), Some(0));
        assert!(out(&o).starts_with("samurai "));
    }
}

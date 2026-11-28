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
    // Parallel tests fork while a sibling's freshly copied binary is still open for writing:
    // exec then fails with ETXTBSY (os error 26). Retry; it clears within milliseconds.
    for _ in 0..50 {
        match Command::new(dir.join("samurai")).args(args).output() {
            Err(e) if e.raw_os_error() == Some(26) => {
                std::thread::sleep(std::time::Duration::from_millis(20))
            }
            // the same race one level down: the dispatcher could not exec a just-written stub
            Ok(o) if String::from_utf8_lossy(&o.stderr).contains("os error 26") => {
                std::thread::sleep(std::time::Duration::from_millis(20))
            }
            r => return r.unwrap(),
        }
    }
    panic!("samurai binary stayed busy");
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
    stub(&d, "adopt", 4);
    let o = run(&d, &["adopt", "--preview"]);
    assert_eq!(out(&o), "adopt:--preview\n");
    assert_eq!(o.status.code(), Some(4));
}

#[test]
fn status_is_native_and_read_only() {
    let d = sandbox("status");
    let state = d.join("state");
    fs::create_dir_all(state.join("samurai")).unwrap();
    fs::write(state.join("samurai/update-check"), "arch=2\n").unwrap();
    fs::write(
        state.join("samurai/update-check.json"),
        "{\"schema\": 1, \"arch\": 2}\n",
    )
    .unwrap();
    let run_status = |args: &[&str], xdg: &Path| {
        for _ in 0..50 {
            match Command::new(d.join("samurai"))
                .arg("status")
                .args(args)
                .env("XDG_STATE_HOME", xdg)
                .env("HOME", &d)
                .output()
            {
                Err(e) if e.raw_os_error() == Some(26) => {
                    std::thread::sleep(std::time::Duration::from_millis(20))
                }
                // the same race one level down: the dispatcher could not exec a just-written stub
                Ok(o) if String::from_utf8_lossy(&o.stderr).contains("os error 26") => {
                    std::thread::sleep(std::time::Duration::from_millis(20))
                }
                r => return r.unwrap(),
            }
        }
        panic!("samurai binary stayed busy");
    };
    let o = run_status(&[], &state);
    assert_eq!(o.status.code(), Some(0));
    assert!(out(&o).starts_with("arch=2\n"));
    assert_eq!(
        out(&run_status(&["--json"], &state)),
        "{\"schema\": 1, \"arch\": 2}\n"
    );
    // no state: message, exit 0, and no directory is created
    let empty = d.join("empty");
    assert_eq!(out(&run_status(&[], &empty)), "no update check yet\n");
    assert!(!empty.exists(), "status must not create state dirs");
}

#[test]
fn status_other_flags_go_to_the_notifier() {
    let d = sandbox("status-fwd");
    stub(&d, "notifier", 0);
    assert_eq!(
        out(&run(&d, &["status", "--check"])),
        "notifier:--status --check\n"
    );
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

#[test]
fn health_list_help_and_usage_errors() {
    let d = sandbox("health-cli");
    let o = run(&d, &["health", "--list"]);
    assert_eq!(o.status.code(), Some(0));
    assert!(out(&o).lines().any(|l| l == "services.failed"));
    let o = run(&d, &["health", "--help"]);
    assert_eq!(o.status.code(), Some(0));
    assert!(out(&o).starts_with("usage: samurai health"));
    for bad in [
        &["health", "--bogus"][..],
        &["health", "--id", "nope"],
        &["health", "--category", "nope"],
    ] {
        assert_eq!(run(&d, bad).status.code(), Some(2), "{bad:?}");
    }
}

#[test]
fn health_json_is_schema_1_and_exit_code_matches_issues() {
    let d = sandbox("health-json");
    // pacman.conf is absent or differs here, so only assert the invariants, not the findings
    let o = run(&d, &["health", "--json", "--category", "storage"]);
    let text = out(&o);
    assert!(
        text.starts_with("{\"schema\":1,\"checked_epoch\":"),
        "{text}"
    );
    assert!(text.contains("\"id\":\"storage.disk-free\""));
    let issues = text.contains("\"issues\":0");
    assert_eq!(o.status.code(), Some(if issues { 0 } else { 1 }));
}

#[test]
fn diag_usage_help_and_bundle_redacts_and_is_private() {
    let d = sandbox("diag");
    for bad in [
        &["diag"][..],
        &["diag", "nope"],
        &["diag", "info", "--x"],
        &["diag", "logs", "--lines", "0"],
    ] {
        assert_eq!(run(&d, bad).status.code(), Some(2), "{bad:?}");
    }
    let o = run(&d, &["diag", "--help"]);
    assert_eq!(o.status.code(), Some(0));
    assert!(out(&o).starts_with("usage: samurai diag"));

    let file = d.join("bundle.txt");
    let o = run(&d, &["diag", "bundle", "--out", file.to_str().unwrap()]);
    assert_eq!(
        o.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    assert!(out(&o).contains("(redacted)"));
    let mode = fs::metadata(&file).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "bundle must be private");
    let text = fs::read_to_string(&file).unwrap();
    assert!(text.contains("== kernel ==") && text.contains("== samurai health (json) =="));
    // refuses to overwrite an existing bundle
    assert_eq!(
        run(&d, &["diag", "bundle", "--out", file.to_str().unwrap()])
            .status
            .code(),
        Some(1)
    );
    // --no-redact without a terminal is refused
    let o = run(
        &d,
        &[
            "diag",
            "bundle",
            "--no-redact",
            "--out",
            d.join("raw.txt").to_str().unwrap(),
        ],
    );
    assert_eq!(o.status.code(), Some(2));
    assert!(!d.join("raw.txt").exists());
}

#[test]
fn clean_usage_preview_and_refusals() {
    let d = sandbox("clean");
    for bad in [
        &["clean", "nope"][..],
        &["clean", "--bogus"],
        &["clean", "--keep", "x"],
        &["clean", "--json"],
    ] {
        assert_eq!(run(&d, bad).status.code(), Some(2), "{bad:?}");
    }
    let o = run(&d, &["clean", "--help"]);
    assert_eq!(o.status.code(), Some(0));
    assert!(out(&o).starts_with("usage: samurai clean"));
    // preview is read-only and always exits 0
    let o = run(&d, &["clean", "--preview"]);
    assert_eq!(o.status.code(), Some(0));
    let text = out(&o);
    assert!(
        text.contains("pkgcache") && text.contains("thumbnails") && text.contains("total"),
        "{text}"
    );
    assert!(
        !text.contains("orphans"),
        "orphans are not part of the default selection"
    );
    let o = run(&d, &["clean", "orphans", "cache", "--preview", "--json"]);
    assert!(
        out(&o).starts_with("{\"schema\":1,\"targets\":[{\"id\":\"orphans\""),
        "{}",
        out(&o)
    );
    // without a terminal a confirmation target is declined, nothing is touched
    let o = run(&d, &["clean", "orphans"]);
    assert!(
        out(&o).contains("declined orphans") || out(&o).contains("skipped  orphans"),
        "{}",
        out(&o)
    );
    assert_eq!(o.status.code(), Some(1));
}

#[test]
fn fix_usage_help_and_root_requirement() {
    let d = sandbox("fix");
    for bad in [
        &["fix"][..],
        &["fix", "nope"],
        &["fix", "lock", "--bogus"],
        &["fix", "lock", "--reset"],
    ] {
        assert_eq!(run(&d, bad).status.code(), Some(2), "{bad:?}");
    }
    let o = run(&d, &["fix", "--help"]);
    assert_eq!(o.status.code(), Some(0));
    assert!(out(&o).starts_with("usage: samurai fix"));
    // the test process is not root: a valid repair is refused before anything runs
    let uid = Command::new("id")
        .arg("-u")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
    if uid.is_ok_and(|u| u != "0") {
        let o = run(&d, &["fix", "lock"]);
        assert_eq!(o.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&o.stderr).contains("needs root"));
    }
}

#[test]
fn mirrors_usage_and_help() {
    let d = sandbox("mirrors");
    for bad in [
        &["mirrors"][..],
        &["mirrors", "nope"],
        &["mirrors", "refresh", "--all"],
    ] {
        assert_eq!(run(&d, bad).status.code(), Some(2), "{bad:?}");
    }
    let o = run(&d, &["mirrors", "--help"]);
    assert_eq!(o.status.code(), Some(0));
    assert!(out(&o).starts_with("usage: samurai mirrors"));
}

#[test]
fn boot_usage_and_help() {
    let d = sandbox("boot");
    for bad in [&["boot"][..], &["boot", "nope"], &["boot", "sync", "x"]] {
        assert_eq!(run(&d, bad).status.code(), Some(2), "{bad:?}");
    }
    let o = run(&d, &["boot", "--help"]);
    assert_eq!(o.status.code(), Some(0));
    assert!(out(&o).starts_with("usage: samurai boot <sync|get|set>"));
}

#[test]
fn kernel_usage_and_help() {
    let d = sandbox("kernel");
    for bad in [
        &["kernel"][..],
        &["kernel", "install"],
        &["kernel", "list", "x"],
    ] {
        assert_eq!(run(&d, bad).status.code(), Some(2), "{bad:?}");
    }
    let o = run(&d, &["kernel", "--help"]);
    assert_eq!(o.status.code(), Some(0));
    assert!(out(&o).starts_with("usage: samurai kernel"));
}

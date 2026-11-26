//! `samurai status`: print the last update check (written by `samurai-notifier`) and the last
//! `samurai update` report. Read-only: never creates directories or runs a check.

use std::fs;
use std::path::{Path, PathBuf};

pub const REPORT_PATH: &str = "/var/lib/samurai/update-report.json";

/// `$XDG_STATE_HOME/samurai`, else `$HOME/.local/state/samurai`.
pub fn state_dir(xdg_state_home: Option<&str>, home: Option<&str>) -> Option<PathBuf> {
    let base = match xdg_state_home.filter(|s| !s.is_empty()) {
        Some(x) => PathBuf::from(x),
        None => Path::new(home.filter(|s| !s.is_empty())?).join(".local/state"),
    };
    Some(base.join("samurai"))
}

/// Which arguments the native implementation accepts.
pub fn is_native(args: &[String]) -> bool {
    args.is_empty() || (args.len() == 1 && (args[0] == "--json" || args[0] == "--report"))
}

/// The last `samurai update` report (JSON, schema 1), or an error object when there is none.
pub fn render_report(report: &Path) -> String {
    fs::read_to_string(report).unwrap_or_else(|_| "{\"schema\": 1, \"error\": 1}\n".to_string())
}

pub fn render(state_dir: Option<&Path>, report: &Path, json: bool) -> String {
    let read = |name: &str| state_dir.and_then(|d| fs::read_to_string(d.join(name)).ok());
    if json {
        return read("update-check.json")
            .unwrap_or_else(|| "{\"schema\": 1, \"error\": 1}\n".to_string());
    }
    let mut out = read("update-check").unwrap_or_else(|| "no update check yet\n".to_string());
    if let Ok(r) = fs::read_to_string(report) {
        out.push_str(&format!(
            "--- last samurai update report ({})\n{r}",
            report.display()
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("samurai-status-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn state_dir_resolution() {
        assert_eq!(
            state_dir(Some("/x"), Some("/h")),
            Some(PathBuf::from("/x/samurai"))
        );
        assert_eq!(
            state_dir(Some(""), Some("/h")),
            Some(PathBuf::from("/h/.local/state/samurai"))
        );
        assert_eq!(state_dir(None, None), None);
    }

    #[test]
    fn native_args() {
        assert!(is_native(&[]));
        assert!(is_native(&["--json".to_string()]));
        assert!(is_native(&["--report".to_string()]));
        assert!(!is_native(&["--check".to_string()]));
        assert!(!is_native(&["--json".to_string(), "x".to_string()]));
    }

    #[test]
    fn no_state_no_report() {
        let d = tmp("none");
        let missing = d.join("report.json");
        assert_eq!(render(Some(&d), &missing, false), "no update check yet\n");
        assert_eq!(
            render(None, &missing, true),
            "{\"schema\": 1, \"error\": 1}\n"
        );
    }

    #[test]
    fn report_is_the_file_or_an_error_object() {
        let d = tmp("report");
        let f = d.join("report.json");
        assert_eq!(render_report(&f), "{\"schema\": 1, \"error\": 1}\n");
        fs::write(&f, "{\"ok\": true}\n").unwrap();
        assert_eq!(render_report(&f), "{\"ok\": true}\n");
    }

    #[test]
    fn state_and_report_text_and_json() {
        let d = tmp("full");
        fs::write(d.join("update-check"), "checked=now\narch=3\n").unwrap();
        fs::write(
            d.join("update-check.json"),
            "{\"schema\": 1, \"arch\": 3}\n",
        )
        .unwrap();
        let rep = d.join("report.json");
        fs::write(&rep, "{\"ok\": true}\n").unwrap();
        let text = render(Some(&d), &rep, false);
        assert!(text.starts_with("checked=now\narch=3\n--- last samurai update report ("));
        assert!(text.ends_with("{\"ok\": true}\n"));
        // JSON output never appends the report
        assert_eq!(
            render(Some(&d), &rep, true),
            "{\"schema\": 1, \"arch\": 3}\n"
        );
    }
}

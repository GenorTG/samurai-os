//! `samurai mirrors` (spec 04): show and refresh the Arch mirrorlist with reflector.
//!
//! A new list replaces the old one only after it validates: every active line must be an https
//! `Server =` for the Arch layout. The previous list is kept as `.bak`. The package databases are
//! deliberately NOT synced here: `-Sy` without `-u` is a partial upgrade.

use crate::health::system::System;

pub const MIRRORLIST: &str = "/etc/pacman.d/mirrorlist";
pub const CANDIDATE: &str = "/etc/pacman.d/mirrorlist.samurai-new";
const REFLECTOR: [&str; 12] = [
    "300",
    "reflector",
    "--protocol",
    "https",
    "--latest",
    "20",
    "--sort",
    "rate",
    "--connection-timeout",
    "10",
    "--download-timeout",
    "10",
];

fn valid_server(line: &str) -> bool {
    let Some(url) = line.strip_prefix("Server = https://") else {
        return false;
    };
    let Some(base) = url.strip_suffix("$repo/os/$arch") else {
        return false;
    };
    !base.is_empty()
        && base
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._:/-~".contains(c))
}

/// Number of servers when every active line is a valid https Arch server and there is at least one.
pub fn validate(list: &str) -> Result<usize, String> {
    let mut n = 0;
    for line in list
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
    {
        if !valid_server(line) {
            return Err(format!("unexpected line in the new mirrorlist: {line}"));
        }
        n += 1;
    }
    if n == 0 {
        return Err("the new mirrorlist has no servers".into());
    }
    Ok(n)
}

pub fn status(sys: &dyn System) -> Result<String, String> {
    let list = sys
        .read(MIRRORLIST)
        .ok_or(format!("cannot read {MIRRORLIST}"))?;
    let servers: Vec<&str> = list
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with("Server"))
        .collect();
    let age = sys.mtime(MIRRORLIST).map_or("unknown".to_string(), |t| {
        format!("{} day(s)", sys.now().saturating_sub(t) / 86_400)
    });
    let mut out = format!("{MIRRORLIST}\nage: {age}\nservers: {}\n", servers.len());
    for s in servers.iter().take(5) {
        out.push_str(&format!("  {s}\n"));
    }
    if servers.len() > 5 {
        out.push_str(&format!("  ... and {} more\n", servers.len() - 5));
    }
    Ok(out)
}

pub fn refresh(sys: &dyn System) -> Result<String, String> {
    let mut args: Vec<&str> = REFLECTOR.to_vec();
    args.extend(["--save", CANDIDATE]);
    let result = apply_candidate(sys, &args);
    // the candidate never outlives the run, whatever happened
    let _ = sys.run("rm", &["-f", CANDIDATE]);
    result
}

fn apply_candidate(sys: &dyn System, args: &[&str]) -> Result<String, String> {
    match sys.run("timeout", args) {
        Some(o) if o.code == 0 => {}
        Some(o) if o.code == 127 => {
            return Err("reflector is not installed (pacman -S reflector)".into())
        }
        Some(o) if o.code == 124 => {
            return Err("reflector timed out after 300 s; the old list is unchanged".into())
        }
        Some(o) => {
            return Err(format!(
                "reflector failed (exit {}); the old list is unchanged",
                o.code
            ))
        }
        None => return Err("timeout (coreutils) is not available".into()),
    }
    let new = sys
        .read(CANDIDATE)
        .ok_or("reflector wrote no list; the old one is unchanged")?;
    let n = validate(&new)?;
    if sys.exists(MIRRORLIST) {
        match sys.run("cp", &["-p", MIRRORLIST, &format!("{MIRRORLIST}.bak")]) {
            Some(o) if o.code == 0 => {}
            _ => return Err("could not back up the old mirrorlist; not replacing it".into()),
        }
    }
    match sys.run("install", &["-m644", CANDIDATE, MIRRORLIST]) {
        Some(o) if o.code == 0 => Ok(format!(
            "{n} servers written (previous list: {MIRRORLIST}.bak). Next: samurai update"
        )),
        _ => Err("could not install the new mirrorlist".into()),
    }
}

pub const USAGE: &str = "usage: samurai mirrors <status|refresh>

  status     age and servers of the Arch mirrorlist
  refresh    pick the 20 most recently synced https mirrors with reflector, validate the result,
             keep the old list as mirrorlist.bak (root). Does not sync package databases.

Exit codes: 0 ok, 1 failed, 2 usage error.
";

#[derive(Debug, PartialEq, Eq)]
pub enum Cmd {
    Status,
    Refresh,
    Help,
}

pub fn parse_args(args: &[String]) -> Result<Cmd, String> {
    match args {
        [a] if a == "status" => Ok(Cmd::Status),
        [a] if a == "refresh" => Ok(Cmd::Refresh),
        [a] if a == "--help" || a == "-h" => Ok(Cmd::Help),
        [] => Err("a command is required: status or refresh".into()),
        [a, ..] if a != "status" && a != "refresh" => Err(format!("unknown command: {a}")),
        [_, b, ..] => Err(format!("unexpected argument: {b}")),
        [a] => Err(format!("unknown command: {a}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::health::system::fake::Fake;

    const GOOD: &str = "# reflector\nServer = https://a.example/arch/$repo/os/$arch\n\nServer = https://b.example:8443/~m/$repo/os/$arch\n";

    fn reflector_line() -> String {
        format!("timeout {} --save {CANDIDATE}", REFLECTOR.join(" "))
    }

    #[test]
    fn validation_accepts_only_https_arch_servers() {
        assert_eq!(validate(GOOD), Ok(2));
        for bad in [
            "",
            "# only comments\n",
            "Server = http://a.example/$repo/os/$arch\n",
            "Server = https://a.example/\n",
            "Server = https:///$repo/os/$arch x\n",
            "Server = https://a.example/$(rm -rf)/$repo/os/$arch\n",
            "Include = /tmp/evil\n",
            "Server = https://a.example/$repo/os/$arch\nSigLevel = Never\n",
        ] {
            assert!(validate(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn refresh_backs_up_installs_and_removes_the_candidate() {
        let bak = format!("cp -p {MIRRORLIST} {MIRRORLIST}.bak");
        let inst = format!("install -m644 {CANDIDATE} {MIRRORLIST}");
        let rm = format!("rm -f {CANDIDATE}");
        let sys = Fake::default()
            .path(MIRRORLIST)
            .file(CANDIDATE, GOOD)
            .cmd(&reflector_line(), 0, "")
            .cmd(&bak, 0, "")
            .cmd(&inst, 0, "")
            .cmd(&rm, 0, "");
        assert!(refresh(&sys).unwrap().starts_with("2 servers written"));
        assert_eq!(sys.calls(), vec![reflector_line(), bak, inst, rm]);
        assert!(
            !sys.calls().iter().any(|c| c.starts_with("pacman")),
            "never syncs databases"
        );
    }

    #[test]
    fn a_bad_result_never_touches_the_current_list() {
        let cases = [
            Fake::default().cmd(&reflector_line(), 1, ""),
            Fake::default().cmd(&reflector_line(), 124, ""),
            Fake::default().cmd(&reflector_line(), 127, ""),
            Fake::default().cmd(&reflector_line(), 0, ""),
            Fake::default()
                .cmd(&reflector_line(), 0, "")
                .file(CANDIDATE, "Server = http://x/$repo/os/$arch\n"),
            Fake::default()
                .path(MIRRORLIST)
                .cmd(&reflector_line(), 0, "")
                .file(CANDIDATE, GOOD),
        ];
        for sys in cases {
            assert!(refresh(&sys).is_err());
            let calls = sys.calls();
            assert!(!calls.iter().any(|c| c.starts_with("install")), "{calls:?}");
            assert_eq!(calls.last().unwrap(), &format!("rm -f {CANDIDATE}"));
        }
    }

    #[test]
    fn status_reports_age_and_servers() {
        let sys = Fake::default()
            .at(10 * 86_400)
            .file(MIRRORLIST, GOOD)
            .mtime(MIRRORLIST, 7 * 86_400);
        let s = status(&sys).unwrap();
        assert!(
            s.contains("age: 3 day(s)") && s.contains("servers: 2"),
            "{s}"
        );
        assert!(status(&Fake::default()).is_err());
    }

    #[test]
    fn argument_parsing() {
        let a = |v: &[&str]| v.iter().map(|x| (*x).to_string()).collect::<Vec<_>>();
        assert_eq!(parse_args(&a(&["status"])), Ok(Cmd::Status));
        assert_eq!(parse_args(&a(&["refresh"])), Ok(Cmd::Refresh));
        assert_eq!(parse_args(&a(&["-h"])), Ok(Cmd::Help));
        for bad in [&[][..], &["nope"], &["refresh", "--all"], &["status", "x"]] {
            assert!(parse_args(&a(bad)).is_err(), "{bad:?}");
        }
    }
}

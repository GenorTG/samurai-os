//! `samurai diag` (spec 05): read-only diagnostics and the redacted bundle.
//!
//! Every collector goes through [`System`], so each is unit-tested against the scripted fake.

pub mod redact;

use crate::health::system::System;
use crate::health::{self, checks::ALL};
use redact::Redactor;

pub struct Section {
    pub title: String,
    pub body: String,
}

pub fn render(sections: &[Section]) -> String {
    let mut out = String::new();
    for s in sections {
        out.push_str(&format!("== {} ==\n{}\n\n", s.title, s.body.trim_end()));
    }
    out
}

fn sec(title: &str, body: String) -> Section {
    Section {
        title: title.to_string(),
        body,
    }
}

/// Command output, or a visible marker when the tool is missing or failed.
fn cmd(sys: &dyn System, prog: &str, args: &[&str]) -> String {
    match sys.run(prog, args) {
        None => format!("(not available: {prog})"),
        Some(o) if o.code == 0 => o.stdout,
        Some(o) => format!("(exit {}) {}", o.code, o.stdout),
    }
}

fn file(sys: &dyn System, path: &str) -> String {
    sys.read(path)
        .unwrap_or_else(|| format!("(unreadable: {path})"))
}

fn head(s: &str, n: usize) -> String {
    s.lines().take(n).collect::<Vec<_>>().join("\n")
}

fn tail(s: &str, n: usize) -> String {
    let all: Vec<&str> = s.lines().collect();
    all[all.len().saturating_sub(n)..].join("\n")
}

fn grep_lines(s: &str, prefixes: &[&str]) -> String {
    s.lines()
        .filter(|l| prefixes.iter().any(|p| l.trim_start().starts_with(p)))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn info(sys: &dyn System) -> Vec<Section> {
    let loaders = [
        "/boot/limine/limine.conf",
        "/boot/limine.conf",
        "/boot/grub/grub.cfg",
        "/boot/loader/loader.conf",
    ];
    let present: Vec<&str> = loaders.iter().copied().filter(|p| sys.exists(p)).collect();
    let boot = format!(
        "firmware: {}\nbootloader config: {}\nsecure boot: {}",
        if sys.exists("/sys/firmware/efi") {
            "UEFI"
        } else {
            "BIOS"
        },
        if present.is_empty() {
            "none found".to_string()
        } else {
            present.join(", ")
        },
        head(&cmd(sys, "mokutil", &["--sb-state"]), 1),
    );
    vec![
        sec("kernel", cmd(sys, "uname", &["-srm"])),
        sec("os-release", file(sys, "/etc/os-release")),
        sec(
            "cpu",
            grep_lines(
                &cmd(sys, "lscpu", &[]),
                &["Architecture", "Vendor ID", "Model name", "CPU(s):"],
            ),
        ),
        sec(
            "memory",
            grep_lines(&file(sys, "/proc/meminfo"), &["MemTotal", "SwapTotal"]),
        ),
        sec("pci devices and drivers", cmd(sys, "lspci", &["-nnk"])),
        sec(
            "block devices",
            cmd(sys, "lsblk", &["-o", "NAME,SIZE,TYPE,FSTYPE,MOUNTPOINTS"]),
        ),
        sec("virtualization", cmd(sys, "systemd-detect-virt", &[])),
        sec("boot", boot),
    ]
}

pub fn services(sys: &dyn System) -> Vec<Section> {
    vec![
        sec("boot time", cmd(sys, "systemd-analyze", &[])),
        sec(
            "slowest units (top 20)",
            head(&cmd(sys, "systemd-analyze", &["blame"]), 20),
        ),
        sec(
            "critical chain",
            cmd(sys, "systemd-analyze", &["critical-chain"]),
        ),
        sec(
            "failed units",
            cmd(sys, "systemctl", &["--failed", "--no-legend", "--plain"]),
        ),
    ]
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct LogOpts {
    pub kernel: bool,
    pub pacman: bool,
    pub update: bool,
    pub lines: Option<usize>,
}

pub fn logs(sys: &dyn System, o: &LogOpts) -> Vec<Section> {
    let n = o.lines.unwrap_or(200);
    let n_s = n.to_string();
    let mut out = Vec::new();
    let any = o.kernel || o.pacman || o.update;
    if !any {
        out.push(sec(
            "journal errors, this boot",
            cmd(
                sys,
                "journalctl",
                &["-p", "err", "-b", "0", "--no-pager", "-n", &n_s],
            ),
        ));
        out.push(sec(
            "journal errors, previous boot",
            cmd(
                sys,
                "journalctl",
                &["-p", "err", "-b", "-1", "--no-pager", "-n", &n_s],
            ),
        ));
    }
    if o.kernel {
        out.push(sec(
            "kernel log (errors and warnings)",
            tail(&cmd(sys, "dmesg", &["--level=err,warn"]), n),
        ));
    }
    if o.pacman {
        out.push(sec(
            "pacman log (tail)",
            tail(&file(sys, "/var/log/pacman.log"), n),
        ));
    }
    if o.update {
        let latest = sys
            .list_dir("/var/log/samurai")
            .into_iter()
            .rfind(|f| f.starts_with("update-") && f.ends_with(".log"));
        let body = match latest {
            Some(f) => format!(
                "{f}\n{}",
                tail(&file(sys, &format!("/var/log/samurai/{f}")), n)
            ),
            None => "(no update log yet)".to_string(),
        };
        out.push(sec("last samurai update log", body));
    }
    out
}

pub fn disks(sys: &dyn System) -> Vec<Section> {
    let mut smart = String::new();
    let scan = cmd(sys, "smartctl", &["--scan"]);
    for dev in scan
        .lines()
        .filter_map(|l| l.split_whitespace().next())
        .filter(|d| d.starts_with('/'))
    {
        smart.push_str(&format!(
            "{dev}: {}\n",
            head(&cmd(sys, "smartctl", &["-H", dev]), 8)
        ));
    }
    if smart.is_empty() {
        smart = "(no SMART devices found or smartctl missing)".to_string();
    }
    vec![
        sec("mounts", cmd(sys, "findmnt", &[])),
        sec(
            "filesystem usage",
            cmd(sys, "df", &["-hT", "-x", "tmpfs", "-x", "devtmpfs"]),
        ),
        sec(
            "btrfs usage (/)",
            cmd(sys, "btrfs", &["filesystem", "usage", "/"]),
        ),
        sec(
            "btrfs device errors (/)",
            cmd(sys, "btrfs", &["device", "stats", "/"]),
        ),
        sec("SMART health", smart),
    ]
}

pub fn gpu(sys: &dyn System) -> Vec<Section> {
    let lspci = cmd(sys, "lspci", &["-nnk"]);
    let display: Vec<&str> = lspci
        .lines()
        .filter(|l| {
            l.contains("VGA") || l.contains("3D controller") || l.contains("Display controller")
        })
        .collect();
    let modules = grep_lines(
        &cmd(sys, "lsmod", &[]),
        &["nvidia", "amdgpu", "i915", "xe ", "nouveau"],
    );
    vec![
        sec(
            "display adapters",
            if display.is_empty() {
                "(none found)".into()
            } else {
                display.join("\n")
            },
        ),
        sec(
            "loaded graphics modules",
            if modules.is_empty() {
                "(none)".into()
            } else {
                modules
            },
        ),
        sec("kernel command line", file(sys, "/proc/cmdline")),
        sec("vulkan", head(&cmd(sys, "vulkaninfo", &["--summary"]), 40)),
        sec("opengl renderer", head(&cmd(sys, "glxinfo", &["-B"]), 20)),
        sec(
            "chwd installed profiles",
            cmd(sys, "chwd", &["--list", "-i"]),
        ),
    ]
}

/// Everything above plus the health report, package lists and config files. Not yet redacted.
pub fn bundle(sys: &dyn System, now_epoch: u64) -> Vec<Section> {
    let mut all = info(sys);
    all.extend(services(sys));
    all.extend(logs(sys, &LogOpts::default()));
    all.extend(logs(
        sys,
        &LogOpts {
            kernel: true,
            pacman: true,
            update: true,
            lines: Some(100),
        },
    ));
    all.extend(disks(sys));
    all.extend(gpu(sys));
    let findings = health::run(sys, ALL, &health::Filter::default());
    all.push(sec(
        "samurai health (json)",
        health::render_json(&findings, now_epoch),
    ));
    all.push(sec(
        "explicitly installed packages",
        cmd(sys, "pacman", &["-Qe"]),
    ));
    all.push(sec("foreign packages", cmd(sys, "pacman", &["-Qm"])));
    all.push(sec("pacman.conf", file(sys, "/etc/pacman.conf")));
    for p in [
        "/boot/limine/limine.conf",
        "/boot/limine.conf",
        "/boot/grub/grub.cfg",
        "/etc/mkinitcpio.conf",
    ] {
        if sys.exists(p) {
            all.push(sec(&format!("boot config {p}"), file(sys, p)));
        }
    }
    all
}

/// Names of login accounts (uid >= 1000) plus any extra names (the invoking user).
pub fn user_names(sys: &dyn System, extra: &[String]) -> Vec<String> {
    let mut names: Vec<String> = sys
        .read("/etc/passwd")
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split(':').collect();
            let uid: u32 = f.get(2)?.parse().ok()?;
            f.first()
                .filter(|_| (1000..65534).contains(&uid))
                .map(|n| (*n).to_string())
        })
        .collect();
    for e in extra {
        if e != "root" && !names.contains(e) {
            names.push(e.clone());
        }
    }
    names
}

/// Home directory of `user` from /etc/passwd.
pub fn home_of(sys: &dyn System, user: &str) -> Option<String> {
    sys.read("/etc/passwd")?.lines().find_map(|l| {
        let f: Vec<&str> = l.split(':').collect();
        (f.first() == Some(&user))
            .then(|| f.get(5).map(|h| (*h).to_string()))
            .flatten()
    })
}

/// True when `candidate`'s directory really lives inside `base` (symlinks resolved, `..` gone).
/// Root writing a bundle for a `sudo` user must stay inside that user's home.
pub fn confined(base: &std::path::Path, candidate: &std::path::Path) -> bool {
    let (Ok(base), Some(parent)) = (std::fs::canonicalize(base), candidate.parent()) else {
        return false;
    };
    let Ok(parent) = std::fs::canonicalize(parent) else {
        return false;
    };
    candidate.file_name().is_some() && parent.starts_with(&base)
}

/// `--out` value that prints the bundle instead of creating a file. A privileged caller (the
/// GUI helper) uses it so that root never writes to a path chosen by a user.
pub const STDOUT: &str = "-";

pub fn redactor(sys: &dyn System, extra_users: &[String]) -> Redactor {
    Redactor::new(user_names(sys, extra_users), sys.read("/etc/hostname"))
}

#[derive(Debug, PartialEq, Eq)]
pub enum Cmd {
    Info,
    Services,
    Logs(LogOpts),
    Disks,
    Gpu,
    /// `out` of `-` means standard output.
    Bundle {
        redact: bool,
        out: Option<String>,
    },
    Help,
}

pub fn parse_args(args: &[String]) -> Result<Cmd, String> {
    let Some((sub, rest)) = args.split_first() else {
        return Err("a command is required".into());
    };
    let no_opts = |c: Cmd| match rest.first() {
        None => Ok(c),
        Some(o) => Err(format!("unknown option: {o}")),
    };
    match sub.as_str() {
        "-h" | "--help" | "help" => Ok(Cmd::Help),
        "info" => no_opts(Cmd::Info),
        "services" => no_opts(Cmd::Services),
        "disks" => no_opts(Cmd::Disks),
        "gpu" => no_opts(Cmd::Gpu),
        "logs" => {
            let mut o = LogOpts::default();
            let mut it = rest.iter();
            while let Some(a) = it.next() {
                match a.as_str() {
                    "--kernel" => o.kernel = true,
                    "--pacman" => o.pacman = true,
                    "--update" => o.update = true,
                    "--lines" => {
                        let v = it.next().ok_or("--lines needs a number")?;
                        o.lines = Some(
                            v.parse()
                                .ok()
                                .filter(|n| (1..=100_000).contains(n))
                                .ok_or(format!("bad --lines: {v}"))?,
                        );
                    }
                    other => return Err(format!("unknown option: {other}")),
                }
            }
            Ok(Cmd::Logs(o))
        }
        "bundle" => {
            let (mut redact, mut out) = (true, None);
            let mut it = rest.iter();
            while let Some(a) = it.next() {
                match a.as_str() {
                    "--no-redact" => redact = false,
                    "--out" => out = Some(it.next().ok_or("--out needs a file name")?.clone()),
                    other => return Err(format!("unknown option: {other}")),
                }
            }
            // an unredacted bundle only ever goes into a private file
            if !redact && out.as_deref() == Some(STDOUT) {
                return Err("--no-redact cannot write to standard output".into());
            }
            Ok(Cmd::Bundle { redact, out })
        }
        other => Err(format!("unknown command: {other}")),
    }
}

/// (uid, gid) of `user` from /etc/passwd, so a bundle written by root via sudo is handed to the user.
pub fn ids_of(sys: &dyn System, user: &str) -> Option<(u32, u32)> {
    sys.read("/etc/passwd")?.lines().find_map(|l| {
        let f: Vec<&str> = l.split(':').collect();
        (f.first() == Some(&user))
            .then(|| Some((f.get(2)?.parse().ok()?, f.get(3)?.parse().ok()?)))
            .flatten()
    })
}

pub const USAGE: &str = "usage: samurai diag <command> [options]

Read-only diagnostics.

  info                 CPU, memory, GPU + driver, disks, kernel, boot, firmware mode
  services             boot time, slowest units, critical chain, failed units
  logs [--kernel] [--pacman] [--update] [--lines N]
                       journal errors (this + previous boot); or kernel, pacman or
                       the last `samurai update` log
  disks                mounts, usage, btrfs usage + device errors, SMART health
  gpu                  adapters, loaded modules, kernel command line, Vulkan/GL
  bundle [--no-redact] [--out FILE|-]
                       all of the above + `samurai health --json`, package lists, pacman.conf
                       and boot config in one text file (default ~/samurai-diag-<time>.txt,
                       mode 0600). Redacted by default: user names, hostname, MACs, IPs,
                       e-mail addresses, SSIDs, serial numbers, tokens and keys become
                       stable placeholders. --no-redact asks twice and needs a terminal.

Exit codes: 0 ok, 1 failure, 2 usage error.
";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::health::system::fake::Fake;

    #[test]
    fn missing_and_failing_commands_are_visible_not_fatal() {
        let sys = Fake::default()
            .cmd("uname -srm", 0, "Linux 6.12 x86_64\n")
            .cmd("systemd-detect-virt", 1, "none\n");
        let s = info(&sys);
        assert_eq!(s[0].body.trim(), "Linux 6.12 x86_64");
        let virt = &s.iter().find(|x| x.title == "virtualization").unwrap().body;
        assert_eq!(virt.trim(), "(exit 1) none");
        let cpu = &s.iter().find(|x| x.title == "cpu").unwrap().body;
        assert!(
            cpu.is_empty(),
            "lscpu is missing: the filtered section is empty, not an error: {cpu:?}"
        );
        let pci = &s
            .iter()
            .find(|x| x.title == "pci devices and drivers")
            .unwrap()
            .body;
        assert_eq!(pci, "(not available: lspci)");
    }

    #[test]
    fn info_keeps_only_interesting_cpu_and_memory_lines() {
        let sys = Fake::default()
            .cmd("lscpu", 0, "Architecture: x86_64\nFlags: a b c\nModel name: Test CPU\nCPU(s): 8\nStepping: 1\n")
            .file("/proc/meminfo", "MemTotal: 100 kB\nMemFree: 5 kB\nSwapTotal: 0 kB\n");
        let s = info(&sys);
        let cpu = &s.iter().find(|x| x.title == "cpu").unwrap().body;
        assert_eq!(cpu, "Architecture: x86_64\nModel name: Test CPU\nCPU(s): 8");
        let mem = &s.iter().find(|x| x.title == "memory").unwrap().body;
        assert_eq!(mem, "MemTotal: 100 kB\nSwapTotal: 0 kB");
    }

    #[test]
    fn boot_section_reports_firmware_and_loader() {
        let sys = Fake::default()
            .path("/sys/firmware/efi")
            .path("/boot/limine/limine.conf");
        let b = &info(&sys)
            .into_iter()
            .find(|x| x.title == "boot")
            .unwrap()
            .body;
        assert!(
            b.contains("firmware: UEFI") && b.contains("/boot/limine/limine.conf"),
            "{b}"
        );
    }

    #[test]
    fn logs_default_vs_selected_sources() {
        let sys = Fake::default()
            .cmd("journalctl -p err -b 0 --no-pager -n 200", 0, "err now\n")
            .cmd(
                "journalctl -p err -b -1 --no-pager -n 200",
                0,
                "err before\n",
            );
        let s = logs(&sys, &LogOpts::default());
        assert_eq!(s.len(), 2);
        assert!(s[0].body.contains("err now") && s[1].body.contains("err before"));

        let sys = Fake::default()
            .file("/var/log/pacman.log", "l1\nl2\nl3\n")
            .dir(
                "/var/log/samurai",
                &[
                    "update-20261101-000000.log",
                    "update-20261120-101010.log",
                    "other.txt",
                ],
            )
            .file("/var/log/samurai/update-20261120-101010.log", "newest\n")
            .file("/var/log/samurai/update-20261101-000000.log", "older\n");
        let s = logs(
            &sys,
            &LogOpts {
                pacman: true,
                update: true,
                lines: Some(2),
                ..LogOpts::default()
            },
        );
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].body, "l2\nl3");
        assert!(
            s[1].body.starts_with("update-20261120-101010.log") && s[1].body.contains("newest")
        );
        assert!(!s[1].body.contains("older"));
        let none = logs(
            &Fake::default(),
            &LogOpts {
                update: true,
                ..LogOpts::default()
            },
        );
        assert_eq!(none[0].body, "(no update log yet)");
    }

    #[test]
    fn disks_loop_over_smart_devices() {
        let sys = Fake::default()
            .cmd(
                "smartctl --scan",
                0,
                "/dev/sda -d sat # /dev/sda, SAT\n/dev/nvme0 -d nvme # /dev/nvme0\n",
            )
            .cmd("smartctl -H /dev/sda", 0, "SMART overall-health: PASSED\n")
            .cmd(
                "smartctl -H /dev/nvme0",
                0,
                "SMART overall-health: PASSED\n",
            );
        let smart = &disks(&sys)
            .into_iter()
            .find(|x| x.title == "SMART health")
            .unwrap()
            .body;
        assert!(
            smart.contains("/dev/sda: SMART") && smart.contains("/dev/nvme0: SMART"),
            "{smart}"
        );
        let none = &disks(&Fake::default())
            .into_iter()
            .find(|x| x.title == "SMART health")
            .unwrap()
            .body;
        assert!(none.contains("no SMART devices"), "{none}");
    }

    #[test]
    fn gpu_filters_display_adapters_and_modules() {
        let sys = Fake::default()
            .cmd("lspci -nnk", 0, "00:02.0 VGA compatible controller [0300]: Intel\n\tKernel driver in use: i915\n00:1f.3 Audio device: x\n")
            .cmd("lsmod", 0, "i915 123 1\nsnd 5 0\nnvidia 9 2\n");
        let s = gpu(&sys);
        assert!(s[0].body.contains("VGA compatible") && !s[0].body.contains("Audio"));
        assert_eq!(s[1].body, "i915 123 1\nnvidia 9 2");
    }

    #[test]
    fn bundle_contains_every_part_including_health_json() {
        let sys = Fake::default()
            .path("/boot/limine/limine.conf")
            .file("/boot/limine/limine.conf", "timeout: 3\n");
        let text = render(&bundle(&sys, 7));
        for needle in [
            "== kernel ==",
            "== failed units ==",
            "== journal errors, this boot ==",
            "== pacman log (tail) ==",
            "== SMART health ==",
            "== display adapters ==",
            "\"schema\":1,\"checked_epoch\":7",
            "== explicitly installed packages ==",
            "== pacman.conf ==",
            "== boot config /boot/limine/limine.conf ==",
        ] {
            assert!(text.contains(needle), "missing {needle}");
        }
    }

    #[test]
    fn users_come_from_passwd_and_extra_but_never_root_or_system() {
        let sys = Fake::default().file(
            "/etc/passwd",
            "root:x:0:0::/root:/bin/bash\ndaemon:x:2:2::/:/usr/bin/nologin\nalice:x:1000:1000::/home/alice:/bin/bash\nnobody:x:65534:65534::/:/usr/bin/nologin\n",
        );
        assert_eq!(
            user_names(&sys, &["bob".into(), "root".into(), "alice".into()]),
            vec!["alice", "bob"]
        );
        assert_eq!(home_of(&sys, "alice").as_deref(), Some("/home/alice"));
        assert_eq!(home_of(&sys, "zed"), None);
    }

    fn a(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| (*x).to_string()).collect()
    }

    #[test]
    fn argument_parsing() {
        assert_eq!(parse_args(&a(&["info"])), Ok(Cmd::Info));
        assert_eq!(parse_args(&a(&["--help"])), Ok(Cmd::Help));
        assert_eq!(
            parse_args(&a(&["logs", "--kernel", "--lines", "50"])),
            Ok(Cmd::Logs(LogOpts {
                kernel: true,
                lines: Some(50),
                ..LogOpts::default()
            }))
        );
        assert_eq!(
            parse_args(&a(&["bundle"])),
            Ok(Cmd::Bundle {
                redact: true,
                out: None
            })
        );
        assert_eq!(
            parse_args(&a(&["bundle", "--no-redact", "--out", "/tmp/x"])),
            Ok(Cmd::Bundle {
                redact: false,
                out: Some("/tmp/x".into())
            })
        );
        for bad in [
            &[][..],
            &["nope"],
            &["info", "--x"],
            &["logs", "--lines"],
            &["logs", "--lines", "0"],
            &["logs", "--lines", "x"],
            &["bundle", "--out"],
            &["bundle", "--bogus"],
        ] {
            assert!(parse_args(&a(bad)).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn confinement_resolves_symlinks_and_dotdot_and_prefix_tricks() {
        use std::fs;
        let root = std::env::temp_dir().join(format!("samurai-confine-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let home = root.join("home");
        let outside = root.join("home2");
        fs::create_dir_all(home.join("sub")).unwrap();
        fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, home.join("link")).unwrap();
        assert!(confined(&home, &home.join("bundle.txt")));
        assert!(confined(&home, &home.join("sub/bundle.txt")));
        assert!(
            !confined(&home, &outside.join("bundle.txt")),
            "sibling directory with a common name prefix"
        );
        assert!(!confined(&home, &home.join("../home2/bundle.txt")), "..");
        assert!(
            !confined(&home, &home.join("link/bundle.txt")),
            "a symlinked directory escaping the home"
        );
        assert!(
            !confined(&home, &home.join("missing/bundle.txt")),
            "parent must exist"
        );
        assert!(!confined(&home, std::path::Path::new("/")), "no file name");
        assert!(
            !confined(&root.join("nope"), &home.join("x")),
            "base must exist"
        );
    }

    #[test]
    fn passwd_ids() {
        let sys = Fake::default().file("/etc/passwd", "alice:x:1000:1001::/home/alice:/bin/bash\n");
        assert_eq!(ids_of(&sys, "alice"), Some((1000, 1001)));
        assert_eq!(ids_of(&sys, "bob"), None);
    }

    #[test]
    fn redacted_bundle_has_no_private_data() {
        let sys = Fake::default()
            .file("/etc/passwd", "alice:x:1000:1000::/home/alice:/bin/bash\n")
            .file("/etc/hostname", "archbox\n")
            .cmd("ip -o addr", 0, "")
            .cmd("uname -srm", 0, "Linux archbox 6.12 x86_64\n")
            .cmd(
                "journalctl -p err -b 0 --no-pager -n 200",
                0,
                "alice at 10.0.2.15 mac 08:00:27:4e:66:a1 mail a@b.org\n",
            )
            .file("/etc/os-release", "NAME=\"Samurai OS\"\n");
        let text = render(&bundle(&sys, 1));
        let red = redactor(&sys, &[]).redact(&text);
        for leaked in ["archbox", "alice", "10.0.2.15", "08:00:27", "a@b.org"] {
            assert!(!red.contains(leaked), "{leaked} leaked");
        }
        assert!(red.contains("Samurai OS"), "non-private content survives");
    }
}

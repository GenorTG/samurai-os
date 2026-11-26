//! `samurai kernel` (spec 04): list, install, remove and choose the default kernel.
//!
//! pacman does the work (ADR-0002); the boot menu follows through `boot sync`. The rules that
//! keep a machine bootable live here: never remove the running kernel or the last bootable one,
//! and never silently add a kernel that would have no NVIDIA module.

use crate::boot;
use crate::health::json_escape;
use crate::health::system::System;

/// Kernels this tool manages: package name, one-line description.
pub const KNOWN: [(&str, &str); 6] = [
    ("linux", "Arch stable kernel"),
    ("linux-lts", "Arch long-term support kernel"),
    ("linux-zen", "desktop-tuned kernel (Arch)"),
    ("linux-hardened", "security-hardened kernel (Arch)"),
    (
        "linux-cachyos",
        "CachyOS performance kernel (Samurai default)",
    ),
    ("linux-cachyos-lts", "CachyOS long-term support kernel"),
];

/// NVIDIA module packages built for specific kernels; a new kernel gets nothing from them.
const PREBUILT_NVIDIA: [&str; 7] = [
    "nvidia",
    "nvidia-open",
    "nvidia-lts",
    "nvidia-open-lts",
    "linux-cachyos-nvidia-open",
    "linux-cachyos-lts-nvidia-open",
    "linux-zen-nvidia-open",
];

fn is_known(name: &str) -> bool {
    KNOWN.iter().any(|(k, _)| *k == name)
}

fn installed(sys: &dyn System) -> Result<Vec<String>, String> {
    match sys.run("pacman", &["-Qq"]) {
        Some(o) if o.code == 0 => Ok(o.stdout.lines().map(str::to_string).collect()),
        _ => Err("cannot query pacman".into()),
    }
}

/// Package base of the running kernel; `None` after its modules were replaced by an update.
pub fn running(sys: &dyn System) -> Option<String> {
    let release = sys.read("/proc/sys/kernel/osrelease")?;
    let base = sys.read(&format!("/usr/lib/modules/{}/pkgbase", release.trim()))?;
    Some(base.trim().to_string())
}

/// `list` for machines. For each kernel: what `install` would warn about, and why `remove` or
/// `default` would be refused, so a client can explain before anything runs. `bootable` is null
/// when `/boot` cannot be read (not root).
pub fn list_json(sys: &dyn System) -> Result<String, String> {
    let have = installed(sys)?;
    let run = running(sys);
    let default = boot::default_kernel(sys);
    let readable = boot::boot_readable(sys);
    let bootable: Vec<String> = boot::bootable_kernels(sys)
        .into_iter()
        .map(|k| k.name)
        .collect();
    let text =
        |v: Option<&str>| v.map_or("null".to_string(), |v| format!("\"{}\"", json_escape(v)));
    let mut rows = Vec::new();
    for (name, what) in KNOWN {
        let is_installed = have.iter().any(|h| h == name);
        let boots = readable.then(|| bootable.iter().any(|b| b == name));
        let warnings: Vec<String> = if is_installed {
            Vec::new()
        } else {
            plan_install(sys, name)?
                .warnings
                .iter()
                .map(|w| format!("\"{}\"", json_escape(w)))
                .collect()
        };
        let blocker = if is_installed {
            plan_remove(sys, name).err()
        } else {
            Some(format!("{name} is not installed"))
        };
        rows.push(format!(
            "{{\"name\":\"{name}\",\"description\":\"{}\",\"installed\":{is_installed},\"running\":{},\"default\":{},\"bootable\":{},\"install_warnings\":[{}],\"remove_blocker\":{}}}",
            json_escape(what),
            run.as_deref() == Some(name),
            default.as_deref() == Some(name),
            boots.map_or("null".to_string(), |b| b.to_string()),
            warnings.join(","),
            text(blocker.as_deref())
        ));
    }
    Ok(format!(
        "{{\"schema\":1,\"boot_readable\":{readable},\"running\":{},\"default\":{},\"kernels\":[{}]}}\n",
        text(run.as_deref()),
        text(default.as_deref()),
        rows.join(",")
    ))
}

pub fn list(sys: &dyn System) -> Result<String, String> {
    let have = installed(sys)?;
    let run = running(sys);
    let default = boot::default_kernel(sys);
    let readable = boot::boot_readable(sys);
    let bootable: Vec<String> = boot::bootable_kernels(sys)
        .into_iter()
        .map(|k| k.name)
        .collect();
    let mut out = String::new();
    for (name, what) in KNOWN {
        let mut flags = Vec::new();
        if have.iter().any(|h| h == name) {
            flags.push("installed");
            if readable && !bootable.iter().any(|b| b == name) {
                flags.push("NOT bootable");
            }
        }
        if run.as_deref() == Some(name) {
            flags.push("running");
        }
        if default.as_deref() == Some(name) {
            flags.push("default");
        }
        let line = format!("{name:<18} {what:<46} {}", flags.join(", "));
        out.push_str(line.trim_end());
        out.push('\n');
    }
    if !readable {
        out.push_str("(run as root to see boot state and the default entry)\n");
    }
    Ok(out)
}

#[derive(Debug, PartialEq, Eq)]
pub struct InstallPlan {
    pub packages: Vec<String>,
    pub warnings: Vec<String>,
}

pub fn plan_install(sys: &dyn System, name: &str) -> Result<InstallPlan, String> {
    if !is_known(name) {
        return Err(format!("unknown kernel: {name} (see: samurai kernel list)"));
    }
    let have = installed(sys)?;
    let has = |p: &str| have.iter().any(|h| h == p);
    let mut packages = vec![name.to_string()];
    let mut warnings = Vec::new();
    // headers are only needed to build DKMS modules for the new kernel
    if has("dkms") {
        packages.push(format!("{name}-headers"));
        if name.starts_with("linux-cachyos") {
            // CachyOS headers need CachyOS's newer pahole; plain `pahole` resolves to Arch's
            packages.push("cachyos/pahole".into());
        }
    }
    let prebuilt: Vec<&str> = PREBUILT_NVIDIA.into_iter().filter(|p| has(p)).collect();
    if !prebuilt.is_empty() {
        warnings.push(format!(
            "NVIDIA modules here are prebuilt ({}); {name} would boot without an NVIDIA driver. \
             Switch to nvidia-open-dkms to cover every kernel.",
            prebuilt.join(", ")
        ));
    }
    Ok(InstallPlan { packages, warnings })
}

pub fn install(sys: &dyn System, plan: &InstallPlan) -> Result<(), String> {
    let mut args = vec!["-S", "--needed", "--noconfirm", "--"];
    args.extend(plan.packages.iter().map(String::as_str));
    match sys.run_visible("pacman", &args) {
        Some(0) => Ok(()),
        Some(c) => Err(format!(
            "pacman exited {c}; if packages were not found, run `samurai update` first"
        )),
        None => Err("pacman is not available".into()),
    }
}

pub fn plan_remove(sys: &dyn System, name: &str) -> Result<Vec<String>, String> {
    if !is_known(name) {
        return Err(format!("unknown kernel: {name} (see: samurai kernel list)"));
    }
    let have = installed(sys)?;
    if !have.iter().any(|h| h == name) {
        return Err(format!("{name} is not installed"));
    }
    match running(sys) {
        Some(r) if r == name => {
            return Err(format!(
                "{name} is the running kernel; boot another one first"
            ))
        }
        Some(_) => {}
        None => {
            return Err(
                "cannot tell which kernel is running (updated since boot?); reboot first".into(),
            )
        }
    }
    if !boot::boot_readable(sys) {
        return Err("/boot cannot be read (is the ESP mounted?)".into());
    }
    if !boot::bootable_kernels(sys).iter().any(|k| k.name != name) {
        return Err(format!(
            "no other bootable kernel would remain; not removing {name}"
        ));
    }
    let headers = format!("{name}-headers");
    let mut packages = vec![name.to_string()];
    if have.contains(&headers) {
        packages.push(headers);
    }
    Ok(packages)
}

pub fn remove(sys: &dyn System, packages: &[String]) -> Result<(), String> {
    let mut args = vec!["-R", "--noconfirm", "--"];
    args.extend(packages.iter().map(String::as_str));
    match sys.run_visible("pacman", &args) {
        Some(0) => Ok(()),
        Some(c) => Err(format!(
            "pacman exited {c} (another package may depend on it)"
        )),
        None => Err("pacman is not available".into()),
    }
}

pub const USAGE: &str = "usage: samurai kernel <list|install|remove|default> [kernel] [--yes]

  list [--json]       what is available, installed, running and booted by default
  install <kernel>    install it (with headers when DKMS is used) and add boot entries
  remove <kernel>     remove it and its entries; never the running or last bootable kernel
  default <kernel>    boot this kernel by default (Limine, systemd-boot)

  --yes               approve warnings (for example a kernel without an NVIDIA module)

All but `list` need root. Exit codes: 0 ok, 1 failed or refused, 2 usage error.
";

#[derive(Debug, PartialEq, Eq)]
pub enum Cmd {
    List { json: bool },
    Install { name: String, yes: bool },
    Remove { name: String },
    Default { name: String },
    Help,
}

pub fn parse_args(args: &[String]) -> Result<Cmd, String> {
    let yes = args.iter().any(|a| a == "--yes" || a == "-y");
    let words: Vec<&str> = args
        .iter()
        .map(String::as_str)
        .filter(|a| *a != "--yes" && *a != "-y")
        .collect();
    let cmd = match words.as_slice() {
        ["-h" | "--help"] => Cmd::Help,
        ["list"] => Cmd::List { json: false },
        ["list", "--json"] => Cmd::List { json: true },
        ["install", n] if !n.starts_with('-') => Cmd::Install {
            name: (*n).into(),
            yes,
        },
        ["remove", n] if !n.starts_with('-') => Cmd::Remove { name: (*n).into() },
        ["default", n] if !n.starts_with('-') => Cmd::Default { name: (*n).into() },
        ["install" | "remove" | "default"] => return Err("a kernel name is required".into()),
        [] => return Err("a command is required: list, install, remove or default".into()),
        _ => return Err(format!("unexpected arguments: {}", words.join(" "))),
    };
    if yes && !matches!(cmd, Cmd::Install { .. }) {
        return Err("--yes only applies to `install`".into());
    }
    Ok(cmd)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::health::system::fake::Fake;

    fn machine(pkgs: &str, kernels: &[&str], running: &str) -> Fake {
        let mut f = Fake::default()
            .cmd("pacman -Qq", 0, pkgs)
            .dir("/boot", &["limine"])
            .dir("/usr/lib/modules", kernels)
            .file("/proc/sys/kernel/osrelease", &format!("{running}\n"));
        for k in kernels {
            f = f
                .file(&format!("/usr/lib/modules/{k}/pkgbase"), &format!("{k}\n"))
                .path(&format!("/boot/vmlinuz-{k}"))
                .path(&format!("/boot/initramfs-{k}.img"));
        }
        f
    }

    #[test]
    fn list_marks_installed_running_and_unbootable() {
        let f = machine(
            "linux\nlinux-lts\nlinux-zen\n",
            &["linux", "linux-lts"],
            "linux",
        );
        let out = list(&f).unwrap();
        let line = |n: &str| {
            out.lines()
                .find(|l| l.split_whitespace().next() == Some(n))
                .unwrap()
                .to_string()
        };
        assert!(line("linux").ends_with("installed, running"), "{out}");
        assert!(line("linux-lts").ends_with("installed"));
        assert!(line("linux-zen").ends_with("installed, NOT bootable"));
        assert!(!line("linux-hardened").contains("installed"));
        assert!(list(&Fake::default()).is_err());
    }

    #[test]
    fn list_json_carries_warnings_and_blockers() {
        let f = machine(
            "linux\nlinux-lts\nnvidia-open\n",
            &["linux", "linux-lts"],
            "linux",
        );
        let out = list_json(&f).unwrap();
        assert!(
            out.starts_with("{\"schema\":1,\"boot_readable\":true,\"running\":\"linux\","),
            "{out}"
        );
        let row = |n: &str| {
            let at = out.find(&format!("{{\"name\":\"{n}\",")).unwrap();
            let end = out[at..].find('}').unwrap();
            out[at..at + end].to_string()
        };
        assert!(row("linux").contains("\"running\":true"), "{out}");
        assert!(row("linux").contains("is the running kernel"), "{out}");
        assert!(
            row("linux-lts").ends_with("\"remove_blocker\":null"),
            "{out}"
        );
        assert!(row("linux-zen").contains("\"installed\":false"), "{out}");
        assert!(row("linux-zen").contains("nvidia-open-dkms"), "{out}");
        // without a readable /boot nothing is claimed about bootability
        let blind = Fake::default().cmd("pacman -Qq", 0, "linux\n");
        let out = list_json(&blind).unwrap();
        assert!(out.contains("\"boot_readable\":false"), "{out}");
        assert!(out.contains("\"bootable\":null"), "{out}");
        assert!(!out.contains("\"bootable\":false"), "{out}");
    }

    #[test]
    fn install_adds_headers_only_with_dkms_and_cachyos_pahole() {
        let plain = machine("linux\n", &["linux"], "linux");
        assert_eq!(
            plan_install(&plain, "linux-lts").unwrap(),
            InstallPlan {
                packages: vec!["linux-lts".into()],
                warnings: vec![]
            }
        );
        let dkms = machine("linux\ndkms\n", &["linux"], "linux");
        assert_eq!(
            plan_install(&dkms, "linux-zen").unwrap().packages,
            vec!["linux-zen", "linux-zen-headers"]
        );
        assert_eq!(
            plan_install(&dkms, "linux-cachyos").unwrap().packages,
            vec!["linux-cachyos", "linux-cachyos-headers", "cachyos/pahole"]
        );
        for bad in ["bash", "linux; rm", "--overwrite", "linux-lts-headers"] {
            assert!(plan_install(&plain, bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn prebuilt_nvidia_is_a_warning_dkms_is_not() {
        let prebuilt = machine("linux\nnvidia-open\n", &["linux"], "linux");
        assert!(
            plan_install(&prebuilt, "linux-zen").unwrap().warnings[0].contains("nvidia-open-dkms")
        );
        let dkms = machine("linux\nnvidia-open-dkms\ndkms\n", &["linux"], "linux");
        assert!(plan_install(&dkms, "linux-zen")
            .unwrap()
            .warnings
            .is_empty());
    }

    #[test]
    fn install_and_remove_run_pacman_with_names_after_a_separator() {
        let f = Fake::default()
            .cmd("pacman -S --needed --noconfirm -- linux-lts", 0, "")
            .cmd("pacman -R --noconfirm -- linux-lts", 1, "");
        assert!(install(
            &f,
            &InstallPlan {
                packages: vec!["linux-lts".into()],
                warnings: vec![]
            }
        )
        .is_ok());
        assert!(remove(&f, &["linux-lts".into()])
            .unwrap_err()
            .contains("exited 1"));
        assert!(
            !f.calls().iter().any(|c| c.contains("-Sy")),
            "never a partial sync"
        );
    }

    #[test]
    fn removal_protects_the_running_and_the_last_bootable_kernel() {
        let two = machine(
            "linux\nlinux-lts\nlinux-lts-headers\n",
            &["linux", "linux-lts"],
            "linux",
        );
        assert_eq!(
            plan_remove(&two, "linux-lts").unwrap(),
            vec!["linux-lts", "linux-lts-headers"]
        );
        assert!(plan_remove(&two, "linux").unwrap_err().contains("running"));
        assert!(plan_remove(&two, "linux-zen")
            .unwrap_err()
            .contains("not installed"));
        assert!(plan_remove(&two, "bash").unwrap_err().contains("unknown"));
        // the other installed kernel has no image: removing the only bootable one is refused
        let broken = machine("linux\nlinux-lts\n", &["linux-lts"], "linux-zen")
            .file("/usr/lib/modules/linux-zen/pkgbase", "linux-zen\n");
        assert!(plan_remove(&broken, "linux-lts")
            .unwrap_err()
            .contains("no other bootable"));
        // running kernel unknown after an update: refuse rather than guess
        let updated = machine("linux\nlinux-lts\n", &["linux", "linux-lts"], "6.0.0-gone");
        assert!(plan_remove(&updated, "linux-lts")
            .unwrap_err()
            .contains("reboot first"));
    }

    #[test]
    fn argument_parsing() {
        let a = |v: &[&str]| v.iter().map(|x| (*x).to_string()).collect::<Vec<_>>();
        assert_eq!(parse_args(&a(&["list"])), Ok(Cmd::List { json: false }));
        assert_eq!(
            parse_args(&a(&["list", "--json"])),
            Ok(Cmd::List { json: true })
        );
        assert_eq!(
            parse_args(&a(&["install", "linux-lts", "--yes"])),
            Ok(Cmd::Install {
                name: "linux-lts".into(),
                yes: true
            })
        );
        assert_eq!(
            parse_args(&a(&["remove", "linux"])),
            Ok(Cmd::Remove {
                name: "linux".into()
            })
        );
        assert_eq!(
            parse_args(&a(&["default", "linux"])),
            Ok(Cmd::Default {
                name: "linux".into()
            })
        );
        assert_eq!(parse_args(&a(&["--help"])), Ok(Cmd::Help));
        for bad in [
            &[][..],
            &["install"],
            &["nope"],
            &["remove", "--force"],
            &["list", "x"],
            &["remove", "linux", "--yes"],
            &["install", "a", "b"],
        ] {
            assert!(parse_args(&a(bad)).is_err(), "{bad:?}");
        }
    }
}

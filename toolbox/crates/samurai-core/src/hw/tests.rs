//! `lspci` and `chwd --list` texts below are real outputs of those tools on fake-sysfs machines
//! (ci/test-hw.sh). Package state, snapper and pacman answers are scripted.

use super::apply::{self, SnapshotPolicy};
use super::devices::{parse_lspci, Generation};
use super::fingerprint::{self, State};
use super::plan::{self, module_branch, Branch, Plan, Request, Status};
use super::profiles;
use super::*;
use crate::health::system::fake::Fake;

const GPU_3090: &str = r#"0000:01:00.0 "VGA compatible controller [0300]" "NVIDIA Corporation [10de]" "GA102 [GeForce RTX 3090] [2204]" -p00 "" """#;
const GPU_1070: &str = r#"0000:02:00.0 "VGA compatible controller [0300]" "NVIDIA Corporation [10de]" "GP104 [GeForce GTX 1070] [1b81]" -p00 "" """#;
const GPU_5080: &str = r#"0000:01:00.0 "VGA compatible controller [0300]" "NVIDIA Corporation [10de]" "GB203 [GeForce RTX 5080] [2c02]" -p00 "" """#;

const DB: &str = r#"
[nvidia-open-dkms]
desc = 'open'
priority = 10
driver_conflict_group = "nvidia"
packages = 'nvidia-utils libva-nvidia-driver'
conditional_packages = """
echo open-modules
"""
post_install = """
echo [not-a-table]
key = value
"""

[nvidia-open-dkms.prime]
priority = 11
packages = 'nvidia-utils libva-nvidia-driver nvidia-prime'

[nvidia-dkms-580xx]
desc = 'legacy'
priority = 12
driver_conflict_group = "nvidia"
packages = 'nvidia-580xx-utils libva-nvidia-driver'
conditional_packages = """
echo legacy-modules
"""

[fallback]
desc = 'fallback'
priority = 3
packages = 'mesa vulkan-swrast'
"#;

const DB_PATH: &str = "/var/lib/chwd/db/pci/graphic_drivers/profiles.toml";
const LOCAL_PATH: &str = "/var/lib/chwd/local/pci/graphic_drivers/profiles.toml";
const RELEASE: &str = "6.99.1-linux-cachyos";

fn chwd_table(addr: &str, id: &str, rows: &[(&str, u32)]) -> String {
    let mut out = format!(
        "> {addr} (0300:10de:{id}) VGA compatible controller NVIDIA Corporation:\n\n╭──────┬──────────╮\n│ Name ┆ Priority │\n╞══════╪══════════╡\n"
    );
    for (name, priority) in rows {
        out.push_str(&format!("│ {name} ┆ {priority} │\n├╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┤\n"));
    }
    out.push_str("╰──────┴──────────╯\n\n");
    out
}

const OFFER_NEW: [(&str, u32); 2] = [("nvidia-open-dkms", 10), ("fallback", 3)];
const OFFER_OLD: [(&str, u32); 3] = [
    ("nvidia-dkms-580xx", 12),
    ("nvidia-open-dkms", 10),
    ("fallback", 3),
];

/// A machine with these cards, `installed` packages (`pacman -Qq`) and chwd offers.
fn machine(cards: &[&str], offers: &str, installed: &str) -> Fake {
    Fake::default()
        .cmd("lspci -Dnnmm", 0, &cards.join("\n"))
        .file("/sys/bus/pci/devices/0000:01:00.0/boot_vga", "1\n")
        .file(
            "/sys/bus/pci/devices/0000:01:00.0/uevent",
            "DRIVER=nouveau\nPCI_ID=10DE:2204\n",
        )
        .cmd("chwd --list", 0, offers)
        .dir("/var/lib/chwd/db/pci", &["graphic_drivers"])
        .file(DB_PATH, DB)
        .cmd("pacman -Qq", 0, installed)
        .cmd("sh -c echo open-modules\n", 0, "nvidia-open-dkms dkms")
        .cmd("sh -c echo legacy-modules\n", 0, "nvidia-580xx-dkms dkms")
        .dir("/usr/lib/modules", &[RELEASE])
        .path(&format!("/usr/lib/modules/{RELEASE}/pkgbase"))
}

fn single(id: &str, rows: &[(&str, u32)]) -> String {
    chwd_table("0000:01:00.0", id, rows)
}

fn modinfo(sys: Fake, code: i32) -> Fake {
    sys.cmd(&format!("modinfo -k {RELEASE} -F version nvidia"), code, "")
}

const OPEN_SET: &str = "nvidia-open-dkms dkms nvidia-utils libva-nvidia-driver";
const OPEN_T: &str = "pacman -T nvidia-utils libva-nvidia-driver nvidia-open-dkms dkms";
const LEGACY_T: &str = "pacman -T nvidia-580xx-utils libva-nvidia-driver nvidia-580xx-dkms dkms";

/// Fresh machine with an RTX 3090 and nothing installed.
fn fresh() -> Fake {
    machine(&[GPU_3090], &single("2204", &OFFER_NEW), "base").cmd(
        OPEN_T,
        127,
        "nvidia-utils\nlibva-nvidia-driver\nnvidia-open-dkms\ndkms\n",
    )
}

/// RTX 3090 machine that still runs the 580xx branch (card swapped from a GTX 10xx).
fn old_branch() -> Fake {
    machine(
        &[GPU_3090],
        &single("2204", &OFFER_NEW),
        "base nvidia-580xx-dkms nvidia-580xx-utils dkms",
    )
    .cmd(
        OPEN_T,
        127,
        "nvidia-utils\nlibva-nvidia-driver\nnvidia-open-dkms\n",
    )
}

fn built(sys: &Fake, request: &Request) -> Plan {
    plan::build(sys, request).expect("plan")
}

fn words(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| (*s).to_string()).collect()
}

fn mutating(sys: &Fake) -> Vec<String> {
    sys.calls()
        .into_iter()
        .filter(|c| {
            ["pacman -S", "pacman -R", "chwd -i", "mkinitcpio", "snapper"]
                .iter()
                .any(|p| c.starts_with(p))
        })
        .collect()
}

#[test]
fn lspci_lines_become_devices() {
    let d = parse_lspci(&format!(
        "{GPU_3090}\n{}",
        r#"0000:00:1f.6 "Ethernet controller [0200]" "Intel Corporation [8086]" "Ethernet \"I219-V\" [15b8]" -p00 "" """#
    ));
    assert_eq!(d.len(), 2);
    assert_eq!(
        (
            d[0].class.as_str(),
            d[0].vendor.as_str(),
            d[0].device.as_str()
        ),
        ("0300", "10de", "2204")
    );
    assert_eq!(d[0].name, "GA102 [GeForce RTX 3090]");
    assert!(d[0].is_nvidia_display() && !d[1].is_display());
    assert_eq!(d[1].name, "Ethernet \"I219-V\"");
    assert!(parse_lspci("garbage\n").is_empty());
}

#[test]
fn nvidia_generations_and_what_they_support() {
    for (name, generation, open, cuda) in [
        ("G94 [GeForce 9600 GT]", Generation::Tesla, false, false),
        ("GT218 [GeForce 210]", Generation::Tesla, false, false),
        ("GF108 [GeForce GT 430]", Generation::Fermi, false, false),
        ("GK104 [GeForce GTX 680]", Generation::Kepler, false, false),
        ("GM107 [GeForce GTX 750]", Generation::Maxwell, false, false),
        ("GP104 [GeForce GTX 1070]", Generation::Pascal, false, false),
        ("GV100 [TITAN V]", Generation::Volta, false, false),
        ("TU117M [GeForce GTX 1650]", Generation::Turing, true, true),
        ("GA102 [GeForce RTX 3090]", Generation::Ampere, true, true),
        ("AD104 [GeForce RTX 4070]", Generation::Ada, true, true),
        ("GH100 [H100]", Generation::Hopper, true, true),
        (
            "GB203 [GeForce RTX 5080]",
            Generation::Blackwell,
            true,
            true,
        ),
        ("Device 3f00", Generation::Unknown, false, false),
        ("XY100 [Future]", Generation::Unknown, false, false),
    ] {
        let g = Generation::of(name);
        assert_eq!(g, generation, "{name}");
        assert_eq!((g.open_modules(), g.cuda()), (open, cuda), "{name}");
    }
    assert!(!Generation::Blackwell.legacy_580());
}

#[test]
fn real_chwd_list_output_parses() {
    let text = "\u{1b}[31m>\u{1b}[0m 0000:02:00.0 (0300:10de:1b81) VGA compatible controller NVIDIA Corporation:

╭───────────────────┬──────────╮
│ Name              ┆ Priority │
╞═══════════════════╪══════════╡
│ nvidia-dkms-580xx ┆ 12       │
├╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌┤
│ nvidia-open-dkms  ┆ 10       │
╰───────────────────┴──────────╯

> 0000:01:00.0 (0300:10de:2204) VGA compatible controller NVIDIA Corporation:

╭──────────────────┬──────────╮
│ Name             ┆ Priority │
╞══════════════════╪══════════╡
│ nvidia-open-dkms ┆ 10       │
╰──────────────────┴──────────╯
";
    let offers = profiles::parse_chwd_list(text);
    assert_eq!(offers.len(), 2);
    assert_eq!(offers[0].addr, "0000:02:00.0");
    assert_eq!(
        offers[0].profiles,
        words(&["nvidia-dkms-580xx", "nvidia-open-dkms"])
    );
    assert_eq!(offers[1].profiles, words(&["nvidia-open-dkms"]));
}

#[test]
fn profile_files_parse_with_inheritance_and_blocks() {
    let p = profiles::parse(DB, profiles::GRAPHICS_SET);
    assert_eq!(p.len(), 4, "{p:?}");
    assert!(!p.iter().any(|p| p.name == "not-a-table"));
    let prime = p
        .iter()
        .find(|p| p.name == "nvidia-open-dkms.prime")
        .unwrap();
    assert_eq!(prime.priority, 11);
    assert_eq!(prime.conflict_group.as_deref(), Some("nvidia"));
    assert_eq!(prime.conditional, "echo open-modules\n");
    assert!(prime.packages.contains(&"nvidia-prime".to_string()));
}

#[test]
fn a_record_is_removed_without_touching_its_neighbours() {
    let text = "[amd]\npackages = 'mesa'\n\n[nvidia-open-dkms]\npackages = 'x'\npost_install = \"\"\"\n[amd]\n\"\"\"\n\n[intel]\npackages = 'y'\n";
    let out = profiles::without_record(text, "nvidia-open-dkms");
    assert!(out.contains("[amd]") && out.contains("[intel]"), "{out}");
    assert!(
        !out.contains("nvidia") && !out.contains("post_install"),
        "{out}"
    );
}

#[test]
fn module_packages_are_classified() {
    for (name, branch) in [
        ("nvidia-open", Some(Branch::Open)),
        ("nvidia-open-lts", Some(Branch::Open)),
        ("linux-cachyos-nvidia-open", Some(Branch::Open)),
        ("nvidia-580xx-dkms", Some(Branch::Legacy580)),
        ("linux-cachyos-nvidia-580xx", Some(Branch::Legacy580)),
        ("nvidia-470xx-dkms", Some(Branch::Foreign)),
        ("nvidia-dkms", Some(Branch::Foreign)),
        ("linux-cachyos-nvidia", Some(Branch::Foreign)),
        ("gpu-unlock-dkms", Some(Branch::Foreign)),
        ("nvidia-utils", None),
        ("nvidia-container-toolkit", None),
        ("nvidia-prime", None),
        ("libva-nvidia-driver", None),
        ("linux-firmware-nvidia", None),
    ] {
        assert_eq!(module_branch(name), branch, "{name}");
    }
}

#[test]
fn a_fresh_card_gets_its_profile() {
    let plan = built(&fresh(), &Request::default());
    assert!(plan.refusals.is_empty(), "{:?}", plan.refusals);
    assert_eq!(plan.steps.len(), 1);
    assert_eq!(plan.steps[0].status, Status::Missing);
    assert_eq!(
        plan.steps[0].install,
        words(&[
            "nvidia-utils",
            "libva-nvidia-driver",
            "nvidia-open-dkms",
            "dkms"
        ])
    );
    assert!(plan.steps[0].remove.is_empty());
}

#[test]
fn an_installed_provider_of_the_same_branch_is_kept() {
    // the installer put a prebuilt module there; the profile would pick DKMS
    let sys = modinfo(
        machine(
            &[GPU_3090],
            &single("2204", &OFFER_NEW),
            "base linux-cachyos-nvidia-open nvidia-utils",
        ),
        0,
    )
    .cmd(OPEN_T, 127, "libva-nvidia-driver\ndkms\n");
    let plan = built(&sys, &Request::default());
    assert_eq!(plan.steps[0].status, Status::Extras);
    assert_eq!(plan.steps[0].install, words(&["libva-nvidia-driver"]));
    assert!(plan.steps[0].remove.is_empty());
    assert!(plan.steps[0].keep_provider);
    // chwd would add a second, conflicting module package: pacman installs the rest
    let sys = sys
        .cmd("pacman -Sw --noconfirm libva-nvidia-driver", 0, "")
        .cmd("pacman -S --needed --noconfirm libva-nvidia-driver", 0, "")
        .cmd(OPEN_T, 127, "nvidia-open-dkms\ndkms\n");
    let plan = built(&sys, &Request::default());
    apply::apply(&sys, &plan, SnapshotPolicy::Skip).expect("extras installed");
    assert!(!sys.calls().iter().any(|c| c.starts_with("chwd -i")));
    // --reinstall takes the profile's own provider instead
    let redo = built(
        &sys,
        &Request {
            reinstall: true,
            ..Request::default()
        },
    );
    assert!(!redo.steps[0].keep_provider);
    assert!(redo.steps[0]
        .install
        .contains(&"nvidia-open-dkms".to_string()));
    assert_eq!(redo.steps[0].remove, words(&["linux-cachyos-nvidia-open"]));
}

#[test]
fn a_kernel_without_the_module_replaces_the_provider_when_they_conflict() {
    let sys = modinfo(
        machine(
            &[GPU_3090],
            &single("2204", &OFFER_NEW),
            "nvidia-open nvidia-utils",
        ),
        1,
    )
    .cmd(OPEN_T, 127, "libva-nvidia-driver\ndkms\n");
    let step = built(&sys, &Request::default()).steps.remove(0);
    assert_eq!(step.status, Status::Missing);
    assert_eq!(step.remove, words(&["nvidia-open"]));
    assert!(step.install.contains(&"nvidia-open-dkms".to_string()));
}

#[test]
fn a_branch_switch_needs_the_flag() {
    let plan = built(&old_branch(), &Request::default());
    assert_eq!(plan.steps[0].status, Status::Switch);
    assert!(plan.refusals.iter().any(|r| r.contains("--switch-driver")));
    let allowed = built(
        &old_branch(),
        &Request {
            switch_driver: true,
            ..Request::default()
        },
    );
    assert!(allowed.refusals.is_empty(), "{:?}", allowed.refusals);
    assert_eq!(
        allowed.steps[0].remove,
        words(&["nvidia-580xx-dkms", "nvidia-580xx-utils"])
    );
}

#[test]
fn an_unmanaged_driver_is_never_touched() {
    let sys = machine(
        &[GPU_3090],
        &single("2204", &OFFER_NEW),
        "gpu-unlock-dkms nvidia-utils",
    )
    .cmd(OPEN_T, 127, "nvidia-open-dkms\n");
    let sys = modinfo(sys, 1);
    let plan = built(
        &sys,
        &Request {
            switch_driver: true,
            ..Request::default()
        },
    );
    assert!(
        plan.refusals.iter().any(|r| r.contains("does not manage")),
        "{:?}",
        plan.refusals
    );
    assert!(apply::apply(&sys, &plan, SnapshotPolicy::Skip).is_err());
    assert!(mutating(&sys).is_empty(), "{:?}", mutating(&sys));
}

#[test]
fn mixed_generations_get_the_branch_that_runs_both_cards() {
    // chwd lists the Pascal card first and would give both the open modules
    let offers = format!(
        "{}{}",
        chwd_table("0000:02:00.0", "1b81", &OFFER_OLD),
        chwd_table("0000:01:00.0", "2204", &OFFER_NEW)
    );
    let sys = machine(&[GPU_3090, GPU_1070], &offers, "base").cmd(LEGACY_T, 127, "nvidia-utils\n");
    let plan = built(&sys, &Request::default());
    assert_eq!(plan.steps.len(), 1);
    assert_eq!(plan.steps[0].profile, "nvidia-dkms-580xx");
    assert_eq!(plan.steps[0].devices.len(), 2);
    assert!(plan.refusals.is_empty(), "{:?}", plan.refusals);
}

#[test]
fn open_modules_are_refused_for_a_pascal_card() {
    let offers = format!(
        "{}{}",
        chwd_table("0000:02:00.0", "1b81", &OFFER_OLD),
        chwd_table("0000:01:00.0", "2204", &OFFER_NEW)
    );
    let sys = machine(&[GPU_3090, GPU_1070], &offers, "base").cmd(OPEN_T, 127, "nvidia-utils\n");
    let plan = built(
        &sys,
        &Request {
            profile: Some("nvidia-open-dkms".into()),
            ..Request::default()
        },
    );
    assert!(
        plan.refusals
            .iter()
            .any(|r| r.contains("does not support Pascal")),
        "{:?}",
        plan.refusals
    );
}

#[test]
fn an_incompatible_pair_keeps_the_boot_card_and_says_so() {
    let offers = format!(
        "{}{}",
        chwd_table("0000:02:00.0", "1b81", &OFFER_OLD),
        chwd_table("0000:01:00.0", "2c02", &OFFER_NEW)
    );
    // Blackwell (boot) + Pascal: no branch runs both
    let sys = machine(&[GPU_5080, GPU_1070], &offers, "base").cmd(OPEN_T, 127, "nvidia-utils\n");
    let plan = built(&sys, &Request::default());
    assert_eq!(plan.steps[0].profile, "nvidia-open-dkms");
    assert_eq!(plan.steps[0].devices, words(&["0000:01:00.0"]));
    assert!(plan.refusals.is_empty(), "{:?}", plan.refusals);
    assert!(
        plan.warnings.iter().any(|w| w.contains("0000:02:00.0")),
        "{:?}",
        plan.warnings
    );
}

#[test]
fn an_unknown_generation_is_refused() {
    let card = GPU_3090.replace("GA102 [GeForce RTX 3090]", "Device");
    let sys =
        machine(&[&card], &single("2204", &OFFER_NEW), "base").cmd(OPEN_T, 127, "nvidia-utils\n");
    let plan = built(&sys, &Request::default());
    assert!(plan.refusals[0].contains("unknown NVIDIA generation"));
}

#[test]
fn profiles_chwd_does_not_offer_are_refused() {
    let err = plan::build(
        &fresh(),
        &Request {
            profile: Some("nvidia-dkms-580xx".into()),
            ..Request::default()
        },
    )
    .unwrap_err();
    assert!(err.contains("does not offer"), "{err}");
}

#[test]
fn without_a_kernel_module_package_the_plan_fails() {
    // the conditional snippet answers nothing (no installed kernel)
    let empty = Fake::default()
        .cmd("lspci -Dnnmm", 0, GPU_3090)
        .cmd("chwd --list", 0, &single("2204", &OFFER_NEW))
        .dir("/var/lib/chwd/db/pci", &["graphic_drivers"])
        .file(DB_PATH, DB)
        .cmd("pacman -Qq", 0, "base")
        .cmd("sh -c echo open-modules\n", 0, "")
        .cmd("pacman -T nvidia-utils libva-nvidia-driver", 0, "");
    let err = plan::build(&empty, &Request::default()).unwrap_err();
    assert!(err.contains("kernel module package"), "{err}");
}

fn snapper(sys: Fake, code: i32) -> Fake {
    sys.path("/usr/bin/snapper").cmd(
        "snapper -c root create --print-number --cleanup-algorithm number --description samurai hw",
        code,
        "42\n",
    )
}

#[test]
fn no_snapshot_means_no_change() {
    for sys in [fresh(), snapper(fresh(), 1)] {
        let plan = built(&sys, &Request::default());
        let failure = apply::apply(&sys, &plan, SnapshotPolicy::Require).unwrap_err();
        assert_eq!(failure.outcome, "nothing was changed");
        assert!(!mutating(&sys)
            .iter()
            .any(|c| c.starts_with("pacman") || c.starts_with("chwd")));
    }
}

#[test]
fn a_failed_download_changes_nothing() {
    let sys = old_branch().cmd(
        "pacman -Sw --noconfirm nvidia-utils libva-nvidia-driver nvidia-open-dkms dkms",
        1,
        "",
    );
    let plan = built(
        &sys,
        &Request {
            switch_driver: true,
            ..Request::default()
        },
    );
    let failure = apply::apply(&sys, &plan, SnapshotPolicy::Skip).unwrap_err();
    assert_eq!(failure.outcome, "nothing was changed");
    assert_eq!(mutating(&sys).len(), 1, "{:?}", mutating(&sys));
}

fn switch_downloads(sys: Fake) -> Fake {
    sys.cmd(
        "pacman -Sw --noconfirm nvidia-utils libva-nvidia-driver nvidia-open-dkms dkms",
        0,
        "",
    )
    .cmd(
        "pacman -Sw --noconfirm nvidia-580xx-dkms nvidia-580xx-utils",
        0,
        "",
    )
    .cmd(
        "pacman -Rdd --noconfirm nvidia-580xx-dkms nvidia-580xx-utils",
        0,
        "",
    )
    .cmd("mkinitcpio -P", 0, "")
}

#[test]
fn a_switch_downloads_first_then_replaces_and_verifies() {
    let plan = built(
        &modinfo(old_branch(), 0),
        &Request {
            switch_driver: true,
            ..Request::default()
        },
    );
    // after chwd the profile is satisfied
    let sys = switch_downloads(modinfo(
        machine(&[GPU_3090], &single("2204", &OFFER_NEW), OPEN_SET).cmd(OPEN_T, 0, ""),
        0,
    ))
    .cmd("chwd -i nvidia-open-dkms", 0, "");
    let report = apply::apply(&sys, &plan, SnapshotPolicy::Skip).expect("applied");
    assert!(report.reboot);
    let calls = mutating(&sys);
    assert_eq!(
        calls,
        words(&[
            "pacman -Sw --noconfirm nvidia-utils libva-nvidia-driver nvidia-open-dkms dkms",
            "pacman -Sw --noconfirm nvidia-580xx-dkms nvidia-580xx-utils",
            "pacman -Rdd --noconfirm nvidia-580xx-dkms nvidia-580xx-utils",
            "chwd -i nvidia-open-dkms",
        ])
    );
}

#[test]
fn a_failed_switch_restores_the_previous_driver() {
    let plan = built(
        &modinfo(old_branch(), 0),
        &Request {
            switch_driver: true,
            ..Request::default()
        },
    );
    // chwd fails. The first `pacman -Qq` is the old state, the second shows no NVIDIA package.
    let sys = switch_downloads(
        Fake::default()
            .cmd(
                "pacman -Qq",
                0,
                "base dkms nvidia-580xx-dkms nvidia-580xx-utils",
            )
            .cmd("pacman -Qq", 0, "base dkms"),
    )
    .cmd("chwd -i nvidia-open-dkms", 1, "")
    .cmd(
        "pacman -S --noconfirm nvidia-580xx-dkms nvidia-580xx-utils",
        0,
        "",
    )
    .file(LOCAL_PATH, "[nvidia-dkms-580xx]\npackages = 'x'\n")
    .dir("/var/lib/chwd/local/pci", &["graphic_drivers"]);
    let failure = apply::apply(&sys, &plan, SnapshotPolicy::Skip).unwrap_err();
    assert!(
        failure
            .outcome
            .contains("previous driver packages were restored"),
        "{failure:?}"
    );
    assert!(mutating(&sys)
        .contains(&"pacman -S --noconfirm nvidia-580xx-dkms nvidia-580xx-utils".to_string()));
    let writes = sys.writes();
    assert!(
        writes
            .iter()
            .rev()
            .find(|(p, _)| p == LOCAL_PATH)
            .is_some_and(|(_, c)| c.contains("[nvidia-dkms-580xx]")),
        "the old record is back: {writes:?}"
    );
}

#[test]
fn a_module_that_does_not_build_takes_the_new_packages_out_again() {
    let plan = built(&fresh(), &Request::default());
    let sys = Fake::default()
        .cmd("pacman -Qq", 0, "base")
        .cmd("pacman -Qq", 0, "base nvidia-utils nvidia-open-dkms")
        .cmd(
            "pacman -Sw --noconfirm nvidia-utils libva-nvidia-driver nvidia-open-dkms dkms",
            0,
            "",
        )
        .cmd("chwd -i nvidia-open-dkms", 0, "")
        .cmd(OPEN_T, 0, "")
        .cmd(
            "pacman -Rdd --noconfirm nvidia-utils nvidia-open-dkms",
            0,
            "",
        )
        .cmd("mkinitcpio -P", 0, "")
        .dir("/usr/lib/modules", &[RELEASE])
        .path(&format!("/usr/lib/modules/{RELEASE}/pkgbase"));
    let failure = apply::apply(&modinfo(sys, 1), &plan, SnapshotPolicy::Skip).unwrap_err();
    assert!(failure.reason.contains("no nvidia module"), "{failure:?}");
    assert!(failure.outcome.contains("removed again"), "{failure:?}");
}

#[test]
fn a_present_cards_profile_cannot_be_removed() {
    let sys = fresh();
    let failure = apply::remove(&sys, "nvidia-open-dkms", SnapshotPolicy::Skip).unwrap_err();
    assert!(failure.reason.contains("present device"), "{failure:?}");
    // the legacy profile is not offered here, but an NVIDIA card is present
    let failure = apply::remove(&sys, "nvidia-dkms-580xx", SnapshotPolicy::Skip).unwrap_err();
    assert!(
        failure.reason.contains("NVIDIA card is present"),
        "{failure:?}"
    );
    assert!(mutating(&sys).is_empty());
}

#[test]
fn fingerprint_tracks_cards_not_bus_numbers() {
    let sys = |cards: &str| Fake::default().cmd("lspci -Dnnmm", 0, cards);
    let first = sys(GPU_3090);
    assert_eq!(fingerprint::check(&first).unwrap(), State::New);
    let baseline = first.writes().pop().unwrap().1;

    let moved = sys(&GPU_3090.replace("0000:01:00.0", "0000:41:00.0"))
        .file(fingerprint::BASELINE, &baseline)
        .path(fingerprint::NOTICE);
    assert_eq!(fingerprint::check(&moved).unwrap(), State::Unchanged);
    assert_eq!(moved.writes()[0].1, "<removed>");

    let swapped = sys(GPU_5080).file(fingerprint::BASELINE, &baseline);
    let state = fingerprint::check(&swapped).unwrap();
    assert!(matches!(state, State::Changed { .. }));
    let (path, notice) = swapped.writes().pop().unwrap();
    assert_eq!(
        path,
        fingerprint::NOTICE,
        "the baseline stays until accepted"
    );
    assert!(
        notice.contains("New hardware: NVIDIA Corporation GB203 [GeForce RTX 5080] (was NVIDIA Corporation GA102 [GeForce RTX 3090])"),
        "{notice}"
    );
    // an older installer's free-form listing is replaced silently
    let legacy = sys(GPU_3090).file(fingerprint::BASELINE, "  00:00.0 Host bridge\n");
    assert_eq!(fingerprint::check(&legacy).unwrap(), State::New);
    // an unreadable bus never overwrites the baseline
    let broken = Fake::default().file(fingerprint::BASELINE, &baseline);
    assert!(fingerprint::check(&broken).is_err() && broken.writes().is_empty());
}

#[test]
fn arguments() {
    let parse = |v: &[&str]| parse_args(&words(v));
    assert!(parse(&["detect", "--json"]).unwrap().json);
    assert_eq!(parse(&["list"]).unwrap().cmd, Cmd::List(Listing::Available));
    assert_eq!(
        parse(&["apply", "--profile", "amd", "--switch-driver", "--yes"])
            .unwrap()
            .cmd,
        Cmd::Apply {
            request: Request {
                profile: Some("amd".into()),
                reinstall: false,
                switch_driver: true
            },
            dry_run: false,
            no_snapshot: false
        }
    );
    for bad in [
        &[][..],
        &["bogus"],
        &["detect", "--yes"],
        &["list", "--all", "--installed"],
        &["apply", "--json"],
        &["apply", "--profile", "--yes"],
        &["apply", "--profile", "a;b"],
        &["remove"],
        &["remove", "a", "b"],
        &["fingerprint", "--check", "--accept"],
        &["detect", "--profile", "amd"],
    ] {
        assert!(parse(bad).is_err(), "{bad:?}");
    }
    assert_eq!(parse(&["apply", "--help"]).unwrap().cmd, Cmd::Help);
}

#[test]
fn json_output_carries_the_plan() {
    let plan = built(&fresh(), &Request::default());
    let json = render_json(&plan, &State::Unchanged);
    for part in [
        "\"schema\":1",
        "\"allowed\":true",
        "\"generation\":\"Ampere\"",
        "\"cuda\":true",
        "\"boot_display\":true",
        "\"status\":\"missing\"",
        "\"driver\":\"nouveau\"",
    ] {
        assert!(json.contains(part), "{part}: {json}");
    }
}

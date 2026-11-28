//! What `samurai hw apply` would do, and every reason it must not. Read-only.

use crate::health::system::System;

use super::devices::{self, Device, Generation};
use super::profiles::{self, Offer, Profile};

/// Which NVIDIA kernel module branch a package or profile belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Branch {
    Open,
    Legacy580,
    /// A provider we do not manage (470xx, closed, beta, gpu-unlock, ...).
    Foreign,
}

impl Branch {
    pub fn as_str(self) -> &'static str {
        match self {
            Branch::Open => "nvidia-open",
            Branch::Legacy580 => "nvidia-580xx",
            Branch::Foreign => "unmanaged",
        }
    }
}

/// The branch of an NVIDIA **kernel module** package; `None` for everything else.
pub fn module_branch(pkg: &str) -> Option<Branch> {
    // `linux-firmware-nvidia` is firmware, not a module
    let kernel_module = pkg.starts_with("linux") && !pkg.starts_with("linux-firmware");
    if matches!(pkg, "nvidia-open" | "nvidia-open-lts" | "nvidia-open-dkms")
        || (kernel_module && pkg.ends_with("-nvidia-open"))
    {
        return Some(Branch::Open);
    }
    if pkg == "nvidia-580xx-dkms" || (kernel_module && pkg.ends_with("-nvidia-580xx")) {
        return Some(Branch::Legacy580);
    }
    let foreign =
        matches!(
            pkg,
            "nvidia"
                | "nvidia-lts"
                | "nvidia-dkms"
                | "nvidia-beta"
                | "nvidia-beta-dkms"
                | "nvidia-open-beta"
                | "nvidia-open-beta-dkms"
                | "nvidia-open-git"
                | "gpu-unlock-dkms"
        ) || (pkg.starts_with("nvidia-") && pkg.contains("xx") && pkg.ends_with("xx-dkms"))
            || (kernel_module && (pkg.ends_with("-nvidia") || pkg.contains("-nvidia-")));
    foreign.then_some(Branch::Foreign)
}

/// Userspace packages that belong to one branch and conflict with the other.
const OPEN_USERSPACE: [&str; 5] = [
    "nvidia-utils",
    "nvidia-settings",
    "opencl-nvidia",
    "lib32-nvidia-utils",
    "lib32-opencl-nvidia",
];
const LEGACY_USERSPACE: [&str; 5] = [
    "nvidia-580xx-utils",
    "nvidia-580xx-settings",
    "opencl-nvidia-580xx",
    "lib32-nvidia-580xx-utils",
    "lib32-opencl-nvidia-580xx",
];

fn userspace(branch: Branch) -> &'static [&'static str] {
    match branch {
        Branch::Open => &OPEN_USERSPACE,
        Branch::Legacy580 => &LEGACY_USERSPACE,
        Branch::Foreign => &[],
    }
}

/// Packages whose absence means the card has no working driver (the rest of a profile is optional:
/// guest tools and software Vulkan never make a fresh install "differ from the profile").
const CORE: [&str; 5] = [
    "nvidia-utils",
    "nvidia-580xx-utils",
    "mesa",
    "vulkan-radeon",
    "vulkan-intel",
];

/// The NVIDIA branch a profile installs; `None` for non-proprietary profiles.
pub fn profile_branch(profile: &Profile) -> Option<Branch> {
    if profile.packages.iter().any(|p| p == "nvidia-utils") {
        Some(Branch::Open)
    } else if profile.packages.iter().any(|p| p == "nvidia-580xx-utils") {
        Some(Branch::Legacy580)
    } else {
        None
    }
}

/// The installed NVIDIA module providers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NvidiaState {
    pub modules: Vec<String>,
    pub branch: Option<Branch>,
}

pub fn nvidia_state(installed: &[String]) -> NvidiaState {
    let modules: Vec<String> = installed
        .iter()
        .filter(|p| module_branch(p).is_some())
        .cloned()
        .collect();
    let mut branches: Vec<Branch> = modules.iter().filter_map(|p| module_branch(p)).collect();
    branches.dedup();
    let mut branch = match branches.as_slice() {
        [] => None,
        [one] => Some(*one),
        // two branches at once is nothing we may untangle
        _ => Some(Branch::Foreign),
    };
    // userspace without any module package (a hand-built module, or a broken switch)
    if branch.is_none() {
        let has = |n: &str| installed.iter().any(|p| p == n);
        if has("nvidia-utils") {
            branch = Some(Branch::Open);
        } else if has("nvidia-580xx-utils") {
            branch = Some(Branch::Legacy580);
        }
    }
    NvidiaState { modules, branch }
}

/// Installed kernel releases (module trees that belong to a package).
pub fn kernel_releases(sys: &dyn System) -> Vec<String> {
    sys.list_dir("/usr/lib/modules")
        .into_iter()
        .filter(|r| sys.exists(&format!("/usr/lib/modules/{r}/pkgbase")))
        .collect()
}

/// Kernel releases that have no `nvidia` module.
pub fn kernels_without_module(sys: &dyn System) -> Vec<String> {
    kernel_releases(sys)
        .into_iter()
        .filter(|r| {
            !sys.run("modinfo", &["-k", r, "-F", "version", "nvidia"])
                .is_some_and(|o| o.code == 0)
        })
        .collect()
}

pub fn installed_packages(sys: &dyn System) -> Result<Vec<String>, String> {
    match sys.run("pacman", &["-Qq"]) {
        Some(o) if o.code == 0 => Ok(o.stdout.split_whitespace().map(str::to_string).collect()),
        _ => Err("pacman -Qq failed (database locked?)".into()),
    }
}

/// The subset of `wanted` that no installed package satisfies (`pacman -T` honours `provides`).
pub fn unsatisfied(sys: &dyn System, wanted: &[String]) -> Result<Vec<String>, String> {
    if wanted.is_empty() {
        return Ok(Vec::new());
    }
    let mut args = vec!["-T"];
    args.extend(wanted.iter().map(String::as_str));
    match sys.run("pacman", &args) {
        Some(o) if o.code == 0 || o.code == 127 => {
            Ok(o.stdout.split_whitespace().map(str::to_string).collect())
        }
        Some(o) => Err(format!("pacman -T exited {}", o.code)),
        None => Err("pacman is not available".into()),
    }
}

pub fn valid_package_name(name: &str) -> bool {
    !name.starts_with('-')
        && !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "@._+-".contains(c))
}

fn multilib(sys: &dyn System) -> bool {
    sys.read("/etc/pacman.conf")
        .is_some_and(|c| c.lines().any(|l| l.trim_start().starts_with("[multilib]")))
}

/// The full package list of a profile on this machine: static plus the conditional snippet,
/// exactly as chwd's helper script builds it (lib32 dropped without multilib).
pub fn resolve_packages(sys: &dyn System, profile: &Profile) -> Result<Vec<String>, String> {
    let mut names = profile.packages.clone();
    if !profile.conditional.trim().is_empty() {
        let out = sys
            .run("sh", &["-c", &profile.conditional])
            .ok_or("sh is not available")?;
        names.extend(out.stdout.split_whitespace().map(str::to_string));
    }
    if let Some(bad) = names.iter().find(|n| !valid_package_name(n)) {
        return Err(format!(
            "profile {} produced an invalid package name: {bad:?}",
            profile.name
        ));
    }
    if !multilib(sys) {
        names.retain(|n| !n.starts_with("lib32-"));
    }
    let mut seen = Vec::new();
    for n in names {
        if !seen.contains(&n) {
            seen.push(n);
        }
    }
    Ok(seen)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Everything the profile wants is there.
    Ok,
    /// The driver works; optional packages of the profile are absent.
    Extras,
    /// Driver packages or kernel modules are missing.
    Missing,
    /// Another NVIDIA branch has to be replaced.
    Switch,
    /// Redo an otherwise satisfied profile (`--reinstall`).
    Reinstall,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Ok => "ok",
            Status::Extras => "extras",
            Status::Missing => "missing",
            Status::Switch => "switch",
            Status::Reinstall => "reinstall",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub profile: String,
    pub desc: String,
    pub graphics: bool,
    pub branch: Option<Branch>,
    /// PCI addresses this profile covers.
    pub devices: Vec<String>,
    pub status: Status,
    /// Everything the profile wants here.
    pub packages: Vec<String>,
    /// What would be installed.
    pub install: Vec<String>,
    /// What must be removed first (conflicting NVIDIA packages).
    pub remove: Vec<String>,
    pub recorded: bool,
    /// The installed module package differs from the profile's but serves every kernel: only the
    /// other packages are installed (chwd would try to add a conflicting module package).
    pub keep_provider: bool,
}

impl Step {
    pub fn acts(&self) -> bool {
        self.status != Status::Ok
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    /// Display and network devices that have a profile, plus every display device.
    pub devices: Vec<Device>,
    pub offers: Vec<Offer>,
    pub steps: Vec<Step>,
    pub nvidia: NvidiaState,
    pub refusals: Vec<String>,
    pub warnings: Vec<String>,
}

impl Plan {
    pub fn changes(&self) -> bool {
        self.steps.iter().any(Step::acts)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Request {
    /// Apply this profile instead of the recommended one for its devices.
    pub profile: Option<String>,
    pub reinstall: bool,
    /// Allow replacing the NVIDIA driver branch.
    pub switch_driver: bool,
}

/// chwd's choice per device, including its one-driver-per-conflict-group rule
/// (`data::resolve_conflict_groups`): the best profile common to all devices of a group, else the
/// highest-priority device wins and the others are skipped.
/// Returns (profile name -> addresses, skipped addresses).
pub fn recommend(offers: &[Offer], db: &[Profile]) -> (Vec<(String, Vec<String>)>, Vec<String>) {
    let find = |name: &str| db.iter().find(|p| p.name == name);
    let group_of = |o: &Offer| find(o.profiles.first()?)?.conflict_group.clone();
    let mut choice: Vec<(String, String)> = Vec::new();
    let mut skipped = Vec::new();
    let mut done_groups: Vec<String> = Vec::new();
    for offer in offers {
        let Some(best) = offer.profiles.first() else {
            continue;
        };
        let Some(group) = group_of(offer) else {
            choice.push((offer.addr.clone(), best.clone()));
            continue;
        };
        if done_groups.contains(&group) {
            continue;
        }
        done_groups.push(group.clone());
        let members: Vec<&Offer> = offers
            .iter()
            .filter(|o| group_of(o).as_deref() == Some(group.as_str()))
            .collect();
        let common = offer.profiles.iter().find(|name| {
            find(name).and_then(|p| p.conflict_group.as_deref()) == Some(group.as_str())
                && members.iter().all(|m| m.profiles.contains(name))
        });
        match common {
            Some(name) => {
                choice.extend(members.iter().map(|m| (m.addr.clone(), name.clone())));
            }
            None => {
                let priority = |o: &&Offer| {
                    o.profiles
                        .first()
                        .and_then(|n| find(n))
                        .map_or(0, |p| p.priority)
                };
                if let Some(primary) = members.iter().copied().max_by_key(priority) {
                    choice.push((primary.addr.clone(), primary.profiles[0].clone()));
                    skipped.extend(
                        members
                            .iter()
                            .filter(|m| m.addr != primary.addr)
                            .map(|m| m.addr.clone()),
                    );
                }
            }
        }
    }
    let mut grouped: Vec<(String, Vec<String>)> = Vec::new();
    for (addr, name) in choice {
        match grouped.iter_mut().find(|(n, _)| *n == name) {
            Some((_, addrs)) => addrs.push(addr),
            None => grouped.push((name, vec![addr])),
        }
    }
    grouped.sort_by_key(|(n, _)| std::cmp::Reverse(find(n).map_or(0, |p| p.priority)));
    (grouped, skipped)
}

fn supports(branch: Branch, generation: Generation) -> bool {
    match branch {
        Branch::Open => generation.open_modules(),
        Branch::Legacy580 => generation.legacy_580(),
        Branch::Foreign => false,
    }
}

/// chwd offers the open modules to every NVIDIA card, so on a machine with cards of different
/// generations its common choice can leave one without a driver (RTX 3090 + GTX 1070: open
/// modules for both). Prefer one branch that supports every card (here 580xx); when none does,
/// keep the one for the boot display (else the newest card) and leave the others out.
fn settle_nvidia(
    chosen: &mut Vec<(String, Vec<String>)>,
    skipped: &mut Vec<String>,
    offers: &[Offer],
    db: &[Profile],
    devices: &[Device],
) {
    let cards: Vec<&Device> = devices
        .iter()
        .filter(|d| d.is_nvidia_display() && offers.iter().any(|o| o.addr == d.addr))
        .collect();
    let branch_of = |name: &str| db.iter().find(|p| p.name == name).and_then(profile_branch);
    let broken = chosen.iter().any(|(name, addrs)| {
        branch_of(name).is_some_and(|b| {
            cards
                .iter()
                .any(|d| addrs.contains(&d.addr) && !supports(b, Generation::of(&d.name)))
        })
    }) || cards.iter().any(|d| skipped.contains(&d.addr));
    if cards.len() < 2 || !broken {
        return;
    }
    // best proprietary profile of any card, per number of cards its branch supports
    let mut candidates: Vec<(&Profile, Vec<String>)> = offers
        .iter()
        .filter(|o| cards.iter().any(|d| d.addr == o.addr))
        .flat_map(|o| &o.profiles)
        .filter_map(|name| db.iter().find(|p| &p.name == name))
        .filter_map(|p| {
            let branch = profile_branch(p)?;
            let covered = cards
                .iter()
                .filter(|d| supports(branch, Generation::of(&d.name)))
                .map(|d| d.addr.clone())
                .collect::<Vec<_>>();
            Some((p, covered))
        })
        .collect();
    let boot = cards.iter().find(|d| d.boot_vga).map(|d| &d.addr);
    let newest = cards
        .iter()
        .max_by_key(|d| {
            Generation::of(&d.name) as u8 * u8::from(Generation::of(&d.name) != Generation::Unknown)
        })
        .map(|d| &d.addr);
    let prefer = boot.or(newest);
    candidates.sort_by_key(|(p, covered)| {
        std::cmp::Reverse((
            covered.len(),
            prefer.is_some_and(|a| covered.contains(a)),
            p.priority,
        ))
    });
    let Some((profile, covered)) = candidates.into_iter().next() else {
        return;
    };
    if covered.is_empty() {
        return;
    }
    let ours = |addr: &String| cards.iter().any(|d| &d.addr == addr);
    for (_, addrs) in chosen.iter_mut() {
        addrs.retain(|a| !ours(a));
    }
    chosen.retain(|(_, addrs)| !addrs.is_empty());
    skipped.retain(|a| !ours(a));
    skipped.extend(
        cards
            .iter()
            .filter(|d| !covered.contains(&d.addr))
            .map(|d| d.addr.clone()),
    );
    chosen.insert(0, (profile.name.clone(), covered));
}

fn step_for(
    sys: &dyn System,
    profile: &Profile,
    devices: Vec<String>,
    state: &NvidiaState,
    installed: &[String],
    recorded: bool,
    reinstall: bool,
) -> Result<Step, String> {
    let packages = resolve_packages(sys, profile)?;
    let branch = profile_branch(profile);
    let mut install = unsatisfied(sys, &packages)?;
    let mut remove = Vec::new();
    let mut core_missing = install.iter().any(|p| CORE.contains(&p.as_str()));
    // network profiles have no optional parts
    if !profile.graphics() {
        core_missing = !install.is_empty();
    }
    let mut status = None;
    let mut keep_provider = false;

    if let Some(branch) = branch {
        if !packages.iter().any(|p| module_branch(p) == Some(branch)) {
            return Err(format!(
                "profile {}: cannot tell which kernel module package to install (no installed kernel found?)",
                profile.name
            ));
        }
        let present: Vec<String> = userspace_and_modules(installed);
        match state.branch {
            Some(current) if current != branch => {
                status = Some(Status::Switch);
                remove = present;
                install = packages.clone();
            }
            current => {
                let uncovered = kernels_without_module(sys);
                if current.is_some() && uncovered.is_empty() && !reinstall {
                    // every kernel has the module: keep the installed provider as it is
                    keep_provider = packages
                        .iter()
                        .any(|p| module_branch(p).is_some() && !installed.contains(p));
                    install.retain(|p| {
                        module_branch(p).is_none() && p != "dkms" && !p.ends_with("-headers")
                    });
                } else {
                    core_missing |= current.is_none() || !uncovered.is_empty();
                    // a DKMS provider and a prebuilt one cannot both provide NVIDIA-MODULE
                    let wants_dkms = packages.iter().any(|p| p.ends_with("-dkms"));
                    remove = state
                        .modules
                        .iter()
                        .filter(|m| !packages.contains(m))
                        .filter(|m| wants_dkms || m.ends_with("-dkms"))
                        .cloned()
                        .collect();
                    for m in packages.iter().filter(|p| module_branch(p).is_some()) {
                        if !install.contains(m) && !installed.contains(m) {
                            install.push(m.clone());
                        }
                    }
                }
            }
        }
    } else if profile.graphics() && state.branch.is_some() && profile.name.starts_with("nouveau") {
        status = Some(Status::Switch);
        remove = userspace_and_modules(installed);
    }

    let status = status.unwrap_or(if core_missing {
        Status::Missing
    } else if reinstall {
        Status::Reinstall
    } else if !install.is_empty() {
        Status::Extras
    } else {
        Status::Ok
    });
    Ok(Step {
        profile: profile.name.clone(),
        desc: profile.desc.clone(),
        graphics: profile.graphics(),
        branch,
        devices,
        status,
        packages,
        install,
        remove,
        recorded,
        keep_provider,
    })
}

/// Every installed package of either managed NVIDIA branch.
pub fn userspace_and_modules(installed: &[String]) -> Vec<String> {
    installed
        .iter()
        .filter(|p| {
            module_branch(p).is_some()
                || userspace(Branch::Open).contains(&p.as_str())
                || userspace(Branch::Legacy580).contains(&p.as_str())
        })
        .cloned()
        .collect()
}

/// The safety rules (spec 07, "never leave a machine without a display driver").
fn refusals(plan: &Plan, request: &Request, skipped: &[String]) -> (Vec<String>, Vec<String>) {
    let mut no = Vec::new();
    let mut warn = Vec::new();
    let device = |addr: &str| plan.devices.iter().find(|d| d.addr == addr);
    for step in plan.steps.iter().filter(|s| s.acts()) {
        let Some(branch) = step.branch else { continue };
        for d in step.devices.iter().filter_map(|a| device(a)) {
            if !d.is_nvidia_display() {
                continue;
            }
            let generation = Generation::of(&d.name);
            let supported = supports(branch, generation);
            if generation == Generation::Unknown {
                no.push(format!(
                    "{}: unknown NVIDIA generation (\"{}\"); not installing {} on a guess. Update the system (hwdata) and try again",
                    d.addr, d.name, step.profile
                ));
            } else if !supported {
                no.push(format!(
                    "{}: {} does not support {} cards ({})",
                    d.addr,
                    step.profile,
                    generation.as_str(),
                    d.name
                ));
            }
        }
    }
    let touches_nvidia = plan
        .steps
        .iter()
        .any(|s| s.acts() && (s.branch.is_some() || s.status == Status::Switch));
    if touches_nvidia {
        if plan.nvidia.branch == Some(Branch::Foreign) {
            no.push(format!(
                "an NVIDIA driver that samurai does not manage is installed ({}); remove it yourself first",
                plan.nvidia.modules.join(", ")
            ));
        }
        if !request.switch_driver && plan.steps.iter().any(|s| s.status == Status::Switch) {
            no.push(
                "this replaces the installed NVIDIA driver branch; add --switch-driver to allow it"
                    .into(),
            );
        }
    }
    for addr in skipped {
        let Some(d) = device(addr) else { continue };
        let text = format!(
            "{addr} ({}) cannot share a driver with the other NVIDIA card and would have none",
            d.name
        );
        if d.boot_vga && touches_nvidia {
            no.push(format!("{text}; it drives the boot display"));
        } else {
            warn.push(text);
        }
    }
    (no, warn)
}

pub fn build(sys: &dyn System, request: &Request) -> Result<Plan, String> {
    let all = devices::scan(sys)?;
    let mut offers = profiles::offers(sys)?;
    let db = profiles::database(sys);
    if db.is_empty() {
        return Err("the chwd profile database is empty (install samurai-hardware)".into());
    }
    for name in offers.iter().flat_map(|o| &o.profiles) {
        if !db.iter().any(|p| &p.name == name) {
            return Err(format!("chwd offers {name}, which is not in the database"));
        }
    }
    offers.retain(|o| !o.profiles.is_empty());
    let installed = installed_packages(sys)?;
    let state = nvidia_state(&installed);
    let records = profiles::records(sys);
    let (mut chosen, mut skipped) = recommend(&offers, &db);

    if let Some(name) = &request.profile {
        let covered: Vec<String> = offers
            .iter()
            .filter(|o| o.profiles.contains(name))
            .map(|o| o.addr.clone())
            .collect();
        if covered.is_empty() {
            return Err(format!(
                "chwd does not offer the profile {name} for this hardware (samurai hw list)"
            ));
        }
        chosen = vec![(name.clone(), covered.clone())];
        skipped.retain(|a| !covered.contains(a));
        // one NVIDIA driver serves every NVIDIA card: check them all against the choice
        if db
            .iter()
            .any(|p| &p.name == name && profile_branch(p).is_some())
        {
            let others = all
                .iter()
                .filter(|d| d.is_nvidia_display() && !covered.contains(&d.addr));
            chosen[0].1.extend(others.map(|d| d.addr.clone()));
        }
    } else {
        settle_nvidia(&mut chosen, &mut skipped, &offers, &db, &all);
    }

    let mut plan = Plan {
        devices: all
            .into_iter()
            .filter(|d| d.is_display() || offers.iter().any(|o| o.addr == d.addr))
            .collect(),
        offers,
        nvidia: state.clone(),
        ..Plan::default()
    };
    for (name, addrs) in chosen {
        let Some(profile) = db.iter().find(|p| p.name == name) else {
            continue;
        };
        let recorded = records.iter().any(|(n, _)| *n == name);
        let reinstall = request.reinstall;
        plan.steps.push(step_for(
            sys, profile, addrs, &state, &installed, recorded, reinstall,
        )?);
    }
    let (refusals, warnings) = refusals(&plan, request, &skipped);
    plan.refusals = refusals;
    plan.warnings = warnings;
    for d in plan.devices.iter().filter(|d| d.is_display()) {
        if !plan.offers.iter().any(|o| o.addr == d.addr) {
            plan.warnings
                .push(format!("{} ({}) has no driver profile", d.addr, d.label()));
        }
    }
    if state.branch.is_some() && !plan.devices.iter().any(Device::is_nvidia_display) {
        plan.warnings.push(format!(
            "NVIDIA driver packages are installed but there is no NVIDIA card ({})",
            state.modules.join(", ")
        ));
    }
    Ok(plan)
}

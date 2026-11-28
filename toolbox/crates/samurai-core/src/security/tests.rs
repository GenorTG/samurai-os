use super::*;
use crate::health::system::fake::Fake;

fn root() -> Fake {
    Fake::default().cmd("id -u", 0, "0\n")
}

fn by_id<'a>(items: &'a [Item], id: &str) -> &'a Item {
    items
        .iter()
        .find(|i| i.id == id)
        .unwrap_or_else(|| panic!("no item {id}"))
}

const LSBLK: &str = "lsblk -lspno NAME,TYPE /dev/mapper/luks-1";

fn encrypted(sys: Fake) -> Fake {
    sys.cmd("findmnt -no SOURCE /", 0, "/dev/mapper/luks-1[/@]\n")
        .cmd(
            LSBLK,
            0,
            "/dev/mapper/luks-1 crypt\n/dev/vda2 part\n/dev/vda disk\n",
        )
}

#[test]
fn encryption_and_tpm_states() {
    let items = disk_items(&encrypted(root()), true);
    assert_eq!(by_id(&items, "disk.encryption").state, State::Pass);
    assert_eq!(by_id(&items, "disk.tpm").state, State::Skip, "no TPM");

    let tpm = |dump: &str| {
        let sys = encrypted(root()).path("/sys/class/tpm/tpm0").cmd(
            "cryptsetup luksDump /dev/vda2",
            0,
            dump,
        );
        disk_items(&sys, true).remove(1)
    };
    assert_eq!(tpm("Keyslots:\n 0: luks2\n").state, State::Skip);
    assert_eq!(
        tpm("Tokens:\n  0: systemd-tpm2\n\ttpm2-pin:   false\n").state,
        State::Warn
    );
    assert_eq!(
        tpm("Tokens:\n  0: systemd-tpm2\n\ttpm2-pin:   true\n").state,
        State::Pass
    );
    let user = disk_items(
        &encrypted(Fake::default()).path("/sys/class/tpm/tpm0"),
        false,
    );
    assert_eq!(by_id(&user, "disk.tpm").state, State::Unknown);

    let plain = Fake::default()
        .cmd("findmnt -no SOURCE /", 0, "/dev/vda2[/@]\n")
        .cmd(
            "lsblk -lspno NAME,TYPE /dev/vda2",
            0,
            "/dev/vda2 part\n/dev/vda disk\n",
        );
    assert_eq!(disk_items(&plain, false)[0].state, State::Warn);
}

#[test]
fn secure_boot_reads_the_value_byte() {
    let sys = |value: &str, setup: &str| {
        Fake::default()
            .path("/sys/firmware/efi")
            .file(SECUREBOOT_VAR, value)
            .file(SETUPMODE_VAR, setup)
    };
    let on = "\u{6}\0\0\0\u{1}";
    let off = "\u{6}\0\0\0\0";
    assert_eq!(secureboot_item(&sys(on, off)).state, State::Pass);
    let i = secureboot_item(&sys(off, on));
    assert_eq!(i.state, State::Warn);
    assert!(i.summary.contains("setup mode"));
    assert_eq!(secureboot_item(&Fake::default()).state, State::Skip);
}

#[test]
fn apparmor_needs_the_lsm_and_the_profiles() {
    let lsm = || {
        root()
            .file("/sys/module/apparmor/parameters/enabled", "Y\n")
            .file(
                "/sys/kernel/security/lsm",
                "capability,landlock,apparmor,bpf",
            )
    };
    let off = root()
        .file("/sys/module/apparmor/parameters/enabled", "Y\n")
        .file("/sys/kernel/security/lsm", "capability,landlock,bpf");
    assert_eq!(apparmor_item(&off, true).state, State::Fail);
    assert_eq!(
        apparmor_item(&lsm(), true).state,
        State::Fail,
        "service not active"
    );
    let loaded = lsm()
        .cmd("systemctl is-active apparmor.service", 0, "active\n")
        .file(
            "/sys/kernel/security/apparmor/profiles",
            "ping (enforce)\nfirefox (unconfined)\n",
        );
    let i = apparmor_item(&loaded, true);
    assert_eq!(i.state, State::Pass);
    assert!(i.summary.contains("1 of 2"), "{}", i.summary);
}

#[test]
fn sysctls_are_compared_with_the_running_kernel_per_interface() {
    let conf = "# c\nkernel.kptr_restrict = 2\nnet.ipv4.conf.*.accept_redirects = 0\nnet.core.bpf_jit_harden = 2\nnot.on.this.kernel = 1\n";
    let sys = |eth: &str| {
        Fake::default()
            .file(SYSCTL_FILE, conf)
            .file("/proc/sys/kernel/kptr_restrict", "2\n")
            .dir("/proc/sys/net/ipv4/conf", &["all", "eth0.5"])
            .file("/proc/sys/net/ipv4/conf/all/accept_redirects", "0\n")
            .file("/proc/sys/net/ipv4/conf/eth0.5/accept_redirects", eth)
            .path("/proc/sys/net/core/bpf_jit_harden")
    };
    let ok = sysctl_item(&sys("0\n"));
    assert_eq!(ok.state, State::Pass, "{ok:?}");
    assert!(
        ok.summary.starts_with("3 kernel settings"),
        "{}",
        ok.summary
    );
    assert!(ok.details[0].contains("net.core.bpf_jit_harden"));
    let bad = sysctl_item(&sys("1\n"));
    assert_eq!(bad.state, State::Warn);
    assert_eq!(
        bad.details,
        vec!["net.ipv4.conf.eth0.5.accept_redirects = 1 (default: 0)"]
    );
    assert_eq!(sysctl_item(&Fake::default()).state, State::Fail);
}

#[test]
fn firewall_states() {
    assert_eq!(firewall_items(&Fake::default())[0].state, State::Fail);
    let sys = |zone: &str| {
        Fake::default()
            .cmd("systemctl is-active firewalld", 0, "active\n")
            .cmd("firewall-cmd --get-default-zone", 0, zone)
            .cmd(
                &format!("firewall-cmd --zone={zone} --list-services"),
                0,
                "dhcpv6-client\n",
            )
            .cmd(&format!("firewall-cmd --zone={zone} --list-ports"), 0, "\n")
    };
    assert_eq!(firewall_items(&sys("public"))[0].state, State::Pass);
    assert_eq!(firewall_items(&sys("home"))[0].state, State::Warn);
    assert_eq!(firewall_items(&sys("trusted"))[0].state, State::Fail);
    let denied = Fake::default().cmd("systemctl is-active firewalld", 0, "active\n");
    assert_eq!(firewall_items(&denied)[0].state, State::Unknown);
    let tail = sys("public").path("/sys/class/net/tailscale0").cmd(
        "firewall-cmd --get-zone-of-interface=tailscale0",
        0,
        "public\n",
    );
    assert_eq!(tailnet_item(&tail).state, State::Warn);
}

#[test]
fn ssh_uses_the_effective_configuration() {
    assert_eq!(ssh_item(&Fake::default()).state, State::Pass);
    let sys = |t: &str| {
        Fake::default()
            .cmd("systemctl is-active sshd", 0, "active\n")
            .cmd("sshd -T", 0, t)
    };
    let good = "passwordauthentication no\nkbdinteractiveauthentication no\npermitrootlogin no\n";
    assert_eq!(ssh_item(&sys(good)).state, State::Pass);
    let bad = ssh_item(&sys("passwordauthentication yes\npermitrootlogin yes\n"));
    assert_eq!(bad.state, State::Fail);
    assert_eq!(bad.details.len(), 3);
}

#[test]
fn passwordless_sudo_and_unlocked_root_are_reported() {
    let sys = root()
        .cmd("passwd -S root", 0, "root P 2026-01-01\n")
        .dir("/etc/sudoers.d", &["10-wheel", "old.bak"])
        .file(
            "/etc/sudoers.d/10-wheel",
            "%wheel ALL=(ALL) NOPASSWD: ALL\n",
        )
        .file("/etc/sudoers.d/old.bak", "x ALL=(ALL) NOPASSWD: ALL\n");
    let items = account_items(&sys, true);
    assert_eq!(items[0].state, State::Warn);
    assert_eq!(items[1].details.len(), 1, "sudo ignores names with a dot");
}

#[test]
fn docker_ports_on_every_address_are_flagged() {
    let sys = Fake::default()
        .cmd("systemctl is-active docker", 0, "active\n")
        .cmd(
            "docker ps --format {{.Names}} {{.Ports}}",
            0,
            "web 0.0.0.0:80->80/tcp\ndb 127.0.0.1:5432->5432/tcp\n",
        );
    assert_eq!(docker_item(&sys).details, vec!["web 0.0.0.0:80->80/tcp"]);
}

#[test]
fn report_rendering_exit_codes_and_arguments() {
    let items = vec![
        item("a.b", State::Pass, "fine"),
        item("c.d", State::Fail, "bad \u{1b}[31m \"q\"").hint("do"),
    ];
    assert_eq!(exit_code(&items), 1);
    assert_eq!(exit_code(&items[..1]), 0);
    assert!(!render_text(&items, false).contains('\u{1b}'));
    let json = render_json(&items, true);
    assert!(
        json.contains("\"schema\":1") && json.contains("\\\"q\\\""),
        "{json}"
    );
    let args = |a: &[&str]| parse_args(&a.iter().map(|s| (*s).to_string()).collect::<Vec<_>>());
    assert!(args(&["status", "--json"]).unwrap().json);
    assert!(args(&[]).unwrap().help);
    assert!(args(&["fix"]).is_err() && args(&["status", "--yes"]).is_err());
}

#[test]
fn an_empty_system_never_panics() {
    let items = collect(&Fake::default(), true);
    assert!(items.len() >= 12);
    assert_eq!(by_id(&items, "accounts.root").state, State::Unknown);
}

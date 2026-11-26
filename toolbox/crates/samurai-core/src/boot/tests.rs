use super::*;
use crate::health::system::fake::Fake;

const CONF: &str = "timeout: 3

# Samurai OS boot entries
/Samurai OS (linux)
    protocol: linux
    path: boot():/vmlinuz-linux
    cmdline: root=UUID=abc rw rootflags=subvol=@
    module_path: boot():/initramfs-linux.img

/Snapshot 12 before update
    protocol: linux
    path: boot():/vmlinuz-linux
    cmdline: root=UUID=abc rootflags=subvol=@/.snapshots/12/snapshot
    module_path: boot():/initramfs-linux.img

/Windows
    protocol: efi_chainload
    path: boot():/EFI/Microsoft/Boot/bootmgfw.efi
";

fn k(name: &str, fallback: bool) -> Kernel {
    Kernel {
        name: name.into(),
        fallback,
    }
}

#[test]
fn in_sync_menu_is_returned_unchanged() {
    let (text, c) = plan_limine(CONF, &[k("linux", false)]).unwrap();
    assert!(c.is_empty());
    assert_eq!(text, CONF);
}

#[test]
fn a_new_kernel_gets_entries_with_the_existing_command_line() {
    let (text, c) = plan_limine(CONF, &[k("linux", false), k("linux-zen", true)]).unwrap();
    assert_eq!(
        c.added,
        vec![
            "Samurai OS (linux-zen)",
            "Samurai OS (linux-zen, fallback initramfs)"
        ]
    );
    assert!(c.removed.is_empty());
    assert!(text.contains("/Samurai OS (linux-zen)\n    protocol: linux\n    path: boot():/vmlinuz-linux-zen\n    cmdline: root=UUID=abc rw rootflags=subvol=@\n    module_path: boot():/initramfs-linux-zen.img\n"), "{text}");
    assert!(text.contains("module_path: boot():/initramfs-linux-zen-fallback.img"));
    // the default (first) entry and the foreign ones keep their place and text
    assert!(text.starts_with("timeout: 3\n"));
    assert!(text.find("/Samurai OS (linux)").unwrap() < text.find("/Snapshot 12").unwrap());
    let windows = CONF.split("/Windows").nth(1).unwrap().trim_end();
    assert!(text.contains(windows));
    // idempotent
    let (again, c2) = plan_limine(&text, &[k("linux", false), k("linux-zen", true)]).unwrap();
    assert!(c2.is_empty());
    assert_eq!(again, text);
}

#[test]
fn removed_kernels_lose_only_their_own_entries() {
    let (two, _) = plan_limine(CONF, &[k("linux", false), k("linux-zen", false)]).unwrap();
    let (text, c) = plan_limine(&two, &[k("linux-zen", false)]).unwrap();
    assert_eq!(c.removed, vec!["Samurai OS (linux)"]);
    assert!(!text.contains("/Samurai OS (linux)\n"), "{text}");
    assert!(
        text.contains("/Snapshot 12") && text.contains("/Windows"),
        "foreign entries stay"
    );
    assert!(text.contains("/Samurai OS (linux-zen)"));
}

fn titles(conf: &str) -> Vec<&str> {
    conf.lines().filter(|l| l.starts_with('/')).collect()
}

#[test]
fn removing_the_default_kernel_never_hands_the_default_to_a_foreign_entry() {
    // found on a VM: install linux-lts, remove linux, reboot -> the menu booted the next foreign entry
    let (two, _) = plan_limine(CONF, &[k("linux", false), k("linux-lts", false)]).unwrap();
    assert_eq!(
        titles(&two),
        vec![
            "/Samurai OS (linux)",
            "/Samurai OS (linux-lts)",
            "/Snapshot 12 before update",
            "/Windows"
        ],
        "new kernels sit with ours, not behind foreign entries"
    );
    let (one, _) = plan_limine(&two, &[k("linux-lts", false)]).unwrap();
    assert_eq!(titles(&one)[0], "/Samurai OS (linux-lts)");
    // the same when the menu already had our other kernel after the foreign entries
    let scattered = format!("{CONF}\n/Samurai OS (linux-lts)\n    protocol: linux\n    path: boot():/vmlinuz-linux-lts\n    cmdline: x\n");
    let (moved, _) = plan_limine(&scattered, &[k("linux-lts", false)]).unwrap();
    assert_eq!(
        titles(&moved),
        vec![
            "/Samurai OS (linux-lts)",
            "/Snapshot 12 before update",
            "/Windows"
        ]
    );
    // and when the only kernel is swapped, the new entry takes the old one's place
    let (swapped, _) = plan_limine(CONF, &[k("linux-zen", false)]).unwrap();
    assert_eq!(titles(&swapped)[0], "/Samurai OS (linux-zen)");
}

#[test]
fn a_default_the_user_chose_is_left_alone() {
    let user_first = "timeout: 3\n/Windows\n    protocol: efi_chainload\n\n/Samurai OS (linux)\n    protocol: linux\n    path: boot():/vmlinuz-linux\n    cmdline: x\n";
    let (text, _) = plan_limine(user_first, &[k("linux-zen", false)]).unwrap();
    assert_eq!(titles(&text), vec!["/Windows", "/Samurai OS (linux-zen)"]);
}

#[test]
fn replacing_the_only_kernel_still_copies_the_command_line() {
    let (text, c) = plan_limine(CONF, &[k("linux-lts", false)]).unwrap();
    assert_eq!((c.added.len(), c.removed.len()), (1, 1));
    assert!(text.contains("cmdline: root=UUID=abc rw rootflags=subvol=@\n    module_path: boot():/initramfs-linux-lts.img"));
}

#[test]
fn refuses_to_empty_the_menu_or_invent_a_command_line() {
    assert!(plan_limine(CONF, &[]).unwrap_err().contains("refusing"));
    let foreign = "timeout: 3\n/Windows\n    protocol: efi_chainload\n";
    assert!(plan_limine(foreign, &[k("linux", false)])
        .unwrap_err()
        .contains("command line"));
}

#[test]
fn lookalike_titles_are_not_ours() {
    let conf = format!(
        "{CONF}\n/Samurai OS (custom)\n    protocol: linux\n    path: boot():/my/own/kernel\n"
    );
    let (text, c) = plan_limine(&conf, &[k("linux", false)]).unwrap();
    assert!(c.is_empty(), "{c:?}");
    assert!(text.contains("/Samurai OS (custom)"));
}

fn machine(kernels: &[&str]) -> Fake {
    let mut f = Fake::default()
        .dir("/boot", &["limine"])
        .dir("/usr/lib/modules", kernels);
    for name in kernels {
        f = f
            .file(
                &format!("/usr/lib/modules/{name}/pkgbase"),
                &format!("{name}\n"),
            )
            .path(&format!("/boot/vmlinuz-{name}"))
            .path(&format!("/boot/initramfs-{name}.img"));
    }
    f
}

#[test]
fn bootable_kernels_need_image_initramfs_and_a_sane_name() {
    let f = machine(&["linux"])
        .dir("/usr/lib/modules", &["linux", "noimg", "evil"])
        .file("/usr/lib/modules/noimg/pkgbase", "noimg\n")
        .file("/usr/lib/modules/evil/pkgbase", "../x y\n");
    assert_eq!(bootable_kernels(&f), vec![k("linux", false)]);
}

#[test]
fn sync_writes_a_backup_then_the_new_menu_and_check_writes_nothing() {
    let sys = machine(&["linux", "linux-zen"]).file(LIMINE_CONF, CONF);
    let Outcome::Changed(lines) = sync(&sys, false).unwrap() else {
        panic!()
    };
    assert_eq!(lines, vec!["add entry: Samurai OS (linux-zen)"]);
    assert!(sys.writes().is_empty(), "--check must not write");
    sync(&sys, true).unwrap();
    let w = sys.writes();
    assert_eq!(w.len(), 2);
    assert_eq!(w[0], (format!("{LIMINE_CONF}.bak"), CONF.to_string()));
    assert_eq!(w[1].0, LIMINE_CONF);
    assert!(w[1].1.contains("/Samurai OS (linux-zen)"));
}

#[test]
fn unreadable_boot_or_no_bootloader_never_writes() {
    let locked = Fake::default().file(LIMINE_CONF, CONF);
    assert!(sync(&locked, true).unwrap_err().contains("cannot be read"));
    assert!(locked.writes().is_empty());
    let none = Fake::default().dir("/boot", &["x"]);
    assert!(matches!(sync(&none, true).unwrap(), Outcome::NotManaged(_)));
    // ESP mounted but no kernel images: refuse rather than delete every entry
    let empty = Fake::default()
        .dir("/boot", &["limine"])
        .file(LIMINE_CONF, CONF);
    assert!(sync(&empty, true).is_err());
    assert!(empty.writes().is_empty());
}

#[test]
fn systemd_boot_adds_and_removes_only_samurai_files() {
    let entry = "title   Samurai OS (linux)\nlinux   /vmlinuz-linux\ninitrd  /initramfs-linux.img\noptions root=UUID=abc rw\n";
    let sys = machine(&["linux-zen"])
        .file(SD_LOADER, "timeout 3\n")
        .dir(SD_ENTRIES, &["samurai-linux.conf", "windows.conf"])
        .file(&format!("{SD_ENTRIES}/samurai-linux.conf"), entry);
    sync(&sys, true).unwrap();
    let w = sys.writes();
    assert_eq!(w[0].0, format!("{SD_ENTRIES}/samurai-linux-zen.conf"));
    assert!(w[0].1.contains("options root=UUID=abc rw\n"), "{}", w[0].1);
    assert_eq!(
        w[1],
        (
            format!("{SD_ENTRIES}/samurai-linux.conf"),
            "<removed>".to_string()
        ),
        "new entry first, then removal"
    );
    assert_eq!(w.len(), 2, "windows.conf untouched");
}

#[test]
fn grub_regenerates_only_when_the_kernel_set_differs() {
    let cfg = "linux /vmlinuz-linux root=x\ninitrd /initramfs-linux.img\n";
    let ok = machine(&["linux"]).file(GRUB_CFG, cfg);
    assert_eq!(sync(&ok, true).unwrap(), Outcome::InSync);
    for kernels in [&["linux", "linux-zen"][..], &["linux-zen"]] {
        let sys = machine(kernels).file(GRUB_CFG, cfg).cmd(
            &format!("grub-mkconfig -o {GRUB_CFG}"),
            0,
            "",
        );
        assert!(
            matches!(sync(&sys, true).unwrap(), Outcome::Changed(_)),
            "{kernels:?}"
        );
        assert_eq!(sys.calls(), vec![format!("grub-mkconfig -o {GRUB_CFG}")]);
    }
}

#[test]
fn argument_parsing() {
    let a = |v: &[&str]| v.iter().map(|x| (*x).to_string()).collect::<Vec<_>>();
    assert_eq!(parse_args(&a(&["sync"])), Ok(Cmd::Sync { check: false }));
    assert_eq!(
        parse_args(&a(&["sync", "--check"])),
        Ok(Cmd::Sync { check: true })
    );
    assert_eq!(parse_args(&a(&["--help"])), Ok(Cmd::Help));
    assert_eq!(parse_args(&a(&["get"])), Ok(Cmd::Get { json: false }));
    assert_eq!(
        parse_args(&a(&["get", "--json"])),
        Ok(Cmd::Get { json: true })
    );
    assert_eq!(
        parse_args(&a(&["set", "timeout", "5"])),
        Ok(Cmd::Set(settings::Change::Timeout(5)))
    );
    assert_eq!(
        parse_args(&a(&["set", "param", "add", "quiet"])),
        Ok(Cmd::Set(settings::Change::Add("quiet".into())))
    );
    for bad in [
        &["set"][..],
        &["set", "timeout", "x"],
        &["set", "param", "quiet"],
        &["get", "x"],
    ] {
        assert!(parse_args(&a(bad)).is_err(), "{bad:?}");
    }
    for bad in [
        &[][..],
        &["nope"],
        &["sync", "x"],
        &["sync", "--check", "x"],
    ] {
        assert!(parse_args(&a(bad)).is_err(), "{bad:?}");
    }
}

#[test]
fn choosing_a_default_moves_only_that_entry_to_the_front() {
    let (two, _) = plan_limine(CONF, &[k("linux", false), k("linux-lts", true)]).unwrap();
    let new = limine_with_default(&two, "linux-lts").unwrap();
    assert_eq!(
        titles(&new)[0],
        "/Samurai OS (linux-lts)",
        "the normal entry, not the fallback"
    );
    let mut a = titles(&two);
    let mut b = titles(&new);
    a.sort_unstable();
    b.sort_unstable();
    assert_eq!(a, b, "nothing added or lost");
    assert!(new.starts_with("timeout: 3\n"));
    assert!(limine_with_default(CONF, "linux-zen")
        .unwrap_err()
        .contains("boot sync"));

    let sys = machine(&["linux", "linux-lts"]).file(LIMINE_CONF, &two);
    assert_eq!(default_kernel(&sys).as_deref(), Some("linux"));
    set_default(&sys, "linux-lts").unwrap();
    assert_eq!(sys.writes()[0].0, format!("{LIMINE_CONF}.bak"));
    assert_eq!(sys.writes()[1].1, new);
    assert!(set_default(&sys, "linux-zen")
        .unwrap_err()
        .contains("not an installed"));
    // a foreign first entry means no kernel of ours is the default
    let foreign = Fake::default().file(
        LIMINE_CONF,
        "timeout: 3\n/Windows\n    protocol: efi_chainload\n",
    );
    assert_eq!(default_kernel(&foreign), None);
}

#[test]
fn systemd_boot_default_is_written_to_loader_conf() {
    let sys = machine(&["linux"])
        .file(SD_LOADER, "timeout 3\ndefault @saved\n")
        .path(&format!("{SD_ENTRIES}/samurai-linux.conf"));
    set_default(&sys, "linux").unwrap();
    assert_eq!(
        sys.writes(),
        vec![(
            SD_LOADER.to_string(),
            "timeout 3\ndefault samurai-linux.conf\n".to_string()
        )]
    );
}

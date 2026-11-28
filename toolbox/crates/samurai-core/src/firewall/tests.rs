use super::*;
use crate::health::system::fake::Fake;

fn args(a: &[&str]) -> Result<Cmd, String> {
    parse_args(&a.iter().map(|s| (*s).to_string()).collect::<Vec<_>>())
}

#[test]
fn ports_are_strictly_validated() {
    assert_eq!(parse_port("8080/tcp").as_deref(), Some("8080/tcp"));
    assert_eq!(
        parse_port("1714-1764/udp").as_deref(),
        Some("1714-1764/udp")
    );
    for bad in [
        "8080",
        "0/tcp",
        "65536/tcp",
        "80/sctp",
        "90-80/tcp",
        "+80/tcp",
        "80-/tcp",
        "/tcp",
        "80/tcp ",
        "80/tcp;reboot",
    ] {
        assert_eq!(parse_port(bad), None, "{bad}");
    }
}

#[test]
fn arguments() {
    assert_eq!(args(&["status", "--json"]), Ok(Cmd::Status { json: true }));
    assert_eq!(
        args(&["open", "kdeconnect", "--zone=home", "--yes"]),
        Ok(Cmd::Change {
            open: true,
            target: Target::Preset("kdeconnect"),
            zone: Some("home".into()),
            flags: Flags {
                dry_run: false,
                yes: true
            },
        })
    );
    assert!(matches!(
        args(&["close", "8080/tcp", "--zone", "work", "--dry-run"]),
        Ok(Cmd::Change { open: false, .. })
    ));
    assert!(matches!(
        args(&["zone", "wlan0", "home"]),
        Ok(Cmd::Zone { .. })
    ));
    for bad in [
        &["open", "nonsense"][..],
        &["open", "ssh", "--zone", "--permanent"],
        &["open", "ssh", "--zone=a b"],
        &["zone", "wlan0;x", "home"],
        &["zone", "-x", "home"],
        &["status", "--yes"],
        &["open"],
    ] {
        assert!(args(bad).is_err(), "{bad:?}");
    }
}

fn running() -> Fake {
    Fake::default()
        .cmd("systemctl is-active firewalld", 0, "active\n")
        .cmd("firewall-cmd --get-default-zone", 0, "public\n")
        .cmd(
            "firewall-cmd --get-zones",
            0,
            "home public samurai-tailnet trusted\n",
        )
        .cmd(
            "firewall-cmd --info-service=samurai-ollama",
            0,
            "samurai-ollama\n  ports: 11434/tcp\n  protocols: \n",
        )
}

#[test]
fn opening_previews_then_changes_permanent_first_and_verifies() {
    let permanent = "firewall-cmd --permanent --zone=public --query-service=samurai-ollama";
    let runtime = "firewall-cmd --zone=public --query-service=samurai-ollama";
    let sys = running()
        .cmd(permanent, 1, "no\n")
        .cmd(permanent, 0, "yes\n")
        .cmd(runtime, 1, "no\n")
        .cmd(runtime, 0, "yes\n")
        .cmd(
            "firewall-cmd --permanent --zone=public --add-service=samurai-ollama",
            0,
            "success",
        )
        .cmd(
            "firewall-cmd --zone=public --add-service=samurai-ollama",
            0,
            "success",
        );
    let p = plan(&sys, true, &Target::Preset("ollama"), None).unwrap();
    let text = p.preview.join("\n");
    assert!(
        text.contains("11434/tcp")
            && text.contains("no authentication")
            && text.contains("public Wi-Fi"),
        "{text}"
    );
    assert_eq!(p.commands()[0][0], "--permanent");
    assert!(
        !sys.calls().iter().any(|c| c.contains("--add-")),
        "a plan changes nothing"
    );
    apply(&sys, &p).unwrap();
    assert!(sys
        .calls()
        .iter()
        .any(|c| c.ends_with("--add-service=samurai-ollama")));
    assert!(!sys
        .calls()
        .iter()
        .any(|c| c.contains("firewall-cmd --reload")));
}

#[test]
fn closing_something_closed_is_a_no_op_and_errors_surface() {
    let sys = running()
        .cmd(
            "firewall-cmd --permanent --zone=home --query-port=80/tcp",
            1,
            "no\n",
        )
        .cmd("firewall-cmd --zone=home --query-port=80/tcp", 1, "no\n");
    let p = plan(&sys, false, &Target::Port("80/tcp".into()), Some("home")).unwrap();
    assert!(p.steps.is_empty() && p.commands().is_empty());
    assert!(
        plan(&sys, true, &Target::Port("80/tcp".into()), Some("nowhere"))
            .unwrap_err()
            .contains("no zone")
    );
    let unauthorised = running().cmd(
        "firewall-cmd --permanent --zone=public --query-port=80/tcp",
        253,
        "",
    );
    assert!(
        plan(&unauthorised, true, &Target::Port("80/tcp".into()), None)
            .unwrap_err()
            .contains("needs root")
    );
    assert!(plan(&running(), true, &Target::Preset("sunshine"), None)
        .unwrap_err()
        .contains("firewall-cmd --reload"));
    assert!(plan(&Fake::default(), true, &Target::Preset("ssh"), None)
        .unwrap_err()
        .contains("not running"));
}

#[test]
fn a_change_that_did_not_stick_is_an_error() {
    let sys = running()
        .cmd(
            "firewall-cmd --permanent --zone=public --query-port=80/tcp",
            1,
            "no\n",
        )
        .cmd("firewall-cmd --zone=public --query-port=80/tcp", 1, "no\n")
        .cmd(
            "firewall-cmd --permanent --zone=public --add-port=80/tcp",
            0,
            "",
        )
        .cmd("firewall-cmd --zone=public --add-port=80/tcp", 0, "");
    let p = plan(&sys, true, &Target::Port("80/tcp".into()), None).unwrap();
    assert!(apply(&sys, &p).unwrap_err().contains("still closed"));
}

#[test]
fn zone_change_needs_an_existing_interface_and_zone() {
    assert!(zone_commands(&running(), "wlan9", "home")
        .unwrap_err()
        .contains("no network interface"));
    let sys = running().path("/sys/class/net/wlan0");
    assert_eq!(
        zone_commands(&sys, "wlan0", "home").unwrap(),
        vec![
            vec!["--permanent", "--zone=home", "--change-interface=wlan0"],
            vec!["--zone=home", "--change-interface=wlan0"],
        ]
    );
}

#[test]
fn samurai_services_named_by_presets_are_shipped() {
    let dir = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../pkgbuilds/samurai-settings-security"
    );
    if !std::path::Path::new(dir).is_dir() {
        return; // package build stages only toolbox/
    }
    for p in PRESETS {
        for s in p.services.iter().filter(|s| s.starts_with("samurai-")) {
            assert!(
                std::path::Path::new(&format!("{dir}/{s}.xml")).is_file(),
                "{s}"
            );
        }
    }
}

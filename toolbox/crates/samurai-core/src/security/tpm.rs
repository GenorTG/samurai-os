//! TPM2 + PIN unlock of the encrypted root, and the recovery key (spec 09).
//!
//! The rule that prevents lock-outs: a keyslot that works without the TPM (passphrase or recovery
//! key) must exist before and after every change, and nothing here ever wipes anything but the TPM
//! slot. When the TPM refuses (new firmware, changed Secure Boot keys, another board, too many
//! wrong PINs) the boot falls back to asking for the passphrase.

use super::json::{self, Json};
use super::{is_root, root_stack};
use crate::health::system::System;

/// libcryptsetup loads this to unlock with a systemd-tpm2 token; the initramfs needs it too.
const TOKEN_PLUGIN: &str = "/usr/lib/cryptsetup/libcryptsetup-token-systemd-tpm2.so";
const PLUGIN_NAME: &str = "libcryptsetup-token-systemd-tpm2.so";
const TPM_VERSION: &str = "/sys/class/tpm/tpm0/tpm_version_major";
/// Tokens whose keyslot needs a device: they are not a way back in.
const HARDWARE_TOKENS: [&str; 3] = ["systemd-tpm2", "systemd-fido2", "systemd-pkcs11"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Enroll,
    Remove,
    RecoveryKey,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Header {
    pub keyslots: Vec<u32>,
    /// (token type, its keyslots, `tpm2-pin`)
    pub tokens: Vec<(String, Vec<u32>, bool)>,
}

impl Header {
    /// Keyslots that open the disk without any device.
    pub fn fallback_slots(&self) -> Vec<u32> {
        self.keyslots
            .iter()
            .copied()
            .filter(|slot| {
                !self.tokens.iter().any(|(kind, slots, _)| {
                    slots.contains(slot) && HARDWARE_TOKENS.contains(&kind.as_str())
                })
            })
            .collect()
    }

    /// `Some(pin required)` when a TPM token is enrolled; `false` if any of them needs no PIN.
    pub fn tpm(&self) -> Option<bool> {
        self.tokens
            .iter()
            .filter(|(kind, _, _)| kind == "systemd-tpm2")
            .map(|(_, _, pin)| *pin)
            .reduce(|a, b| a && b)
    }
}

/// Parse `cryptsetup luksDump --dump-json-metadata` (the LUKS2 JSON area, a stable format).
pub fn parse_header(metadata: &str) -> Option<Header> {
    let root = json::parse(metadata.trim())?;
    let slot = |text: &str| text.parse::<u32>().ok();
    let keyslots = root
        .get("keyslots")?
        .entries()
        .iter()
        .map(|(n, _)| slot(n))
        .collect::<Option<Vec<_>>>()?;
    let tokens = root
        .get("tokens")?
        .entries()
        .iter()
        .map(|(_, token)| {
            let kind = token.get("type")?.text()?.to_string();
            let slots = match token.get("keyslots") {
                Some(Json::List(items)) => items
                    .iter()
                    .map(|i| i.text().and_then(slot))
                    .collect::<Option<Vec<_>>>()?,
                _ => return None,
            };
            let pin = token.get("tpm2-pin") == Some(&Json::Bool(true));
            Some((kind, slots, pin))
        })
        .collect::<Option<Vec<_>>>()?;
    Some(Header { keyslots, tokens })
}

/// The LUKS2 header of `device`. Anything unreadable or unexpected is an error, never "empty".
pub fn header(sys: &dyn System, device: &str) -> Result<Header, String> {
    let dump = sys
        .run("cryptsetup", &["luksDump", "--dump-json-metadata", device])
        .filter(|o| o.code == 0)
        .ok_or(format!("cannot read the LUKS2 header of {device}"))?;
    parse_header(&dump.stdout).ok_or(format!("unexpected LUKS2 metadata on {device}"))
}

fn backing_device(sys: &dyn System) -> Result<String, String> {
    match root_stack(sys) {
        Some((true, Some(device))) => Ok(device),
        Some(_) => Err("the root filesystem is not encrypted: there is nothing to unlock".into()),
        None => Err("cannot inspect the root device".into()),
    }
}

/// Everything that would make an enrolment useless or leave the next boot unable to use it.
fn boot_chain_ready(sys: &dyn System) -> Result<(), String> {
    if sys.read(TPM_VERSION).map(|v| v.trim() == "2") != Some(true) {
        return Err("no TPM 2.0 found (enable it in the firmware setup: TPM / fTPM / PTT)".into());
    }
    let cmdline = sys.read("/proc/cmdline").unwrap_or_default();
    if cmdline
        .split_whitespace()
        .any(|w| w.starts_with("rd.luks.key="))
    {
        return Err(
            "the disk already opens with a key file (rd.luks.key): a TPM PIN would protect nothing"
                .into(),
        );
    }
    if !sys.exists(TOKEN_PLUGIN) {
        return Err(format!("{TOKEN_PLUGIN} is missing (systemd's TPM plugin)"));
    }
    let images: Vec<String> = sys
        .list_dir("/boot")
        .into_iter()
        .filter(|f| f.starts_with("initramfs-") && f.ends_with(".img") && !f.contains("-fallback"))
        .collect();
    if images.is_empty() {
        return Err("no initramfs found in /boot".into());
    }
    for image in images {
        let path = format!("/boot/{image}");
        let listing = sys
            .run("lsinitcpio", &[&path])
            .filter(|o| o.code == 0)
            .ok_or(format!("cannot list {path}"))?;
        let has = |name: &str| listing.stdout.lines().any(|l| l.ends_with(name));
        if !has(PLUGIN_NAME) || !has("systemd-cryptsetup") {
            return Err(format!(
                "{path} cannot unlock with the TPM (needs the systemd and sd-encrypt hooks and tpm2-tss): fix /etc/mkinitcpio.conf and run mkinitcpio -P"
            ));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub device: String,
    pub preview: Vec<String>,
    /// Arguments of `systemd-cryptenroll`.
    pub args: Vec<String>,
}

pub fn plan(sys: &dyn System, action: Action) -> Result<Plan, String> {
    if !is_root(sys) {
        return Err("needs root (try: sudo samurai security ...)".into());
    }
    let device = backing_device(sys)?;
    let head = header(sys, &device)?;
    if head.fallback_slots().is_empty() {
        return Err(format!(
            "{device} has no passphrase or recovery key slot: refusing to touch it"
        ));
    }
    let (preview, flags): (Vec<String>, &[&str]) = match action {
        Action::Enroll => {
            boot_chain_ready(sys)?;
            (
                vec![
                    format!("Let the TPM of this machine open {device} after you type a PIN."),
                    "Your passphrase keeps working and is what you type if the TPM refuses (firmware update, Secure Boot key change, new mainboard, too many wrong PINs).".into(),
                    "Bound to PCR 7 (Secure Boot state). Without Secure Boot this does not detect a replaced boot loader.".into(),
                    "You will be asked for the passphrase, then twice for the new PIN.".into(),
                ],
                &["--wipe-slot=tpm2", "--tpm2-device=auto", "--tpm2-pcrs=7", "--tpm2-with-pin=yes"],
            )
        }
        Action::Remove => {
            if head.tpm().is_none() {
                return Err("TPM unlock is not enrolled".into());
            }
            (
                vec![format!(
                    "Remove the TPM keyslot from {device}. The passphrase and recovery key stay."
                )],
                &["--wipe-slot=tpm2"],
            )
        }
        Action::RecoveryKey => (
            vec![
                format!("Add a recovery key to {device}."),
                "It is shown ONCE: write it down and keep it away from this machine.".into(),
            ],
            &["--recovery-key"],
        ),
    };
    let mut args: Vec<String> = flags.iter().map(|f| (*f).to_string()).collect();
    args.push(device.clone());
    Ok(Plan {
        device,
        preview,
        args,
    })
}

/// Run systemd-cryptenroll on the terminal, then check the header says what was promised.
pub fn apply(sys: &dyn System, action: Action, plan: &Plan) -> Result<String, String> {
    let before = header(sys, &plan.device)?.fallback_slots();
    let args: Vec<&str> = plan.args.iter().map(String::as_str).collect();
    match sys.run_visible("systemd-cryptenroll", &args) {
        Some(0) => {}
        Some(code) => return Err(format!("systemd-cryptenroll exited {code}")),
        None => return Err("systemd-cryptenroll is not installed".into()),
    }
    let after = header(sys, &plan.device)?;
    if !before.iter().all(|s| after.fallback_slots().contains(s)) {
        return Err("a passphrase or recovery slot disappeared: do NOT reboot before adding one (cryptsetup luksAddKey)".into());
    }
    match (action, after.tpm()) {
        (Action::Enroll, Some(true)) => Ok(
            "TPM unlock with PIN is enrolled. Reboot to use it; the passphrase still works (empty PIN or wait for the prompt).\nIf it ever stops working: boot with the passphrase, then run this command again."
                .into(),
        ),
        (Action::Enroll, _) => Err("the header shows no TPM token with a PIN".into()),
        (Action::Remove, None) => Ok("TPM unlock removed; the disk asks for the passphrase.".into()),
        (Action::Remove, Some(_)) => Err("the TPM token is still there".into()),
        (Action::RecoveryKey, _) if after.fallback_slots().len() > before.len() => {
            Ok("recovery key added".into())
        }
        (Action::RecoveryKey, _) => Err("no new keyslot appeared".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::health::system::fake::Fake;

    const PASS_SLOT: &str = r#""0":{"type":"luks2","kdf":{"salt":"AAAA"}},"#;
    const DUMP: &str = r#"{"keyslots":{"0":{"type":"luks2","kdf":{"salt":"AAAA"}},"1":{"type":"luks2"}},"tokens":{"0":{"type":"systemd-tpm2","keyslots":["1"],"tpm2-blob":"x","tpm2-pin":true}},"segments":{},"digests":{},"config":{}}"#;
    const LUKSDUMP: &str = "cryptsetup luksDump --dump-json-metadata /dev/vda2";

    #[test]
    fn header_parsing_finds_the_way_back_in() {
        let h = parse_header(DUMP).unwrap();
        assert_eq!(h.keyslots, [0, 1]);
        assert_eq!(h.fallback_slots(), [0]);
        assert_eq!(h.tpm(), Some(true));
        let only_tpm = parse_header(&DUMP.replace(PASS_SLOT, "")).unwrap();
        assert!(only_tpm.fallback_slots().is_empty());
        let recovery = parse_header(&DUMP.replace("systemd-tpm2", "systemd-recovery")).unwrap();
        assert_eq!(recovery.fallback_slots(), [0, 1]);
        let no_pin = DUMP.replace("\"tpm2-pin\":true", "\"tpm2-pin\":false");
        assert_eq!(parse_header(&no_pin).unwrap().tpm(), Some(false));
        assert_eq!(parse_header("LUKS header information"), None);
        assert_eq!(
            parse_header("{}"),
            None,
            "no keyslots object is not an empty header"
        );
    }

    fn system(dump: &str) -> Fake {
        Fake::default()
            .cmd("id -u", 0, "0\n")
            .cmd("findmnt -no SOURCE /", 0, "/dev/mapper/luks-1[/@]\n")
            .cmd(
                "lsblk -lspno NAME,TYPE /dev/mapper/luks-1",
                0,
                "/dev/mapper/luks-1 crypt\n/dev/vda2 part\n",
            )
            .cmd(LUKSDUMP, 0, dump)
            .file(TPM_VERSION, "2\n")
            .file("/proc/cmdline", "root=UUID=x rd.luks.uuid=y\n")
            .path(TOKEN_PLUGIN)
            .dir("/boot", &["initramfs-linux.img", "vmlinuz-linux"])
            .cmd(
                "lsinitcpio /boot/initramfs-linux.img",
                0,
                "usr/lib/cryptsetup/libcryptsetup-token-systemd-tpm2.so\nusr/lib/systemd/systemd-cryptsetup\n",
            )
    }

    #[test]
    fn enrolment_only_ever_wipes_the_tpm_slot() {
        let p = plan(&system(DUMP), Action::Enroll).unwrap();
        assert!(p.args.contains(&"--tpm2-with-pin=yes".to_string()));
        assert_eq!(p.args.last().map(String::as_str), Some("/dev/vda2"));
        for action in [Action::Enroll, Action::Remove, Action::RecoveryKey] {
            let p = plan(&system(DUMP), action).unwrap();
            for a in &p.args {
                assert!(
                    !a.starts_with("--wipe-slot=") || a == "--wipe-slot=tpm2",
                    "{a}"
                );
            }
        }
    }

    #[test]
    fn refuses_whatever_could_lock_the_user_out_or_do_nothing() {
        let refuse = |sys: Fake, needle: &str| {
            let e = plan(&sys, Action::Enroll).unwrap_err();
            assert!(e.contains(needle), "{e}");
        };
        let no_slot = DUMP.replace(PASS_SLOT, "");
        refuse(system(&no_slot), "no passphrase");
        refuse(system("not json"), "unexpected LUKS2 metadata");
        refuse(Fake::default().cmd("id -u", 0, "1000\n"), "needs root");
        // the first scripted answer of a command wins, so build the variants from scratch
        let base = || {
            Fake::default()
                .cmd("id -u", 0, "0\n")
                .cmd("findmnt -no SOURCE /", 0, "/dev/mapper/luks-1\n")
                .cmd(
                    "lsblk -lspno NAME,TYPE /dev/mapper/luks-1",
                    0,
                    "/dev/mapper/luks-1 crypt\n/dev/vda2 part\n",
                )
                .cmd(LUKSDUMP, 0, DUMP)
        };
        refuse(base(), "no TPM 2.0");
        refuse(
            base()
                .file(TPM_VERSION, "2\n")
                .file("/proc/cmdline", "rd.luks.key=/etc/k\n"),
            "key file",
        );
        refuse(
            base()
                .file(TPM_VERSION, "2\n")
                .path(TOKEN_PLUGIN)
                .dir("/boot", &["initramfs-linux.img"])
                .cmd(
                    "lsinitcpio /boot/initramfs-linux.img",
                    0,
                    "usr/bin/busybox\n",
                ),
            "sd-encrypt",
        );
    }

    #[test]
    fn a_vanished_passphrase_slot_is_an_error() {
        let gone = DUMP.replace(PASS_SLOT, "");
        let sys = system(DUMP)
            .cmd(LUKSDUMP, 0, DUMP)
            .cmd(LUKSDUMP, 0, &gone)
            .cmd("systemd-cryptenroll --wipe-slot=tpm2 /dev/vda2", 0, "");
        let p = plan(&sys, Action::Remove).unwrap();
        assert!(apply(&sys, Action::Remove, &p)
            .unwrap_err()
            .contains("do NOT reboot"));
    }
}

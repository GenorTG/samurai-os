//! TPM2 + PIN unlock of the encrypted root, and the recovery key (spec 09).
//!
//! The rule that prevents lock-outs: a keyslot that works without the TPM (passphrase or recovery
//! key) must exist before and after every change, and nothing here ever wipes anything but the TPM
//! slot. When the TPM refuses (new firmware, changed Secure Boot keys, another board, too many
//! wrong PINs) the boot falls back to asking for the passphrase.

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
    pub luks2: bool,
    pub keyslots: Vec<u32>,
    /// (token type, keyslot, `tpm2-pin`)
    pub tokens: Vec<(String, Option<u32>, bool)>,
}

impl Header {
    /// Keyslots that open the disk without any device.
    pub fn fallback_slots(&self) -> Vec<u32> {
        self.keyslots
            .iter()
            .copied()
            .filter(|slot| {
                !self.tokens.iter().any(|(kind, k, _)| {
                    *k == Some(*slot) && HARDWARE_TOKENS.contains(&kind.as_str())
                })
            })
            .collect()
    }

    pub fn tpm(&self) -> Option<bool> {
        self.tokens
            .iter()
            .find(|(kind, _, _)| kind == "systemd-tpm2")
            .map(|(_, _, pin)| *pin)
    }
}

/// Parse `cryptsetup luksDump` of a LUKS2 header.
pub fn parse_header(dump: &str) -> Header {
    let mut header = Header::default();
    let mut section = "";
    for line in dump.lines() {
        if !line.starts_with(char::is_whitespace) {
            section = line.split(':').next().unwrap_or("");
            if section == "Version" {
                header.luks2 = line.split_whitespace().nth(1) == Some("2");
            }
            continue;
        }
        // "  0: luks2" starts an entry; deeper "\tName: value" lines belong to it
        let entry = line
            .strip_prefix("  ")
            .filter(|rest| !rest.starts_with(char::is_whitespace))
            .and_then(|rest| rest.split_once(':'))
            .and_then(|(n, kind)| Some((n.parse::<u32>().ok()?, kind.trim())));
        match (section, entry) {
            ("Keyslots", Some((n, _))) => header.keyslots.push(n),
            ("Tokens", Some((_, kind))) => header.tokens.push((kind.to_string(), None, false)),
            ("Tokens", None) => {
                let Some(token) = header.tokens.last_mut() else {
                    continue;
                };
                let Some((name, value)) = line.trim().split_once(':') else {
                    continue;
                };
                match name {
                    "Keyslot" => token.1 = value.trim().parse().ok(),
                    "tpm2-pin" => token.2 = value.trim() == "true",
                    _ => {}
                }
            }
            _ => {}
        }
    }
    header
}

fn header(sys: &dyn System, device: &str) -> Result<Header, String> {
    let dump = sys
        .run("cryptsetup", &["luksDump", device])
        .filter(|o| o.code == 0)
        .ok_or(format!("cannot read the LUKS header of {device}"))?;
    Ok(parse_header(&dump.stdout))
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
    if !head.luks2 {
        return Err(format!("{device} is not LUKS2"));
    }
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

    const DUMP: &str = "LUKS header information\nVersion:       \t2\nEpoch:         \t7\n\nData segments:\n  0: crypt\n\toffset: 16777216 [bytes]\n\nKeyslots:\n  0: luks2\n\tKey:        512 bits\n\tPriority:   normal\n  1: luks2\n\tKey:        512 bits\nTokens:\n  0: systemd-tpm2\n\ttpm2-hash-pcrs:   7\n\ttpm2-pin:         true\n\tKeyslot:    1\nDigests:\n  0: pbkdf2\n\tHash:       sha256\n";

    #[test]
    fn header_parsing_finds_the_way_back_in() {
        let h = parse_header(DUMP);
        assert!(h.luks2);
        assert_eq!(h.keyslots, [0, 1]);
        assert_eq!(h.fallback_slots(), [0]);
        assert_eq!(h.tpm(), Some(true));
        let only_tpm = parse_header(&DUMP.replace(
            "  0: luks2\n\tKey:        512 bits\n\tPriority:   normal\n",
            "",
        ));
        assert!(only_tpm.fallback_slots().is_empty());
        let recovery = parse_header(&DUMP.replace("systemd-tpm2", "systemd-recovery"));
        assert_eq!(recovery.fallback_slots(), [0, 1]);
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
            .cmd("cryptsetup luksDump /dev/vda2", 0, dump)
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
        let no_slot = DUMP.replace(
            "  0: luks2\n\tKey:        512 bits\n\tPriority:   normal\n",
            "",
        );
        refuse(system(&no_slot), "no passphrase");
        refuse(
            system(&DUMP.replace("Version:       \t2", "Version: 1")),
            "not LUKS2",
        );
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
                .cmd("cryptsetup luksDump /dev/vda2", 0, DUMP)
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
        let gone = DUMP.replace(
            "  0: luks2\n\tKey:        512 bits\n\tPriority:   normal\n",
            "",
        );
        let sys = system(DUMP)
            .cmd("cryptsetup luksDump /dev/vda2", 0, DUMP)
            .cmd("cryptsetup luksDump /dev/vda2", 0, &gone)
            .cmd("systemd-cryptenroll --wipe-slot=tpm2 /dev/vda2", 0, "");
        let p = plan(&sys, Action::Remove).unwrap();
        assert!(apply(&sys, Action::Remove, &p)
            .unwrap_err()
            .contains("do NOT reboot"));
    }
}

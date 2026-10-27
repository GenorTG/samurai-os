# 09 — Security baseline

## Threat model (single-owner desktop/laptop)
| Threat | Mitigation |
|--------|-----------|
| Stolen or lost machine | LUKS2 full-root encryption (default on), TPM2+PIN unlock optional, screen lock |
| Evil maid / boot tampering | Secure Boot with our own keys, Limine config hash enrolled, TPM binding to PCR 7 |
| Network attacks | firewall default-deny inbound, no listening services by default, sshd off |
| Compromised or tampered packages | signed `[samurai]` repo + db, Arch signing as-is, no unsigned repos, AUR never as root |
| Our own supply chain (GitHub account, CI) | hardware-key 2FA, pinned Actions, signing key isolated (below) |
| App compromise (browser, game) | AppArmor, Flatpak sandbox for untrusted apps, user namespaces kept for sandboxes |
| Known vulnerabilities | CVE notice via `arch-audit`, firmware via fwupd, microcode installed |

Constraint: games, Steam, Proton and anti-cheat must keep working on the default profile. Anything
that breaks them goes in the opt-in `paranoid` profile.

## Disk encryption
- LUKS2, argon2id, default on in the installer. `/boot` is the unencrypted ESP; Limine verifies
  kernel and initramfs hashes (ADR-0003), so they are integrity-protected even though they are not
  encrypted.
- First boot offers a **recovery key** (`systemd-cryptenroll --recovery-key`), shown once with the
  advice to store it offline.
- Optional TPM2 unlock: `systemd-cryptenroll --tpm2-device=auto --tpm2-pcrs=7 --tpm2-with-pin=yes`.
  With **PIN by default**: PCR 7 alone means anyone holding the machine reaches the login screen with
  the disk unlocked. Requires `sd-encrypt` in the initramfs (spec 08). Firmware updates or Secure Boot
  key changes reset PCR 7. The toolbox re-enrols after `samurai security sign` and warns before fwupd
  updates.

## Secure Boot
- `samurai security secureboot setup` (offered in the welcome app when the firmware is in setup mode):
  `sbctl create-keys` → `sbctl enroll-keys --microsoft` → sign Limine's EFI binaries → enrol the Limine
  config checksum (`limine-enroll-config`) → verify (`sbctl verify`) → reboot to firmware to turn it on.
- **Keep the Microsoft keys** (`--microsoft`). GPU and NVMe option ROMs are signed with them, and
  enrolling only our keys can leave a machine without display or boot device. The toolbox refuses to
  drop them unless forced.
- Do not sign the kernels with sbctl: Limine checks them by hash, and the config holding the hashes is
  itself enrolled. A pacman hook re-runs signing + enrolment whenever Limine or its config changes
  (kernel updates, snapshot entries). Verify the snapshot-entry case in the M4 spike.
- What this does **not** cover: kernel module signature enforcement (lockdown). NVIDIA DKMS modules
  are unsigned, and enforcing signatures would need a MOK signing setup. v1 leaves lockdown off;
  `paranoid` turns it on only on machines without out-of-tree modules.

## Kernel and sysctl hardening (`samurai-settings-security`, `standard` profile)
```
kernel.kptr_restrict = 2
kernel.dmesg_restrict = 1
kernel.unprivileged_bpf_disabled = 1
net.core.bpf_jit_harden = 2
kernel.yama.ptrace_scope = 1
kernel.kexec_load_disabled = 1
dev.tty.ldisc_autoload = 0
vm.unprivileged_userfaultfd = 0
fs.protected_symlinks = 1, fs.protected_hardlinks = 1, fs.protected_fifos = 2, fs.protected_regular = 2
fs.suid_dumpable = 0
net.ipv4.tcp_syncookies = 1
net.ipv4.conf.all.rp_filter = 2            # loose: strict (1) breaks Tailscale/VPN asymmetric routing
net.ipv4.conf.all.accept_redirects = 0, net.ipv6.conf.all.accept_redirects = 0
net.ipv4.conf.all.send_redirects = 0
net.ipv4.conf.all.accept_source_route = 0
net.ipv4.icmp_ignore_bogus_error_responses = 1
```
- **Kept on purpose**: unprivileged user namespaces. Flatpak, Steam's pressure-vessel and browser
  sandboxes need them; turning them off makes those apps less safe, not more.
- Boot parameters: `lsm=landlock,lockdown,yama,integrity,apparmor,bpf`; CPU mitigations left at the
  kernel default. The toolbox's "mitigations off" switch exists (Garuda parity) behind a red warning,
  and the health check reports it.
- Core dumps: systemd-coredump with `Storage=external`, max 1 GiB, cleaned by the cleaner.

## Mandatory access control
- AppArmor enabled at boot, with the profiles shipped by packages in enforce mode.
- `apparmor.d` (the large community profile set) is opt-in: installed in complain mode, then the
  toolbox shows which profiles logged denials before you switch it to enforce.

## Network
- **firewalld** (nftables backend), enabled at install; ufw is the installer/toolbox alternative,
  OpenSnitch an optional per-app outbound firewall on top. Default zone `public`: inbound denied except
  DHCPv6 client and mDNS. ADR-0006 explains firewalld over ufw.
- **Opening ports and services should be easy**, in two places that change the same firewalld config:
  - System Settings → Firewall (`plasma-firewall`, KDE's own module; works with firewalld and ufw);
  - toolbox → Firewall page, and `samurai firewall`:
    ```
    samurai firewall status                        # zones, interfaces, open services/ports, Docker-published ports
    samurai firewall open kdeconnect               # a preset
    samurai firewall open 8080/tcp --zone home     # a port (ranges and udp too)
    samurai firewall close 8080/tcp
    samurai firewall zone wlan0 home               # trust level per network
    ```
  - Presets (one click): KDE Connect, Steam Remote Play, Steam local game transfer, Sunshine/Moonlight,
    LocalSend, Syncthing, Samba, SSH, Minecraft server, Ollama API (LAN), web dev server 3000/5173/8000.
    Each preset says which ports it opens. Every change is reversible from the same page.
- **Tailscale** (default on, installer toggle; spec 08): `tailscaled` enabled, login at first boot.
  `tailscale0` goes into its own zone `samurai-tailnet`: inbound denied like `public`, so joining your
  tailnet exposes nothing until you open a service for that zone (e.g. SSH only on the tailnet). The
  toolbox Tailscale page shows status, login/logout, exit node and the zone's open services. Turning it
  off disables the service and leaves the package installed.
- NetworkManager: Wi-Fi scan MAC randomisation on (the NM default), `stable` per-connection MAC
  option in the toolbox.
- systemd-resolved; the toolbox can switch to DNS-over-TLS providers (Garuda parity).
- No listening services out of the box; `security.listening` (spec 05) watches for new ones.
- sshd not installed. Enabling it through the toolbox writes key-only, no-root config.

## Accounts and privilege
- Root locked; the installer user is in `wheel`; sudo `timestamp_timeout=5`; pam_faillock (Arch
  default: 3 tries, 10 min).
- polkit: `wheel` members authenticate as themselves; toolbox actions have individual polkit
  actions (spec 01), read-only ones need no auth.

## Applications
- Firefox ships with an enterprise policy that installs uBlock Origin and turns telemetry off.
- Flatpak with Flathub enabled for apps outside Arch; Flatseal included.
- `arch-audit` feeds the notifier's "security updates" state (spec 03).
- fwupd + LVFS for firmware (toolbox page; spec 04).

## Our supply chain
- GitHub account: hardware-key 2FA; `samurai-os` and `samurai-repo` with branch protection on `main`.
- Actions: third-party actions pinned by commit SHA; `permissions:` least privilege per job; the
  signing subkey lives in a protected environment that only `main` workflows can use. Pull requests
  from forks never see secrets.
- The primary key stays offline (spec 02). Publishing needs the subkey; certifying a new subkey needs
  the offline key.
- Each ISO release ships a package manifest (name, version, sha256) as an SBOM-like list.
- Later: reproducible-build check: rebuild a random `[samurai]` package and compare hashes.

## Profiles
| Profile | Adds |
|---------|------|
| `standard` (default) | everything above |
| `paranoid` (opt-in) | `apparmor.d` enforce, USBGuard (allow present devices, prompt on new), `init_on_free=1`, `lockdown=integrity` when no out-of-tree modules, `nosmt` offer on affected CPUs, stricter firewall (no mDNS) |

`samurai security status` reports each item with pass/warn/fail; `samurai security profile <name>`
applies one with a snapshot first.

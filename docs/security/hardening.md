# Security defaults (`samurai-settings-security`) and what they can break

Spec: `docs/specs/09-SECURITY.md`. Effective state: `samurai security status`. Every value below is
asserted on a running system by `tests/vm/test_security.py`.

## Kernel settings (`/usr/lib/sysctl.d/80-samurai-security.conf`)
Undo one with a line in `/etc/sysctl.d/99-local.conf`, then `sudo sysctl --system` (or reboot).

| Setting | Value | What it can break |
|---------|-------|-------------------|
| `kernel.kptr_restrict` | 2 | `perf`/`bpftrace` kernel symbols without root |
| `kernel.dmesg_restrict` | 1 | `dmesg` needs `sudo` (bug-report scripts that call it plainly) |
| `kernel.unprivileged_bpf_disabled` | 1 | unprivileged BPF tools; cannot be turned back on until reboot |
| `net.core.bpf_jit_harden` | 2 | small BPF JIT cost (seccomp, firewalls) |
| `kernel.yama.ptrace_scope` | 1 | attaching a debugger to an already running process needs `sudo`; Wine/Proton work (they declare their tracer) |
| `kernel.kexec_load_disabled` | 1 | kdump, `systemctl kexec`; one-way until reboot |
| `dev.tty.ldisc_autoload` | 0 | serial line disciplines requested by unprivileged programs (some serial/CAN adapters) |
| `vm.unprivileged_userfaultfd` | 0 | post-copy live migration of user QEMU VMs |
| `fs.protected_*` | 1/1/2/2 | scripts that open another user's file or FIFO in `/tmp` |
| `fs.suid_dumpable` | 0 | core dumps of setuid programs |
| `net.ipv4.conf.all.rp_filter` | 2 (loose) | nothing known; strict (1) would break Tailscale/VPNs |
| `*.accept_redirects`, `*.secure_redirects`, `*.send_redirects` | 0 | networks that rely on ICMP redirects to find a second gateway (add a static route) |
| `*.accept_source_route` | 0 | source-routed packets (obsolete) |

Spec 09 listed `conf.all` only for redirects. That has no effect on an existing interface: while
forwarding is off the kernel enables the feature when **either** `all` or the interface says so
(kernel `ip-sysctl`: `accept_redirects`, `send_redirects`). The file therefore sets `all`, `default`
and every interface.

Unprivileged user namespaces stay on (Flatpak, Steam pressure-vessel, browser sandboxes).

## Other defaults
| File | Effect | What it can break |
|------|--------|-------------------|
| `/etc/sudoers.d/50-samurai-security` | `timestamp_timeout=5`, `use_pty` | piping into a long-running command started through `sudo` behaves as in any current sudo |
| `/usr/lib/systemd/coredump.conf.d/50-samurai.conf` | core dumps on disk, max 1 GiB | older dumps are removed sooner |
| `/usr/lib/systemd/resolved.conf.d/50-samurai.conf` | `LLMNR=no`: no listener on port 5355, no LLMNR poisoning | single-label names of Windows machines that only answer LLMNR (use `name.local` or DNS) |
| `apparmor.service` + `lsm=...apparmor...` (installer) | profiles shipped by packages are enforced | a program doing something its packaged profile does not allow (`journalctl -k \| grep DENIED`); off: remove the `lsm=` parameter (`sudo samurai boot set param remove lsm`) |
| `/etc/firefox/policies/policies.json` | telemetry and studies off, uBlock Origin installed (can be disabled; needs network on first start) | sites that break with an ad blocker; edit or delete the file (pacman keeps your version) |
| firewalld services `samurai-*`, zone `samurai-tailnet` | presets; `tailscale0` refuses inbound like `public` | services reached over the tailnet must be opened for that zone (`samurai firewall open ssh --zone samurai-tailnet`). Tailscale SSH is handled inside `tailscaled` and is not filtered by firewalld |

## Polkit
Arch's default already makes `wheel` members authenticate as themselves
(`/usr/share/polkit-1/rules.d/50-default.rules`); nothing is added. Upstream rules that skip the
password for local `wheel` users (Flatpak system installs) are left as upstream ships them.

## Existing installs
Installing the package applies the kernel, sudo, Firefox and firewall files. AppArmor additionally
needs the kernel parameter:
`sudo samurai boot set param add lsm=landlock,lockdown,yama,integrity,apparmor,bpf`, then reboot.

## TPM unlock (`samurai security tpm enroll`, optional)
Adds one keyslot bound to PCR 7 and a PIN; the passphrase slot is never touched, and a key-file
system, a header without a passphrase or recovery slot, or an initramfs without the TPM plugin is
refused. Proven with an emulated TPM (`tests/vm/test_security_tpm.py`):

| Situation | What the boot does |
|-----------|--------------------|
| right PIN | unlocks |
| forgotten PIN | asks again (three PIN prompts), then asks for the passphrase |
| different or reset TPM | one PIN prompt, then the passphrase |
| after `tpm remove` | passphrase prompt only |

Not covered: real TPM hardware, its dictionary-attack lockout timing, a PCR 7 change by a firmware
update on real firmware. With Secure Boot off, PCR 7 does not change when the boot loader is
replaced: a tampered loader could capture the PIN.

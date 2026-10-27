# 07 — Hardware and driver detection

## Engine: chwd, our profiles
CachyOS's `chwd` (Rust, GPL-3.0) does the hard parts. It matches PCI/USB devices by class, vendor,
device id or name pattern (+ chassis type for laptops) against TOML profiles. Each profile carries
packages, conditional packages (a shell snippet that prints package names) and pre/post
install/remove hooks. It also handles conflict groups (one NVIDIA driver at a time) and priorities.

What we change (checked against the chwd source, 2026-10-27):
- chwd reads profiles from a fixed path, `/var/lib/chwd/db/{pci,usb}/`, and its helper script from
  `/var/lib/chwd/scripts/chwd`. We split it into two packages: our `chwd` package builds the binary +
  scripts from CachyOS's tag, and `samurai-hardware` installs **our** profile set into the db dir. No
  patching of chwd itself.
- Our profiles start from CachyOS's set with CachyOS-only packages removed (e.g.
  `cachyos-drm-awaiter`). Kernel module packages are chosen by kernel: Arch's prebuilt
  `nvidia-open` for `linux` and `nvidia-open-lts` for `linux-lts`, `nvidia-open-dkms` for anything
  else (zen, cachyos). The same snippet style as CachyOS's `conditional_packages`.
- Tracking upstream: a weekly CI job diffs CachyOS's `profiles/` against ours and opens an issue when
  they change. We port the changes by hand; CachyOS is upstream for device knowledge.

## Profile coverage (v1)
| Area | Profiles |
|------|----------|
| NVIDIA | open modules for Turing and newer (RTX 20xx–50xx; Blackwell needs open modules); legacy `580xx` branch for Maxwell/Pascal (GTX 9xx/10xx); Optimus laptops (`.prime` variants: nvidia-prime, switcheroo-control, nvidia-powerd where supported); nouveau fallback |
| AMD | Mesa + Vulkan (radv) + VA-API, lib32 counterparts, `amdgpu` early KMS |
| Intel | Mesa + `vulkan-intel` + `intel-media-driver` (Broadwell+) / `libva-intel-driver` (older) |
| Hybrid | Intel/AMD iGPU + NVIDIA dGPU handled by the `.prime` profiles |
| VMs | QEMU/KVM (`qemu-guest-agent`, `spice-vdagent`, virtio GPU), VirtualBox guest, VMware guest |
| Network | Broadcom `broadcom-wl` where the open driver fails, Realtek out-of-tree only where needed |
| Laptops/handhelds | chassis-based power profile defaults; CachyOS's handheld profiles (Steam Deck, ROG Ally, Legion Go) kept |
| Not PCI/USB | handled by `samurai hw` itself, not chwd: CPU microcode (`intel-ucode`/`amd-ucode`), firmware packages, `sof-firmware` for Intel audio DSPs, fingerprint readers (`fprintd`) and Wacom tablets by USB id, printers via the setup assistant |

## When detection runs
1. **Installer**: the Calamares chwd step (same as CachyOS) applies profiles into the target system,
   using packages from the ISO's offline cache when they are there, the network otherwise.
2. **First boot**: `samurai-hw-firstboot.service` re-runs detection once with network. It catches
   anything the offline install skipped.
3. **Hardware change**: at every boot, `samurai-hw-fingerprint.service` hashes the PCI device list
   (class + vendor + device) and compares it with `/var/lib/samurai/hw-fingerprint`. If it changed,
   the notifier says "New hardware: NVIDIA GeForce RTX 5080 (was RTX 3090). Apply the recommended
   driver profile?". One click runs `samurai hw apply --reinstall` with a snapshot first. A GPU swap
   without a driver reinstall (which is what produced the stutter on fan-dragon) gets caught here.
4. **Manual**: `samurai hw detect` (dry run, prints what it would do), `samurai hw apply`,
   `samurai hw list [--installed|--available|--all]`, `samurai hw remove <profile>`.
5. **Health**: the `drivers.*` checks (spec 05) re-verify state daily.

## NVIDIA specifics (the most common breakage)
- The profile writes `nvidia_drm.modeset=1 nvidia_drm.fbdev=1` through `samurai boot` (kernel
  parameters stay in one place). It puts the nvidia modules in the initramfs for early KMS, and
  enables `nvidia-suspend/resume/hibernate` services.
- Post-update check (spec 03 stage 7): the module exists for every installed kernel. If not, the
  update ends with "do not reboot" and `samurai fix dkms`.
- Driver branch changes (e.g. when a new driver drops a GPU generation): an intervention (spec 03)
  moves affected machines to the right branch before the update that would break them. This is how
  Pascal users get moved to the 580xx branch.

## Testing
- **Profile matching**: unit tests run chwd against recorded `lspci -nnmm` / `lsusb` dumps of real
  machines in `tests/hw-fixtures/` (fan-dragon, stealth-dragon, genorbox1, plus common laptops).
  They check which profile is selected, without installing anything. Adding a machine means
  capturing one dump.
- **Install path**: VM tests cover the QEMU profile and the fingerprint-change flow (spec 11; QEMU can
  add or remove emulated PCI devices between boots).
- **Real GPUs**: tested only on real hardware from a live USB (spec 11, hardware checklist). No GPU
  passthrough to test VMs: the GPUs on our hosts are reserved for the services and LLM inference
  already pinned to them.

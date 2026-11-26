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
  `cachyos-drm-awaiter`, unless the CachyOS repo provides it). Kernel module packages are chosen
  per installed kernel: CachyOS's prebuilt `linux-cachyos-nvidia-open` for `linux-cachyos`, Arch's
  prebuilt `nvidia-open` for `linux` and `nvidia-open-lts` for `linux-lts`, `nvidia-open-dkms` for the
  rest (`linux-zen`). The same snippet style as CachyOS's `conditional_packages`.
- Tracking upstream: a weekly CI job diffs CachyOS's `profiles/` against ours and opens an issue when
  they change. We port the changes by hand; CachyOS is upstream for device knowledge.

## Profile coverage (v1)
The base install (including the baked offline closure) includes Mesa's AMD, Intel and Nouveau
userspace stacks and Vulkan drivers for Radeon/Intel/Nouveau. The offline closure also includes the
standard NVIDIA userspace and kernel modules from their configured package repositories. The module
package is filtered to the selected kernel: CachyOS prebuilt for `linux-cachyos`, Arch prebuilt for
`linux`/`linux-lts`, and DKMS plus headers for `linux-zen`. Open modules support Turing and newer;
older cards retain Nouveau until a suitable legacy branch is selected by the hardware-profile flow.
ADR-0008's non-redistribution rule applies to the custom gpu-unlock firmware/module, not these
standard repository driver packages.

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
1. **Installer**: the installer's chwd step applies profiles into the target system,
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

## CUDA (NVIDIA, default on)
- When the NVIDIA profile applies and the installer's CUDA toggle is on (default): `cuda`, `cudnn`,
  `nvtop` from Arch `extra` (CUDA 13.x on 2026-10-27). Docker gets `nvidia-container-toolkit` (spec 13).
- Only for **Turing (RTX 20) and newer**. CUDA 13 no longer targets Maxwell, Pascal or Volta, so a
  GTX 10xx machine gets the driver without the CUDA toolkit, and the installer says why. (Older CUDA
  versions for those cards are a manual install.)
- `cuda` is about 4–5 GiB, so the summary page shows it in the download size.

## gpu-unlock (FP16/BF16 tensor throughput unlock, default on for supported cards)
[`Zagitta/gpu-unlock`](https://github.com/Zagitta/gpu-unlock) is a kernel module that runs before
the unmodified NVIDIA driver. It sets the speed selectors on consumer cards to the professional
rate (upstream's 3090 Ti measurement: BF16 87.5 → 157.4 TFLOPS, FP16 87.5 → 118.4, TF32 42.6 → 82.6,
dense GEMM with FP32 accumulation). We reviewed and built it on genorbox1 today (`~/work/gpu-unlock`,
upstream 1.1.0 @ `6081407`); the stealth-dragon (Arch) test is running.

What it supports (from its `kernel/gpu_unlock/recipes.c`, not from our own list):
| GPU | Status in upstream 1.1.0 |
|-----|--------------------------|
| Ampere GA102/GA103/GA104/GA106 (RTX 3060–3090 Ti), GA100 | supported |
| RTX 4090 (AD102) | supported, one hardware success report so far |
| Turing (RTX 20), other Ada (RTX 4060–4080) | fork recipes (upstream plans to integrate them) |
| Blackwell (RTX 50) | not supported |

**Source of recipes (Genor 2026-11-17).** Fork recipes for Turing and non-4090 Ada are **included**:
the builder pulls the recipe table from the upstream repo plus our pinned fork list, and the
installer lists the card's source (upstream / fork) next to it. Per Genor's decision, **every**
recipe — upstream ones included — is labelled "untested, potentially unstable" until it has been
validated on real hardware; validated cards lose the label. Nothing is on by default in the
installer without that label being shown.

**Distribution constraint (decides the design).** The module needs firmware extracted from an
NVIDIA driver package. The repo's `firmware/NOTICE.md` says redistribution approval is not claimed,
and personal local packaging is the intended use. So we **never publish the firmware or a built
package** in `[samurai]` or on the ISO. Instead:
- `samurai-gpu-unlock` (in `[samurai]`) contains only the pinned upstream source reference and a
  builder. On the target machine it runs upstream's own Arch packaging (`packaging/arch/PKGBUILD`):
  it downloads the NVIDIA inputs from nvidia.com and checks them against pinned SHA-256 sums, builds
  `gpu-unlock-dkms` + `gpu-unlock-firmware` locally, and installs them. That is the same model as an
  AUR package.
- Install flow (spec 08 step 5): for each detected card that matches a recipe, the hardware page shows
  it with a checkbox (default on). The build needs network; without network it is deferred to first
  boot.
- Kernel command line through `samurai boot`: `gpu_unlock.targets=<PCI address list>`, listing the exact
  cards (`0000:01:00.0`), not `all`. A machine with an unsupported second GPU (e.g. genorbox1's GTX 1070)
  is never touched.
- DKMS rebuilds the module for every installed kernel (cachyos + zen). mkinitcpio puts it right
  before `nvidia` (upstream drop-in `zz-gpu-unlock.conf`).
- Upstream updates: we bump the pinned version in `samurai-gpu-unlock` after the VM build test plus a
  hardware check on a 3090. The package hook then rebuilds locally on each machine.
- Health check `drivers.gpu-unlock`: for every target, the kernel log reports success this boot
  (`journalctl -k -b | grep gpu_unlock`); after resume too (the module re-unlocks after suspend to RAM;
  hibernation is not covered). The toolbox shows per-card state and a toggle that removes the card from
  `targets`.
- Side effects to tell the user: the display card's screen is blank for about a second during boot
  (between the unlock's reset and `nvidia-drm` taking over); resume takes about a second longer; the
  card may run slightly warmer under tensor loads (normal power and thermal limits still apply).

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

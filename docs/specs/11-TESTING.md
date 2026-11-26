# 11 — Testing

## Test host: genorbox1 (checked 2026-10-27)
- `/dev/kvm` usable by the `genorbox1` user (ACL), 8 threads, 62 GiB RAM, ~460 GiB free.
- QEMU 7.2, OVMF (`OVMF_CODE_4M.fd`, `.secboot.fd`, `.ms.fd` + matching VARS), swtpm 0.7, libvirt,
  Docker (ISO and package builds run in `archlinux` containers; the host is Debian).
- Budget: at most 2 test VMs at once (4 vCPU / 8 GiB each), so the host's services keep running.
  Disks are qcow2 under `~/projects/samurai-os/.vm/` (git-ignored); the harness deletes overlays after
  each run and keeps at most 3 ISOs and 2 base images.

## Harness (`tests/vm/`, Python + pytest)
- Boots QEMU directly (not via libvirt), so every run is reproducible from the command line:
  `q35`, OVMF, `virtio-blk` disk, `virtio-net` (user mode + port forward), `virtio-vga-gl` for Plasma
  or `-display none` for headless steps, swtpm for TPM2 tests, a serial console logged to a file.
- **Control channel**: QMP for power, keys (`send-key`), screenshots (`screendump`) and device hot-plug;
  `qemu-guest-agent` in the guest for running commands and reading files. The QEMU chwd profile
  installs the agent anyway, and it opens no network port. The live ISO includes it too.
- **State reuse**: a test starts from a cached base image (e.g. "fresh install of build X") through
  a qcow2 overlay, so most tests skip the install and take minutes.
- **Visual checks**: screenshots saved per step. Assertions are "a process is running" or "a unit is
  active", never pixel matching. A failing test attaches its last screenshots + serial log + journal.
- **Package source**: the VM's `pacman.conf` points at a local mirror on the host. It serves
  `[samurai-testing]` from the latest CI or local build, plus a caching proxy for Arch packages so
  repeated installs do not re-download from public mirrors.
- **Old systems**: to test upgrade paths, the harness installs with Arch repos pinned to a past date
  through the Arch Linux Archive (`archive.archlinux.org/repos/YYYY/MM/DD/`), then updates to today.
  This is how interventions and `samurai update` get tested against real history.

## Suites
| Suite | Scenarios | Runs |
|-------|-----------|------|
| `unit` | `cargo test` (toolbox), chwd profile matching on `tests/hw-fixtures/` dumps, PKGBUILD lint, shellcheck | every push (GitHub-hosted) |
| `smoke` | `iso-boot`, `install-default`, `firstboot`, `update-noop`, `health-green` | every testing publish (self-hosted, genorbox1) |
| `full` | everything below | nightly + before promoting to stable + before an ISO release |

Scenarios in `full`:
| Scenario | Asserts |
|----------|---------|
| `iso-boot` | ISO boots on OVMF to a Plasma live session; toolbox and `samurai rescue` start |
| `install-default` | scripted install, LUKS on; first boot reaches SDDM after unlocking; layout and subvolumes match spec 06 |
| `install-gui` | `tests/vm/test_installer_gui.py`: the graphical wizard driven by keys finishes, the installed system boots; screenshots reviewed |
| `firstboot` | no failed units; `samurai health` all green; hw fingerprint written; snapper configs present |
| `update-from-archive` | install pinned to a date ≥ 30 days ago, `samurai update` to today succeeds, reboots, healthy |
| `intervention-<id>` | one per intervention: before-state machine → update → after-state verified |
| `rollback-after-bad-update` | install `samurai-test-breaker` from a test-only repo (breaks the SDDM start), reboot fails visibly, boot the pre snapshot from the Limine menu, `samurai snap rollback`, reboot healthy |
| `rollback-from-snapshot-boot` | banner button path of the same |
| `health-<check>` | break the thing on purpose, `detect` flags it, `fix` repairs it, check is green |
| `hw-fingerprint-change` | add a NIC or swap the emulated GPU between boots; notifier and `drivers.profile` notice; apply works |
| `secureboot` | OVMF secboot firmware in setup mode; `samurai security secureboot setup`; reboot with SB on; a kernel update keeps it booting; snapshot boot still works |
| `tpm-unlock` | swtpm; enrol TPM+PIN; reboot unlocks with the PIN; recovery key works |
| `rescue-bootloader` | delete the Limine EFI binary; boot the ISO; `samurai rescue bootloader`; installed system boots |
| `uninstall-check` | remove every `samurai-*` package (keeping Arch equivalents); the system still boots to a shell (spec 01 design test) |
| `toolbox-actions` | each CLI action in spec 04 with `--dry-run`, plus the non-destructive ones for real |
| `install-profile-parity` | the same choices made through the graphical and the terminal installer produce byte-identical `install-profile.toml` files and the same `pacman -Qq` set |
| `install-tui` | terminal installer driven through the serial console (expect script) in guided mode |
| `catalog-resolve` | every package in `catalog/` resolves to a repo; a full "everything ticked" install in a VM succeeds (nightly, long) |
| `python-versions` | `python3.10`…`python3.14` each create a venv and import `ssl, sqlite3, ctypes`; `uv venv --python 3.11` downloads nothing; `conda create -n t python=3.12` works as the user |
| `docker` | `docker run hello-world` as the user; `docker compose version` |
| `kde-layout` | after first login: top + bottom panels exist, the task manager is not icons-only, one monitor widget per CPU socket and GPU (VM: virtio GPU counts as one), effects config matches spec 14 |
| `firewall-ux` | each preset opens exactly the ports it lists and closes them again; Tailscale zone denies inbound by default |
| `gpu-unlock-builder` | builder runs in a VM with no NVIDIA card: detects nothing, installs nothing, exits 0; recipe matching against hw fixtures (3090 → supported, 1070 → skipped, 5080 → unsupported) |

The gpu-unlock build and unlock themselves are tested only on real hardware (stealth-dragon's 3090,
spec 07), because a VM has no supported NVIDIA card.

## CI safety for the self-hosted runner
The genorbox1 runner runs only on `push` to `main` and `workflow_dispatch`, never on `pull_request`,
so code from a fork can never run on Genor's server. It runs as a dedicated `samurai-ci` user that can
access `/dev/kvm` and the test dir only, with no access to the main user's files, Docker socket or
secrets.

## Real hardware (manual checklist, needs Genor)
Live USB first (no install), then install to a spare disk or USB SSD, never over an existing system
without a backup.
| Machine | Covers |
|---------|--------|
| fan-dragon (RTX 5080, Blackwell) | NVIDIA open modules, Wayland on NVIDIA, gaming suite, fingerprint-change flow (it just went 3090 → 5080) |
| stealth-dragon (RTX 3090) | gpu-unlock end to end: build, boot unlock, BF16 GEMM before/after, suspend/resume |
| a laptop | hybrid graphics (prime), power profiles, Wi-Fi/Bluetooth, suspend/resume, TPM unlock |
Each run fills `tests/hardware/<machine>-<date>.md`: boot, install, drivers, suspend, gaming smoke
(one Proton title), update, rollback, Secure Boot.

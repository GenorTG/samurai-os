# Install profile schema (version 1)

The contract between the installers (terminal wizard, graphical installer, hand-written files) and
`samurai-postinstall` (spec 08). A front-end only writes this TOML; it never installs anything.

- Validator (the authority): `installer/validate-profile.py <profile.toml>`: exit 0 = accepted,
  exit 1 = `VALIDATION ERROR: <reason>` on stderr. Run it before offering "Install".
- Planner (what a profile installs): `installer/samurai-profile-plan <profile.toml> [--json]`.
  Use it for the summary page; do not re-implement the package selection.
- Unknown tables and keys are errors. Strings are single-line; booleans are real TOML booleans.
- Options the system cannot deliver yet are **refused**, never silently ignored.

## Rules for front-ends
1. Write **every** key of the sections you show. An absent optional section means "do not install"
   (a saved profile keeps producing the same system), so the spec 08 defaults must be written out.
2. `[kde]` only with `desktop = "kde"`. `[gaming] enabled = true` only with a desktop.
3. Never write a plaintext password; only `user.password_hash` (`openssl passwd -6 -stdin`).

## Full example (spec 08 defaults)
```toml
version = 1

[system]                    # page 2
hostname = "samurai"
timezone = "Europe/Warsaw"
locale = "en_US.UTF-8"
keyboard = "us"

[disk]                      # page 3
device = "/dev/nvme0n1"
layout = "erase"
filesystem = "btrfs"
encryption = true
auto_unlock_keyfile = false # VM tests only

[boot]                      # pages 5, 5a
kernels = ["linux-cachyos", "linux-zen"]   # first entry boots by default
bootloader = "limine"

[user]                      # page 4
name = "samurai"
shell = "fish"
groups = ["wheel"]
root_locked = true
password_hash = "$6$..."    # optional; absent = generated into /root/initial-credentials

[packages]                  # pages 6 and 10
desktop = "kde"
browsers = ["firefox"]
apps = []
install = ["samurai-release", "samurai-mirrorlist", "samurai-keyring",
           "qemu-guest-agent", "networkmanager", "wpa_supplicant"]

[kde]                       # page 6a
theme = "breeze"
monitor_widgets = true
reduced_effects = true

[gaming]                    # page 8
enabled = true
steam = true
heroic = true
gamemode = true
mangohud = true
gamescope = true
proton_ge = true
proton_cachyos = true
wine = true

[development]               # page 9
python = []
conda = "miniforge"
docker = "group"
ai_clis = []
ollama = false

[network]                   # page 11
tailscale = true
reverse_proxy = "none"

[security]                  # page 11
firewall = "firewalld"
ssh_server = false

[hardware]                  # page 7
nvidia = "auto"

[repos]                     # fixed; may be omitted
samurai = true
cachyos = true
chaotic_aur = true
arch_mirror = "auto"
```

## New keys (this version)

`[packages]`
| Key | Type | Absent | Values |
|-----|------|--------|--------|
| `browsers` | list | `["firefox"]` with a desktop, `[]` with `none` | catalog ids in category `browsers` (`samurai apps list --json`); `[]` = no browser |

`[kde]` (page 6a)
| Key | Type | Absent | Values |
|-----|------|--------|--------|
| `theme` | string | `"breeze"` | `"breeze"`. `otto`, `scratchy` are refused until their packages exist |
| `monitor_widgets` | bool | `true` | CPU/GPU monitor widgets in the top panel |
| `reduced_effects` | bool | `true` | speed-first KWin effects (spec 14) |

`[gaming]` (page 8)
| Key | Type | Absent | Installs |
|-----|------|--------|----------|
| `enabled` | bool | `false` | master switch; `false` = nothing from this table (the items must then be absent or `false`) |
| `steam` | bool | `true` | `steam`, 32-bit Mesa/Vulkan drivers (`lib32-nvidia-utils` with NVIDIA), `protontricks` |
| `heroic` | bool | `true` | `heroic-games-launcher-bin` |
| `gamemode` | bool | `true` | `gamemode`, `lib32-gamemode` |
| `mangohud` | bool | `true` | `mangohud`, `lib32-mangohud`, `goverlay` |
| `gamescope` | bool | `true` | `gamescope` |
| `proton_ge` | bool | `true` | `samurai-proton-ge` |
| `proton_cachyos` | bool | `true` | `proton-cachyos-slr` |
| `wine` | bool | `true` | `samurai-wine`, `umu-launcher` |

`[development]` (page 9)
| Key | Type | Absent | Values |
|-----|------|--------|--------|
| `python` | list | `[]` | any of `"3.10"`, `"3.11"`, `"3.12"`, `"3.13"` (3.14 is the system python) |
| `conda` | string | `"none"` | `"miniforge"`, `"none"`. `miniconda` is refused (not packaged: Anaconda licence) |
| `docker` | string | `"none"` | `"group"` (user joins `docker`, root-equivalent: say so on the page), `"rootless"`, `"none"` |
| `ai_clis` | list | `[]` | catalog ids in category `ai-cli`: `claude-code`, `codex`, `gemini-cli`, `copilot-cli`, `opencode`, `aider`, `cursor-cli` |
| `ollama` | bool | `false` | Ollama + service; `-cuda` / `-rocm` build chosen from the GPU |

`[network]` (page 11)
| Key | Type | Absent | Values |
|-----|------|--------|--------|
| `tailscale` | bool | `false` | package + `tailscaled`; login is a first-boot step |
| `reverse_proxy` | string | `"none"` | `"none"`, `"caddy"`, `"nginx"` (with certbot), `"traefik"`. Installed **disabled**; no port is opened |

`[repos] chaotic_aur` is now `true` and fixed (ADR-0007); `false` is refused.

## Always installed (not profile choices)
`samurai-base`, `samurai-settings-system`, `samurai-settings-shell`, `samurai-settings-dev`; with a
desktop also `samurai-desktop-base` (audio, fonts, Bluetooth, Flatpak); with KDE
`samurai-desktop-kde`.

## Offline installs
The offline ISO carries the packages for: base, every desktop default above (KDE), Firefox, all
gaming items, every Python version, Miniforge, Docker (both modes), Tailscale, the reverse proxies
and both firewalls. Anything else a profile asks for (other browsers, AI CLIs, Ollama, catalog apps)
is **deferred**: the install succeeds and writes the exact command to
`/var/lib/samurai/firstboot-notes` (also shown in `/etc/motd`). A front-end should mark those as
"installs after first boot" when offline: `samurai-profile-plan --json` flags them with
`"offline": false`. With `--offline` the planner moves such choices into `deferred` itself.

## `samurai-profile-plan --json`
```json
{"packages": [{"name": "steam", "group": "gaming", "offline": true}],
 "services": ["docker.socket"],
 "user_services": [],
 "user_groups": ["docker"],
 "deferred": [{"id": "aider", "command": "samurai apps install aider", "reason": "per-user installer"}]}
```
Options: `--offline`, `--nvidia`, `--amd` (GPU-dependent packages), `--catalog DIR`.
`--list browsers|ai-cli` prints the selectable catalog ids with names; `--offline-closure` prints
the packages the offline ISO carries for these choices. `"offline"` says whether the offline medium
carries a package (extra `packages.install` names are assumed to be on it).

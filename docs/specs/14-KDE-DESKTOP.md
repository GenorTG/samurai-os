# 14 — KDE Plasma desktop setup

Applies when the installer's desktop choice is KDE Plasma (the default). Everything ships in
`samurai-settings-kde` and the theme packages. It is applied for new users through `/etc/skel` plus
a one-time first-login script, so no package ever overwrites an existing user's config.

## Theme (installer page 6a)
| Choice | Contents | Package |
|--------|----------|---------|
| **Otto** (default) | dark, orange/yellow gradients: global theme, Plasma style, colour schemes, Kvantum style, window decoration, Konsole scheme (KDE Store 1360125 and related items) | `samurai-theme-otto` (+ `kvantum`) |
| Scratchy | dark, Catppuccin Macchiato palette, same set of parts (KDE Store 1898344) | `samurai-theme-scratchy` |
| Breeze Dark | KDE's own | — |

- Before packaging: confirm each theme's licence allows redistribution (KDE Store items are usually
  GPL or CC-BY-SA) and that it renders correctly on the current Plasma 6. If a licence does not allow
  redistribution, the package fetches the theme from the KDE Store at first login instead (KNewStuff,
  same as "Get New…"). This check is task M7a in spec 12.
- SDDM, Plymouth and Limine themes are matched to the chosen theme's colours (`samurai-branding`).
- Fonts: Noto Sans for the UI, JetBrains Mono Nerd Font for Konsole. Konsole's default profile uses
  fish (spec 10).

## Panel layout
```
┌─ top panel ───────────────────────────────────────────────────────────────────────────┐
│ [Launcher] [Pager]                         ··· [CPU0 12% 4.8GHz 54°C] [GPU0 3% 41°C] │
│                                                [systray] [clock] [show desktop]       │
└───────────────────────────────────────────────────────────────────────────────────────┘
                                 (desktop)
┌─ bottom panel, full width ────────────────────────────────────────────────────────────┐
│ [Konsole — ~/projects] [Firefox — Arch Wiki] [Dolphin — Downloads] [Steam]             │
└───────────────────────────────────────────────────────────────────────────────────────┘
```
- **Top panel**: the usual Plasma panel contents (launcher, pager, system tray, clock, show desktop),
  moved to the top edge. No task manager here.
- **Bottom panel**: full width, only the Task Manager, set to show **names, not just icons**
  (`iconsOnly=false`), no grouping (`groupingStrategy=0`, so every window shows its own title), all
  desktops, one row. When too many windows are open to fit, Plasma shortens titles to keep each
  button visible; full titles are guaranteed only while they fit. The toolbox has a "two rows when
  crowded" option.
- **Monitor widgets** (installer toggle, default on): Plasma System Monitor widgets in the top panel,
  right side, text-only face, one per device:
  - one per **physical CPU** (socket, not core): usage %, average clock in GHz, average temperature;
  - one per **GPU**: usage %, temperature.

### How the widgets are built
- A one-time first-login script, `samurai-plasma-layout`, autostarts from `/etc/skel` and removes its
  own autostart entry when done. It:
  1. counts sockets (`lscpu -p=SOCKET`) and GPUs (the sensors ksystemstats exposes, from
     `kstatsviewer --list`);
  2. writes a Plasma layout script with one System Monitor widget per device, using the sensor ids it
     found;
  3. applies it with `qdbus6 org.kde.plasmashell /PlasmaShell org.kde.PlasmaShell.evaluateScript`.
- Sensors to use (confirm the exact ids with `kstatsviewer --list` in the M7 VM spike):
  `cpu/all/usage`, `cpu/all/averageFrequency`, `cpu/all/averageTemperature`; `gpu/gpuN/usage`,
  `gpu/gpuN/temperature`. ksystemstats reads NVIDIA via `nvidia-smi` and AMD/Intel via sysfs.
- **Multi-socket gap**: ksystemstats has per-core and all-CPU sensors, but no per-socket usage or clock
  sensor. On single-socket machines (all of ours) `cpu/all/*` is exactly "per CPU". For multi-socket
  machines the spike chooses one of two options: per-socket temperature from lm_sensors (package
  temperature per socket) plus all-CPU usage and clock, or a small ksystemstats plugin that averages
  per socket (C++, maintained by us). The first is the v1 fallback.
- Same script also on demand: toolbox → Desktop → "Rebuild panel widgets". After a GPU change (spec
  07 fingerprint) the notifier offers it too, so a GPU swap updates the widgets.

## Speed-first effects (installer toggle "reduced effects", default on)
Written to `/etc/xdg/kwinrc` and `/etc/xdg/kdeglobals` defaults (users can change them in System
Settings; our values are only the starting point):
- Animations off: `AnimationDurationFactor=0` (instant).
- Off: blur, background contrast, translucency, wobbly windows, magic lamp, window open/close (scale),
  minimise (squash), maximise animation, sliding and fading popups, dim screen, login/logout fades.
- Kept: Overview, Desktop Grid and Present Windows. They cost nothing until you open them.
- Turning the toggle off in the toolbox restores KDE's own defaults.

## Other KDE defaults
- File indexing (Baloo): file names only, no content indexing. Content indexing is a known source of
  I/O and CPU spikes.
- Wayland session only. NVIDIA gets the kernel parameters from spec 07.
- Discover for Flatpak only (no PackageKit; spec 04).

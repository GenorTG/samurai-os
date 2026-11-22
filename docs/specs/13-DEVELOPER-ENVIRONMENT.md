# 13 — Developer environment (Python, conda, Docker, AI tools)

Installer page 9 (spec 08). Everything here is pacman-managed, so `samurai update` keeps it current.
Where a tool has its own self-updater, we turn the self-updater off.

## Python: versions selectable during install, 3.14 as the system python
| Version | Comes from | Path | Role |
|---------|-----------|------|------|
| 3.14 | Arch `core/python` (3.14.8 on 2026-10-27) | `/usr/bin/python`, `python3`, `python3.14` | **system python**: Arch's `python-*` packages are built against it |
| 3.13, 3.12, 3.11, 3.10 | `samurai-python313` … `samurai-python310` in `[samurai]` | `/opt/samurai/python/3.X/`, symlinks `/usr/bin/python3.X`, `pip3.X` | extra interpreters for venvs and projects |

- **Installer choice (Genor 2026-11-17):** every version is a separate checkbox on page 9,
  **all unticked by default**. uv's `python-downloads = "automatic"` covers versions not installed,
  so nothing is missing functionally; the checkboxes only pre-download them.

- The extra packages repackage **python-build-standalone** (Astral's prebuilt CPython, the same builds
  `uv python install` downloads). No compiling during install. CI bumps a package when a new patch
  release appears and runs `python3.X -c "import ssl, sqlite3, ctypes, venv"` in the VM test.
- "Latest is system-wide" holds on its own: Arch already ships 3.14. We never replace `/usr/bin/python`
  with another build, because every pacman-installed Python tool and library would break. When Arch
  moves to 3.15, it becomes the system python, and CI adds `samurai-python314` automatically. The rule
  is "every minor version from 3.10 up to the one before Arch's".
- **3.10 reached end of life in October 2026**: no more security fixes upstream. The toolbox marks
  it EOL and the health check shows an info notice (not a warning).
- Arch's python stays "externally managed" (PEP 668). `pip install` into the system python is
  refused; use uv, venvs or conda envs. The shell config prints a hint when it happens.

## uv (default)
- `uv` from Arch `extra`. System config `/etc/uv/uv.toml`:
  ```toml
  python-preference = "system"     # use the pacman-managed interpreters above first
  python-downloads = "automatic"   # still allowed for versions we do not ship (e.g. 3.9, 3.15 betas)
  ```
- So `uv venv --python 3.11` finds `/usr/bin/python3.11` and downloads nothing, and
  `uv tool install <x>` works per user (`~/.local/bin` is on PATH in our fish and bash configs).

## Conda: Miniforge by default (Miniconda as an option)
Recommendation: **Miniforge** instead of Miniconda. It is the same `conda` command, it adds `mamba`,
and it uses the conda-forge channel. Miniconda's default channel falls under Anaconda's terms of
service (free for personal use, paid for organisations above 200 people). Both are offered; the
setup is the same:
- Installed read-only under `/opt/miniforge` (or `/opt/miniconda`) by `samurai-miniforge` /
  `samurai-miniconda` (repackaged official installers, updated by pacman).
- `/opt/miniforge/.condarc`: `auto_activate_base: false`, `envs_dirs: [~/.conda/envs]`,
  `pkgs_dirs: [~/.conda/pkgs]`. Users create their own envs without root.
- Shell hooks in `/etc/fish/conf.d/` and `/etc/profile.d/` make the `conda` command available
  without activating anything.
- Why no base auto-activation: with conda's base env first on PATH, `python` stops being Arch's python
  and things break: AUR builds through yay, system Python scripts, pacman hooks. This is the most
  common way conda damages an Arch system.

## Docker (default on)
- `docker`, `docker-compose` (the Compose v2 plugin, so `docker compose` works), `docker-buildx`;
  `docker.socket` enabled (the daemon starts on first use).
- NVIDIA machines also get `nvidia-container-toolkit`, configured for Docker, so `--gpus all` works
  in containers out of the box.
- Data on the `@docker` subvolume (rootful Podman: `@containers`; rootless: under `~/.local/share/containers` in `@home`), outside root snapshots (spec 06).
- **Docker mode is chosen during install (Genor 2026-11-17):** "docker group" or "rootless". The
  installer notes that group membership is equivalent to root; rootless trades a little convenience
  for safety. Rootless is also switchable later in the toolbox.
- Firewall caveat: ports published with `-p` are reachable from the network regardless of firewalld
  zones (Docker programs its own rules). The security page shows published ports (spec 09).

## AI command-line tools (checkboxes, all off by default)
| Tool | Package | Source |
|------|---------|--------|
| Claude Code | `claude-code` | Chaotic-AUR |
| Codex CLI | `openai-codex-bin` | `[samurai]` (AUR only) |
| Gemini CLI | `gemini-cli` | Arch `extra` |
| GitHub Copilot CLI | `github-copilot-cli` | Chaotic-AUR |
| OpenCode | `opencode-bin` | `[samurai]` (AUR only) |
| Aider | `uv tool install aider-chat` | per user |
| Cursor CLI | vendor install script, run as the user | per user, last resort |

- pacman packages first, so updates go through `samurai update`. Built-in self-updaters are turned
  off where they exist (e.g. `DISABLE_AUTOUPDATER=1` for Claude Code in `/etc/environment.d/`).
- No API keys or logins during install; each tool logs in on first run. The welcome app lists the
  installed tools with their login commands.
- Adding a tool is one catalog entry (`catalog/samurai.toml`, category `ai-cli`).

## Local AI (optional)
Ollama from Arch `extra`. The hardware step swaps in `ollama-cuda` (NVIDIA) or `ollama-rocm` (AMD).
`ollama.service` is enabled only if ticked. More local runtimes (llama.cpp, LM Studio) are later
catalog entries.

## Other toolchains (Applications page, "development" category)
Garuda's development list (VS Code / VSCodium / Zed / JetBrains / Podman / Cockpit / Ansible /
Terraform …) plus our additions: `nodejs` + `npm` + `pnpm`, `rustup`, `go`, `jdk-openjdk`,
`dotnet-sdk`, `github-cli`, `git-lfs`. All off by default except `git`, `github-cli` and `base-devel`
(needed by yay).

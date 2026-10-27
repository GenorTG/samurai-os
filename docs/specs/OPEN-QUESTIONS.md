# Open questions (Genor decides)

Each has the default the specs assume. Work proceeds on the default until he answers.

| # | Question | Default in the specs | Notes |
|---|----------|----------------------|-------|
| Q1 | Main kernel: `linux-zen` (Arch) or `linux-cachyos` (CachyOS repo, BORE scheduler, prebuilt NVIDIA modules)? | `linux-zen` + `linux-lts` | `linux-cachyos` means trusting CachyOS's repo for the kernel. That is "an official source", but not Arch |
| Q2 | Final name | "Samurai OS", id `samurai` | one-variable rename (spec 00) |
| Q3 | Toolbox stack: Rust core + Qt6/QML GUI, or Rust core + Tauri/web GUI? | Rust + Qt6/QML (ADR-0004) | the CLI/helper part is the same either way |
| Q4 | Look: clean Breeze-based, or a flashy Garuda-dr460nized-style theme? | clean, accent colour + our wallpapers | |
| Q5 | Encryption on by default, with TPM+PIN offered at first boot? | yes / offered | |
| Q6 | Source repo public from day one, or private until v1? The **package** repo must be public, because pacman cannot authenticate to private GitHub release downloads | `samurai-os` private until M7, `samurai-repo` public from M1 | creating either GitHub repo waits for Genor's OK |
| Q7 | Diagnostics upload host: reuse `~/projects/genor-paste`, or deploy PrivateBin at `paste.smart-samurai.pl`? | PrivateBin (E2E encrypted, proven) | |
| Q8 | Chaotic-AUR available as an option in the Repos page? | yes, off by default | it is Garuda's main extra repo |
| Q9 | Browser(s) preinstalled | Firefox only | |
| Q10 | Real-hardware testing on fan-dragon: a spare disk/USB SSD we can install to? | live USB only until a spare disk exists | fan-dragon is a GPU host with running services |

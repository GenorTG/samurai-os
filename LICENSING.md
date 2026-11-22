# Licensing

Samurai OS is **source-available, free for noncommercial use**. This page explains what that
means, what is covered, and what is not. The legal text is in [`LICENSE`](LICENSE); this page is
a summary and does not change it.

## Our code: PolyForm Noncommercial 1.0.0

Everything we wrote (installer, `samurai` tools, package recipes, CI, tests, docs, website) is under the
[PolyForm Noncommercial License 1.0.0](https://polyformproject.org/licenses/noncommercial/1.0.0)
(SPDX: `PolyForm-Noncommercial-1.0.0`).

You may, free of charge:

- install and use Samurai OS for personal use, hobby projects, study, research and experiments;
- use it in schools, universities, public research bodies, charities and government institutions;
- read, modify and share the code for those purposes, as long as the license text and the
  `Required Notice:` line in `LICENSE` travel with it.

You may **not** use it commercially without a separate license. Examples that need one: running it
on machines or in a product of a for-profit company, selling it, bundling it into a paid product or
service, or offering it as a hosted service for money.

Commercial licenses are available: open an issue at <https://github.com/GenorTG/samurai-os/issues>
(or contact the owner through the GitHub profile) and say what you need.

This is a source-available license, not an OSI-approved open-source license. If you need an
OSI license for your use case, ask for a commercial or special-purpose license instead.

## What is not covered

- **Everything the ISO and installed system contain that we did not write.** Arch Linux, CachyOS and
  other repository packages keep their own licenses (mostly GPL, LGPL, MIT, BSD, Apache). We do not
  relicense them and the PolyForm terms do not apply to them. Each package carries its license under
  `/usr/share/licenses/<package>/` and in `pacman -Qi <package>`.
- **Source for bundled GPL software** is available from the upstream package sources: Arch Linux
  (<https://gitlab.archlinux.org/archlinux/packaging/packages>) and CachyOS
  (<https://github.com/CachyOS/CachyOS-PKGBUILDS>). If you cannot get a source you are entitled to,
  open an issue and we will provide it.
- **The ISO profile (`iso/`)** is derived from Arch's `archiso` "releng" profile and stays
  **GPL-3.0-or-later**, as do our edits to those files. Our own tools that the build copies into the
  image remain PolyForm Noncommercial.
- **`catalog/garuda-setup-wizard.toml`** is package-list data imported from the Garuda Linux toolbox
  and stays **GPL-3.0-or-later**.
- **Website fonts** (Shippori Mincho, IBM Plex Sans) are under the SIL Open Font License 1.1; their
  notices are in `site/fonts/`.
- **NVIDIA** firmware and modules are never redistributed by us (see ADR-0008).

## Names

"Arch Linux", "CachyOS", "Garuda Linux" and other names belong to their owners. Samurai OS is an
independent project and is not affiliated with or endorsed by them.

## No warranty

As far as the law allows, the software comes as is, without any warranty, and the licensor is not
liable for damages arising from its use (see `LICENSE`). Back up your data before installing on
real hardware.

## For contributors

Contributions are accepted under the same license as the file they change. By sending a change you
confirm you have the right to license it that way and allow the owner to also offer it commercially.
The REUSE metadata in [`REUSE.toml`](REUSE.toml) maps every path to its license
(`pipx run reuse lint` checks it).

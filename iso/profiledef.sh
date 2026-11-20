#!/usr/bin/env bash
# shellcheck disable=SC2034

iso_name="samurai-os"
iso_label="SAMURAI_$(date --date="@${SOURCE_DATE_EPOCH:-$(date +%s)}" +%Y%m)"
iso_publisher="Samurai OS <https://github.com/GenorTG/samurai-os>"
iso_application="Samurai OS Live/Rescue DVD"
iso_version="$(date --date="@${SOURCE_DATE_EPOCH:-$(date +%s)}" +%Y.%m.%d)"
install_dir="arch"
bootmodes=('bios.syslinux'
           'uefi.systemd-boot')
pacman_conf="pacman.conf"
airootfs_image_type="squashfs"
airootfs_image_tool_options=('-comp' 'xz' '-Xbcj' 'x86,arm64' '-b' '1M' '-Xdict-size' '1M')
bootstrap_tarball_compression=('zstd' '-c' '-T0' '--auto-threads=logical' '--long' '-19')
kernel_params_aarch64="clk_ignore_unused pd_ignore_unused arm64.nopauth"
kernel_params_x86_64="console=tty1 console=ttyS0,115200"
file_permissions=(
  ["/etc/shadow"]="0:0:400"
  ["/root"]="0:0:750"
  ["/root/.automated_script.sh"]="0:0:755"
  ["/root/.gnupg"]="0:0:700"
  ["/usr/local/bin/choose-mirror"]="0:0:755"
  ["/usr/local/bin/Installation_guide"]="0:0:755"
  ["/usr/local/bin/livecd-sound"]="0:0:755"
  ["/usr/local/bin/samurai-install"]="0:0:755"
  ["/usr/local/bin/samurai-postinstall"]="0:0:755"
  ["/usr/local/bin/validate-profile.py"]="0:0:755"
  ["/usr/local/bin/samurai-update"]="0:0:755"
  ["/usr/local/bin/samurai-notifier"]="0:0:755"
  ["/usr/local/bin/samurai"]="0:0:755"
  ["/usr/local/bin/samurai-snap"]="0:0:755"
  ["/usr/local/bin/samurai-rescue"]="0:0:755"
)

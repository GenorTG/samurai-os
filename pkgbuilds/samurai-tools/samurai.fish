# fish completion for samurai (shipped by samurai-tools; fish is the default shell)

function __samurai_no_cmd
    not __fish_seen_subcommand_from update adopt apps snap rescue status health diag clean fix mirrors boot kernel hw security firewall welcome version help
end

function __samurai_apps_ids
    samurai-apps ids 2>/dev/null
end

complete -c samurai -f

# top level
complete -c samurai -n __samurai_no_cmd -a update  -d 'staged system update'
complete -c samurai -n __samurai_no_cmd -a adopt   -d 'add the current defaults to an older install'
complete -c samurai -n '__fish_seen_subcommand_from adopt' -l preview -d 'show what would change'
complete -c samurai -n '__fish_seen_subcommand_from adopt' -l yes -s y -d 'do not ask'
complete -c samurai -n '__fish_seen_subcommand_from adopt' -l with -x -a 'gaming tailscale chaotic' -d 'optional set'
complete -c samurai -n '__fish_seen_subcommand_from adopt' -l with-upgrade -d 'include pending upgrades'
complete -c samurai -n '__fish_seen_subcommand_from adopt' -l no-snapshot -d 'no rollback point'
complete -c samurai -n __samurai_no_cmd -a apps    -d 'optional apps from the catalog'
complete -c samurai -n __samurai_no_cmd -a snap    -d 'snapshot manager'
complete -c samurai -n __samurai_no_cmd -a rescue  -d 'recovery tool (live ISO or installed system)'
complete -c samurai -n __samurai_no_cmd -a status  -d 'last update check and update report'
complete -c samurai -n __samurai_no_cmd -a health  -d 'read-only health checks'
complete -c samurai -n __samurai_no_cmd -a diag    -d 'diagnostics and redacted support bundle'
complete -c samurai -n __samurai_no_cmd -a clean   -d 'free disk space (preview first)'
complete -c samurai -n __samurai_no_cmd -a fix     -d 'targeted repairs'
complete -c samurai -n __samurai_no_cmd -a mirrors -d 'show or refresh the mirrorlist'
complete -c samurai -n __samurai_no_cmd -a boot    -d 'sync boot entries with installed kernels'
complete -c samurai -n __samurai_no_cmd -a kernel  -d 'list, install, remove, choose kernels'
complete -c samurai -n __samurai_no_cmd -a hw      -d 'hardware and driver profiles'
complete -c samurai -n __samurai_no_cmd -a security -d 'security state of this machine'
complete -c samurai -n __samurai_no_cmd -a firewall -d 'zones, ports and presets'
complete -c samurai -n __samurai_no_cmd -a welcome -d 'first steps on a new system'
complete -c samurai -n __samurai_no_cmd -a version -d 'print the samurai-tools version'
complete -c samurai -n __samurai_no_cmd -a help    -d 'usage'

# update
set -l upd '__fish_seen_subcommand_from update'
complete -c samurai -n $upd -l json -d 'JSON report on stdout'
complete -c samurai -n $upd -l noconfirm -s y -d 'answer pacman prompts with the default'
complete -c samurai -n $upd -l no-snapshot -d 'continue without a snapper rollback point'
complete -c samurai -n $upd -l skip-mirrors -d 'leave the mirrorlist alone'
complete -c samurai -n $upd -l skip-news -d 'skip the Arch news check'
complete -c samurai -n $upd -l ack-news -d 'acknowledge intervention news items'
complete -c samurai -n $upd -l allow-battery -d 'update on battery power'
complete -c samurai -n $upd -l skip-interventions -d 'skip every pending intervention (escape hatch)'
complete -c samurai -n $upd -l help -d 'usage'

# apps
set -l apps '__fish_seen_subcommand_from apps'
set -l apps_no_sub "$apps; and not __fish_seen_subcommand_from list categories search info bundles install remove check ids"
complete -c samurai -n $apps_no_sub -a list       -d 'apps by category'
complete -c samurai -n $apps_no_sub -a categories -d 'categories with counts'
complete -c samurai -n $apps_no_sub -a search     -d 'search apps'
complete -c samurai -n $apps_no_sub -a info       -d 'one app or @bundle in detail'
complete -c samurai -n $apps_no_sub -a bundles    -d 'curated bundles'
complete -c samurai -n $apps_no_sub -a install    -d 'install apps / @bundles'
complete -c samurai -n $apps_no_sub -a remove     -d 'remove apps / @bundles'
complete -c samurai -n $apps_no_sub -a check      -d 'catalog availability report'
complete -c samurai -n "$apps; and __fish_seen_subcommand_from install remove info" -a '(__samurai_apps_ids)'
complete -c samurai -n "$apps; and __fish_seen_subcommand_from install remove" -l yes -s y -d 'no confirmation'
complete -c samurai -n "$apps; and __fish_seen_subcommand_from install remove" -l dry-run -s n -d 'print the plan only'
complete -c samurai -n "$apps; and __fish_seen_subcommand_from install" -l skip-unavailable -d 'install what is available'
complete -c samurai -n "$apps; and __fish_seen_subcommand_from install" -l with-upgrade -d 'full upgrade + install in one transaction'
complete -c samurai -n "$apps; and __fish_seen_subcommand_from install" -l ignore-db-age -d 'ignore stale sync databases'
complete -c samurai -n "$apps; and __fish_seen_subcommand_from list" -l category -s c -r -a '(samurai-apps categories --ids 2>/dev/null)' -d 'only this category'
complete -c samurai -n "$apps; and __fish_seen_subcommand_from list" -l installed -d 'only installed apps'
complete -c samurai -n "$apps; and __fish_seen_subcommand_from list" -l available -d 'only apps ready to install'
complete -c samurai -n "$apps; and __fish_seen_subcommand_from list search" -l all -d 'include hidden entries'
complete -c samurai -n "$apps; and __fish_seen_subcommand_from check" -l strict -d 'exit 3 when a visible entry does not resolve'
complete -c samurai -n $apps -l json -d 'machine-readable output'
complete -c samurai -n $apps -l help -d 'usage'

# snap
set -l snap '__fish_seen_subcommand_from snap'
set -l snap_no_sub "$snap; and not __fish_seen_subcommand_from list create delete diff restore-file repair rollback"
complete -c samurai -n $snap_no_sub -a list         -d 'snapshots and bootable snapshot entries'
complete -c samurai -n $snap_no_sub -a create       -d 'create a snapshot with a label'
complete -c samurai -n $snap_no_sub -a delete       -d 'delete a snapshot'
complete -c samurai -n $snap_no_sub -a diff         -d 'changed files between two snapshots'
complete -c samurai -n $snap_no_sub -a restore-file -d 'restore one path from a snapshot'
complete -c samurai -n $snap_no_sub -a repair       -d 'recreate the snapper root config'
complete -c samurai -n $snap_no_sub -a rollback     -d 'roll the system back to a snapshot'
complete -c samurai -n $snap -l yes -s y -d 'no confirmation'
complete -c samurai -n $snap -l help -d 'usage'

# rescue
set -l rescue '__fish_seen_subcommand_from rescue'
set -l rescue_no_sub "$rescue; and not __fish_seen_subcommand_from mount chroot bootloader initramfs kernel rollback unmount"
complete -c samurai -n $rescue_no_sub -a mount      -d 'find, unlock and mount the install under /mnt'
complete -c samurai -n $rescue_no_sub -a chroot     -d 'mount + arch-chroot'
complete -c samurai -n $rescue_no_sub -a bootloader -d 'reinstall the bootloader'
complete -c samurai -n $rescue_no_sub -a initramfs  -d 'regenerate every initramfs'
complete -c samurai -n $rescue_no_sub -a kernel     -d 'reinstall kernels from the package cache'
complete -c samurai -n $rescue_no_sub -a rollback   -d 'snapshot rollback from outside the system'
complete -c samurai -n $rescue_no_sub -a unmount    -d 'unmount and close LUKS'
complete -c samurai -n $rescue -l yes -s y -d 'no confirmation'
complete -c samurai -n $rescue -l device -r -d 'root partition or LUKS container to use'
complete -c samurai -n $rescue -l help -d 'usage'

# status
complete -c samurai -n '__fish_seen_subcommand_from status' -l json -d 'machine-readable output'

# health
complete -c samurai -n '__fish_seen_subcommand_from health' -l category -x -a 'packages boot drivers storage snapshots services security hardware config' -d 'only this category'
complete -c samurai -n '__fish_seen_subcommand_from health' -l id -x -a '(samurai health --list 2>/dev/null)' -d 'only this check'
complete -c samurai -n '__fish_seen_subcommand_from health' -l json -d 'machine-readable output'
complete -c samurai -n '__fish_seen_subcommand_from health' -l list -d 'print the check ids'
complete -c samurai -n '__fish_seen_subcommand_from health' -l write -d 'write the report for the notifier'
complete -c samurai -n '__fish_seen_subcommand_from health' -l fix -d 'repair what has an automatic fix'
complete -c samurai -n '__fish_seen_subcommand_from health' -l yes -d 'approve risky fixes'

# diag
complete -c samurai -n '__fish_seen_subcommand_from diag; and not __fish_seen_subcommand_from info services logs disks gpu bundle' -a 'info services logs disks gpu bundle'
complete -c samurai -n '__fish_seen_subcommand_from logs' -l kernel -d 'kernel log'
complete -c samurai -n '__fish_seen_subcommand_from logs' -l pacman -d 'pacman log'
complete -c samurai -n '__fish_seen_subcommand_from logs' -l update -d 'last samurai update log'
complete -c samurai -n '__fish_seen_subcommand_from logs' -l lines -x -d 'number of lines'
complete -c samurai -n '__fish_seen_subcommand_from bundle' -l no-redact -d 'keep private data (asks twice)'
complete -c samurai -n '__fish_seen_subcommand_from bundle' -l out -r -d 'output file'

# clean
complete -c samurai -n '__fish_seen_subcommand_from clean' -a 'pkgcache journal coredumps thumbnails orphans cache all'
complete -c samurai -n '__fish_seen_subcommand_from clean' -l preview -d 'show sizes, change nothing'
complete -c samurai -n '__fish_seen_subcommand_from clean' -l json -d 'machine-readable preview'
complete -c samurai -n '__fish_seen_subcommand_from clean' -l yes -d 'approve confirmations'
complete -c samurai -n '__fish_seen_subcommand_from clean' -l keep -x -d 'package versions to keep'

# fix
complete -c samurai -n '__fish_seen_subcommand_from fix; and not __fish_seen_subcommand_from lock keyring dkms' -a 'lock keyring dkms'
complete -c samurai -n '__fish_seen_subcommand_from keyring' -l reset -d 'rebuild the keyring (asks)'
complete -c samurai -n '__fish_seen_subcommand_from keyring' -l yes -d 'approve the reset'

# mirrors
complete -c samurai -n '__fish_seen_subcommand_from mirrors' -a 'status refresh'

# boot
complete -c samurai -n '__fish_seen_subcommand_from boot; and not __fish_seen_subcommand_from sync get set' -a 'sync get set'
complete -c samurai -n '__fish_seen_subcommand_from set; and not __fish_seen_subcommand_from timeout param' -a 'timeout param'
complete -c samurai -n '__fish_seen_subcommand_from param; and not __fish_seen_subcommand_from add remove' -a 'add remove'
complete -c samurai -n '__fish_seen_subcommand_from sync' -l check -d 'report only'

# kernel
complete -c samurai -n '__fish_seen_subcommand_from kernel; and not __fish_seen_subcommand_from list install remove default' -a 'list install remove default'
complete -c samurai -n '__fish_seen_subcommand_from kernel; and __fish_seen_subcommand_from install remove default' -a 'linux linux-lts linux-zen linux-hardened linux-cachyos linux-cachyos-lts'

# hw
set -l hw '__fish_seen_subcommand_from hw'
complete -c samurai -n "$hw; and not __fish_seen_subcommand_from detect list apply remove fingerprint" -a 'detect list apply remove fingerprint'
complete -c samurai -n "$hw; and __fish_seen_subcommand_from detect list fingerprint apply" -l json -d 'machine-readable output'
complete -c samurai -n "$hw; and __fish_seen_subcommand_from list" -l installed -d 'installed profiles'
complete -c samurai -n "$hw; and __fish_seen_subcommand_from list" -l available -d 'profiles for this machine'
complete -c samurai -n "$hw; and __fish_seen_subcommand_from list" -l all -d 'every profile'
complete -c samurai -n "$hw; and __fish_seen_subcommand_from apply" -l profile -x -d 'apply this profile'
complete -c samurai -n "$hw; and __fish_seen_subcommand_from apply" -l reinstall -d 'run the profile setup again'
complete -c samurai -n "$hw; and __fish_seen_subcommand_from apply" -l switch-driver -d 'allow an NVIDIA branch change'
complete -c samurai -n "$hw; and __fish_seen_subcommand_from apply" -l dry-run -d 'preview only'
complete -c samurai -n "$hw; and __fish_seen_subcommand_from apply remove" -l no-snapshot -d 'continue without a snapshot'
complete -c samurai -n "$hw; and __fish_seen_subcommand_from apply remove" -l yes -d 'no confirmation'
complete -c samurai -n "$hw; and __fish_seen_subcommand_from fingerprint" -l check -d 'update the notice (root)'
complete -c samurai -n "$hw; and __fish_seen_subcommand_from fingerprint" -l accept -d 'accept the present hardware (root)'

# welcome
complete -c samurai -n '__fish_seen_subcommand_from welcome' -l text -d 'print here'
complete -c samurai -n '__fish_seen_subcommand_from welcome' -l json -d 'machine-readable'

# security
complete -c samurai -n '__fish_seen_subcommand_from security; and not __fish_seen_subcommand_from status tpm recovery-key' -a 'status tpm recovery-key'
complete -c samurai -n '__fish_seen_subcommand_from security; and __fish_seen_subcommand_from tpm' -a 'enroll remove'
complete -c samurai -n '__fish_seen_subcommand_from security; and __fish_seen_subcommand_from tpm recovery-key' -l dry-run -d 'show, change nothing'
complete -c samurai -n '__fish_seen_subcommand_from security' -l json -d 'machine-readable'
complete -c samurai -n '__fish_seen_subcommand_from security' -l online -d 'ask arch-audit too'

# firewall
set -l fw '__fish_seen_subcommand_from firewall'
complete -c samurai -n "$fw; and not __fish_seen_subcommand_from status presets open close zone" -a 'status presets open close zone'
complete -c samurai -n "$fw; and __fish_seen_subcommand_from open close" -a 'kdeconnect steam-remote-play steam-transfer sunshine localsend syncthing samba ssh minecraft ollama web-dev web'
complete -c samurai -n "$fw; and __fish_seen_subcommand_from open close" -l zone -r -d 'firewalld zone'
complete -c samurai -n "$fw; and __fish_seen_subcommand_from open close zone" -l dry-run -d 'show, change nothing'
complete -c samurai -n "$fw; and __fish_seen_subcommand_from open close zone" -l yes -d 'do not ask'

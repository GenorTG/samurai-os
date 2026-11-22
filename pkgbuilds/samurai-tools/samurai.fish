# fish completion for samurai (shipped by samurai-tools; fish is the default shell)

function __samurai_no_cmd
    not __fish_seen_subcommand_from update apps snap rescue status version help
end

function __samurai_apps_ids
    samurai-apps ids 2>/dev/null
end

complete -c samurai -f

# top level
complete -c samurai -n __samurai_no_cmd -a update  -d 'staged system update'
complete -c samurai -n __samurai_no_cmd -a apps    -d 'optional apps from the catalog'
complete -c samurai -n __samurai_no_cmd -a snap    -d 'snapshot manager'
complete -c samurai -n __samurai_no_cmd -a rescue  -d 'recovery tool (live ISO or installed system)'
complete -c samurai -n __samurai_no_cmd -a status  -d 'last update check and update report'
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

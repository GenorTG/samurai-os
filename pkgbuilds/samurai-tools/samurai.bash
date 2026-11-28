# bash completion for samurai (shipped by samurai-tools)
# Self-contained: does not need the bash-completion package.

_samurai_ids() {
    # catalog ids and @bundles only (no package index: stays fast)
    samurai-apps ids 2>/dev/null
}

_samurai() {
    local cur prev cmd sub
    COMPREPLY=()
    cur="${COMP_WORDS[COMP_CWORD]}"
    prev="${COMP_WORDS[COMP_CWORD-1]}"
    cmd="${COMP_WORDS[1]:-}"
    sub="${COMP_WORDS[2]:-}"

    if [ "$COMP_CWORD" -eq 1 ]; then
        mapfile -t COMPREPLY < <(compgen -W "update apps snap rescue status health diag clean fix mirrors boot kernel security firewall version help" -- "$cur")
        return 0
    fi

    case "$cmd" in
        update)
            mapfile -t COMPREPLY < <(compgen -W "--json --noconfirm --no-snapshot --skip-mirrors \
                --skip-news --ack-news --allow-battery --skip-interventions --help --version" -- "$cur")
            ;;
        apps)
            if [ "$COMP_CWORD" -eq 2 ]; then
                mapfile -t COMPREPLY < <(compgen -W "list categories search info bundles install \
                    remove check ids" -- "$cur")
                return 0
            fi
            case "$sub" in
                install)
                    mapfile -t COMPREPLY < <(compgen -W "--yes --dry-run --skip-unavailable \
                        --with-upgrade --ignore-db-age --help $(_samurai_ids)" -- "$cur") ;;
                remove)
                    mapfile -t COMPREPLY < <(compgen -W "--yes --dry-run --help $(_samurai_ids)" -- "$cur") ;;
                info)
                    mapfile -t COMPREPLY < <(compgen -W "--json --help $(_samurai_ids)" -- "$cur") ;;
                list)
                    if [ "$prev" = "--category" ] || [ "$prev" = "-c" ]; then
                        mapfile -t COMPREPLY < <(compgen -W "$(samurai-apps categories --ids 2>/dev/null)" -- "$cur")
                    else
                        mapfile -t COMPREPLY < <(compgen -W "--category --installed --available --all --json --help" -- "$cur")
                    fi ;;
                *)
                    mapfile -t COMPREPLY < <(compgen -W "--json --help" -- "$cur") ;;
            esac
            ;;
        snap)
            if [ "$COMP_CWORD" -eq 2 ]; then
                mapfile -t COMPREPLY < <(compgen -W "list create delete diff restore-file repair rollback" -- "$cur")
            else
                mapfile -t COMPREPLY < <(compgen -W "--yes --help" -- "$cur")
            fi
            ;;
        rescue)
            if [ "$COMP_CWORD" -eq 2 ]; then
                mapfile -t COMPREPLY < <(compgen -W "mount chroot bootloader initramfs kernel rollback unmount" -- "$cur")
            else
                mapfile -t COMPREPLY < <(compgen -W "--yes --device --help" -- "$cur")
            fi
            ;;
        diag)
            if [ "$COMP_CWORD" -eq 2 ]; then
                mapfile -t COMPREPLY < <(compgen -W "info services logs disks gpu bundle --help" -- "$cur")
            elif [ "$sub" = logs ]; then
                mapfile -t COMPREPLY < <(compgen -W "--kernel --pacman --update --lines" -- "$cur")
            elif [ "$sub" = bundle ]; then
                mapfile -t COMPREPLY < <(compgen -W "--no-redact --out" -- "$cur")
            fi
            ;;
        clean)
            mapfile -t COMPREPLY < <(compgen -W "pkgcache journal coredumps thumbnails orphans cache all \
                --preview --json --yes --keep --help" -- "$cur")
            ;;
        security)
            mapfile -t COMPREPLY < <(compgen -W "status --json --online --help" -- "$cur")
            ;;
        firewall)
            if [ "$COMP_CWORD" -eq 2 ]; then
                mapfile -t COMPREPLY < <(compgen -W "status presets open close zone --help" -- "$cur")
                return 0
            fi
            case "$sub" in
                open|close)
                    mapfile -t COMPREPLY < <(compgen -W "kdeconnect steam-remote-play steam-transfer \
                        sunshine localsend syncthing samba ssh minecraft ollama web-dev web \
                        --zone --dry-run --yes" -- "$cur") ;;
                zone) mapfile -t COMPREPLY < <(compgen -W "--dry-run --yes" -- "$cur") ;;
                *) mapfile -t COMPREPLY < <(compgen -W "--json" -- "$cur") ;;
            esac
            ;;
        kernel)
            if [ "$COMP_CWORD" -eq 2 ]; then
                mapfile -t COMPREPLY < <(compgen -W "list install remove default --help" -- "$cur")
            else
                mapfile -t COMPREPLY < <(compgen -W "linux linux-lts linux-zen linux-hardened linux-cachyos linux-cachyos-lts --yes" -- "$cur")
            fi
            ;;
        boot)
            mapfile -t COMPREPLY < <(compgen -W "sync get set timeout param add remove --check --help" -- "$cur")
            ;;
        mirrors)
            mapfile -t COMPREPLY < <(compgen -W "status refresh --help" -- "$cur")
            ;;
        fix)
            if [ "$COMP_CWORD" -eq 2 ]; then
                mapfile -t COMPREPLY < <(compgen -W "lock keyring dkms --help" -- "$cur")
            elif [ "$sub" = keyring ]; then
                mapfile -t COMPREPLY < <(compgen -W "--reset --yes" -- "$cur")
            fi
            ;;
        health)
            if [ "$prev" = "--category" ]; then
                mapfile -t COMPREPLY < <(compgen -W "packages boot drivers storage snapshots services security hardware config" -- "$cur")
            elif [ "$prev" = "--id" ]; then
                mapfile -t COMPREPLY < <(compgen -W "$(samurai health --list 2>/dev/null)" -- "$cur")
            else
                mapfile -t COMPREPLY < <(compgen -W "--category --id --json --list --write --fix --yes --help" -- "$cur")
            fi
            ;;
        status)
            mapfile -t COMPREPLY < <(compgen -W "--json --help" -- "$cur")
            ;;
    esac
    return 0
}

complete -F _samurai samurai

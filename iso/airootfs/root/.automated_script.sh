#!/usr/bin/env bash

script_cmdline() {
    local param
    for param in $(</proc/cmdline); do
        case "${param}" in
            script=*)
                echo "${param#*=}"
                return 0
                ;;
        esac
    done
}

automated_script() {
    local script rt
    script="$(script_cmdline)"
    if [[ -n "${script}" && ! -x /tmp/startup_script ]]; then
        # Samurai OS: https or a local path only. Plain http/ftp/tftp let anyone on
        # the network hand the live root a script to execute.
        if [[ "${script}" =~ ^(http|ftp|tftp):// ]]; then
            printf '%s: refusing non-https script URL %s\n' "$0" "${script}" >&2
            return 1
        fi
        if [[ "${script}" =~ ^https:// ]]; then
            printf '%s: downloading %s\n' "$0" "${script}"
            # there's no synchronization for network availability before executing this script; to ensure the network
            # is online, we use a transient systemd service that depends on network-online.target to download the
            # script rather than manually polling the target
            systemd-run --pty --quiet -p Wants=network-online.target -p After=network-online.target \
                curl "${script}" --location --retry-connrefused --retry 10 --fail -s -o /tmp/startup_script
            rt=$?
        else
            cp "${script}" /tmp/startup_script
            rt=$?
        fi
        if [[ ${rt} -eq 0 ]]; then
            chmod +x /tmp/startup_script
            printf '%s: executing automated script\n' "$0"
            # note that script is executed when other services (like pacman-init) may be still in progress, please
            # synchronize to "systemctl is-system-running --wait" when your script depends on other services
            /tmp/startup_script
        fi
    fi
}

if [[ $(tty) == "/dev/tty1" ]]; then
    automated_script
fi

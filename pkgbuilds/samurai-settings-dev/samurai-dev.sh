# Samurai OS developer environment for POSIX login shells (spec 13)
export DISABLE_AUTOUPDATER=1   # AI CLIs are updated by pacman
# conda command available, base environment never auto-activated
if [ -n "${BASH_VERSION:-}${ZSH_VERSION:-}" ] && [ -r /opt/miniforge/etc/profile.d/conda.sh ]; then
    . /opt/miniforge/etc/profile.d/conda.sh
fi
# rootless Docker: point the client at the user's own daemon socket
if [ -n "${XDG_RUNTIME_DIR:-}" ] && [ -S "$XDG_RUNTIME_DIR/docker.sock" ] && [ -z "${DOCKER_HOST:-}" ]; then
    export DOCKER_HOST="unix://$XDG_RUNTIME_DIR/docker.sock"
fi

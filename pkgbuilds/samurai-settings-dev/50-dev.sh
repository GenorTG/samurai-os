# interactive bash: developer hooks + PEP 668 hint (spec 13)
[ -r /etc/profile.d/samurai-dev.sh ] && . /etc/profile.d/samurai-dev.sh
pip() {
    if [ -z "${VIRTUAL_ENV:-}${CONDA_PREFIX:-}" ] && [ "${1:-}" = install ]; then
        echo "samurai: the system python is managed by pacman. Use 'uv venv', 'uv tool install' or a conda env." >&2
    fi
    command pip "$@"
}

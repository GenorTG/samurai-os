# Samurai OS developer environment for fish (spec 13)
set -gx DISABLE_AUTOUPDATER 1
if test -r /opt/miniforge/etc/fish/conf.d/conda.fish
    source /opt/miniforge/etc/fish/conf.d/conda.fish
end
if set -q XDG_RUNTIME_DIR; and test -S "$XDG_RUNTIME_DIR/docker.sock"; and not set -q DOCKER_HOST
    set -gx DOCKER_HOST "unix://$XDG_RUNTIME_DIR/docker.sock"
end
if status is-interactive
    function pip --wraps pip
        if not set -q VIRTUAL_ENV; and not set -q CONDA_PREFIX; and test "$argv[1]" = install
            echo "samurai: the system python is managed by pacman. Use 'uv venv', 'uv tool install' or a conda env." >&2
        end
        command pip $argv
    end
end

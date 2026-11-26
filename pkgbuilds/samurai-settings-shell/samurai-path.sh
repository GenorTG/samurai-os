# Samurai OS: per-user tools (uv tool, pipx) on PATH for login shells
if [ -n "${HOME:-}" ]; then
    case ":$PATH:" in
        *":$HOME/.local/bin:"*) ;;
        *) PATH="$HOME/.local/bin:$PATH"; export PATH ;;
    esac
fi

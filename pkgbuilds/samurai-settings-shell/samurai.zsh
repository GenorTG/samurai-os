# Samurai OS interactive zsh defaults (spec 10), sourced from /etc/zsh/zshenv
[[ -o interactive ]] || return 0
[ -r /etc/profile.d/samurai-path.sh ] && . /etc/profile.d/samurai-path.sh
if command -v eza >/dev/null 2>&1; then
    alias ls='eza --group-directories-first'
    alias ll='eza -l --group-directories-first --git'
    alias la='eza -la --group-directories-first --git'
fi
if command -v starship >/dev/null 2>&1 && [ "${TERM:-}" != linux ]; then eval "$(starship init zsh)"; fi
if command -v zoxide >/dev/null 2>&1; then eval "$(zoxide init zsh)"; fi

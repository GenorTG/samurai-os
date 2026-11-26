# Samurai OS interactive bash defaults (spec 10), sourced from /etc/bash.bashrc
[[ $- == *i* ]] || return 0
[ -r /etc/profile.d/samurai-path.sh ] && . /etc/profile.d/samurai-path.sh
if command -v eza >/dev/null 2>&1; then
    # `ls` itself stays the user's: Arch's default ~/.bashrc aliases it after us
    alias ll='eza -l --group-directories-first --git'
    alias la='eza -la --group-directories-first --git'
fi
if command -v starship >/dev/null 2>&1 && [ "${TERM:-}" != linux ]; then
    eval "$(starship init bash)"
fi
if command -v zoxide >/dev/null 2>&1; then eval "$(zoxide init bash)"; fi
if command -v fzf >/dev/null 2>&1; then eval "$(fzf --bash)"; fi
for _samurai_snippet in /usr/share/samurai/shell/bash.d/*.sh; do
    [ -r "$_samurai_snippet" ] && . "$_samurai_snippet"
done
unset _samurai_snippet

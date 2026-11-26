# Samurai OS shell defaults for fish (spec 10). Override in ~/.config/fish/.
if test -d $HOME/.local/bin; and not contains -- $HOME/.local/bin $PATH
    set -gx PATH $HOME/.local/bin $PATH
end
if status is-interactive
    set -g fish_greeting
    if command -q eza
        alias ls 'eza --group-directories-first'
        alias ll 'eza -l --group-directories-first --git'
        alias la 'eza -la --group-directories-first --git'
    end
    if command -q starship; and test "$TERM" != linux
        starship init fish | source
    end
    if command -q zoxide
        zoxide init fish | source
    end
    if command -q fzf
        fzf --fish | source
    end
end

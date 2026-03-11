zsh-patina() {
    "$_zsh_patina_path/target/release/zsh-patina" "$@"
}

# ensure the daemon is running
_zsh_patina_ensure_running() {
    local daemon_path="$_zsh_patina_path/target/release/zsh-patina"

    if [[ ! -x "$daemon_path" ]]; then
        echo "zsh-patina: daemon not found or not executable: $daemon_path" >&2
        return 1
    fi

    # `start` is a no-op when the daemon is already up, so this is always safe
    "$daemon_path" start
}

_zsh_patina() {
    # start=$EPOCHREALTIME

    if (( ! _zsh_patina_zsh_net_socket_available )); then
        print -u2 "zsh-patina: failed to load zsh/net/socket module"
        return
    fi

    # remove tokens we have set earlier - do not clear the whole array as this
    # might reset syntax highlighting from other plugins (e.g. auto suggestions)
    region_highlight=( ${region_highlight:#*memo=zsh_patina} )

    local socket_path
    socket_path="$HOME/.local/share/zsh-patina/daemon.sock"

    # if the socket does not exist, the daemon is stopped – try to start it
    if [[ ! -S "$socket_path" ]]; then
        _zsh_patina_ensure_running || return
        # give it a moment to create the socket
        sleep 0.1
        [[ ! -S "$socket_path" ]] && return
    fi

    # Split pre-buffer into lines. In a multi-line input at the secondary
    # prompt, the pre-buffer contains the lines before the one the cursor is
    # currently in.
    local pre_count
    local -a pre_lines
    if [[ -n "$PREBUFFER" ]]; then
        pre_lines=("${(@f)PREBUFFER}")
        pre_count=${#pre_lines}
    else
        pre_lines=()
        pre_count=0
    fi

    # Split edit buffer into lines
    local count
    local -a lines
    if [[ -n "$BUFFER" ]]; then
        lines=("${(@f)BUFFER}")
        count=${#lines}
    else
        lines=()
        count=0
    fi

    if ! zsocket "$socket_path" 2>/dev/null; then
        # if the socket exists but we cannot connect to it, the daemon might
        # have crashed - try to start it
        _zsh_patina_ensure_running
        sleep 0.1

        if ! zsocket "$socket_path" 2>/dev/null; then
            print -u2 "zsh-patina: failed to connect to socket at $socket_path"
            return
        fi
    fi
    local fd=$REPLY

    {
        # send header
        print -r -- "term_cols=$COLUMNS term_rows=$LINES cursor=$CURSOR pre_buffer_line_count=$pre_count buffer_line_count=$count"

        # send pre-buffer lines
        if (( pre_count != 0 )); then
            print -r -- "$PREBUFFER"
        fi

        # send lines
        if (( count != 0 )); then
            print -r -- "$BUFFER"
        fi
    } >&$fd || {
        print -u2 "zsh-patina: Write to socket failed"
        exec {fd}>&-
        return
    }

    local line
    while IFS= read -r -u $fd line; do
        [[ -n "$line" ]] && region_highlight+=("$line memo=zsh_patina")
    done

    exec {fd}>&-

    # alternative but spawns an additional process (i.e. nc):
    # printf '%s\n' "$1" | nc -U "$sock" 2>/dev/null

    # end=$EPOCHREALTIME
    # elapsed_ms=$(( (end - start) * 1000 ))
    # printf "%.3f ms\n" $elapsed_ms
}

if ! zmodload zsh/net/socket 2>/dev/null; then
    _zsh_patina_zsh_net_socket_available=0
else
    _zsh_patina_zsh_net_socket_available=1
fi

_zsh_patina_path="${0:A:h}"

autoload -U add-zle-hook-widget
add-zle-hook-widget line-pre-redraw _zsh_patina

# ensure the daemon is running
_zsh_patina_ensure_running

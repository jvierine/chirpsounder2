#!/usr/bin/env bash
# Install a reviewed native build without changing acquisition or transmitter timing.
set -euo pipefail
build_dir=${1:?directory containing iono-receiver, iono-prn and iono-plots}
source_root=${2:?absolute chirpsounder2 Rust source directory}
[[ "$source_root" == /* && "$source_root" != *[[:space:]%]* ]]
test -f "$source_root/Cargo.toml"
test -d "$source_root/crates/iono-receiver"
test -f "$build_dir/iono-prn"
state="$HOME/.local/state/chirpsounder2-rollout"
mkdir -p "$HOME/bin" "$HOME/.config/chirpsounder2" "$state"
names=(iono-receiver iono-prn iono-plots)
[[ ! -x "$build_dir/iono-verso" ]] || names+=(iono-verso)
for name in "${names[@]}"; do
    test -x "$build_dir/$name"
    if ldd "$build_dir/$name" | grep -q 'not found'; then
        echo "Missing runtime libraries for $name" >&2; exit 1
    fi
done
units=()
quote_arg() {
    local value=$1
    value=${value//\\/\\\\}
    value=${value//\"/\\\"}
    value=${value//%/%%}
    printf ' "%s"' "$value"
}
# Capture the running command lines before installing/restarting anything.
while read -r unit _; do
    [[ -n "$unit" ]] || continue
    pid=$(systemctl --user show "$unit" --property=MainPID --value)
    [[ "$pid" =~ ^[1-9][0-9]*$ ]] || continue
    mapfile -d '' -t argv < "/proc/$pid/cmdline"
    name=$(basename -- "${argv[0]}")
    if [[ "$name" == bash && ${#argv[@]} -gt 1 && "${argv[1]}" == */iono-delta-publish.sh ]]; then
        argv=("${argv[@]:1}")
        name=iono-delta-publish.sh
    fi
    case "$name" in iono-receiver|iono-prn|iono-verso|iono-plots|iono-delta-publish.sh|iono-delta-publish) ;; *) continue;; esac
    [[ "$name" != iono-verso || -x "$build_dir/iono-verso" ]] || continue
    [[ -e "$state/$unit.before.conf" ]] || systemctl --user cat "$unit" > "$state/$unit.before.conf"
    mkdir -p "$HOME/.config/systemd/user/$unit.d"
    for ((i=1;i<${#argv[@]};i++)); do
        # Preserve established, reviewed JSON content. Move only old GUI-owned
        # config locations; explicit ~/.config/iono-prn files stay in place.
        if [[ "${argv[i]}" == */src/ionogui2/*.json && -f "${argv[i]}" ]]; then
            destination="$HOME/.config/chirpsounder2/$(basename -- "${argv[i]}")"
            if [[ -e "$destination" ]] && ! cmp -s "${argv[i]}" "$destination"; then
                echo "Conflicting station config: $destination" >&2; exit 1
            fi
            cp -p -- "${argv[i]}" "$destination"
            argv[i]=$destination
        fi
    done
    {
        printf '[Service]\nWorkingDirectory=%s\nExecStart=\nExecStart=' "$source_root"
        case "$name" in
            iono-delta-publish*) quote_arg "$source_root/deploy/iono-delta-publish.sh";;
            *) printf '%%h/bin/%s' "$name";;
        esac
        for ((i=1;i<${#argv[@]};i++)); do quote_arg "${argv[i]}"; done
        printf '\n'
    } > "$HOME/.config/systemd/user/$unit.d/zzzz-chirpsounder2.conf"
    # Keep an in-flight capped upload alive; its next start uses the new path.
    [[ "$name" == iono-delta-publish* ]] || units+=("$unit")
done < <(systemctl --user list-units --type=service --state=running --plain --no-legend 'iono*')
for name in "${names[@]}"; do
    if [[ -e "$HOME/bin/$name" && ! -e "$state/$name.before" ]]; then
        cp -p -- "$HOME/bin/$name" "$state/$name.before"
    fi
    install -m755 "$build_dir/$name" "$HOME/bin/$name.next"
    mv -- "$HOME/bin/$name.next" "$HOME/bin/$name"
done
for name in "${names[@]}"; do sha256sum "$HOME/bin/$name"; done > "$state/installed-binaries.sha256"
systemctl --user daemon-reload
if ((${#units[@]})); then
    systemctl --user restart "${units[@]}"
    systemctl --user is-active "${units[@]}"
fi

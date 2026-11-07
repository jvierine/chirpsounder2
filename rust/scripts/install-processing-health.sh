#!/usr/bin/env bash
# Run as receiver user after transferring receiver-compatible release binaries.
# Does not restart acquisition, global discovery, or change processing budgets.
set -euo pipefail
source_dir=${1:?directory containing iono-receiver, iono-prn, iono-verso, iono-plots}
backup="$HOME/.local/state/iono-processing/backup-20261107"
mkdir -p "$backup" "$HOME/bin"
for b in iono-receiver iono-prn iono-verso iono-plots; do
    test -x "$source_dir/$b"
    if ldd "$source_dir/$b" | grep -q 'not found'; then echo "Missing dependencies for $b" >&2; exit 1; fi
    if [[ -f "$HOME/bin/$b" && ! -e "$backup/$b" ]]; then cp "$HOME/bin/$b" "$backup/$b"; fi
    install -m755 "$source_dir/$b" "$HOME/bin/$b.next"
    mv "$HOME/bin/$b.next" "$HOME/bin/$b"
done
while read -r unit rest; do
    command=$(systemctl --user show "$unit" -p ExecStart --value)
    case "$command" in *' detect '*|*' spectrum '*) continue;; esac
    case "$command" in
        *"$HOME/bin/iono-receiver "*|*"$HOME/bin/iono-prn "*|*"$HOME/bin/iono-verso "*|*"$HOME/bin/iono-plots "*)
            systemctl --user restart "$unit"
            printf '%s: ' "$unit"; systemctl --user is-active "$unit";;
    esac
done < <(systemctl --user list-units --type=service --state=running --no-legend 'iono*')
sha256sum "$HOME/bin/iono-receiver" "$HOME/bin/iono-prn" "$HOME/bin/iono-verso" "$HOME/bin/iono-plots" > "$backup/installed-sha256.txt"

#!/usr/bin/env bash
# Run as krea-sc after staging these deployment files and the two Linux binaries.
set -euo pipefail
test "$(id -un)" = krea-sc
test "$(hostname)" = krea-sc-OptiPlex-3050
source_dir=$(cd -- "$(dirname -- "$0")" && pwd)
receiver_binary=${1:?staged iono-receiver binary required}
plots_binary=${2:?staged iono-plots binary required}
publisher=${3:?staged iono-delta-publish.sh required}
for binary in "$receiver_binary" "$plots_binary"; do
    test -s "$binary"
    if ldd "$binary" | grep -q 'not found'; then
        echo "Missing shared library for $binary" >&2; exit 1
    fi
done
test -w /data0
install -d "$HOME/bin" "$HOME/.config/iono" "$HOME/.config/systemd/user"
systemctl --user stop iono-rust-detect iono-rust-spectrum iono-browser-arrays iono-delta-publish
install -m755 "$receiver_binary" "$HOME/bin/iono-receiver.next"
install -m755 "$plots_binary" "$HOME/bin/iono-plots.next"
mv "$HOME/bin/iono-receiver.next" "$HOME/bin/iono-receiver"
mv "$HOME/bin/iono-plots.next" "$HOME/bin/iono-plots"
install -m755 "$publisher" "$HOME/bin/iono-delta-publish"
install -m755 "$source_dir/ringbuffer-clean.sh" "$HOME/bin/iono-ringbuffer-clean"
install -m644 "$source_dir/receiver.json" "$HOME/.config/iono/receiver.json"
for unit in iono-acquisition iono-ringbuffer-clean iono-rust-detect iono-rust-spectrum iono-browser-arrays iono-delta-publish; do
    install -m644 "$source_dir/$unit.service" "$HOME/.config/systemd/user/"
done
systemd-analyze --user verify "$HOME/.config/systemd/user/iono-"*.service
systemctl --user daemon-reload
systemctl --user enable --now iono-ringbuffer-clean iono-acquisition iono-rust-detect iono-rust-spectrum iono-browser-arrays iono-delta-publish --no-block

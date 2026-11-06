# Deployed station DSP configuration

The station directories contain the actual native receiver JSON configurations,
not example timing defaults. `snapshot.json` records their hashes and collection
time. Keep paths under the matching account's `~/.config/` when installing.
`iono-prn/` belongs under `~/.config/iono-prn/`; `verso/` and other JSON files
belong under `~/.config/chirpsounder2/`. VERSO programs are generated with the
Rust importer from `../verso/SGO.yaml` and its binary waveform.

Ramfjordmoen Digisonde reception has first processing priority, SGO scheduled
LFM reception second, and global chirp detection third. Additional sounders
use lower CPU scheduling weights. Existing CPU and memory ceilings remain.
Acquisition remains above processing. Run `scripts/station-priority.sh --install`
from the Rust workspace, then install `iono-priority.service` and its timer
with the script at `~/.local/lib/chirpsounder2/station-priority.sh` to retain
legacy child priorities after automatic restarts. No acquisition restart is
required. Capacity measurements must use CPU time differences, not load
average alone or the available number of hyperthreads.

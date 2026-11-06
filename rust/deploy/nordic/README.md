# Native DSP on DOB and KHO

The units and deployed station JSON files replace Python Digisonde, scheduled
SGO LFM and global chirp detection. Rust `iono-plots --browser-only` already
prepares the RTI and display arrays locally. The retired Python plotters and
metadata consolidator are not needed. Python health monitoring and housekeeping
are control-plane tasks, not RF processing; UHD acquisition remains the existing
native recorder.

`install-native-dsp.sh STATION ABSOLUTE_BUNDLE REVIEWED_BINARY.gz` backs up existing files, installs
reviewed native units/configuration, stops only named legacy analysis workers,
and checks that the acquisition PID is unchanged. Never run the acquisition
launcher to migrate an already recording station. The normal launchers now
start the enabled Rust units after a boot/restart. Runtime limits and scheduling
preserve acquisition, Ramfjordmoen, scheduled SGO and global detection priority.
DOB's manual bundle/binary transfer is capped at 16 KiB/s. Native workers write
HDF5 locally; the existing receiver-local publisher handles bounded transport.

A language migration does not establish correct transmitter timing. The initial
migration preserved the old Dourbes/Chilton parameters. Their JSON
now incorporates GIRO-backed mode/cadence/range corrections; the individual
source/timing interpretation still requires on-air validation. Do not call those
ionograms validated merely because their files are fresh.

## Global detector capacity

The detector reuses its bounded FFT/peak buffers and dispatches complex-f32
multiplication through the shared AVX/FMA, AVX or SSE kernels. Recorder publication
grace is accounted for separately from the permitted processing backlog.
KHO has no AVX2/FMA. A receiver-local, single-worker benchmark under operating
load measured 1.292 s per 5,000,000-sample block versus 0.777 s per
4,194,304-sample block. These are wall times under the benchmark unit's 100%
CPU ceiling, not an end-to-end speedup or a guarantee of complete coverage.

KHO therefore uses four bounded workers and the power-of-two block length.
Input samples remain consecutive and timestamps derive from integer sample
indices. Coherent integration is 0.16777216 s rather than 0.2 s, with approximately
5.96 Hz rather than 5 Hz FFT bins, and approximately 0.77 dB less coherent energy
for a fixed-amplitude signal. This tradeoff is explicit in its JSON. Native
chirp products retain their actual sample count; a resumed summary bucket can
contain older rows, so its worker metadata describes the current writer only.
The detector's 2 GiB memory ceiling and lower priority protect primary reception.
Use logged skipped-block counts and observation age to assess actual coverage.

The receiver CLI also accepts `iono-receiver check-config CONFIG.json`. It checks
staged configuration without reading RF. Legacy non-aligned Digisonde upper
frequency limits retain their stop-exclusive bin counts. Runtime station JSON
is authoritative; old INI signal-processing sections are compatibility settings
and are not used by the enabled native services.

The active maintenance helpers now live under `station_ops/`. Update the
station checkout before installing a new native bundle. The installer checks
for those helpers before changing files; archived `python/` entry points must
not be used. A repository-only update does not restart existing receivers.

DOB Chilton now has an experimental frequency-search timing profile. See
[the on-air validation report](../../validation/dob-timing-20261106/README.md).
Its absolute propagation-range zero and transmitter association remain
provisional; other candidate modes were not promoted.

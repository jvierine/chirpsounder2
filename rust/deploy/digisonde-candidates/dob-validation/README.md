# Bounded DOB timing and mode checks

These are commissioning tools, not confirmed transmitter receivers. They never
change receiver clock calibration or open UHD. The recorder and higher priority
Ramfjordmoen/SGO processing stay running.

`launch.sh FUTURE_UTC_EPOCH` schedules 14 short, resource-limited captures at
4, 5 and 6 MHz over 112 seconds. Each is 100 ksample/s complex-f32 voltage,
not a full-band recording. Cached HDF5 voltage stays under the commissioning
root on DOB. This is one finite experiment, not an always-on additional worker.
Timers are persisted in the user unit directory and survive reboot. A power
outage can still remove voltage coverage; inspect successful-window counts.

For a saved cache, use the native replay and small-summary modes:

```sh
~/bin/iono-digisonde-verify replay.json --iq /absolute/path/to/cache.h5
~/bin/iono-digisonde-verify --summary /absolute/path/to/cache.h5 /absolute/path/to/summary.h5
```

Replay reads no DigitalRF source; the deliberately invalid `data_dir` makes that
separation explicit. It can compare 5/10/20 ms code templates, alternate
fine-frequency blocks and both A-B/O-X pulse orders against identical samples.
`replay_carrier_search_hz` sets a bounded search within the saved 100 kHz channel.
Split large hypothesis banks or reduce Doppler extent if CPU/runtime limits
prevent completion. A cache is limited to 512 windows, about 62.5 MiB of IQ.

A normal candidate HDF5 file supplies `sample_index` (u64), `frequency_hz` (f64)
and `fold_contrast` (f32). Pass its maximum row budget followed by `--dense`
only when adjacent windows are intentional. The ordinary discovery default
continues to suppress repeated tests of a stationary carrier. Inspect
`requested_windows`, `attempted_windows` and `successful_windows` separately.

Only transfer metadata-only summaries across DOB's limited link; never rsync
the IQ cache or raw ring buffer. Summary export preserves sample/carrier axes,
coverage counts and code-match diagnostics, and omits the voltage dataset.
`iono-digisonde-timing-plot` renders these summaries with Rustplotlib.

Isolated code peaks are not transmitter identification or calibrated false-alarm
probabilities. Require repeatable frequency-time progression, a supported code
order, pulse timing, and a plausible propagation interval. Codeword lag contains
packet-phase and repetition ambiguities; it is not automatically absolute range.
GIRO measurement timestamps alone do not calibrate transmitter pulse epochs.

The cache holds independent 160 ms channelized windows. The oscillator is
frame-relative. Do not concatenate their complex samples for coherent
integration without accounting for the integer source sample indices and
recorded carrier frequency. Independent per-window code replay does not need
that extra phase alignment.

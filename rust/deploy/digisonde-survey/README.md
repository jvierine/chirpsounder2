# Receiver-local Digisonde discovery survey

DOB's finite survey searches 0.7–24.5 MHz in the existing 25 MHz complex64 ring
buffer. It does not open UHD, retune acquisition, use the web server's CPU or
transfer raw voltage. The unit is manually started, has no boot install target,
and ends after a 30-minute survey (35-minute hard runtime limit). Scheduled
Ramfjordmoen and SGO reception and global chirp detection retain priority.

The PINS-style finder soft clips narrow spectral peaks, computes a pulse-folded
STFT and records coarse 5/10 ms recurrence candidates. Stratified selection
limits each MHz to two candidates so a strong radar band cannot consume the
whole search budget. The configured 0.16-second windows are spaced 0.32 seconds
apart: this is sampled coverage, not continuous voltage coverage. One native
worker bounds memory, and the Linux idle CPU policy with a one-core ceiling
keeps it below the primary receivers. Peak CPU and missing-window counts must
be checked on the actual host before extending a survey.

Every eighth window can verify one candidate directly from the same voltage,
with MHz-band rotation. The code matcher searches 5/10/20/40 ms effective pulse
spacing and the existing complementary-code/polarization-mode hypotheses,
±30 kHz carrier refinement in 5 kHz increments plus ±5 residual FFT bins.
This is bounded diagnostic sampling, not exhaustive verification of every
candidate. It retains both failed attempts and matches under HDF5 `code_checks`.
Scores and coherence are not calibrated false-alarm probabilities, and lags
are circular correlation coordinates, not calibrated propagation delays.

Atomic compressed HDF5 checkpoints are written after the first processed window
and at least every 30 seconds while work proceeds. The file includes exact
sample indices, planned/scanned coverage, missing-window counts, worker/stride
settings and the searched RF band. Voltage remains local; export only compact
candidate/check metadata, with DOB transfers capped at 16 KiB/s. Output is
outside the public science-product directories and never labels candidates as
known transmitters automatically. Require a consistent stepped-frequency track,
repeat schedule and independent station/program evidence for identity. A
30-minute snapshot cannot guarantee detection of every audible Digisonde, much
less sounders not propagating to DOB during that interval.

Install the reviewed compatible binary under `~/bin`, the JSON under
`~/.config/chirpsounder2/` and the unit under `~/.config/systemd/user/`, then:

```sh
systemctl --user daemon-reload
systemctl --user start iono-digisonde-survey.service
journalctl --user -fu iono-digisonde-survey.service
```

A short commissioning test can use `iono-digisonde-search CONFIG --duration 20`.
Legacy one-shot behavior remains available with `CONFIG --lookback 20`. Existing
configs retain four workers and 0.32-second stride unless explicitly changed.
Do not enable continuous site receivers from raw pulse recurrence alone.

DOB commissioning on 2026-11-06 processed 31 windows over 20 seconds at a
0.64-second stride, with zero missing windows, 1063 coarse candidates and
6.400 CPU seconds. The survey stride was then reduced to 0.32 seconds while
retaining one worker and the one-core ceiling. Counts are search candidates,
not identified transmitters.

## Summarising and inspecting a completed survey

`iono-digisonde-tracks survey.h5 tracks.h5` runs on the receiver and exports a
small HDF5 summary, including all sampled code checks and the original coverage
counts. It seeds upward frequency slopes from 19.5–500 kHz/s, then refines each
candidate by linear regression. It requires at least 12 supporting windows,
three seconds and 250 kHz of RF progression, and breaks tracks over gaps longer
than three sample strides. Stationary carriers are rejected. Nearby duplicate
fits are collapsed, with at most 100 retained tracks. These are bounded search
hypotheses, not calibrated detections: the finder can also follow LFM radars,
interference and aliases. Source candidates remain on the receiver for review.
The actual searched rates are stored in the summary. Code checks are sampled;
zero checks on a track means untested, not a failed waveform match.

`iono-digisonde-survey-plot survey.h5 overview.png` produces a bounded Rustplotlib
overview with UTC labels, pulse contrast and sampled code coherence. It bins
coarse display points at 25 kHz and caps the time-axis raster to 1200 bins.
The overview is diagnostic; it does not place transmitter names on the points.
TeX is required in the normal Rustplotlib installation. Raw numeric results
and source products remain HDF5.

The first six-minute DOB segment recovered a 312.5 kHz/s stepped-frequency
track whose onset agrees with the existing Juliusruh 196-second offset in its
five-minute program. Parallel approximately 78.125 kHz/s families agree with
the nominal 1 MHz and 2 MHz starts on the 450-second schedule. The first is
consistent with known Ramfjordmoen reception; the second is shared by Thule
and Millstone timing and remains unassigned. Their identities must not be
inferred merely from these slopes and approximate starts. Low-coherence
isolated code matches do not settle the association.

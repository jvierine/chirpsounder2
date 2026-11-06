# Finnish SGO VERSO coded sounders

The supplied `SGO.yaml` and `SGO_bauds_complex128.bin` describe four stations
using one shared waveform. These are the technical attachments supplied by
Sari Lasanen on 30 March 2026. Her 12 May follow-up confirmed permission to
share/publish the waveform. Private email correspondence is not included.

## Reading the schedule

The complete cycle is **240 seconds**, beginning at **HH:03:55**, then
HH:07:55, HH:11:55, etc. Its Unix-time phase is 235 modulo 240. Whole-hour
time-zone offsets do not change this phase because 240 divides 3600.

| Transmitter ID | Site | Seconds after cycle start |
| --- | --- | --- |
| JOU | Joutseno | 0–60 |
| KUU | Kuusamo | 60–120 |
| VAA | Vaasa | 120–150 and 180–210 |
| SDT | Sodankylä | 151–179 and 211–239 |
| XXX | No transmission | 150–151, 179–180, 210–211 and 239–240 |

Keep the silent rows when accumulating time. Do not group the two Vaasa or
Sodankylä sections into a contiguous transmission.

`exp_runlist` contains duration, transmitter waveform ID, hardware tuning
frequency and site ID. **Actual received RF frequencies are `groups_subch`**;
the hardware tuning column is not an ionogram frequency. Waveform IDs select
transmitter programs and are not different receiver code seeds.

The binary holds **625 little-endian complex128 values**, not complex64.
At 62.5 ksample/s they form a **10 ms BPSK pulse**. Channel k starts
`k × 625` native samples, or `k × 10 ms`, after the group starts. An IPP
of 7500 means **120 ms**, and 6250 means **100 ms**. A six-second or
five-second active group therefore contains 50 pulses per channel. The receiver
includes the delayed end of the final pulse instead of truncating at the group
boundary. There are deliberate RF gaps; the licensed channels are not a
continuous frequency sweep.

## Native reception

Build `iono-verso` in the Rust workspace, then convert the supplied attachments
once into a typed HDF5 program:

```sh
iono-verso import SGO.yaml SGO_bauds_complex128.bin SGO_VERSO.h5
```

This conversion is also Rust. The program contains native clock units, channel
offsets, the complex64 waveform, silent slots and source checksums. Runtime
reception reads this HDF5 program; it does not parse email or run Python.
Copy the appropriate receiver JSON, program HDF5 and `iono-verso.service` to
the locations used by the unit, then enable the unit with the user manager.
The receiver reads the existing 25 MHz RAM ring and leaves UHD acquisition
untouched. Its working set and CPU have independent limits.

```sh
iono-verso receiver.json --once --ready-only
systemctl --user daemon-reload
systemctl --user enable --now iono-verso
```

`--start UNIX_SECONDS` replays a retained cycle. `--ready-only` skips future
groups during commissioning. Ordinary operation waits for complete measured
pulse windows. Missing voltage never becomes synthetic data. Restarting resumes
accepted columns only when the program checksum and receiver config agree.

The 25 MHz → 100 kHz channelizer uses per-CPU complex64 kernels. Pulse
compression is linear FFT correlation with sufficient zero padding, followed
by coherent pulse-Doppler integration. Real FIR overlap preserves the timing.
All 10 ms inter-channel offsets are removed before assigning apparent range.

Sparse HDF5 ionograms use an **8 dB display threshold**, a **3 dB target versus
conjugate time-reversed-code margin**, and the GUI's **0–20 dB** PRN display.
The control is formed **after resampling**, preserving exactly equal Fourier
magnitude and energy. Reversing the native baud sequence before rational
resampling does not preserve that equality and can falsely favor a carrier
near a spectral null. A carrier regression test covers this failure. The check is not a
calibrated false-alarm probability or an independent transmitter identifier;
site association follows the supplied TDM schedule.

## Commissioning limits

The email mentions mismatched filtering but does not supply that receiver
filter. This implementation explicitly uses **matched filtering**, not an
invented replica of SGO's processing. The supplied unfiltered waveform has
approximately −14.48 dB peak aperiodic correlation sidelobes. Strong echoes can
therefore leave weaker range sidelobes. The output records this limitation.
Absolute transmitter/receiver group delay, station positions and the current
on-air schedule need physical validation before operational LUF/MUF estimates.
The GUI keeps these paths in commissioning and withholds automatic usable
frequency limits while retaining the diagnostic ionograms and RTIs.

At KHO the same measured recorder lag applies to VERSO. Its receiver JSON
uses `writer_grace_s: 4.5`; other stations retain their established default.
Only file availability is delayed. Group times, pulse offsets, apparent-range
calibration and detection thresholds are unchanged. A grace-only change can
resume the same scientific observation without discarding completed channels.

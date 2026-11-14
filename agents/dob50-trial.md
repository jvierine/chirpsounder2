# DOB 50 MHz acquisition trial

DOB only. Existing receiver hosts and their configuration are outside this
trial. The original DOB checkout and its native executables are preserved.

## Recorder interface

`rx_uhd_ext_gps --mode 50mhz` requests 50,000,000 complex samples/s centred
at 25 MHz, with signed 8-bit I and Q both on the wire and in Digital RF.
Nominal RF coverage is 0–50 MHz; analog response and the band edges are not
calibrated by requesting that sample rate. `--mode 25mhz` is the unchanged
25 MS/s, 12.5 MHz-centred, signed-16-bit default. `--rate 25e6` / `50e6`
remain explicit aliases; conflicting/unsupported settings fail before opening
hardware. `--help` does not open the radio.

Received packet lengths and UHD sample timestamps determine the stored index.
The old fixed 363-sample assumption and repeated-data gap filling are removed.
A real gap stays a gap. Coerced sample rates or centre frequencies are rejected.
`--seconds` provides a bounded hardware trial. SIGINT/SIGTERM stop the stream
and close both writers.

`--compat25-outdir PATH` creates a 25 MS/s i16 lower-band view. A UTC-anchored
quarter-rate oscillator `[1,j,-1,-j]` moves the lower half to a 12.5 MHz centre;
a symmetric 33-tap half-band filter suppresses aliases before decimation.
Its 16-input-sample delay is removed from the output timestamps. Filter warmup
and post-gap warmup are omitted, never padded with fabricated voltage.
Arithmetic is widened before negating -128 or summing; narrowing is clamped
and counted. The output retains 8-bit input information, not 16-bit accuracy.

The simple two-sample boxcar was not selected because its weak alias rejection
could turn upper-band signals into false lower-band observations. The signed
8-bit synthetic test suppressed a 30 MHz alias by 39.36 dB relative to a
5 MHz signal. The test bound allows one input quantization level; it is not a
universal stop-band or weak-signal calibration claim.

## Statistics

`--stats-dir /data0/recorder-health/dob50` writes one HDF5 file per UTC hour,
channel and recording UUID, refreshed atomically about once a minute and on
shutdown. UTC hour boundaries split the received sample and rail counts.
Multiple partial recordings in the same hour have different UUIDs; aggregate
counts rather than averaging their fractions. Finalized hour buckets have `complete=1`, even when the recording started
partway through that hour. Use first/last sample indices and received/missing
counts for actual coverage; the current hour bucket remains zero.

Fields include received complex samples, rail-hit samples/components,
near-rail samples, maximum component magnitude, mean complex power,
conversion clamps, UHD overflow events, timestamp-derived missing samples,
startup missing samples and backward packets. `wire_peak_fraction`, component
width, centre frequency and sample rate describe scaling and capture geometry.
Rail hits are observable clipping indicators, not proof of a pre-quantization
ADC overflow or a way to reconstruct wrapped/clipped values. UHD overruns are
transport/data-loss events and are counted separately.

The first 45-second hardware invocation yielded 2,010,120,192 received complex
samples at 50 MS/s, zero UHD overflow events, one rail-hit sample and seven
startup missing samples (the initial version grouped those seven under missing
samples). There were no later timestamp gaps. It saved native signed-8-bit
50 MHz data and signed-16-bit 25 MHz compatibility data, then automatically
restored the original acquisition. One native second of each remains on DOB
under `/data0/recorder-health/dob50-probe` for independent reader checks.

## Sustained trial layout

- Working files/binaries: `/home/tgo/src/dob50-trial`.
- Raw ring: `/dev/shm/hf50`, 40 seconds.
- Compatibility ring: `/dev/shm/hf25`, 90 seconds.
- Combined nominal voltage storage: about 13 GB, below the RAM-filesystem size.
- Montelibretti gets a separate 50 MHz-capable PRN binary/configuration and a
  1–30 MHz frequency table. Its 300-second cycle is retained; the added upper
  frequency schedule still needs on-air validation.
- Other sounders retain their 25 MHz inputs and existing frequency tables.
- The background detector can search the native 50 MHz input at lowest CPU
  priority. A separate local-only spectrum writes to `/data0/dob50-diagnostics`;
  it does not mix incompatible frequency axes into today's published spectrum.
- The original acquisition is `chirpsounder_dombas.service`; its original
  runtime/configuration files are not edited. Temporary user-unit overrides
  select the new Montelibretti and background-detection executables.
- `rollback.sh` removes these specific overrides and restores the original
  acquisition and previously active processing services. `begin-trial.sh`
  arms an independent 12-minute rollback timer before switching hardware.
  Cancel that timer only after verifying capture and processing health.

The first native full-scale (`peak=1`) trial left little of the 8-bit range in
use. The sustained trial selects `peak=0.125`; its clipping and signal level
must be checked before acceptance. Scaling changes are recorded in statistics.
The external-reference sensor already reported unlocked on the old recorder;
this trial does not establish absolute reference/PPS calibration.

## Frequency evidence

The trial records the full nominal 0–50 MHz band without relabelling radar
candidates as confirmed sources. Available references do not establish Cyprus
operation at 50 MHz or JORN at 40 MHz. The IARU Region 1 monitoring manual
hosted by WIA describes Cyprus around 10–29 MHz. The Australian Navy's
*Managing and Protecting the Offshore Estate* discusses OTH operation around
5–32 MHz; the DST JORN sounder paper describes the radar band as 3–30 MHz.
These references constrain what can be claimed, not which unknown signals the
wider receiver is allowed to record.

- https://www.wia.org.au/members/protecting/about/documents/Region%20One%20Monitoring%20Manual.pdf
- https://seapower.navy.gov.au/sites/default/files/2023-02/Managing_and_Protecting_the_Offshore_Estate.pdf
- https://doi.org/10.1002/2015RS005881

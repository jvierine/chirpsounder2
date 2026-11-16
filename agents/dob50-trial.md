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

## N200 amplitude scale

UHD's N2xx `peak` stream argument controls device scaling before sc8 packing.
The recorder also accepts `--digital-gain-db G`, with nominal
`peak = 10^(-G/20)`. Specify one of these controls, not both. Accepted peak
fractions are 1/256 through 1; UHD internally clamps smaller N200 values, so
the recorder rejects those rather than recording an inaccurate setting.
This is digital quantizer scaling; analog gain depends on the daughterboard.
It cannot repair clipping that has already happened in the ADC.

The first sustained trial used `peak=0.125` (nominal +18.06 dB). A subsequent
DOB-only amplitude trial uses `peak=0.03125` (nominal +30.10 dB), a further
fourfold amplitude increase. Check measured mean power and rail counts before
accepting it. `set-peak.sh FRACTION` changes the dedicated acquisition unit and
restarts only that recorder; its hourly files identify each new recording UUID,
peak fraction and nominal digital gain. A deliberate resynchronization gap is
expected when changing the setting.

References:
- https://files.ettus.com/manual/page_configuration.html#stream_args
- https://github.com/EttusResearch/uhd/blob/UHD-4.6/host/lib/usrp/cores/rx_dsp_core_200.cpp

## Signed-byte reader

The first native-50 MHz PRN run was throughput-limited by the generic HDF5
compound-i8 to complex-f64 conversion path (roughly nine seconds per one-second
carrier dwell). The direct signed-byte path in `rdigital_rf` commit
`2528df685b63fbc75e1b9d88a8eadaef93f90c79` reads the native i8 compound type
and widens it directly into complex-f32. Its tests retain -128/+127 exactly and
preserve indexed gaps. Existing `bounds` and `read_complex32` semantics remain
unchanged. No raw-binary backend is deployed; a comparable backend is an option
if the measured direct-read performance still fails to keep up.

## Live follow-up, 2026-11-15 UTC

At peak=0.03125 the 00 UTC bucket reached 21,000,028,160 received samples,
541 rail samples (2.58e-8 fraction), zero UHD overruns and zero post-start
missing samples. This is a measured clipping indicator, not a calibrated ADC
clipping count. Recorder CPU was about 85% of one core; the RAM rings remained
at 81% usage. The direct reader is deployed. A 40-million-sample archived i8
read took 1.175409 s under live station load; live Montelibretti columns took
2.5–2.9 s, versus about 9 s before the reader change. The 30 MHz column was
processed, but no transmitter identification or echo at that frequency was
established. Native-50 decoding still skips columns and is not cadence-complete.
A raw backend is now implemented in `rdigital_rf`, but is not connected to the
C++ recorder. The recorder still writes HDF5. The healthy 50 MHz acquisition is
retained independently of that decoder performance limitation.

## Shared feeds and reader throughput, 2026-11-16

Current deployment supersedes the pending rollout described below. The recorder
derives `/dev/shm/hf25` once from `/dev/shm/hf50`. All inspected 25 MHz workers
share hf25; Montelibretti, wideband detection and the full-band spectrum share
hf50. Both rings still use HDF5. Acquisition was not restarted during this work.

The PRN slice's inherited 150% cap throttled every period in a five-second
sample. `unlimit-prn.sh` removes only that DOB quota, retaining CPU weights;
the five-second post-change sample had zero quota throttling. Historical
Huairou health was moved under `iono-processing/retired/` after stopping it.

`rdigital_rf` main `8419ce10a53a9a3ce3b21b50e9e056f926f6994e` coalesces
continuous packet-index entries and preserves the two-dimensional HDF5 slab
for a single channel. Coalescing alone showed little wall-time improvement;
the rank-preserving read produced the measured speedup on DOB's HDF5 stack.
Three alternating archived Montelibretti comparisons agreed within rtol/atol
2e-5, as did an Uppsala comparison over the same still-retained live samples.
The final production binary repeated the archived comparison: 0.994 to 0.222 s
per Montelibretti carrier; its read stage took 0.145 s. Tests: 31 reader tests,
100 official DigitalRF 2.6.14 parity checks, and 14 receiver tests passed.

At 06:28:44 UTC the new binaries were installed for 18 DOB services, including
PRN, VERSO, LFM, spectra, search and standard Digisondes. Running process hashes
were checked. Backup binaries and unit list are in
`/home/tgo/src/dob50-trial/install-state/io-20261116T062844`; use its recorded
units and `bin/`/`dob50-trial/` files for rollback. Acquisition PID 2265291
stayed unchanged. Installed SHA256 values:

- iono-prn: `8b283495b902d51056ca8e72bb69858ad0faa4b355fc4d2d27f7ffd69a5f358e`
- iono-receiver: `45363864c623d00027be83c600bc3b66028bafc35ca82398d0026229d05a9e55`
- iono-verso: `4ab560435013a0e30127a1b73786c376f2b40d8b58aebd8171ef376fe9cad8b0`

Live post-update carrier times were Kiruna 0.092–0.096 s, Uppsala
0.191–0.247 s and Montelibretti 0.196–0.236 s. All three PRN rolling
15-minute loss counters were zero locally and in the public API. The PRN
group used 0.53–0.70 CPU cores in five-second post-update samples, versus
2.49 immediately after removing the quota. A settled machine sample used
59.0% total CPU. These are observed windows, not fixed station capacity.

The recorder's current-hour count was four UHD overflows / 450120 missing
samples (9.0 ms at 50 MHz), unchanged between the 06:30 and 06:32 health
updates. Their earlier timing was not established; do not claim a zero-loss
hour. Rail fraction remained about 6.2e-8. Numerical timing, CPU and recorder
snapshots, plus processed replay comparisons, are retained in HDF5 under
`/data0/recorder-health/dob-performance/`. No RF files were copied off DOB.

## Typed-reader rollout, 2026-11-15 (historical)

The receiver dependency now pins `rdigital_rf` main commit
`677dff53d1fd98453670470b49300e9302bf3cd0`. PRN, VERSO and the generic receiver
built on work-laptop; 14 receiver tests and seven relevant core tests passed.
The source pin is on chirpsounder2 main (`f837c5d`). These builds are **not yet
installed as running DOB consumers**: the reverse tunnel repeatedly stalled
during transfer. The dedicated stale forwarding session was reset and briefly
reconnected, then became unresponsive again. Acquisition was not restarted;
its last inspected PID was 1953931. Do not report the latest reader as deployed.

Verified artifacts are staged on juha.no in `/tmp/dob-reader-677dff5/`, with
`SHA256SUMS` and `update-reader.sh`. Once DOB access is stable, transfer that
directory at a total maximum of 16 KiB/s, verify checksums on DOB, and run
`bash update-reader.sh STAGED_DIRECTORY`. This updates only the native-50 PRN,
detector and spectrum consumers, backs up their old binaries and checks that
acquisition's PID remains unchanged. Other DOB workers still need an explicit
review and update; no other station is in scope.

The staged `dob-reader-comparison-bin` compares old `2528df6` and new `677dff5`
`read_complex32` calls, alternating order and checking every returned component
bitwise. Its arguments are ROOT START LENGTH REPORT.h5. A bounded archived
fixture exists at `/data0/recorder-health/dob50-probe/hf50`; use start
89733140650100007 and length 2504000. Sources are retained on work-laptop in
`/tmp/dob-reader-comparison/`. The on-station comparison remains pending.

Before attributing the remaining PRN delay to I/O, inspect the live
`iono-prn.slice` CPU quota and its cgroup throttling counters. The repository's
slice configuration caps all PRN instances together at 150%; the observed four
PRN processes each used about 37.6% CPU. Live Montelibretti dwell wall times
were 2.6–2.9 s at 02:19–02:22 UTC. This suggests a shared CPU cap, but the live
quota has not yet been confirmed or modified. Preserve acquisition priority
and keep global search below ionogram processing when correcting the budget.

## Public sweep plots

Huairou was retired from DOB on 2026-11-16 at the user's request after
unsuccessful reception and CPU contention. `iono-prn@Huairou.service` is stopped
and boot-disabled; its configuration and historical products are retained.
`retire-huairou.sh` reapplies this state, and the DOB rollback excludes Huairou
from the saved list of formerly active services. Other receivers are unchanged.

The 48-hour link-history path had a separate 25 MHz cutoff. Commit `fe756c5`
extends DOB link arrays, assumed-source envelopes and frequency color scales to
50 MHz, with a browser fallback that preserves old arrays at their original
frequencies and leaves the upper band empty. The matching `iono-plots` binary
(`0be5407181466182c07604a2cda70725a639a9de8b18de62a9b04e5e59a2747f`)
was installed on DOB at 02:44 UTC on 2026-11-15; acquisition PID 1953931 stayed
unchanged. The public bulk bundle generated at 02:44:10 was verified to contain
0–50 MHz Cyprus link-history arrays and both 100/125 kHz/s sweep products. The
live Cyprus popup was visually inspected. Five JavaScript tests and all 25
iono-plots tests passed. The old exporter is retained as
`/home/tgo/src/dob50-trial/iono-plots.before-fe756c5`.

`iono-plots` used to discard >25 MHz rows while building its browser arrays,
even though native detection searched the full capture band. The full-band
worker accepts `IONO_SWEEP_MAX_MHZ=50` through the DOB-only service overlay
installed by `publish-detections.sh`. It extends both the frequency/range axis
and the peak-frequency color scale; other receivers default to 25 MHz. Bins
above 25 MHz retain actual clustered detection rows, not filled data. A test
covers 40–44 MHz clusters at both 100 and 125 kHz/s. This does not change the
separate spectrum or LFM workers. The full-band view keeps both rate cards visible, including empty arrays
when no clusters pass acceptance. Other receivers retain the legacy behavior.

# Coded sounder reception

The independent `iono-prn` Rust binary reads the existing 25 MHz DigitalRF
ring buffer. It never opens UHD or changes acquisition. Retained observations
are HDF5 under `/data0/YYYY-MM-DD/prn_ionogram-TX-RX-EPOCH.h5`. Receiver-local
`iono-plots` indexes them and publishes the existing bounded browser bundles.
The public server does not read raw voltage or reconstruct ionograms.

## Transmitter configurations

Martin Rönnfalk's supplied configurations and timing patches establish:

* Kiruna KI167: NumPy RandomState seed 4; 10000 chips, 100 kchip/s, ten active
  chips per 500-chip IPP; one second per frequency, 120-second repeat, UTC
  cycle phase +60 seconds.
* Uppsala UP158: seed 2, same code/pulse geometry; two seconds per frequency,
  240-second repeat, UTC cycle phase zero. Only its low-frequency antennas
  were operational in the accompanying message; retain the frequency table
  without assuming equal sensitivity throughout the band.
* Lycksele is disabled. IRF's live latest-image endpoint checked on
  2026-11-05 still returned an image last modified 2024-08-11. Kiruna and
  Uppsala returned current images. Do not infer operation from an old image.

The JSON files retain exact irregular frequency tables and source hashes.
`scripts/import_prn_config.py` is configuration conversion only; run with
`conda run -n base python`. No Python runs in the new DSP workers.

## Active files, not the old experimental INIs

| Receiver | Reviewed deployment configuration | Installed configuration |
| --- | --- | --- |
| DOB | `DOB-Kiruna.json`, `DOB-Uppsala.json` | `~/.config/iono-prn/Kiruna.json`, `~/.config/iono-prn/Uppsala.json` |
| DOB (Italian reception trial) | `DOB-Montelibretti.json` | `~/.config/iono-prn/Montelibretti.json` |
| TGO | `TGO-Kiruna.json`, `TGO-Montelibretti.json` | `~/.config/iono-prn/{Kiruna,Montelibretti}.json` |
| KHO | `KHO-Kiruna.json`, `KHO-Montelibretti.json` | `~/.config/iono-prn/{Kiruna,Montelibretti}.json` |

`iono-prn@Kiruna` and `iono-prn@Uppsala` explicitly name these installed files.
No automatic INI discovery occurs. The old seed-1 `irf_prn.ini`,
`irf_prn2.ini` and `irf_prn_cw.ini` are not active in this pipeline; existing
source experiments are preserved. Only Martin's `irf_prn_kiruna_2min.ini`
and `irf_prn3_uppsala.ini` supply the currently enabled Swedish programs.

Map coordinates are in `web/paths.json` (compiled into the thin server's
station catalogue). Kiruna's supplied INI has zero latitude/longitude
placeholders; the map uses the existing IRF site position 67.840741 N,
20.410559 E instead. Uppsala uses 59.800588 N, 17.597136 E. Rebuild the thin
server after changing these coordinates; copying the JSON web asset alone
does not change the API station catalogue.

### Italian reception trial

`iono-prn@Montelibretti` on DOB, TGO and KHO uses Achille Zirizzotti's supplied
`irf_prn_MTB.INI` and the accompanying `iono_config.py` frequency generator:
seed 10, 10000 chips at 100 kchip/s, pulse length = IPP = 400 (continuous
code), one second per frequency, 300-second period and UTC phase zero.
The INI's `freqs` entry is obsolete: his code replaces it with 300 frequencies
from 1.0 to 30.9 MHz in 100 kHz steps. Reproduce this explicitly with
`--frequency-program ingv-generated`; the default converter still preserves
Swedish irregular tables. The trial additionally uses
`--max-receive-frequency-mhz 24.9`: only the first 240 dwells are observable,
so the inaccessible tail is omitted from the receiver table while retaining
the full **300-second period**. Never recompute a 240-second cycle from this
shorter receive table. No aliases or invented measurements are published.

This is a reception trial, not confirmation of operation or a usable band.
The supplied INI contains copied Kiruna coordinates; do not use them for
Italian map positions. Other Italian transmitter codes/timing are not enabled
by guessing from this configuration. Stop the trial with
`systemctl --user disable --now iono-prn@Montelibretti`.
The installer includes the receiver-specific Montelibretti configuration and
enables its unit. It shares the existing bounded
PRN slice and existing bandwidth-capped publisher; no extra raw-data transfer.

## DSP and interpretation

Complex64 means two f32 components (`num_complex::Complex32` in Rust).
Mixing, real FIR dot products, and complex FFT products dispatch at runtime
to AVX+FMA, AVX, SSE3, or portable scalar kernels. SSE4-capable machines can
also run the SSE fallback; the floating-point kernels do not require SSE4.
FMA uses independent FIR accumulators to reduce dependency stalls. KHO's Ivy Bridge has no AVX2/FMA;
do not compile these binaries with a newer machine's `target-cpu=native`.
RustFFT selects its own compatible transform kernels.
Target and control codes share one forward FFT per received code period.
Each retains its own inverse correlation and normalization; inverse buffers
are reused. Zero transition guards bypass FFTs. Tests compare every decoded
complex sample, power profile and code score against separate transforms.
Signal arrays remain complex-f32; f64 is retained for oscillator phase epochs
and UTC/sample-index arithmetic to avoid timing/phase precision loss.

KHO PRN readers now wait 10.5 seconds for recorder publication, based on live
ring-buffer probes that found samples arriving roughly eight seconds late.
This is a read delay, not a receiver clock correction. The supported grace
range is 0.25--15 seconds and stale-dwell skipping accounts for this grace.
Other RF products retain their separately configured publication waits.
Numerical checks compare every available x86 kernel against scalar results,
including unaligned inputs/tails, and a 191-tap FIR against a double-precision
reference. On a receiver, run `cargo test -p iono-core --release
benchmark_receiver_kernels -- --ignored --nocapture` for a bounded comparison;
these kernel timings are not an end-to-end receiver speedup measurement.

The channelizer mixes/averages by ten, then applies a centered 191-tap FIR
and decimates by 25. Reads include actual overlap; missing voltage rejects
the frequency dwell. First/last code periods are transition guards and are
not read from incomplete future recorder files. Production configurations select `estimator: least-squares` and
`least_squares_solver: cached-inverse`. At startup, form the finite-range
normal matrix and cache its complex64 inverse using OpenBLAS LAPACK.
For each code repetition, FFT correlation computes A^H z; the cached inverse
multiplies the selected range interval. Multiple repetitions use one BLAS
matrix multiply. This computes the original finite-range least-squares x,
without storing the much larger inverse-normal-matrix times A^H operator.
Caches and FFT plans persist across frequency dwells, with explicit bounds
of 160 MiB and 2048 range bins. No matched-filter fallback hides a singular
normal matrix. `matched-filter` remains available for explicit comparisons.

After solving each full code repetition, the complex range samples undergo
a slow-time Doppler FFT at each range. The sampling interval is the full code
period (0.1 s for these programs), not the internal transmit pulse IPP.
The published sparse ionogram retains maximum Doppler SNR at each cell;
it is not a saved complete range–Doppler cube. The per-Doppler power-median
floor includes noise and interference. See `../../validation/prn-matrix/`
for numerical parity fixtures, receiver benchmarks and reproduction commands.

Each dwell also correlates the same narrowband samples with the other known
seeds. HDF5 retains the target-to-strongest-control peak-power
margin. Browser arrays and frequency-band reduction require a finite margin
of at least 3 dB. This commissioning guard is not a calibrated false-alarm
probability. Weak multi-source returns can fail it. Original SNR is retained
for improving the classifier when `retain_candidate_snr` is enabled; routine
operation disables this dense diagnostic after commissioning. Previously
recorded diagnostic HDF5 is retained.
Old prototype PRN files without this control are excluded from interpretation.
Primary ionograms use `SNR_sparse_indices`/`SNR_sparse_values`: only measured
cells above 8 dB and a 3 dB code margin are published. Automatic band estimates
still require 13 dB. The default colorbar is 0–20 dB. No interpolation fills
rejected cells. The per-frequency peak and code margin remain diagnostics;
`candidate_snr` is an optional, separate dense commissioning dataset that the
GUI never renders. Both thresholds are explicit HDF5 metadata.
The inherited five-sample range correction is explicit metadata; absolute
transmitter/receiver group delay still requires physical calibration.

The two-stage FIR, code generation, correlation orientation, known-delay
Doppler return, wrong-seed discrimination and source-preserving display mask
have regression checks. The browser frequency axis inserts actual unmeasured
high-band frequencies as gaps instead of stretching the irregular table.
The latest finished sweep remains the ionogram while the next is collected;
current in-progress measurements still enter the time-history products.

## Deployment and rollback

Build `iono-prn` and `iono-plots` against the receiver's HDF5/glibc. DOB and
KHO use the Ubuntu 24.04 build. TGO needs its Ubuntu 20.04 build.

Run `bash deploy/prn/install.sh STATION /path/iono-prn /path/iono-plots` as the
receiver user from this source snapshot. The new workers share a 150% CPU,
768 MiB slice; each has a 384 MiB limit, low priority and independent restart.
Existing acquisition and chirp/Digisonde DSP remain untouched. TGO's plot
worker switches to browser arrays only now that the legacy dashboard is
retired. DOB retains its independent 16 KiB/s delta publisher and cooldown.

Rollback: disable/stop `iono-prn@Kiruna` (and `iono-prn@Uppsala` on DOB), remove
only the newly installed `zz-prn.conf` browser-unit drop-in, daemon-reload and
restart that browser worker. Its prior executable is retained in the old
checkout. All recorded scientific HDF5 and ring-buffer acquisition remain.
On restart, the worker resumes an existing compatible sweep instead of
discarding frequency columns already measured. Incompatible configurations
are preserved and processing advances to the next cycle.

Useful follow-on experiments are regularized frequency-domain deconvolution
for code sidelobes, robust narrowband interference weighting, joint fitting
of simultaneous known codes, and measured group-delay calibration. Validate
these against saved real HDF5 and negative controls before enabling them.

## Physical range windows and fixed timing

`iono-delay range-window POSITIONS.json TX RX OUTPUT.h5 [IONOGRAM.h5 ...]`
combines great-circle distance, simple virtual-height path proxies and recent
strong retained echoes to propose a range interval. It is a window diagnostic,
not ray tracing or a hardware delay calibration. Inspect its HDF5 evidence
before applying a proposal. Receiver clock corrections and the inherited
five-sample range shift remain fixed; do not align individual observations
by changing them. Receiver-local RTIs crop old profiles to the installed
range window while preserving their original source arrays.

Reviewed 2026-11-07 windows (km): TGO Kiruna 100–1100, Montelibretti
2900–4700; KHO Kiruna 1000–2000, Montelibretti 3900–6100; DOB Kiruna
700–1600, Uppsala 400–1300, Montelibretti 2100–3400. These are configurable
reception gates, not measured limits of possible propagation.

`deploy/openblas/install.sh` installs an official Ubuntu package into the
receiver user's local prefix and selects one BLAS thread per worker. It does
not replace system libraries. DOB must receive a prefetched package under
its 16 KiB/s cap. W2NAF has the library ready but no long-code PRN workers;
Finnish VERSO uses its separate aperiodic-code inverse receiver.

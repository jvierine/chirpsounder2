# DOB/KHO native DSP migration, 6 November 2026

## Production migration

Both stations now run native Rust workers for Ramfjordmoen Digisonde reception,
scheduled SGO LFM processing, global 100/125 kHz/s chirp detection, and RF
spectra. The existing native PRN, VERSO and browser-array workers were retained.
The Python Digisonde, chirp detector, scheduled LFM processor, detection metadata
consolidator and legacy plotters were stopped and removed from both startup
scripts. Python housekeeping and station-health monitoring are retained as
control-plane utilities; UHD capture remains the existing native recorder.

Recorder PIDs remained unchanged: DOB 3883681, KHO 1468893. No acquisition
restart was used. The analysis services are enabled for startup and restart on
failure. The Digisonde instance unit no longer orders itself after the target
that starts it. Original scripts/configuration/binaries were backed up under
the station's native-DSP rollout state directory.

The 15:45 UTC Ramfjordmoen observations completed with native processor metadata
and zero additional receiver timing correction:

| Receiver | Sparse retained cells | Shape |
| --- | ---: | --- |
| DOB | 11,250 | 2 x 340 x 1000 |
| KHO | 3,643 | 2 x 340 x 1000 |

The corresponding HDF5 products are included here. Sparse cell counts are not
independent detections or calibrated false-alarm statistics. Native SGO
observations and consolidated chirp detections also continued to be produced.
The browser's actual rendered DOB Digisonde view was inspected after migration.
The primary station clocks/transmitter timing were not adjusted to fit these
observations.

The native LFM range selection now preserves each station's configured range:
DOB 800--1700 km and KHO 800--1900 km. The former native hard-coded upper bound
of 1500 km would otherwise have truncated the migrated KHO/DOB products.

## Verification and capacity limits

Release tests for iono-core and iono-receiver: 59 passed, one existing ignored,
no failures. Additional regressions cover physical fine-frequency pulse
selection, unchanged coarse-frequency dwell duration, and no stale detections
after reusing a buffer through a voltage dropout.

The native detector reuses work/peak buffers and runtime-dispatched complex-f32
multiplication. KHO uses four workers and power-of-two FFT blocks under a 2 GiB
memory ceiling, as described in deploy/nordic/README.md. Acquisition and primary
scheduled reception keep higher priority. This does not yet establish full
continuous global-detector coverage: operating logs still show skipped older
blocks on KHO. Do not describe the migration as complete-voltage processing or
an apples-to-apples Python/Rust performance benchmark.

A trial of the preinstalled system FFTW3f library was checked against RustFFT
and benchmarked on KHO. Under comparable loaded conditions it did not improve
performance; it was not adopted. The deployed processing remains RustFFT and
the shared Rust SIMD kernels. No FFTW FFI or dependency was retained in source.

## Reception commissioning remains separate

Initial migrated Dourbes/Chilton workers preserve their old configuration.
This migration alone does not validate them. Separate finite local tests use
GIRO program metadata and explicitly select the fine-frequency pulse sequence.
They write outside the production archive. Thule and Millstone currently have
the same 450 s schedule and 2 MHz/25 kHz/0.32 s frequency program, so a good
candidate ionogram alone cannot distinguish them. Coordinated arrival-delay
checks and a Ramfjordmoen control are required before assigning a new source.

## Juliusruh migration regression and recovery

The first migrated Juliusruh worker rejected its legacy upper frequency limit
because the validator incorrectly required an exact integer number of steps.
Its last old observation was 15:38:16 UTC; the first restored native observation
was 17:03:16, published at 17:03:59. This migration caused a real gap in the
Juliusruh series; old voltage outside the short RAM ring cannot be backfilled.
The deployed JSON endpoints were aligned to preserve the original 259/359-bin
axes, and the native validator now accepts the legacy partial-step convention.
A regression covers both original day/night settings. `iono-receiver check-config`
now validates staged receiver JSON without opening voltage or launching DSP;
the installer invokes it before replacing the worker binary/stopping old jobs.

The later Dourbes/Chilton program corrections and coordinated waveform tests are
recorded in ../digisonde-multiplex-20261106/README.md. These did not alter receiver
clock calibration or identify a confirmed new Thule source.

The rendered KHO page also exposed skipped primary soundings: the 4.5 s
writer grace plus 2 s retry was shorter than occasional recorder publication
latency. The primary Ramfjordmoen worker now waits 10 s before reading; this
is a publication wait, not an adjustment to receiver sample timing.

The final reviewed receiver binary installed on both DOB and KHO has SHA256
`e0e7f18d3debdbe86e384281156ba63f9be67ae3fed8dc65c6787ee862b9a4f5`.
Both stations accepted their deployed configuration with native `check-config`.
No Python RF processor or legacy plotter remained in the process audit.

After the KHO publication-wait correction, the 17:30 UTC Ramfjordmoen
sounding completed all 340 frequencies and published its HDF5 product at
17:33:48 UTC. Observation age remained approximately 11.4–12.1 s throughout.
The same recorder PID remained active. The actual rendered KHO page was also
inspected with its 17:15 UTC native ionogram and current PRN/LFM products.

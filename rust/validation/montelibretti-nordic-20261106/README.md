# Montelibretti reception on TGO and KHO, 6 November 2026

Both receivers now run enabled iono-prn@Montelibretti services with seed 10,
continuous 400-chip pulses, 10000-chip code at 100 kchip/s, 1 s dwells and the
unchanged 300 s transmitter period. The receive table is 1.0--24.9 MHz (240
frequencies); the inaccessible upper dwell tail does not shorten the period.
Configs derive from the already deployed DOB program. Sparse display threshold
8 dB, control-code margin 3 dB and 0--20 dB display limits are unchanged.

## Native optimization and checks

PRN signal buffers are num_complex::Complex32 (two f32 components). Mixing,
FIR dot products and complex spectral products use runtime SIMD. KHO selects
AVX; TGO selects AVX+FMA. RustFFT independently selects supported vector FFTs.
Oscillator epochs and UTC arithmetic remain f64/integer to retain precision.

The target and four control codes now share one forward input FFT per code
period, while retaining separate inverse correlations and energy normalizations.
Inverse buffers are reused. Zero transition guards skip transforms. A new test
compares decoded f32 correlations, power profiles and code scores with separate
transforms for continuous and gated codes, delayed/Doppler-shifted echoes and
DC leakage. Existing known-delay, wrong-code and scalar/SIMD reference checks
also passed. The core/receiver suite passed 66 tests (two optional benchmarks
ignored), then the shared FFT benchmark was run explicitly.

Five-code compression benchmark (not end-to-end acquisition or dwell speed):
- Build host AVX+FMA: 0.149293 s separate, 0.082474 s shared (1.810x).
- KHO AVX: 1.275726 s separate, 0.746914 s shared (1.708x), under station load.
The portable test executable ran PRN and SIMD reference checks on KHO itself.

## Deployment and limitations

KHO uses the Ubuntu 24 build, SHA256:
f5c4fcb3bb7bbf53bb89b4d8191f7abb5ad40a151b4107a853b774e35fda078f
TGO's older glibc/HDF5 rejected that build. Its previous executable was restored,
then the same reviewed core/receiver source was built on TGO in an isolated
station source directory, checked with ldd and an actual ring probe, and installed.
TGO native build SHA256:
2ca426d2b7946f7075b1a8b4eaac5d2cb32c78211ddf460d09a98aea662a5f5e
Only PRN services were restarted for the optimized binary. Acquisition was left
running. Both Kiruna and Montelibretti use the optimized executable.

KHO's live probe found newest available voltage about 8 s old. Reading after
4.5 s rejected even valid data that arrived later. Both KHO PRN workers now wait
10.5 s; stale-dwell skipping includes that wait. This changes publication waiting,
not the fixed receiver clock, transmitter epoch or range calibration. Coverage
returned after deployment, but real recorder packet discontinuities remain.

The retained HDF5 snapshots are completed 21:25 UTC sweeps, not raw voltage.
Both had 240/240 completed dwells when copied, and no cells passing the
existing discrimination/display thresholds. These observations do
not establish a confirmed Montelibretti echo at either site. Do not infer a
usable frequency band from this commissioning deployment. GUI cards were
inspected in both live receiver tabs; blank output is explicitly labelled
'No PRN echoes pass the thresholds'. Finished-sweep selection deliberately
retains the last completed ionogram while a new sweep is being acquired.

## Corrected global-search policy

The user's one-core instruction was a continuous baseline, not a ceiling.
Removed CPUQuota from TGO, DOB, KHO and W2NAF detector services; verified
CPUQuotaPerSecUSec=infinity while Nice=19 and minimum CPU scheduling weight
remain. At least one worker runs continuously, and bounded extra workers may
consume spare cycles. W2NAF now has two native workers so it can burst beyond
one CPU. No quota is applied to acquisition. Lowest priority does not guarantee
a hard one-core reservation against arbitrary higher-priority contention.
KREA remains unreachable; its tracked unit has the corrected no-quota policy.
The older global-background report records the superseded capped policy.

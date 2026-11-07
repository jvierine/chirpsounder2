# Finite-range PRN least-squares validation, 2026-11-07

Production computes x = (AᴴA)⁻¹ Aᴴz using FFT correlation for Aᴴz,
then complex64 BLAS multiplication by a cached inverse normal matrix.
The matrix is built once for each code and range width; no inverse is
formed per received code. Cholesky is used only to construct the inverse.

`iono-prn-bench FIRST_LAG RANGE_BINS OUTPUT.h5 51` compares complete x
computation after cache/plan setup, including FFT correlation where used.
It performs five warmups, rotates method order each repetition, and saves
wall/CPU timings, complex-output errors, cache bytes and setup costs in HDF5.
All methods use one OpenBLAS thread. Synthetic data contain two delayed
complex echoes, Doppler phase and weak deterministic interference.
These are decoder timings, not complete receiver timings or validation of
physical propagation. The matched-filter baseline is a different estimator.

Receiver batch-of-eight median wall times (ms):

| Host | Range bins | Cached Bz | FFT + factor solve | FFT + cached inverse | FFT matched filter |
| --- | ---: | ---: | ---: | ---: | ---: |
| KHO | 735 | 122.540 | 11.413 | 12.318 | 3.823 |
| TGO | 602 | 31.418 | 2.819 | 2.828 | 0.899 |
| W2NAF | 735 | 24.004 | 2.698 | 2.542 | 0.665 |

KHO first lag 1306, TGO 973, W2NAF benchmark 1000; code length 10000.
TGO's benchmark uses one more bin than the final 601-bin deployed window.
W2NAF has no active long-code PRN units: its row is a capacity benchmark.
Final interleaved measurements are the `*-four.h5` files.
Cached inverse versus cached B relative complex errors are below 2.2e-6;
batch errors are below 4.8e-7. Independent NumPy cached-B fixtures in
`../../crates/iono-core/tests/data/prn_matrix_parity.h5` also test the Rust
output against the original `jvierine/ionosonde/prc_lib.py` implementation.
That fixture includes its reference source hash and generation metadata.

The inverse cache uses 4.12 MiB on KHO versus 56.08 MiB for B; TGO uses
2.76 versus 45.93 MiB. Peak-memory metadata includes all benchmark methods
simultaneously and must not be interpreted as production per-method memory.
Factor solve is slightly faster on KHO batch data; cached inverse was chosen
explicitly and is about ten times faster than cached B on that workload.
Its extra decoder cost over matched filtering is about 8.5 ms per one-second
dwell on KHO and 1.9 ms on TGO. No whole-receiver speedup is inferred.

Ubuntu packaged OpenBLAS: 0.3.26 on KHO/W2NAF/DOB, 0.3.8 on TGO.
The host CPU, chosen library and SIMD dispatch are recorded in each HDF5.
Reproduce against a receiver-compatible build:

```sh
export IONO_PRN_LAPACK="$HOME/.local/lib/iono-openblas/usr/lib/x86_64-linux-gnu/openblas-pthread/libopenblas.so.0"
export OPENBLAS_NUM_THREADS=1
iono-prn-bench 1306 735 result.h5 51  # KHO workload
cargo test -p iono-core finite_window_matches_original_python_cached_matrix
cargo test -p iono-core matrix_recovers_two_echoes_from_normal_equations
```

`STATION-TRANSMITTER.h5` files contain range-window evidence from exact,
known recent observation filenames. Geometry/virtual-height proxies and
retained strong peaks set reception gates; they do not determine a unique
propagation path or calibrate hardware/transmit timing. No raw wideband
voltage was transferred. The original receiver clock and five-sample shift
are unchanged. Range-Doppler follows the complex least-squares solution
across complete code repetitions; sparse ionograms retain maximum Doppler
SNR, rather than publishing a dense range-Doppler cube.

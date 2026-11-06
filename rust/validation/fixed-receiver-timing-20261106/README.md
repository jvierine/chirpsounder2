# Fixed receiver timing validation, 6 November 2026

The receiver calibration is a station constant, never a fit parameter for each
ionogram or transmitter. Nominal profiles preserve zero additional correction
with unknown uncertainty. No nonzero production receiver correction was applied.

## Verification

- Native release tests for iono-core and iono-receiver: 55 passed, one existing
  test ignored, no failures. Includes existing PRN/VERSO waveform and code
  discrimination tests, new fixed-index/sign/validity checks, the multiplex
  finder regression, delay-reference estimation, and Digisonde dwell budgets.
- All native receiver binaries built successfully on Ubuntu 24. The final
  receiver-timing tests were repeated after tightening immutable-profile checks.
- The delay helper also built on TGO's Ubuntu 20 and was installed as
  `~/bin/iono-delay`, SHA256
  `a5940576607b6f40702ac27f324fb1921ff6c3ae0a75714857eae273663c5c27`.
  Acquisition and reception workers were not restarted or replaced for this
  diagnostic installation. The timing-aware production worker changes are
  tested source, not yet rolled out to running stations.
- `reference-synthetic.h5` has a known +13 us residual bias and an existing
  +4 us correction. The native helper recovered the +17 us proposal and retained
  the 2 us systematic uncertainty despite repeated samples and one outlier.
  This is a synthetic validation, not a measured station calibration.
- On TGO, the helper read the current Vaasa observation at Unix epoch
  1793976835 directly from the station disk, applying the GUI's existing code
  identity masks. It returned 21 retained frequency peaks in the requested
  700--2000 km interval, spanning 914.4--1025.3 km, with 10 us delay bins.
  `current-vaasa-peaks.h5` retains those diagnostics; these are strongest retained
  echoes, not independently established ground-wave reference delays.
- A superseded commissioning Vaasa observation with NaN identity margins
  correctly yielded zero peaks. No masks or thresholds were relaxed to obtain
  the current result.

## Digisonde capability and remaining work

Native Digisonde configuration now distinguishes physical PRI from effective
per-carrier PRI for multiplexed operation. Long 128-pulse 5 ms dwells fit the
existing 128 MB raw-voltage budget; over-budget dwells are rejected before
allocation. These changes support testing programs found in GIRO headers,
including Rome RO041; they do not confirm a transmitter's present on-air mode.

Improved Dourbes/Chilton reception has not yet been demonstrated by this work.
Their per-transmitter timing, multiplex pattern, and range windows require
separate on-air validation. Prague's phase-switch setting also needs decoder
validation. Working Montelibretti reception was not changed. No propagation
peak was used to infer and apply a receiver clock correction.

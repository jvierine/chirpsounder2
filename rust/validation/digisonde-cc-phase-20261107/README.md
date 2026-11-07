# Complementary-code phase correction — DOB, 7 November 2026

## Confirmed fault and correction

The user was correct: the doubled traces are a decoding artifact. Earlier
code-order trials did not reproduce the production read-window origin.

The strong saved 9 MHz DOB cache begins at 15:23:59.680 UTC on an A pulse.
The production window begins at 15:23:59.610006923 UTC, approximately seven
physical 10 ms pulses earlier. Reusing the same A/B index at these two origins
swaps the expected complementary codes. Wrong-phase correlation produces two
nearly equal peaks roughly 100 km apart. Correct-phase correlation produces
one dominant peak. The same fault is reproduced with an independently generated
point target; a second test preserves two distinct physical echoes.

The physical pulse sequence also shows four pulses on the selected fine carrier
before multiplexing away, rather than the configured two. DOB's provisional
THJ76 worker now uses `multiplex_block_pulses=4` and
`multiplex_phase_pulses=1`. The decoder maps physical pulses to their transmitted
code and O/X identity and retains only complete quartets inside the dwell.
Partial edge groups are discarded; it never mixes missing pulses, polarizations
or fine carriers to manufacture a complete pair. The complete-quartet count is
written to new products as `decoded_pulses_per_frequency`.

The fixed read offset remains **10006.923 us**. Receiver timing, range origin,
coarse-frequency timing, sparse threshold and 0–20 dB display limits are unchanged.
Unconfigured legacy workers retain the previous decode path.

## Full-ionogram verification and deployment

Two simultaneous DOB workers processed the **same 16:37:30 UTC sounding**:
the established worker and an isolated phase-aware trial. Inspect
`DOB-163730-before.png` and `DOB-163730-corrected.png`: the duplicated F- and
E-region traces collapse into sharp traces in the corrected result. The two
authoritative sparse HDF5 files are retained alongside the figures.

At **16:44:46 UTC**, DOB's `iono-digisonde@thule.service` was switched to the
committed native build **7754039**. Its stripped Linux binary SHA-256 is
`f2b395392c33278fbb998b1d88428fa442e70b76c9c07b96b70d30080c6edc98`.
The build used an archive of that commit's Cargo manifests and crates on TGO;
its checksum was verified at DOB. A dedicated service override selects
`~/bin/iono-receiver-7754039`; other sounders keep their existing binaries.
The old configuration is backed up at DOB in
`/tmp/thule-before-cc-phase-20261107.json`.

The corrected 16:37:30 product replaced its incorrect production counterpart
after both were retained here. The regular capped receiver-side preparation
and publisher delivered it to juha.no. The public API array
`1fc5635e557186c4b1d48237` has exactly the corrected finite-cell mask (20574 cells)
and agrees within 0.005 dB, as expected from 0.01 dB browser quantization. It
does not match the old result (18015 finite cells). The next 16:45 sounding was
observed processing with the committed build. This verifies the public data;
the Mac was locked, so no successful browser screenshot inspection was available.

**This fixes decoding, not transmitter identification. THJ76 remains provisional.**
The Millstone/Thule association work remains separate, including the previously
demonstrated Millstone pulse-period alias in the W2NAF product.

## European simultaneous-transmission checks

The native `iono-digisonde-overlap` tool screens time *and* RF occupancy rather
than equating simultaneous start times with interference. The deployed DOB
schedule screen identifies only two possibly affected target frequency cells,
7.100 and 7.125 MHz near **15:08:35 UTC**, against Juliusruh's nearby 7.110 MHz
step. It finds no cochannel intersection from configured Chilton, Dourbes or
Ramfjordmoen during this target sweep. Ramfjordmoen does start at 15:07:30;
its configured RF sweep is offset from the target's.

Read-only GIRO queries also checked European measurement epochs from 15:00
through 15:11 UTC and retained their program metadata in
`GIRO-European-programs.h5`. This includes Athens, Pruhonice, Rome, Roquetes,
Dourbes, Juliusruh, Nicosia, Gibilmanna, Sopron and Tromsø. Their program-derived
occupancy screen flags brief *possible* intersections with Nicosia at target
11.625–11.775 MHz and Gibilmanna at 12.575–12.775 MHz. A factor-of-two dwell
sensitivity check instead flags Juliusruh near 7.1 MHz. These are screening
candidates, not measured interference: physical timing must be validated for
each program, the local Juliusruh schedule differs from the GIRO-derived one,
and Rome RM041's non-DPS program does not provide usable coded-pulse timing.
The tool allows 0–50 ms unknown propagation and a conservative 30 kHz RF
proximity, so brief overlaps are deliberately not presented as source IDs.

None of these screened European cases explains the broad 9–11 MHz doubling;
the same-voltage A/B correction directly removes it. The band is not declared
interference-free, and no source label or quantitative propagation limit was
promoted on the basis of these checks.

## Reproduction

All RF decoding, phase comparison and overlap calculations use Rust. Python
helpers only copied HDF5 arrays, normalized GIRO metadata, or compared already
decoded display cells. No Python RF processor or plotter was introduced.

```sh
cargo run --release --manifest-path rust/Cargo.toml -p iono-receiver \
  --bin iono-digisonde-pulse-audit -- \
  rust/validation/thule-millstone-20261107/iq/DOB-152230.h5 \
  9000000 /tmp/DOB-phase-audit.h5 --skip-ipps=1
cargo run --release --manifest-path rust/Cargo.toml -p iono-render \
  --bin iono-digisonde-phase-plot -- /tmp/DOB-phase-audit.h5 /tmp/DOB-code-phase.png
cargo run --release --manifest-path rust/Cargo.toml -p iono-receiver \
  --bin iono-digisonde-overlap -- rust/deploy/stations/DOB \
  rust/deploy/stations/DOB/thule.json 1794064050 /tmp/DOB-overlap.h5
```

Validation: 49 core tests passed, one unrelated benchmark ignored; all ten
receiver tests, three pulse-audit tests and the overlap test passed. The new
regression verifies that code phase cannot change receiver delay or frequency
timestamps. Correct/incorrect pulse ordering, distinct real echoes and shifted
window origins are tested separately.

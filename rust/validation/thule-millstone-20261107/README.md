# Millstone / Pituffik discrimination — 7 November 2026

## Result

**Pituffik is not identified securely by these captures. Do not promote THJ76
to a verified source or use its commissioning ranges as calibrated ranges.**

The strong W2NAF 5–9 MHz feature in the provisional THJ76 product agrees with
the simultaneous Millstone trace after removing 20 ms (5995.85 km) of pulse-period
ambiguity. Its median range residual was about 1.6 km. This establishes a
Millstone alias in that view; it does not identify the separate weak branch or
the DOB screenshot's source.

Four receivers (DOB, TGO, KHO and W2NAF) recorded coordinated, bounded 100 ksample/s
complex32 IQ. No acquisition or receiver-clock correction was changed. The
production decoder and provisional source labels remain unchanged.

## Independent information

Read-only GIRO queries returned program 14153 for Millstone and 16829 for
Thule. At query time, Millstone records extended to 15:37:30 UTC; the newest
returned Thule record was 11:37:30 UTC. A publication gap does not establish
transmitter downtime.

Both programs specify 100 physical pulses/s, complementary code 1, O/X,
two fine-frequency steps, 25 kHz coarse steps and 5 kHz fine steps. Millstone's
program ends at 16 MHz, Thule's at 20 MHz. All four receivers successfully read
18/18 high-band windows per tested carrier (17 and 19 MHz, 15:52:30 sounding).
There was no convincing Thule-only-band detection; one isolated weak KHO
19 MHz match is insufficient.

## Pulse order and compression

`iono-digisonde-pulse-audit` independently decodes physical pulses with both
complementary kernels, enumerates fine-frequency block length, O/X ordering
and code phase, and reconstructs complete quartets. The strongest saved DOB
9 MHz window prefers code-first pairing. The configured block-2 model gives
about 27.1 dB profile SNR and an 11.2 dB main-to-secondary peak ratio. Alternative
phase/block choices improve amplitude but do not remove all secondary peaks.
A complete block-4 replay (`ionograms/DOB-block4.*`) also retains parallel traces.
Block 4 is therefore **not deployed as a fix**.

Analytic-chip and joint-Doppler experiments did not demonstrate sufficient
real-data improvement and were discarded. These experimental DSP changes are
not in the delivered code. A single physical point target and two distinct
physical echoes are separate tests: correct pairing must compress the former
without deleting the latter. Multiple peaks alone do not prove wrong ordering.
The Lowell manual also describes imperfect A/B sidelobe cancellation under
inter-pulse Doppler: https://digisonde.com/pdf/Digisonde4DManual_LDI-web.pdf

## Multi-receiver timing

`iono-digisonde-tdoa` searches common UTC voltage for delay and Doppler, preserving
source ambiguity. In the 15:37:30 sounding's 9/11 MHz subsets, cross-ambiguity
maxima are weak/nonunique (normalized power approximately 0.0013–0.0023).
Conditional phase-scramble controls exceed the observed 9 MHz maximum in 8/32
DOB–W2NAF and 30/32 TGO–W2NAF trials. These checks are diagnostic, not calibrated
false-alarm probabilities. They do not support localization.

Approximate ground-path delay differences, receiver minus W2NAF:

| Receiver | Thule | Millstone |
|---|---:|---:|
| DOB | −2.715 ms | +17.000 ms |
| TGO | −4.314 ms | +17.599 ms |
| KHO | −6.809 ms | +16.585 ms |

DOB–W2NAF is nearly degenerate modulo the 10 ms pulse period. TGO/KHO and complete
code-sequence timing are needed to break that degeneracy; choosing a range gate
or the highest bank score is not sufficient.

The later 16:07:30 sounding was captured over ±1 s of expected packet time at
7/9/11 MHz. Each receiver successfully read all 52/52 windows at each carrier.
DOB had useful 9/11 MHz matches; TGO had none, KHO only one weak 9 MHz match,
and W2NAF useful 11 MHz matches. This does not provide a common strong
multi-receiver signature for a secure assignment.

## Reproduction and retained products

Build the native commissioning tools from this checkout:

```sh
cargo build --release --manifest-path rust/Cargo.toml -p iono-receiver \
  --bin iono-digisonde-pulse-audit --bin iono-digisonde-tdoa
rust/target/release/iono-digisonde-pulse-audit \
  rust/validation/thule-millstone-20261107/iq/DOB-152230.h5 9000000 /tmp/pulse-audit.h5
rust/target/release/iono-digisonde-tdoa \
  rust/validation/thule-millstone-20261107/iq/DOB-153730.h5 \
  rust/validation/thule-millstone-20261107/iq/W2NAF-153730.h5 \
  9000000 /tmp/cross-delay.h5 --phase-null
```

The audit lag is relative to the cached physical-IPP origin, modulo 10 ms;
it is **not** a calibrated propagation delay. TDOA lag is receiver A minus B.
The IQ reader restores the extractor's mixer phase before stitching overlapping
windows, refuses gaps and limits the result to one second. Five focused tests
cover mixer phase, pulse pairing, distinct echoes, delay/Doppler recovery and
two simultaneous sources.

`iq/` contains small exact voltage subsets; full captures remain on receivers.
`bank/`, `high-band/` and `wide/` contain native code-match summaries, not full IQ.
`tdoa/` contains delay/Doppler profiles and conditional null controls.
`pulse-audit/` contains the saved real-DOB order/phase comparison.
`geometry/` contains Rust ground-path estimates. The capture and export scripts
are finite, low-priority commissioning jobs; they are not enabled services.

Next discriminating evidence is a stronger common packet at TGO/KHO and W2NAF,
or a confirmed coded 16–20 MHz packet while Millstone retains this program.
There is currently no validated basis for altering live decoder order, changing
fixed receiver timing, or relabelling the DOB trace as Thule.

# Digisonde multiplex/timing commissioning, 6 November 2026

Source programs were checked through authorized read-only GIRO DIDBase access
and interpreted with the [Lowell Digisonde manual](https://digisonde.com/pdf/Digisonde4DManual_LDI-web.pdf).
No credentials or vendor software are included. DSP and figures were generated
with native Rust and Rustplotlib. Raw wideband voltage remained on receivers.
Manual DOB transfers were capped at 16 KiB/s.

## Production configuration corrections on DOB

Dourbes formerly ran every 15 minutes with an obsolete 1--12 MHz program and a
range window beginning at 2188 km, above its approximately 1360 km ground path.
It now runs every five minutes, with a 3535 us read-window start (about 1060 km),
16 selected-carrier pulses, 20 ms effective recurrence, 10 ms physical PRI and
explicit code-pair multiplex selection. This is a range-window/transmitter-mode
change, not a receiver clock calibration.

| GIRO program | RF span | Slots and UTC hours |
| --- | --- | --- |
| 16882 | 1--23 MHz | quarter hours, 05:30--21:00 |
| 16603 | 1--12 MHz | quarter hours, 21:00--05:30 |
| 16600 | 1.5--15 MHz | +5/+10 minutes, 05:30--18:00 |
| 16953 | 0.5--12 MHz | +5/+10 minutes, 18:00--05:30 |

All four programs have 100 Hz physical pulse rate, two fine frequencies,
5 kHz fine steps, 50 kHz coarse steps and four coherent repeats. The resulting
coarse dwell is 0.32 s. Quarter-hour metadata reports +1 s onset; regular slots
use the nominal five-minute epoch. Metadata can be rounded/jittered and does
not itself validate the on-air phase. The first new regular production product
at 16:55 completed at 16:56:28 and reached the public API by 16:58:58.

Chilton's last available GIRO program (7593, August 2024) specifies 200 Hz and
32 coherent repeats: 128 physical 5 ms pulses, a 0.64 s coarse dwell. Its old
100 Hz/32-pulse configuration had the wrong dwell. The new native configuration
uses the documented values and a range window starting near 1013 km. Current
transmission timing and frequency-search offsets remain unconfirmed. The
16:20 trial did not establish a continuous identified Chilton trace.

Neither corrected configuration is a claim of calibrated source identification
or fully restored reception quality. Receiver timing remains fixed at zero
additional correction, with nominal status and unknown hardware uncertainty.

## Shared Thule/Millstone candidate

Four bounded DOB multiplex hypotheses completed at 16:15 UTC. The new sparse
candidate contains a clear stepped-frequency trace. GIRO shows Thule THJ76 and
Millstone MHJ45 sharing the same 450 s schedule, 2 MHz start, 25 kHz coarse step,
0.32 s dwell and two 5 kHz-spaced fine frequencies. A plot headed THJ76 is a
hypothesis label in a private test, not a confirmed Thule reception.

Coordinated 16:52:30 UTC 9 MHz checks used the complete multiplexed codeword.
All sites attempted the planned 49 windows. DOB's strongest main-carrier result
had coherence 0.0734 and lag 4.44 ms; the corresponding TGO result had coherence
0.0106 and lag about 8.26 ms. Fine-carrier peaks differ by about 40 ms, consistent
with four-pulse fine-frequency blocks. KHO had no accepted candidate match.
The known Ramfjordmoen control at TGO produced repeatable 4.77 ms lags, consistent
with its configured transmitter timing plus short local propagation. Control
signal support was not equally strong at every receiver.

The candidate's TGO-minus-DOB lag difference is about +3.82 ms. This does not
establish Thule from a vacuum ground-path model, which predicts approximately
-1.60 ms; Millstone's ground-path difference is about +0.60 ms. Skywave group
paths and packet/mode/code ambiguities must be considered. We did not shift
receiver clocks or promote an identified Thule map link from these data.

The fullword HDF5 outputs and representative sparse products/figures are
retained here. Code-coherence thresholds and contiguous-bin counts are
commissioning diagnostics, not calibrated false-alarm probabilities. Source
identification and propagation-delay calibration remain distinct from correct
program parsing and native software operation.

A repeat at 17:07:30 UTC included DOB, TGO and W2NAF. The near-Millstone
W2NAF result was weak and does not resolve transmitter identity; a 9 MHz
skip zone can also suppress a nearby transmitter. Repeat HDF5 products are
retained under `repeat-fullword`. No identified link was enabled.

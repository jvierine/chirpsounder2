# DOB Digisonde timing/mode commissioning, 6 November 2026

## Outcome

Chilton has an experimental receiver profile with 200 Hz physical pulses,
128 pulses of integration, a 0.26 s inter-frequency pause, and bounded carrier
search over +/-30 kHz. The resulting 0.90 s frequency interval replaces the
unsupported assumption that the 0.64 s integration is the complete sweep step.
The 286 ms packet alignment is an experimental transmit-epoch estimate;
absolute propagation-range zero is not independently calibrated. The receiver
clock profile stayed fixed throughout. Only the Chilton service was changed.

The 19:50 fixed-band capture found a 5.020 MHz code burst at +72.32--72.80 s,
with 5 ms pulses, legacy mode 3 and a stable sampled codeword lag. The 20:10
capture repeated it at the same sweep time on 5.000 MHz. The moving carrier
and repeated timing support frequency search. The complete experimental
ionogram also recovered coherent carriers around 5.3--5.8 and 7.4--7.9 MHz,
and a curved propagation trace. This is an experimental source association,
not an independently surveyed delay calibration.

Dourbes did not produce a coherent multi-frequency fingerprint in the tested
nighttime windows. Its documented 100 Hz/two-fine-frequency program and
0.32 s dwell were checked against the vendor decoder. Alternative pulse orders
and blocks did not establish improved reception, so none was promoted.

Thule is not identified. It shares published timing/frequency programs with
Millstone. A slower, structured candidate favored a 20 ms code model in some
sampled windows, but this does not establish physical pulse rate or source:
polarization, fine-frequency and code-cycle ambiguities remain. No confirmed
Thule map link or continuously identified receiver was enabled from this trial.

## Diagnostic corrections

The code verifier falsely matched an unmodulated carrier. A failing synthetic
control reproduced this. Removing the stationary component before padding,
tapering the robust spectral branch, and requiring a localized correlation
peak remove that failure. Tests retain recovery of a delayed coded train and
of a code beneath a stronger carrier at both zero and nonzero frequency offset.
These controls are not a field false-alarm calibration.

The native verifier can now save bounded channelized complex-f32 HDF5 windows,
replay them without touching the ring buffer, and export metadata-only products.
Both polarization/code pulse orders can be tested. An end-to-end native test
recovers a known waveform from cached IQ while its configured RF path does not
exist, and checks that summary export excludes the IQ payload.

The initial broad survey processed 938/938 planned 160 ms windows spaced by
1.28 s with no missing windows. This samples 12.5% of voltage time; it is not
continuous full-band coverage. The later fixed-band captures recorded continuous
160 ms slices at two/three selected carriers. All voltage remains on DOB;
manual transfers were capped at 16 KiB/s. No recorder restart was requested.

Representative ionograms, numeric match summaries and receiver configurations
are retained with this report. Candidate labels are hypotheses. Static figures
were generated with Rustplotlib and normal TeX rendering.

## Live deployment and next observation

DOB's production Chilton service published its first revised observation for
20:20:00 UTC at 20:24:14 UTC. The actual live browser card and full-size plot were
inspected. Reception remains weak and the absolute range is provisional; this
is not a claim that all three requested transmitters are fixed. The recorder
was active as PID 1516605 during rollout and final checks; it was not restarted
by this work. Other receiver services were not restarted.

A finite follow-up is installed for 7 November 2026, 09:00:00--09:01:52 UTC:
14 reboot-persistent user timers capture 4/5/6 MHz windows. Each has Nice=15,
75% CPU, 256 MiB memory and a 90 s runtime ceiling. The raw channelized voltage
stays on DOB. Successful-window counts must be checked before interpreting the
follow-up; scheduling it is not evidence of reception.

The full Rust workspace passed 84 tests with one pre-existing ignored test.
The Rustplotlib diagnostic was rendered and its layout checked. The captured
IQ replay test and stationary-carrier negative controls passed. Static plot
fonts use the normal TeX path. No Python DSP or plotting worker was introduced.

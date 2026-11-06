# Coordinated codeword delay checks

These are finite commissioning diagnostics, not continuously enabled receivers.
Copy this directory to each Norwegian receiver and run:

```
bash launch.sh STATION COMMON_FUTURE_UTC_EPOCH /absolute/path/to/iono-digisonde-verify
```

Use the same UTC Unix epoch on every receiver, on the shared 450 s schedule.
The example probes 9 MHz in the current 2 MHz start / 25 kHz step / 0.32 s
program. `phase.json` tests physical 10 ms pulses, two fine frequencies, and
1/2/4-pulse frequency multiplex blocks. `control.json` tests the later known
Ramfjordmoen frequency window. Adjust the RF data/output paths for a different
station; W2NAF uses its RAM-disk ring path. These files contain no credentials.

Timers are calculated from the common epoch on each receiver. Recording stays
running. Both checks run below acquisition/primary processing priority and
have finite CPU, memory and runtime limits. Outputs remain in the commissioning
subdirectory outside the published day archive.

Require the full planned 49 windows in each HDF5 `attempted_windows` attribute.
A smaller count can indicate that recorder metadata had not yet published all
windows. The native verifier creates its output directory and writes compressed
metadata arrays atomically. Check binary provenance before using an old cached
copy: an old TGO diagnostic version computed results but failed saving when the
output directory was absent. The current version is installed in `~/bin`.

`lag_s` is the codeword correlation lag relative to each recorded sample window.
It is not automatically a calibrated propagation delay. Account for window
origin, carrier/fine-channel order, template mode, code-cycle/sign ambiguities
and transmitter group delay. Never change the receiver clock calibration to
force agreement with a candidate location. Cross-site delay differences still
contain propagation-path differences; ground-distance differences alone do not
identify a skywave transmitter.

The regression test delays a synthetic multiplexed waveform by 24.41 ms and
recovers that codeword delay modulo its known code-sign ambiguity, rather than
folding it at the 10 ms physical pulse period. This validates the implementation
on a supplied waveform; it does not identify a transmitter on air.

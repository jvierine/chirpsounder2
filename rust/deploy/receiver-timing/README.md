# Fixed receiver timing

Receiver timing is one constant per receiver hardware configuration. It must
not be fitted independently for each transmitter, sounding, frequency or echo.
All native reception workers read the same JSON file once, at process startup:
`~/.config/chirpsounder2/receiver-timing.json` (or the explicit
`IONO_RECEIVER_TIMING_FILE`). A missing explicit file or station mismatch is an
error. These files contain no credentials.

The versioned station profiles in `../stations/STATION/receiver-timing.json`
preserve the existing UHD/GPS sample-time convention: **zero additional
correction, nominal status, unknown uncertainty**. They do not establish that
analog, cable, FPGA or timestamp latency has been independently measured.
Do not relabel nominal timing as measured or invent a correction from an
ionospheric trace.

A positive `delay_us` means recorded ADC timestamps are late relative to the
antenna input. Reading physical UTC t therefore selects recorded time t+delay;
converting a recorded detection timestamp back to physical UTC subtracts delay.
Integer sample offsets preserve the precision of absolute DigitalRF indices.
The applied correction is quantized to the sample period. Sample-rate, pulse
PRI and original range-bin spacing remain in each scientific product.

`transmit_delay_us` describes the transmitter's emission epoch.
`offset_us` selects the Digisonde read/range window. Neither is a receiver
clock calibration. Writer grace changes when a read is attempted, not its
sample index. Do not change any of these to force an ionospheric path onto
its great-circle ground distance.

Every new product records the fixed receiver delay, revision, reference,
status, uncertainty and validity interval. Resumed PRN/VERSO observations and
chirp-detection buckets reject incompatible timing revisions. Historical files
retain their original timing. A different calibration is installed as an
explicit new revision with a validity interval, after measurement and review;
restart all relevant native workers together outside a sounding. Legacy
Python receivers do not read this profile and must not be assumed corrected.
Standalone `iono-digisonde-verify` is a raw-sample circular-code diagnostic;
its historical `time_unix` means recorded sample time, not calibrated UTC.
Do not interpret its circular lags directly as propagation ranges.

## Delay diagnostics

```
iono-delay peaks ionogram.h5 peaks.h5 MIN_RANGE_KM MAX_RANGE_KM [MIN_SNR_DB]
iono-delay calibrate reference.h5 proposal.h5
```

`peaks` imports the same ionogram axes and sparse masking as the GUI. It reports
the strongest retained echo per frequency and polarization inside the requested
range interval, in microseconds, plus bin spacing and physical PRI when known.
It does not interpolate absent sparse bins, unwrap pulse/code ambiguities, or
claim the strongest peak is a ground wave or first arrival. Peak bin spacing
is a sampling diagnostic, not a statistical delay uncertainty. This routine
leaves the receiver timing fixed.

`calibrate` requires HDF5 vectors `observed_delay_us`, `reference_delay_us`,
`reference_uncertainty_us`, scalar `applied_receiver_delay_us`, and scalar UTF-8
strings `receiver` and `independent_reference`. Supply at least three paired
measurements from a loopback, independently timed local source, or other
validated reference with known emission timing and propagation delay. Reference
uncertainty must include transmitter timing and reference-path uncertainty.
Resolve code/PRI aliases before supplying the measurements. A skywave's ground
distance is not a known propagation delay.

The proposal adds median observed-minus-reference delay to the already applied
receiver correction. It reports robust scatter combined in quadrature with the
largest supplied reference uncertainty; it does not shrink systematic error
with the number of measurements. This diagnostic is not a confidence interval
or automatic calibration validation. Check individual residuals, instrumental
resolution, hardware stability and outliers before accepting any result.
The command creates a separate HDF5 proposal and **never edits configuration**.
Existing output files are not overwritten.

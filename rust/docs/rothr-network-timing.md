# ROTHR network timing

All receiver plot preparers track New Kent (Virginia), Premont (Texas), and
Playa Grande/Vieques (Puerto Rico) separately. The coordinate catalogue records
published site/locality references and antenna-position caveats. There is no
synthetic US map point. Range-gate associations are candidates, not transmitter
identifications. Overlapping gates retain diagnostic histories and withhold
quantitative frequency limits. W2NAF cross-modulation policy still applies.

The compact current-ionogram lane includes latest radar status from the last
30 minutes of the local indexed detection table. This lets DOB publish map
metadata without transferring a new daily raster bundle.

## Offline localization

Use explicit HDF5 input manifests, receiver coordinates and fixed timing calibration.
Do not list raw-voltage archives or change receiver delays to improve a fit.

```sh
iono-lfm-locate manifest.json timing.h5
iono-lfm-site-check timing.h5 positions.json site-comparison.h5
iono-lfm-location-plot timing.h5 coastlines.json surface.pdf
```

`frequency_bin_hz`, `minimum_detections_per_bin`, and
`minimum_frequency_span_hz` support coarse/fine association sensitivity checks.
`anchor_range` selects an apparent-range feature; it is not a measured ground
range. Conditional integer-transmission-epoch fits are stored separately.

For small receiver-side schedule exports from known quarter-hour bucket names:

```sh
iono-lfm-select /data0/2026-11-06 DOB 1793923200 720 80 0.15 timing-DOB.h5
```

This reads precomputed detection tables only, without directory scans or voltage
processing. Transfer DOB exports at 16 KiB/s or less. Schedule selection is a
hypothesis test; report its selection and retain independent/held-out checks.

## 6 November result

Ten 100 kHz/s DOB/TGO/W2NAF coincidences on a 12-minute schedule favour Vieques
among the three known sites: model RMS 51–88 km versus 687–774 km for New Kent
and 732–878 km for Premont. All ten individual events favour Vieques in these
common-height models. Only three coincidences survive the finer-frequency
criterion, below the five-event family requirement. Receiver-specific propagation
modes and waveform association remain unvalidated; this is provisional evidence,
not a uniquely triangulated transmitter or calibrated uncertainty estimate.
The KHO 6000–6500 km feature has a different schedule and remains unresolved.

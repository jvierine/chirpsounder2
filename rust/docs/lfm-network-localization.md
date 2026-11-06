# Network localization of 100 and 125 kHz/s sweeps

`iono-lfm-locate` analyzes the existing receiver-produced `cdetections` HDF5
tables. It runs offline on an analysis computer. It neither reads raw voltages
nor adds processing to the public web server or acquisition workers.

Build and run:

```sh
cargo build --release -p iono-receiver --bin iono-lfm-locate
target/release/iono-lfm-locate manifest.json candidates.h5
```

The manifest specifies exact input files and independently surveyed receiver
coordinates. Example receiver entries below need actual filenames and at least
three quantitative receivers:

```json
{
  "receivers": [
    {"station":"DOB", "latitude":62.073, "longitude":9.111,
     "quantitative":true, "files":["observations/cdetections-DOB-EPOCH.h5"]},
    {"station":"TGO", "latitude":69.66174439, "longitude":18.93912737,
     "quantitative":true, "files":["observations/cdetections-TGO-EPOCH.h5"]},
    {"station":"KHO", "latitude":78.1478, "longitude":16.04035,
     "quantitative":true, "files":["observations/cdetections-KHO-EPOCH.h5"]},
    {"station":"W2NAF", "latitude":41.33509167, "longitude":-75.60079,
     "quantitative":false, "files":["observations/cdetections-W2NAF-EPOCH.h5"]}
  ],
  "minimum_snr":16,
  "minimum_common_bands":2,
  "minimum_events":5,
  "grid_step_deg":1
}
```

`minimum_snr` is the detector's stored peak statistic, **not a dB threshold**.
Input rows are `[zero_frequency_epoch, observation_epoch, frequency_hz,
rate_hz_per_s, detection_statistic]`, with GPS-referenced Unix epochs.
Duplicate input detections do not count twice. Missing input files are errors;
make the manifest reflect actual archived availability.

The estimator groups sweep epochs within 80 ms independently for each rate.
This is a coincidence search window, not a measured delay uncertainty.
It requires three detections per receiver per 1 MHz band, rejects bands with
more than 8 ms of epoch spread, and compares the early 20th-percentile arrival
at common frequencies. At least two common bands and three quantitative
receivers are needed by default; events with more than 1 ms between-band
delay inconsistency are rejected. Repeated events are grouped by their
relative-delay vectors, with a 0.65 ms connection threshold and at least five
events. A timing family can contain multiple unresolved transmitters or modes.
It is not a station identification or a count of physical transmitters.

Global grids retain all geographic aliases. The output includes separate
surfaces for great-circle path lengths and spherical virtual-height proxies
at 110, 200, 300 and 400 km. The proxies use the minimum horizon-compatible
number of straight ionospheric hops. These are sensitivity checks, not ray
tracing: unequal path heights, group delay, long paths, differential recorder
delay, Doppler and true sweep-rate errors can bias locations. No probability
or calibrated confidence region is attached to the RMS residual.

Three nearly collinear Nordic receivers often constrain bearing much better
than source distance. Do not turn a single grid minimum into a map pin.
W2NAF's Cyprus/Australia measurements have known cross-modulation and are
excluded from quantitative interpretation by default. A separate run enabling
W2NAF is useful as a sensitivity diagnostic, not as confirmation. ROTHR can be
retained with its quality flag after event association is independently checked.

Render a standalone Rustplotlib surface from the numeric HDF5 output:

```sh
cargo build --release -p iono-render --bin iono-lfm-location-plot
target/release/iono-lfm-location-plot candidates.h5 coastlines.json candidates.png
```

The renderer shows cells with minimum RMS residual at most 0.5 ms across the
tested models. White means outside that display threshold; this is not a
confidence boundary. Retain the full HDF5 surfaces when interpreting the plot.
The coastline input is a flat `[longitude,latitude,...,null,null,...]` array.
Generator and input provenance remain in code and HDF5 metadata.

Method background: HF skywave TDOA has non-line-of-sight biases; see
[An Improved TDOA Method for Land-Based Long-Range HF Skywave Source
Geolocation and Experimental Results, Sensors 23, 507 (2023)](https://www.mdpi.com/1424-8220/23/1/507).

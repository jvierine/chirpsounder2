# Chirpsounder2

Rust reception and signal processing for **Digisonde**, **LFM/chirp**, and
**PRN/coded** ionosondes, including the Finnish VERSO network.

[Live dashboard](https://juha.no/iono/) · [Rust build and deployment instructions](rust/README.md) · [Public IonoGUI code](ionogui/README.md)

## Current pipeline

1. The native UHD recorder writes complex HF voltage to a local DigitalRF ring buffer.
2. Rust workers receive configured sounders, discover unknown chirps and compute RF spectra.
3. Station-local Rust workers write sparse HDF5 ionograms and bounded RTI/display arrays.
4. IonoGUI consumes these prepared products and renders them in the browser;
   static scientific figures use Rustplotlib.

DSP uses runtime SIMD dispatch with AVX/FMA, AVX and SSE fallbacks appropriate
for each receiver. Raw voltage stays at the station. Numeric data products use
HDF5; processing and display products are bounded to protect memory and bandwidth.

## Build and configure

Start with [the Rust workspace instructions](rust/README.md). Build against the
receiver's Linux/glibc and HDF5 versions. Do not distribute a machine-specific
build to a different CPU or operating-system release.

The root `Makefile` builds the existing C++ UHD acquisition programs only.
The Rust workspace builds the RF processors and Rustplotlib tools. It does not
require the legacy Python scientific/plotting environment.

Reviewed native receiver JSON configurations are in
[rust/deploy/stations](rust/deploy/stations), PRN programs in
[rust/deploy/prn](rust/deploy/prn), and the DOB/KHO migration units in
[rust/deploy/nordic](rust/deploy/nordic/README.md). Preserve receiver clock
calibration, acquisition settings and station bandwidth limits during updates.
Older example INI files are retained for configuration history and maintenance;
they are not the native DSP configuration authority.

## Retired Python pipeline

The old Python DSP, plotting, discovery and web-sync implementation has been
moved to [python/](python/README.md). It is **legacy code, no longer used by the
supported Rust processing pipeline**. Its old launchers, dependencies and
installation instructions are archived there too. Do not use them for new
installations or restart them on migrated stations.

The remaining [station maintenance utilities](station_ops/README.md) perform
file retention and health reporting only. They are still used and are kept
separate from the retired DSP archive. Python helpers under `rust/scripts` and
`rust/deploy` are developer validation/configuration tools, not production RF
processing or plotting workers.

## Examples

Ramfjordmoen Digisonde received at Prestvannet:

<img width="978" alt="Ramfjordmoen ionogram" src="https://github.com/user-attachments/assets/37e4820d-c20a-4f9e-bc0a-6b052f4e19d3" />

[More historical examples](python/README-historical.md#examples).

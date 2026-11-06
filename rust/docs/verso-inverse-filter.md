# VERSO regularized inverse pulse compression

The receiver configuration field `inverse_regularization` selects an extended
finite inverse filter for VERSO pulse compression. Zero retains the legacy
matched filter. The deployed TGO, DOB and KHO value is **0.01**, relative to
the peak reference spectral power:

\[
H(f)=\frac{S^*(f)}{|S(f)|^2+0.01\max_f|S(f)|^2}.
\]

The reference is the supplied waveform resampled to 100 kHz. The inverse is
designed on a densely padded FFT grid, then restricted to finite lags
`[-(L-1)-L, L]`, where `L=1000` output samples. Real 10 ms guard data are read
before and after each pulse/range window. Convolution padding covers the whole
input and FIR support; output indexing removes the guard and preserves the
original range origin. The FIR is normalized to unit response to the reference.
RustFFT and the existing SIMD channelizer/multiply kernels select native CPU
acceleration. Acquisition services and their priorities are unchanged.

The code-identity decision still uses the original matched reference and its
equal-spectrum negative control on the original pulse window. An inverse-filter
peak cannot improve that acceptance margin. Scientific SNR thresholding remains
8 dB, and sparse HDF5 output retains the existing contract.

`decoder=complex64-finite-regularized-inverse-verso-v3` and the regularization
fraction are recorded in every new scientific file. Historical observations are
retained. A partial observation from another configuration is not mixed with
new processing; the worker advances to the next scheduled cycle.

For the supplied waveform passed through the actual 25 MHz → 100 kHz receiver
channelizer, the zero-Doppler peak sidelobe outside ±4 output samples changes
from **−14.59 dB to −26.56 dB**. The sampled half-power width stays at one output
sample. The normalized FIR's white-noise penalty relative to the matched filter
is **2.73 dB**. These are response tests, not universal on-air improvement or
absolute range-resolution calibration.

Doppler sensitivity remains: at 10 Hz the response-test sidelobes are −13.90 dB
matched and −19.43 dB inverse; at 25 Hz they are −10.88 and −11.41 dB. The inverse
is not described as Doppler invariant. Weaker regularization below 0.01 did not
improve the zero-Doppler response in this test and increased noise enhancement.

A same-voltage TGO/Vaasa comparison at 10.177 MHz found a 14.63 dB matched code
margin. Both filters placed the strongest echo at **917.3649 km**; peak SNR was
33.56 dB matched and 30.00 dB inverse. This confirms range alignment and the
expected noise trade-off for that observation, not the absence of all residual
sidelobes in live data.

Reproduce the supplied-code/channelizer and Doppler checks:

```sh
cargo run --release --bin iono-verso-filter-check -- response.h5
cargo run --release -p iono-render --bin iono-verso-filter-plot -- response.h5 response.png
cargo test -p iono-core pulsed::tests
cargo test -p iono-receiver --bin iono-verso
```

For a bounded, same-voltage comparison on a receiver with retained voltage:

```sh
iono-verso receiver.json --compare-filter comparison.h5
```

This reads one recently completed active group and writes reduced HDF5 range
profiles only. It does not publish images or upload raw voltages. The reported
white-noise penalty is theoretical; actual filtered/noisy observations differ.

Mismatch-filter sidelobe suppression trades sensitivity for cleaner compression;
see [Joint design of radar transmit waveform and mismatched filter with low
sidelobes](https://aaltodoc.aalto.fi/items/68f4e731-ce10-49dc-99cc-31e34df46350).

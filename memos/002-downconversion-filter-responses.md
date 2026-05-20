# Chirp Downconversion Filter Responses

The ionogram processor in `calc_ionograms.py` performs chirp downconversion
through `chirp_lib.chirp_downconvert`. The configurable filter is selected with
the `lfm.downconversion_filter` setting. The available options are:

- `fir`: Hann-windowed sinc FIR low-pass filter.
- `boxcar`: one-stage moving-average decimator.
- `cic`: cascaded-integrator-comb decimator with `lfm.cic_stages` stages.

The figures below were generated for `decimation=625`, `filter_len=2`,
and `cic_stages=2`, matching the current Marieluise-style receiver
configuration.
The frequency axis is shown in kHz. For a 25 MHz input sample rate and
`decimation=625`, the output sample rate is 40 kHz and the output Nyquist
frequency is 20 kHz. The wideband response plot extends to 1 MHz to show the
repeated sidelobe structure well beyond the output passband.

![Downconversion filter responses](figures/downconversion_filter_response_dec625_cic2.svg)

![Downconversion filter passband droop](figures/downconversion_filter_passband_dec625_cic2.svg)

## FIR

The `fir` option uses a Hann-windowed ideal sinc low-pass response. In
`chirp_lib.py`, this is constructed as

```python
windows.hann(N) * sin(om0*m) / (pi*m)
```

where `N = filter_len * decimation` and `om0 = 2*pi/decimation`. With the
default `filter_len=2` and `decimation=625`, this is a 1250-tap FIR.

This is the cleanest anti-aliasing option. It has the flattest passband of the
implemented choices and much better sidelobe suppression than the moving
average. The cost is that each output sample requires a long weighted sum. In
the current C implementation this is threaded and uses AVX when available, but
it is still the most expensive option.

## Boxcar

The `boxcar` option averages one decimation block. Its normalized amplitude
response is

```text
|H(f)| = |sin(pi*f*D) / (D*sin(pi*f))|
```

where `D` is the decimation factor. This filter is computationally cheap and
has nulls at multiples of the output sample rate. However, its first sidelobe
is high and the passband has significant droop: at output Nyquist the response
is about `2/pi`, or approximately `-3.9 dB`.

This is useful when speed matters more than spectral cleanliness.

## CIC

The `cic` option uses a cascaded-integrator-comb filter. Its response is the
boxcar response raised to the number of stages:

```text
|H_cic(f)| = |H_boxcar(f)|^N
```

where `N = cic_stages`. With `cic_stages=2`, the sidelobes are lower than a
single boxcar, but the passband droop doubles in dB. At output Nyquist, the
two-stage CIC response is approximately `-7.8 dB`.

The CIC filter is efficient for streaming decimation because it avoids a long
FIR tap convolution. It is a good compromise when CPU cost is important, but it
is not amplitude-flat unless a compensation filter is added later.

## Computational Cost

For the current `decimation=625`, `filter_len=2`, `cic_stages=2`,
configuration, the approximate per-output-sample arithmetic costs are:

- `boxcar`: 625 complex input samples are mixed and accumulated for each
  output sample. This is the reference cost, `1.0x`.
- `fir`: 1250 complex input samples are mixed, weighted, and accumulated for
  each output sample. This is `filter_len * decimation / decimation = 2.0x`
  the boxcar accumulation length. Because these are weighted
  multiply-accumulates, the real CPU cost can be more than exactly `2.0x`,
  but the tap count is `2.0x`.
- `cic`: with two stages, there are about `2 * 625 = 1250` complex integrator
  updates plus two comb updates per output sample. This is
  `(cic_stages * decimation + cic_stages) / decimation = 2.003x` the boxcar
  accumulation count.

Thus, in rough operation-count terms for this configuration, the FIR is
`2.0x` the boxcar and the two-stage CIC is `2.003x` the boxcar. The difference
is that the CIC operations are simple state updates, whereas the FIR performs a
long weighted sum and therefore usually costs more per operation. The FIR has
the best frequency response, the boxcar is the simplest, and the CIC is a
streaming-friendly compromise with stronger passband droop.

The plotting code is in `tools/plot_downconversion_filter_responses.py`.
Regenerate the figures with:

```sh
python3 tools/plot_downconversion_filter_responses.py \
  --decimation 625 \
  --cic-stages 2 \
  --sample-rate 25e6 \
  --max-frequency-khz 1000
```

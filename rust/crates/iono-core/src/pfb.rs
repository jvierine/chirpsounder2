//! Experimental 2x-oversampled PFB with single-bin narrow-band extraction.
//! No production worker selects this frontend. At most 100 kHz of signal fits
//! wholly in one flat passband even halfway between coarse-bin centres.
use crate::{dsp::fir_taps, simd};
use anyhow::{ensure, Result};
use num_complex::Complex32 as Z;
use rustfft::{Fft, FftPlanner};
use std::{collections::BTreeMap, f64::consts::TAU, sync::Arc};
const FS: f64 = 25_000_000.;
const MAX_BW: f64 = 100_000.;
type Poly = unsafe fn(&[Z], &[Z], usize, usize, &mut [Z]);
#[derive(Clone, Copy)]
pub struct Request {
    pub offset_hz: f64,
    pub bandwidth_hz: f64,
}
struct Narrow {
    slot: usize,
    offset: f64,
    phase: Vec<Z>,
    bandwidth: u64,
}
pub struct Output {
    pub first_sample: i64,
    pub values: Vec<Z>,
}
pub struct Coarse {
    pub first_sample: i64,
    pub stride: usize,
    pub bins: Vec<Vec<Z>>,
}
pub struct Pfb {
    channels: usize,
    hop: usize,
    taps_per_phase: usize,
    spacing_hz: f64,
    flat_half_hz: f64,
    stop_hz: f64,
    taps: Vec<Z>,
    fft: Arc<dyn Fft<f32>>,
    poly: Poly,
    active: Vec<usize>,
    narrow: Vec<Narrow>,
    filters: BTreeMap<u64, Vec<Vec<Z>>>,
}
impl Pfb {
    pub fn new(channels: usize, requests: &[Request]) -> Result<Self> {
        ensure!(
            channels >= 8 && channels <= 192 && channels % 2 == 0,
            "M must be even in 8..192"
        );
        ensure!(
            !requests.is_empty() && requests.len() <= 128,
            "invalid request count"
        );
        let spacing = FS / channels as f64;
        let hop = channels / 2;
        // Nearest-bin distance <= spacing/2. Cover another 50 kHz on either side
        // for the entire largest signal, plus 10 kHz design margin. No bin combining.
        let pass = spacing / 2. + MAX_BW / 2. + 10_000.;
        let stop = 2. * spacing - pass;
        ensure!(stop > pass, "insufficient overlap/transition space");
        let estimate = (80. * FS / (2.285 * TAU * (stop - pass))).ceil() as usize;
        let p = (estimate.div_ceil(channels)).max(8).div_ceil(4) * 4;
        let h = fir_taps(channels * p + 1, (pass + stop) / 2. / FS, 8.6);
        let taps = h.into_iter().map(|v| Z::new(v, v)).collect();
        let mut active = vec![];
        let mut narrow = vec![];
        let mut filters = BTreeMap::new();
        for r in requests {
            ensure!(
                r.offset_hz.is_finite()
                    && r.bandwidth_hz.is_finite()
                    && r.bandwidth_hz > 0.
                    && r.bandwidth_hz <= MAX_BW,
                "invalid passband"
            );
            ensure!(
                r.offset_hz.abs() + r.bandwidth_hz / 2. < FS / 2.,
                "signal outside digitized band"
            );
            let signed = (r.offset_hz / spacing).round() as i64;
            let bin = signed.rem_euclid(channels as i64) as usize;
            let offset = r.offset_hz - signed as f64 * spacing;
            ensure!(
                offset.abs() + r.bandwidth_hz / 2. <= pass - 9_999.,
                "single-bin overlap invariant"
            );
            let slot = active.iter().position(|&b| b == bin).unwrap_or_else(|| {
                active.push(bin);
                active.len() - 1
            });
            let phase = (0..4000)
                .map(|i| {
                    let x = -TAU * offset * i as f64 / (FS / hop as f64);
                    Z::new(x.cos() as f32, x.sin() as f32)
                })
                .collect();
            // Match the existing direct channelizer's physical Kaiser-FIR support.
            // This is not a claim of a newly flat 100 kHz final decoding passband.
            let n = ((1910. / hop as f64).round() as usize).max(15) | 1;
            filters.entry(r.bandwidth_hz.to_bits()).or_insert_with(|| {
                (0..hop)
                    .map(|f| {
                        fractional(
                            n,
                            (r.bandwidth_hz / 2.).min(45_000.) / (FS / hop as f64),
                            f as f64 / hop as f64,
                        )
                    })
                    .collect()
            });
            narrow.push(Narrow {
                slot,
                offset,
                phase,
                bandwidth: r.bandwidth_hz.to_bits(),
            });
        }
        Ok(Self {
            channels,
            hop,
            taps_per_phase: p,
            spacing_hz: spacing,
            flat_half_hz: pass,
            stop_hz: stop,
            taps,
            fft: FftPlanner::new().plan_fft_forward(channels),
            poly: select_poly(),
            active,
            narrow,
            filters,
        })
    }
    pub fn hop(&self) -> usize {
        self.hop
    }
    pub fn spacing_hz(&self) -> f64 {
        self.spacing_hz
    }
    pub fn flat_half_hz(&self) -> f64 {
        self.flat_half_hz
    }
    pub fn stop_hz(&self) -> f64 {
        self.stop_hz
    }
    pub fn active_bins(&self) -> usize {
        self.active.len()
    }
    pub fn prototype(&self) -> Vec<f32> {
        self.taps.iter().map(|h| h.re).collect()
    }
    /// Centred FIRs require real read overlap. Unsupported edges are omitted;
    /// missing input voltage propagates as NaN, never synthetic zero padding.
    /// first_sample is relative to a common integer 25 MHz phase origin, not UTC f32.
    pub fn coarse(&self, raw: &[Z], first_sample: i64) -> Coarse {
        let half = (self.taps.len() / 2) as i64;
        let d = self.hop as i64;
        let start = (first_sample + half).div_euclid(d)
            + i64::from((first_sample + half).rem_euclid(d) != 0);
        let end = (first_sample + raw.len() as i64 - 1 - half).div_euclid(d);
        let count = (end - start + 1).max(0) as usize;
        let mut bins: Vec<Vec<Z>> = self
            .active
            .iter()
            .map(|_| Vec::with_capacity(count))
            .collect();
        let mut v = vec![Z::default(); self.channels];
        let mut scratch = vec![Z::default(); self.fft.get_inplace_scratch_len()];
        for t in 0..count {
            let at = (start + t as i64) * d - half - first_sample;
            unsafe {
                (self.poly)(
                    &raw[at as usize..at as usize + self.taps.len()],
                    &self.taps,
                    self.channels,
                    self.taps_per_phase,
                    &mut v,
                )
            };
            self.fft.process_with_scratch(&mut v, &mut scratch);
            // With D=M/2, absolute-time modulation correction is exactly +/-1.
            // FIR midpoint is an integer multiple of M. Keep this correction: without
            // it odd FFT bins alternate sign and create a false Doppler shift.
            let odd = (start + t as i64).rem_euclid(2) != 0;
            for (out, &k) in bins.iter_mut().zip(&self.active) {
                out.push(if odd && k % 2 == 1 { -v[k] } else { v[k] });
            }
        }
        Coarse {
            first_sample: start * d,
            stride: self.hop,
            bins,
        }
    }
    pub fn process(&self, raw: &[Z], first_sample: i64) -> Vec<Output> {
        let coarse = self.coarse(raw, first_sample);
        let d = self.hop as i64;
        let first_out = first_sample.div_euclid(250) + i64::from(first_sample.rem_euclid(250) != 0);
        let end = (first_sample + raw.len() as i64 - 1).div_euclid(250);
        self.narrow
            .iter()
            .map(|n| {
                let input = &coarse.bins[n.slot];
                let mut mixed = vec![Z::default(); input.len()];
                let mut osc = vec![Z::default(); 4000];
                let mut rot = vec![Z::default(); 4000];
                for (b, chunk) in input.chunks(4000).enumerate() {
                    let angle =
                        (-TAU * n.offset * (coarse.first_sample + b as i64 * 4000 * d) as f64 / FS)
                            .rem_euclid(TAU);
                    rot[..chunk.len()].fill(Z::new(angle.cos() as f32, angle.sin() as f32));
                    simd::multiply(
                        &n.phase[..chunk.len()],
                        &rot[..chunk.len()],
                        &mut osc[..chunk.len()],
                    );
                    simd::multiply(
                        chunk,
                        &osc[..chunk.len()],
                        &mut mixed[b * 4000..b * 4000 + chunk.len()],
                    );
                }
                let bank = &self.filters[&n.bandwidth];
                let length = bank[0].len() as i64;
                let mut values =
                    vec![Z::new(f32::NAN, f32::NAN); (end - first_out + 1).max(0) as usize];
                // An exact integer phase accumulator avoids a division and
                // remainder for every output sample. If hop divides 250
                // (e.g. 1 MHz -> 100 kHz), phase remains zero throughout.
                let at = first_out * 250 - coarse.first_sample;
                let mut lo = at.div_euclid(d) - length / 2;
                let mut phase = at.rem_euclid(d);
                let step = 250 / d;
                let remainder = 250 % d;
                for z in &mut values {
                    if lo >= 0 && lo + length <= mixed.len() as i64 {
                        *z = simd::weighted_sum(
                            &mixed[lo as usize..(lo + length) as usize],
                            &bank[phase as usize],
                        );
                    }
                    lo += step;
                    phase += remainder;
                    if phase >= d {
                        phase -= d;
                        lo += 1;
                    }
                }
                Output {
                    first_sample: first_out * 250,
                    values,
                }
            })
            .collect()
    }
}
fn select_poly() -> Poly {
    #[cfg(target_arch = "x86_64")]
    {
        if std::is_x86_feature_detected!("avx") && std::is_x86_feature_detected!("fma") {
            return poly_fma;
        }
        if std::is_x86_feature_detected!("avx") {
            return poly_avx;
        }
        if std::is_x86_feature_detected!("sse3") {
            return poly_sse;
        }
    }
    poly_scalar
}
unsafe fn poly_scalar(raw: &[Z], h: &[Z], m: usize, p: usize, out: &mut [Z]) {
    for q in 0..m {
        let mut s = Z::default();
        for k in 0..p {
            s += raw[k * m + q] * h[k * m + q].re;
        }
        out[q] = s;
    }
    out[0] += raw[m * p] * h[m * p].re;
}
#[cfg(target_arch = "x86_64")]
macro_rules! poly_kernel {
    ($name:ident,$feature:literal,$width:expr,$zero:ident,$load:ident,$store:ident,$add:ident,$mac:ident) => {
        #[target_feature(enable=$feature)]
        unsafe fn $name(raw: &[Z], h: &[Z], m: usize, p: usize, out: &mut [Z]) {
            use std::arch::x86_64::*;
            // Independent phase accumulators hide FMA/add latency. Each phase vector
            // reads contiguous I/Q and duplicated real taps; no gather or overread.
            for q in (0..m / $width * $width).step_by($width) {
                let mut a = [$zero(); 4];
                for k in (0..p).step_by(4) {
                    for (lane, s) in a.iter_mut().enumerate() {
                        let at = (k + lane) * m + q;
                        *s = $mac!(
                            $load(raw.as_ptr().add(at).cast()),
                            $load(h.as_ptr().add(at).cast()),
                            *s
                        );
                    }
                }
                let sum = $add($add(a[0], a[1]), $add(a[2], a[3]));
                $store(out.as_mut_ptr().add(q).cast(), sum);
            }
            // Even FFT sizes such as 50 need a two-phase AVX tail. No load
            // crosses the valid phase extent; SSE handles its own full width.
            for q in m / $width * $width..m {
                let mut sum = Z::default();
                for k in 0..p {
                    sum += raw[k * m + q] * h[k * m + q].re;
                }
                out[q] = sum;
            }
            out[0] += raw[m * p] * h[m * p].re;
        }
    };
}
#[cfg(target_arch = "x86_64")]
macro_rules! mac_fma {
    ($a:expr,$b:expr,$c:expr) => {
        _mm256_fmadd_ps($a, $b, $c)
    };
}
#[cfg(target_arch = "x86_64")]
macro_rules! mac_avx {
    ($a:expr,$b:expr,$c:expr) => {
        _mm256_add_ps(_mm256_mul_ps($a, $b), $c)
    };
}
#[cfg(target_arch = "x86_64")]
macro_rules! mac_sse {
    ($a:expr,$b:expr,$c:expr) => {
        _mm_add_ps(_mm_mul_ps($a, $b), $c)
    };
}
#[cfg(target_arch = "x86_64")]
poly_kernel!(
    poly_fma,
    "avx,fma",
    4,
    _mm256_setzero_ps,
    _mm256_loadu_ps,
    _mm256_storeu_ps,
    _mm256_add_ps,
    mac_fma
);
#[cfg(target_arch = "x86_64")]
poly_kernel!(
    poly_avx,
    "avx",
    4,
    _mm256_setzero_ps,
    _mm256_loadu_ps,
    _mm256_storeu_ps,
    _mm256_add_ps,
    mac_avx
);
#[cfg(target_arch = "x86_64")]
poly_kernel!(
    poly_sse,
    "sse3",
    2,
    _mm_setzero_ps,
    _mm_loadu_ps,
    _mm_storeu_ps,
    _mm_add_ps,
    mac_sse
);
fn fractional(n: usize, cutoff: f64, fraction: f64) -> Vec<Z> {
    fn i0(x: f64) -> f64 {
        let (mut sum, mut t) = (1., 1.);
        for k in 1..60 {
            t *= x * x / (4. * (k * k) as f64);
            sum += t;
            if t < sum * 1e-16 {
                break;
            }
        }
        sum
    }
    let denom = i0(8.6);
    let mut h = vec![];
    for k in 0..n {
        let t = k as f64 - (n - 1) as f64 / 2. - fraction;
        let u = 2. * k as f64 / (n - 1) as f64 - 1.;
        let sinc = if t.abs() < 1e-14 {
            2. * cutoff
        } else {
            (TAU * cutoff * t).sin() / (std::f64::consts::PI * t)
        };
        h.push(sinc * i0(8.6 * (1. - u * u).max(0.).sqrt()) / denom);
    }
    let sum: f64 = h.iter().sum();
    h.into_iter()
        .map(|v| Z::new((v / sum) as f32, (v / sum) as f32))
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn simd_polyphase_matches_independent_scalar() {
        for m in [8, 12, 32, 50, 96, 100, 128, 192] {
            let p = 12;
            let raw: Vec<_> = (0..m * p + 1)
                .map(|i| Z::new((i as f32 * 0.1).sin(), (i as f32 * 0.31).cos()))
                .collect();
            let h: Vec<_> = (0..raw.len())
                .map(|i| {
                    let v = (i as f32 * 0.3).sin() / p as f32;
                    Z::new(v, v)
                })
                .collect();
            let mut expected = vec![Z::default(); m];
            unsafe { poly_scalar(&raw, &h, m, p, &mut expected) };
            let mut kernels: Vec<Poly> = vec![poly_scalar, select_poly()];
            #[cfg(target_arch = "x86_64")]
            {
                if std::is_x86_feature_detected!("sse3") {
                    kernels.push(poly_sse);
                }
                if std::is_x86_feature_detected!("avx") {
                    kernels.push(poly_avx);
                }
                if std::is_x86_feature_detected!("avx") && std::is_x86_feature_detected!("fma") {
                    kernels.push(poly_fma);
                }
            }
            for f in kernels {
                let mut actual = expected.clone();
                unsafe { f(&raw, &h, m, p, &mut actual) };
                assert!(actual
                    .iter()
                    .zip(&expected)
                    .all(|(a, b)| (*a - *b).norm() < 2e-6));
            }
        }
    }
    #[test]
    fn whole_signal_fits_one_bin() {
        for m in [8, 12, 16, 24, 32, 48, 64, 96, 128, 160, 192] {
            let delta = FS / m as f64;
            for x in [-0.5, -0.499, 0., 0.499, 0.5] {
                let r = Request {
                    offset_hz: x * delta,
                    bandwidth_hz: MAX_BW,
                };
                let p = Pfb::new(m, &[r]).unwrap();
                assert!(p.narrow[0].offset.abs() + MAX_BW / 2. <= p.flat_half_hz - 9999.);
            }
        }
    }
}

#[cfg(test)]
mod frame_tests {
    use super::*;
    #[test]
    fn overlapping_frames_preserve_phase_and_integer_time() {
        for m in [12, 32, 50, 100, 128] {
            let f = 1_712_345.67;
            let p = Pfb::new(
                m,
                &[Request {
                    offset_hz: f,
                    bandwidth_hz: 80_000.,
                }],
            )
            .unwrap();
            let raw: Vec<_> = (0..300_000)
                .map(|i| {
                    let a = TAU * (f + 12_000.) * i as f64 / FS;
                    Z::new(a.cos() as f32, a.sin() as f32)
                })
                .collect();
            let a = p.process(&raw, 0).remove(0);
            let b = p.process(&raw[137..260_000], 137).remove(0);
            let mut count = 0;
            for (i, &v) in b.values.iter().enumerate() {
                let time = b.first_sample + i as i64 * 250;
                let j = ((time - a.first_sample) / 250) as usize;
                let u = a.values[j];
                if u.re.is_finite() && v.re.is_finite() {
                    assert!((u - v).norm() < 3e-6, "M={m} phase discontinuity");
                    count += 1;
                }
            }
            assert!(count > 500);
        }
    }
    #[test]
    fn missing_voltage_is_never_filled() {
        let p = Pfb::new(
            32,
            &[Request {
                offset_hz: 0.,
                bandwidth_hz: 80_000.,
            }],
        )
        .unwrap();
        assert!(p.process(&[], 0)[0].values.is_empty());
        let raw = vec![Z::new(f32::NAN, f32::NAN); 100_000];
        assert!(p.process(&raw, 0)[0].values.iter().all(|v| v.re.is_nan()));
    }
}

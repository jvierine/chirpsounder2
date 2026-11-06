//! Linear pulse compression and coherent pulse-Doppler processing. All signal
//! arithmetic is complex64; RustFFT and the shared kernels dispatch per CPU.
use crate::{median, simd};
use anyhow::{ensure, Result};
use num_complex::Complex32 as Z;
use rustfft::{Fft, FftPlanner};
use std::sync::Arc;

pub struct PulseDecoder {
    n: usize,
    input_len: usize,
    nr: usize,
    energy: f32,
    prefix: usize,
    noise_gain: f32,
    fft: Arc<dyn Fft<f32>>,
    ifft: Arc<dyn Fft<f32>>,
    kernel: Vec<Z>,
    work: Vec<Z>,
    product: Vec<Z>,
    scratch: Vec<Z>,
    decoded: Vec<Vec<Z>>,
}
impl PulseDecoder {
    pub fn new(code: &[Z], nr: usize) -> Result<Self> {
        ensure!(
            !code.is_empty() && code.len() <= 20000 && (2..=20000).contains(&nr),
            "invalid pulse/range extent"
        );
        let energy = code.iter().map(|v| v.norm_sqr()).sum::<f32>();
        ensure!(
            energy.is_finite()
                && energy > 0.
                && code.iter().all(|z| z.re.is_finite() && z.im.is_finite()),
            "invalid pulse code"
        );
        let input_len = code.len() + nr - 1;
        // Full linear correlation: late echoes must not wrap into early range.
        let n = (input_len + code.len() - 1).next_power_of_two();
        let mut p = FftPlanner::new();
        let fft = p.plan_fft_forward(n);
        let ifft = p.plan_fft_inverse(n);
        let mut kernel = vec![Z::default(); n];
        kernel[..code.len()].copy_from_slice(code);
        fft.process(&mut kernel);
        for z in &mut kernel {
            *z = z.conj();
        }
        let scratch = vec![
            Z::default();
            fft.get_inplace_scratch_len()
                .max(ifft.get_inplace_scratch_len())
        ];
        Ok(Self {
            n,
            input_len,
            nr,
            energy,
            prefix: 0,
            noise_gain: 1.,
            fft,
            ifft,
            kernel,
            work: vec![Z::default(); n],
            product: vec![Z::default(); n],
            scratch,
            decoded: vec![],
        })
    }
    /// Finite, two-sided Tikhonov inverse, normalized to unit echo amplitude.
    /// The guard is real input context, not circular padding substituted for it.
    /// lambda is relative to the peak reference spectral power.
    pub fn regularized(code: &[Z], nr: usize, lambda: f32) -> Result<Self> {
        ensure!(
            lambda.is_finite() && (0.0001..=1.).contains(&lambda),
            "invalid inverse regularization"
        );
        let mut d = Self::new(code, nr)?;
        let guard = code.len();
        let design_n = (8 * (code.len() + 2 * guard)).next_power_of_two();
        let mut planner = FftPlanner::new();
        let mut spectrum = vec![Z::default(); design_n];
        spectrum[..code.len()].copy_from_slice(code);
        planner.plan_fft_forward(design_n).process(&mut spectrum);
        let floor = lambda * spectrum.iter().map(|z| z.norm_sqr()).fold(0., f32::max);
        for z in &mut spectrum {
            *z = z.conj() / (z.norm_sqr() + floor);
        }
        planner.plan_fft_inverse(design_n).process(&mut spectrum);
        let negative = code.len() - 1 + guard;
        let mut taps: Vec<(isize, Z)> = (-(negative as isize)..=guard as isize)
            .map(|lag| {
                let i = lag.rem_euclid(design_n as isize) as usize;
                (lag, spectrum[i] / design_n as f32)
            })
            .collect();
        // Preserve the full finite response; FFT convolution below is padded
        // to input_len + filter_len - 1 so no late echo can wrap to early range.
        let gain: Z = code
            .iter()
            .enumerate()
            .map(|(j, &v)| v * taps[negative - j].1)
            .sum();
        ensure!(gain.norm() > 1e-6, "singular inverse filter");
        for (_, h) in &mut taps {
            *h /= gain;
        }
        d.noise_gain = d.energy * taps.iter().map(|(_, z)| z.norm_sqr()).sum::<f32>();
        d.prefix = guard;
        d.input_len += 2 * guard;
        d.n = (d.input_len + taps.len() - 1).next_power_of_two();
        d.fft = planner.plan_fft_forward(d.n);
        d.ifft = planner.plan_fft_inverse(d.n);
        d.kernel = vec![Z::default(); d.n];
        for (lag, h) in taps {
            d.kernel[lag.rem_euclid(d.n as isize) as usize] = h * d.energy;
        }
        d.fft.process(&mut d.kernel);
        d.work = vec![Z::default(); d.n];
        d.product = d.work.clone();
        d.scratch = vec![
            Z::default();
            d.fft
                .get_inplace_scratch_len()
                .max(d.ifft.get_inplace_scratch_len())
        ];
        Ok(d)
    }
    pub fn input_prefix(&self) -> usize {
        self.prefix
    }
    pub fn noise_gain_db(&self) -> f32 {
        10. * self.noise_gain.log10()
    }
    pub fn input_len(&self) -> usize {
        self.input_len
    }
    pub fn push(&mut self, input: &[Z]) -> Result<()> {
        let pulse = self.decode_pulse(input)?;
        ensure!(self.decoded.len() < 256, "invalid pulse count");
        self.decoded.push(pulse);
        Ok(())
    }
    pub fn decode_pulse(&mut self, input: &[Z]) -> Result<Vec<Z>> {
        ensure!(
            input.len() == self.input_len && self.decoded.len() < 256,
            "invalid pulse count/length"
        );
        self.work.fill(Z::default());
        let mean = input.iter().copied().sum::<Z>() / input.len() as f32;
        for (v, &z) in self.work.iter_mut().zip(input) {
            *v = z - mean;
        }
        self.fft
            .process_with_scratch(&mut self.work, &mut self.scratch);
        simd::multiply(&self.work, &self.kernel, &mut self.product);
        self.ifft
            .process_with_scratch(&mut self.product, &mut self.scratch);
        let scale = 1. / (self.n as f32 * self.energy);
        Ok(self.product[self.prefix..self.prefix + self.nr]
            .iter()
            .map(|v| *v * scale)
            .collect())
    }
    pub fn finish(&self) -> Result<(Vec<f32>, f32)> {
        let nt = self.decoded.len();
        ensure!(nt >= 8, "at least eight real pulse windows required");
        let fft = FftPlanner::new().plan_fft_forward(nt);
        let mut scratch = vec![Z::default(); fft.get_inplace_scratch_len()];
        let mut bins = vec![Z::default(); nt];
        let mut power = vec![0.; nt * self.nr];
        for r in 0..self.nr {
            for (t, v) in bins.iter_mut().enumerate() {
                let w =
                    (0.5 - 0.5 * (std::f64::consts::TAU * t as f64 / (nt - 1) as f64).cos()) as f32;
                *v = self.decoded[t][r] * w;
            }
            fft.process_with_scratch(&mut bins, &mut scratch);
            for (t, v) in bins.iter().enumerate() {
                power[t * self.nr + r] = v.norm_sqr();
            }
        }
        let mut snr = vec![0f32; self.nr];
        for row in power.chunks_exact(self.nr) {
            let floor = median(row) / std::f32::consts::LN_2;
            if floor.is_finite() && floor > 0. {
                for (out, &v) in snr.iter_mut().zip(row) {
                    *out = out.max((v - floor) / floor);
                }
            }
        }
        Ok((snr, power.into_iter().fold(0., f32::max)))
    }
}
/// Explicit zero-order-hold interpretation of supplied complex baud values.
/// Epochs and channel delays are never inferred from this resampling.
pub fn resample_code(code: &[Z], input_hz: u64, output_hz: u64) -> Result<Vec<Z>> {
    ensure!(
        input_hz > 0 && output_hz > 0 && code.len() as u64 * output_hz % input_hz == 0,
        "non-integral pulse duration"
    );
    let n = code.len() as u64 * output_hz / input_hz;
    ensure!(n > 0 && n <= 20000, "unsupported pulse length");
    Ok((0..n)
        .map(|i| code[(i * input_hz / output_hz) as usize])
        .collect())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn linear_delay_and_doppler_preserve_two_echoes_without_wrapping() {
        let code: Vec<_> = (0..127)
            .map(|i| Z::from_polar(1., ((i * i + 3 * i) % 127) as f32 * 0.173))
            .collect();
        let mut d = PulseDecoder::new(&code, 400).unwrap();
        for p in 0..32 {
            let mut z: Vec<_> = (0..d.input_len())
                .map(|i| {
                    Z::new(
                        ((i * 17 + p * 13) % 37) as f32 * 0.001,
                        ((i * 29 + p * 31) % 41) as f32 * 0.001,
                    )
                })
                .collect();
            for (delay, amp, doppler) in [(23, 1., 0.13), (211, 0.6, -0.18)] {
                let phase = Z::from_polar(amp, p as f32 * doppler);
                for (i, &c) in code.iter().enumerate() {
                    z[i + delay] += c * phase;
                }
            }
            d.push(&z).unwrap();
        }
        let (s, peak) = d.finish().unwrap();
        assert!(peak > 1.);
        let strongest = s
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .unwrap()
            .0;
        assert_eq!(strongest, 23);
        assert!(s[211] > 30.);
        assert!(s[399] < s[211] / 10.);
    }
    #[test]
    fn exact_ten_ms_resampling() {
        let c = vec![Z::new(1., 0.); 625];
        assert_eq!(resample_code(&c, 62500, 100000).unwrap().len(), 1000);
        assert!(resample_code(&c, 62501, 100000).is_err());
    }
    #[test]
    fn inverse_is_finite_linear_convolution_at_both_range_boundaries() {
        let code: Vec<_> = (0..31)
            .map(|i| Z::from_polar(1., ((i * i + 7 * i) % 31) as f32 * 0.73))
            .collect();
        for echo in [0, 127] {
            let mut d = PulseDecoder::regularized(&code, 128, 0.01).unwrap();
            let mut x = vec![Z::default(); d.input_len()];
            x[d.input_prefix() + echo..d.input_prefix() + echo + code.len()].copy_from_slice(&code);
            let got = d.decode_pulse(&x).unwrap();
            let mut kernel = d.kernel.clone();
            d.ifft.process(&mut kernel);
            for z in &mut kernel {
                *z /= d.n as f32 * d.energy;
            }
            let mean = x.iter().copied().sum::<Z>() / x.len() as f32;
            for (r, &value) in got.iter().enumerate() {
                let direct: Z = x
                    .iter()
                    .enumerate()
                    .map(|(j, &v)| {
                        let lag = (r + d.prefix) as isize - j as isize;
                        if lag >= -((code.len() - 1 + d.prefix) as isize)
                            && lag <= d.prefix as isize
                        {
                            (v - mean) * kernel[lag.rem_euclid(d.n as isize) as usize]
                        } else {
                            Z::default()
                        }
                    })
                    .sum();
                assert!((value - direct).norm() < 2e-5);
            }
            assert_eq!(
                got.iter()
                    .enumerate()
                    .max_by(|a, b| a.1.norm_sqr().total_cmp(&b.1.norm_sqr()))
                    .unwrap()
                    .0,
                echo
            );
        }
    }
    #[test]
    fn supplied_verso_code_reduces_sidelobes_with_bounded_noise_gain() {
        let bytes = include_bytes!("../../../deploy/verso/SGO_bauds_complex128.bin");
        let native: Vec<_> = bytes
            .chunks_exact(16)
            .map(|v| {
                Z::new(
                    f64::from_le_bytes(v[..8].try_into().unwrap()) as f32,
                    f64::from_le_bytes(v[8..].try_into().unwrap()) as f32,
                )
            })
            .collect();
        let code = resample_code(&native, 62500, 100000).unwrap();
        let mut mf = PulseDecoder::new(&code, 3000).unwrap();
        let mut inv = PulseDecoder::regularized(&code, 3000, 0.01).unwrap();
        let mut m = vec![Z::default(); mf.input_len()];
        m[1000..2000].copy_from_slice(&code);
        let mut x = vec![Z::default(); inv.input_len()];
        x[2000..3000].copy_from_slice(&code);
        let old = mf.decode_pulse(&m).unwrap();
        let new = inv.decode_pulse(&x).unwrap();
        let sidelobe = |v: &[Z]| {
            v.iter()
                .enumerate()
                .filter(|(i, _)| i.abs_diff(1000) > 4)
                .map(|(_, z)| z.norm())
                .fold(0., f32::max)
                / v[1000].norm()
        };
        assert!(20. * (sidelobe(&old) / sidelobe(&new)).log10() > 10.);
        assert!(inv.noise_gain_db() < 3.);
        // Put a real weak return at the strongest positive-lag matched sidelobe.
        // Compare complex amplitude error, rather than merely counting peaks.
        let lag = (1010..1500)
            .max_by(|&a, &b| old[a].norm_sqr().total_cmp(&old[b].norm_sqr()))
            .unwrap();
        let weak = Z::new(0.08, 0.);
        for (j, &v) in code.iter().enumerate() {
            m[lag + j] += v * weak;
            x[inv.input_prefix() + lag + j] += v * weak;
        }
        let biased = mf.decode_pulse(&m).unwrap()[lag];
        let recovered = inv.decode_pulse(&x).unwrap()[lag];
        assert!((recovered - weak).norm() < (biased - weak).norm() / 3.);
        for invalid in [0., -0.1, f32::NAN, f32::INFINITY] {
            assert!(PulseDecoder::regularized(&code, 3000, invalid).is_err());
        }
    }
}

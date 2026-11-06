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
            fft,
            ifft,
            kernel,
            work: vec![Z::default(); n],
            product: vec![Z::default(); n],
            scratch,
            decoded: vec![],
        })
    }
    pub fn input_len(&self) -> usize {
        self.input_len
    }
    pub fn push(&mut self, input: &[Z]) -> Result<()> {
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
        self.decoded
            .push(self.product[..self.nr].iter().map(|v| *v * scale).collect());
        Ok(())
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
}

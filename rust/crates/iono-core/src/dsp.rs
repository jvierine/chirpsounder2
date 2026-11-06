//! Streaming FIR/matched-filter processing. Buffers are bounded by one IPP.
use num_complex::{Complex32, Complex64};
use rustfft::{Fft, FftPlanner};
use std::{f64::consts::TAU, sync::Arc};
fn i0(x: f64) -> f64 {
    let mut sum = 1.;
    let mut term = 1.;
    for k in 1..60 {
        term *= x * x / (4. * (k * k) as f64);
        sum += term;
        if term < sum * 1e-16 {
            break;
        }
    }
    sum
}
pub fn fir_taps(n: usize, cutoff: f64, beta: f64) -> Vec<f32> {
    let mut h = Vec::with_capacity(n);
    let denom = i0(beta);
    for k in 0..n {
        let t = k as f64 - (n - 1) as f64 / 2.;
        let sinc = if t == 0. {
            2. * cutoff
        } else {
            (TAU * cutoff * t).sin() / (std::f64::consts::PI * t)
        };
        let u = 2. * k as f64 / (n - 1) as f64 - 1.;
        h.push(sinc * i0(beta * (1. - u * u).max(0.).sqrt()) / denom)
    }
    let sum: f64 = h.iter().sum();
    h.into_iter().map(|x| (x / sum) as f32).collect()
}
pub fn downconvert(
    raw: &[Complex32],
    offset: f64,
    sample_rate: f64,
    phase: Complex32,
    taps: &[f32],
) -> Vec<Complex32> {
    let step = Complex64::from_polar(1., -TAU * offset / sample_rate);
    let mut ph = Complex64::new(phase.re as f64, phase.im as f64);
    let mut stage = Vec::with_capacity(raw.len() / 10);
    for block in raw.chunks_exact(10) {
        let mut sum = Complex32::new(0., 0.);
        for &z in block {
            let mixed = Complex64::new(z.re as f64, z.im as f64) * ph;
            sum += Complex32::new(mixed.re as f32, mixed.im as f32);
            ph *= step;
        }
        stage.push(sum * 0.1);
    }
    let mut out = vec![Complex32::new(0., 0.); raw.len() / 250];
    let center = taps.len() / 2;
    for (i, z) in out.iter_mut().enumerate() {
        for (k, &h) in taps.iter().enumerate() {
            let j = i as isize * 25 + k as isize - center as isize;
            if j >= 0 && (j as usize) < stage.len() {
                *z += stage[j as usize] * h;
            }
        }
    }
    out
}
const A: [f32; 16] = [
    1., 1., -1., 1., 1., 1., 1., -1., -1., 1., 1., 1., -1., 1., -1., -1.,
];
const B: [f32; 16] = [
    -1., -1., 1., -1., -1., -1., -1., 1., -1., 1., 1., 1., -1., 1., -1., -1.,
];
pub struct Digisonde {
    n: usize,
    fft: Arc<dyn Fft<f32>>,
    ifft: Arc<dyn Fft<f32>>,
    kernels: Vec<Vec<Complex32>>,
    pub taps: Vec<f32>,
}
impl Digisonde {
    pub fn new(ipp_us: usize) -> Self {
        let n = ipp_us / 10;
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(n);
        let ifft = planner.plan_fft_inverse(n);
        let mut codes = vec![Complex32::new(0., 0.); n * 8];
        let bit = 1. / 30000.;
        for (j, z) in codes.iter_mut().enumerate() {
            let t = j as f64 / 100000. - 16. * bit;
            if t <= 0. {
                continue;
            }
            let i = (t / (ipp_us as f64 * 1e-6)).floor() as usize;
            if i >= 8 {
                continue;
            }
            let phase = t % (ipp_us as f64 * 1e-6);
            if phase > 0. && phase <= 16. * bit {
                let chip = (phase / bit).floor() as usize;
                if chip < 16 {
                    let code = if i % 2 == 0 { A[chip] } else { B[chip] };
                    z.re = if i >= 4 { -code } else { code };
                }
            }
        }
        let mut kernels = Vec::new();
        for code in codes.chunks_exact(n) {
            let mut v = code.to_vec();
            fft.process(&mut v);
            for (k, z) in v.iter_mut().enumerate() {
                let f = if k <= n / 2 {
                    k as f64
                } else {
                    k as f64 - n as f64
                } * 100000.
                    / n as f64;
                *z = if f.abs() <= 30000. {
                    z.conj()
                } else {
                    Complex32::new(1e-30, 0.)
                }
            }
            kernels.push(v)
        }
        Self {
            n,
            fft,
            ifft,
            kernels,
            taps: fir_taps(191, 0.006, 8.6),
        }
    }
    pub fn decode_ipp(
        &self,
        raw: &[Complex32],
        offset: f64,
        sample_rate: f64,
        pi: usize,
    ) -> Vec<Complex32> {
        self.decode_ipp_with_clip(raw, offset, sample_rate, pi, None)
    }
    /// Optional phase-preserving narrow-carrier suppression. Estimate the
    /// reference within the FIR passband so its stopband cannot lower it.
    pub fn decode_ipp_with_clip(
        &self,
        raw: &[Complex32],
        offset: f64,
        sample_rate: f64,
        pi: usize,
        clip_ratio: Option<f32>,
    ) -> Vec<Complex32> {
        let period_samples = self.n * 250;
        let padding = if raw.len() == period_samples + 2000 {
            1000
        } else {
            0
        };
        assert_eq!(raw.len(), period_samples + 2 * padding);
        let phase = Complex64::from_polar(
            1.,
            -TAU * offset * (period_samples as f64 * pi as f64 - padding as f64) / sample_rate,
        );
        let mut z = downconvert(
            raw,
            offset,
            sample_rate,
            Complex32::new(phase.re as f32, phase.im as f32),
            &self.taps,
        );
        if padding > 0 {
            z = z[4..4 + self.n].to_vec();
        }
        self.fft.process(&mut z);
        if let Some(ratio) = clip_ratio {
            assert!(ratio.is_finite() && ratio >= 2.);
            let band = self.n * 15 / 100;
            let amplitudes: Vec<_> = z
                .iter()
                .enumerate()
                .filter(|(i, _)| *i <= band || *i >= self.n - band)
                .map(|(_, v)| v.norm())
                .collect();
            let floor = crate::median(&amplitudes);
            if floor.is_finite() && floor > 0. {
                for v in &mut z {
                    let amp = v.norm();
                    if amp > ratio * floor {
                        *v *= floor / amp;
                    }
                }
            }
        }
        for (v, h) in z.iter_mut().zip(&self.kernels[pi % 8]) {
            *v *= *h
        }
        self.ifft.process(&mut z);
        for v in &mut z {
            *v /= self.n as f32
        }
        z
    }
    pub fn power(&self, decoded: &[Vec<Complex32>]) -> [Vec<f32>; 2] {
        let n = decoded.len() / 4;
        let fft = FftPlanner::new().plan_fft_forward(n);
        let mut spectra = vec![Complex32::new(0., 0.); n];
        let mut out = [vec![0.; self.n], vec![0.; self.n]];
        for mode in 0..2 {
            for r in 0..self.n {
                for i in 0..n {
                    spectra[i] = decoded[i * 4 + mode * 2][r] + decoded[i * 4 + mode * 2 + 1][r]
                }
                fft.process(&mut spectra);
                out[mode][r] = spectra.iter().map(|z| z.norm_sqr()).fold(0., f32::max)
            }
        }
        out
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fir_has_unit_gain_and_symmetry() {
        let h = fir_taps(191, 0.006, 8.6);
        assert!((h.iter().sum::<f32>() - 1.).abs() < 1e-6);
        for i in 0..h.len() {
            assert!((h[i] - h[h.len() - 1 - i]).abs() < 1e-7)
        }
    }
    #[test]
    fn dc_survives_downconversion() {
        let raw = vec![Complex32::new(1., 0.); 250000];
        let h = fir_taps(191, 0.006, 8.6);
        let z = downconvert(&raw, 0., 25e6, Complex32::new(1., 0.), &h);
        assert_eq!(z.len(), 1000);
        assert!((z[500].re - 1.).abs() < 1e-5)
    }
    #[test]
    fn digisonde_carrier_suppression_recovers_delayed_code() {
        let reference = crate::digisonde_match::template(1000, 100e3, 10000, 3);
        let raw: Vec<_> = (0..252000usize)
            .map(|i| reference[((i / 250) + 896) % 1000] + Complex32::new(100., 0.))
            .collect();
        let d = Digisonde::new(10000);
        let clean = d.decode_ipp_with_clip(&raw, 0., 25e6, 0, Some(10.));
        let p: Vec<_> = clean.iter().map(|v| v.norm_sqr()).collect();
        let peak = p
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .unwrap()
            .0;
        assert!(peak.abs_diff(100) <= 2, "peak={peak}");
        assert!(
            p[peak] / crate::median(&p) > 20.,
            "coded return lost under carrier"
        );
    }
}

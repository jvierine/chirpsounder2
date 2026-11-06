//! PINS complementary-code/mode refinement for coarse pulse-search candidates.
use crate::{median, simd};
use num_complex::Complex32 as Z;
use rustfft::{Fft, FftPlanner};
use std::sync::Arc;
const A: [f32; 16] = [
    1., 1., -1., 1., 1., 1., 1., -1., -1., 1., 1., 1., -1., 1., -1., -1.,
];
const B: [f32; 16] = [
    -1., -1., 1., -1., -1., -1., -1., 1., -1., 1., 1., 1., -1., 1., -1., -1.,
];
pub fn template(n: usize, sr: f64, ipp_us: u32, mode: u8) -> Vec<Z> {
    template_polarizations(n, sr, ipp_us, mode, false)
}
fn template_polarizations(n: usize, sr: f64, ipp_us: u32, mode: u8, all: bool) -> Vec<Z> {
    assert!(mode < 4);
    (0..n)
        .map(|i| {
            let t = i as f64 / sr - 16. / 30e3;
            if t <= 0. {
                return Z::default();
            }
            let pulse = (t / (ipp_us as f64 * 1e-6)).floor() as usize;
            let p = t.rem_euclid(ipp_us as f64 * 1e-6);
            if p <= 0. || p >= 16. / 30e3 {
                return Z::default();
            }
            if !all && mode >= 2 && pulse % 4 >= 2 {
                return Z::default();
            }
            let sign = if (mode == 1 && pulse % 4 >= 2) || (mode == 3 && pulse % 8 >= 4) {
                -1.
            } else {
                1.
            };
            let chips = if pulse % 2 == 0 { &A } else { &B };
            Z::new(sign * chips[((p * 30e3).floor() as usize).min(15)], 0.)
        })
        .collect()
}
#[derive(Clone, Debug)]
pub struct CodeMatch {
    pub ipp_us: u32,
    pub mode: u8,
    pub carrier_offset_hz: f64,
    pub lag_s: f64,
    pub score_db: f32,
    pub coherence: f32,
    pub all_polarizations: bool,
}
pub struct CodeSearch {
    n: usize,
    sr: f64,
    fft: Arc<dyn Fft<f32>>,
    ifft: Arc<dyn Fft<f32>>,
    kernels: Vec<(u32, u8, bool, f32, Vec<Z>)>,
    spectrum: Vec<Z>,
    shifted: Vec<Z>,
    work: Vec<Z>,
    scratch: Vec<Z>,
    best_power: Vec<f32>,
}
impl CodeSearch {
    pub fn new(samples: usize, sr: f64) -> Self {
        let n = samples.next_power_of_two();
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(n);
        let ifft = planner.plan_fft_inverse(n);
        let mut kernels = vec![];
        for ipp in [5000, 10000] {
            for mode in 0..4 {
                for all in [false, true] {
                    if all && mode < 3 {
                        continue;
                    }
                    let mut c = template_polarizations(samples, sr, ipp, mode, all);
                    let energy = c.iter().map(|z| z.norm_sqr()).sum();
                    c.resize(n, Z::default());
                    fft.process(&mut c);
                    for z in &mut c {
                        *z = z.conj();
                    }
                    kernels.push((ipp, mode, all, energy, c));
                }
            }
        }
        let scratch = vec![
            Z::default();
            fft.get_inplace_scratch_len()
                .max(ifft.get_inplace_scratch_len())
        ];
        Self {
            spectrum: vec![Z::default(); n],
            shifted: vec![Z::default(); n],
            work: vec![Z::default(); n],
            best_power: vec![0.; n],
            scratch,
            n,
            sr,
            fft,
            ifft,
            kernels,
        }
    }
    pub fn seek(&mut self, z: &[Z], span_hz: f64, step_hz: f64) -> Option<CodeMatch> {
        self.seek_with_doppler(z, span_hz, step_hz, 2)
    }
    pub fn seek_with_doppler(
        &mut self,
        z: &[Z],
        span_hz: f64,
        step_hz: f64,
        doppler_bins: isize,
    ) -> Option<CodeMatch> {
        assert!((0..=100).contains(&doppler_bins));
        assert!(z.len() <= self.n && span_hz >= 0. && step_hz > 0.);
        let spectrum = &mut self.spectrum;
        spectrum.fill(Z::default());
        spectrum[..z.len()].copy_from_slice(z);
        self.fft.process_with_scratch(spectrum, &mut self.scratch);
        let mut best: Option<CodeMatch> = None;
        let shifted = &mut self.shifted;
        let work = &mut self.work;
        let bins = (span_hz / step_hz).ceil() as isize;
        for whiten in [false, true] {
            if whiten {
                let floor = median(&spectrum.iter().map(|z| z.norm()).collect::<Vec<_>>());
                for z in spectrum.iter_mut() {
                    let a = z.norm();
                    if a > 10. * floor {
                        *z *= floor / a;
                    }
                }
            }
            let input_energy = spectrum.iter().map(|z| z.norm_sqr() as f64).sum::<f64>();
            if input_energy <= 0. {
                continue;
            }
            for center in -bins..=bins {
                for doppler in -doppler_bins..=doppler_bins {
                    let shift = (center as f64 * step_hz / self.sr * self.n as f64).round()
                        as isize
                        + doppler;
                    for (k, v) in shifted.iter_mut().enumerate() {
                        *v = spectrum[(k as isize + shift).rem_euclid(self.n as isize) as usize];
                    }
                    for (ipp, mode, all, energy, kernel) in &self.kernels {
                        simd::multiply(shifted, kernel, work);
                        self.ifft.process_with_scratch(work, &mut self.scratch);
                        let (lag, peak) = work
                            .iter()
                            .enumerate()
                            .map(|(i, z)| (i, z.norm_sqr()))
                            .max_by(|a, b| a.1.total_cmp(&b.1))?;
                        let coherence =
                            (peak as f64 / (self.n as f64 * *energy as f64 * input_energy)) as f32;
                        if !coherence.is_finite() || coherence < 0.005 {
                            continue;
                        }
                        if best.as_ref().is_none_or(|b| coherence > b.coherence) {
                            for (p, z) in self.best_power.iter_mut().zip(work.iter()) {
                                *p = z.norm_sqr();
                            }
                            best = Some(CodeMatch {
                                ipp_us: *ipp,
                                mode: *mode,
                                carrier_offset_hz: shift as f64 * self.sr / self.n as f64,
                                lag_s: lag as f64 / self.sr,
                                score_db: 0.,
                                coherence,
                                all_polarizations: *all,
                            });
                        }
                    }
                }
            }
        }
        if let Some(m) = &mut best {
            let peak = self.best_power.iter().copied().fold(0., f32::max);
            let noise = median(&self.best_power) / std::f32::consts::LN_2;
            m.score_db = 10. * (peak / noise.max(f32::MIN_POSITIVE)).log10();
        }
        best
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identifies_known_code_ipp_carrier_and_delay() {
        let c = template(16000, 100e3, 5000, 3);
        let z: Vec<_> = (0..16000)
            .map(|i| {
                c[(i + 16000 - 17) % 16000]
                    * Z::from_polar(
                        1.,
                        (std::f64::consts::TAU * 3000. * i as f64 / 100e3) as f32,
                    )
            })
            .collect();
        let m = CodeSearch::new(16000, 100e3)
            .seek(&z, 4000., 1000.)
            .unwrap();
        assert_eq!((m.ipp_us, m.mode), (5000, 3), "{m:?}");
        assert!((m.carrier_offset_hz - 3000.).abs() < 8., "{m:?}");
        assert!((m.lag_s - 0.00017).abs() < 0.00002, "{m:?}");
        assert!(m.coherence > 0.4);
    }
    #[test]
    fn a_single_impulse_does_not_match_a_code_train() {
        let mut z = vec![Z::default(); 16000];
        z[723] = Z::new(1., 0.);
        assert!(CodeSearch::new(16000, 100e3).seek(&z, 0., 1000.).is_none());
    }
    #[test]
    fn both_polarizations_resolve_mode_three_carrier_alias() {
        let z = template_polarizations(16000, 100e3, 10000, 3, true);
        let m = CodeSearch::new(16000, 100e3).seek(&z, 0., 5000.).unwrap();
        assert_eq!(
            (m.ipp_us, m.mode, m.all_polarizations),
            (10000, 3, true),
            "{m:?}"
        );
        assert!(
            m.carrier_offset_hz.abs() < 1. && m.coherence > 0.99,
            "{m:?}"
        );
    }
}

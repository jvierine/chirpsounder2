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
    template_program(n, sr, ipp_us, mode, all, 1, 1)
}
/// Map A-O,A-X,B-O,B-X physical pulses to the legacy A-O,B-O,A-X,B-X order.
pub fn logical_pulse_index(pulse: usize, polarization_interleaved: bool) -> usize {
    if polarization_interleaved {
        (pulse / 4) * 4 + (pulse % 2) * 2 + (pulse % 4) / 2
    } else {
        pulse
    }
}
fn template_program(
    n: usize,
    sr: f64,
    ipp_us: u32,
    mode: u8,
    all: bool,
    fine: usize,
    block: usize,
) -> Vec<Z> {
    template_ordered(n, sr, ipp_us, mode, all, fine, block, false)
}
fn template_ordered(
    n: usize,
    sr: f64,
    ipp_us: u32,
    mode: u8,
    all: bool,
    fine: usize,
    block: usize,
    polarization_interleaved: bool,
) -> Vec<Z> {
    assert!(mode < 4 && fine > 0 && block > 0);
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
            if (pulse / block) % fine != 0 {
                return Z::default();
            }
            let pulse = logical_pulse_index(
                (pulse / (block * fine)) * block + pulse % block,
                polarization_interleaved,
            );
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
    pub fine_frequency_count: usize,
    pub multiplex_block_pulses: usize,
    pub polarization_interleaved: bool,
}
pub struct CodeSearch {
    n: usize,
    sr: f64,
    fft: Arc<dyn Fft<f32>>,
    ifft: Arc<dyn Fft<f32>>,
    kernels: Vec<(u32, u8, bool, usize, usize, bool, f32, Vec<Z>)>,
    spectrum: Vec<Z>,
    shifted: Vec<Z>,
    work: Vec<Z>,
    scratch: Vec<Z>,
    best_power: Vec<f32>,
}
impl CodeSearch {
    pub fn new(samples: usize, sr: f64) -> Self {
        Self::with_ipps(samples, sr, &[5000, 10000])
    }
    pub fn with_ipps(samples: usize, sr: f64, ipps: &[u32]) -> Self {
        Self::with_programs(samples, sr, ipps, &[(1, 1)])
    }
    pub fn with_programs(
        samples: usize,
        sr: f64,
        ipps: &[u32],
        programs: &[(usize, usize)],
    ) -> Self {
        Self::with_orders(samples, sr, ipps, programs, &[false])
    }
    pub fn with_orders(
        samples: usize,
        sr: f64,
        ipps: &[u32],
        programs: &[(usize, usize)],
        orders: &[bool],
    ) -> Self {
        assert!(!orders.is_empty() && orders.len() <= 2);
        assert!(
            !programs.is_empty()
                && programs.len() <= 16
                && programs
                    .iter()
                    .all(|&(f, b)| (1..=4).contains(&f) && (1..=16).contains(&b))
        );
        let n = samples.next_power_of_two();
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(n);
        let ifft = planner.plan_fft_inverse(n);
        let mut kernels = vec![];
        for &order in orders {
            for &(fine, block) in programs {
                for &ipp in ipps {
                    for mode in 0..4 {
                        for all in [false, true] {
                            if all && mode < 3 {
                                continue;
                            }
                            let mut c =
                                template_ordered(samples, sr, ipp, mode, all, fine, block, order);
                            let energy = c.iter().map(|z| z.norm_sqr()).sum();
                            c.resize(n, Z::default());
                            fft.process(&mut c);
                            for z in &mut c {
                                *z = z.conj();
                            }
                            kernels.push((ipp, mode, all, fine, block, order, energy, c));
                        }
                    }
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
        // Remove a stationary carrier before zero padding. Otherwise its
        // artificial frame edges can become apparent code peaks after clipping.
        let mean = z.iter().copied().sum::<Z>() / z.len() as f32;
        for (out, &v) in spectrum.iter_mut().zip(z) {
            *out = v - mean;
        }
        self.fft.process_with_scratch(spectrum, &mut self.scratch);
        let mut best: Option<CodeMatch> = None;
        let shifted = &mut self.shifted;
        let work = &mut self.work;
        let bins = (span_hz / step_hz).ceil() as isize;
        for whiten in [false, true] {
            if whiten {
                // Taper the real data before the robust spectral branch. A
                // clipped rectangular-window sinusoid produces broadband edge
                // impulses, which are not evidence of transmitted phase codes.
                spectrum.fill(Z::default());
                for (i, (out, &v)) in spectrum.iter_mut().zip(z).enumerate() {
                    let w = (0.5
                        - 0.5 * (std::f64::consts::TAU * i as f64 / (z.len() - 1) as f64).cos())
                        as f32;
                    *out = (v - mean) * w;
                }
                self.fft.process_with_scratch(spectrum, &mut self.scratch);
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
                    for (ipp, mode, all, fine, block, order, energy, kernel) in &self.kernels {
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
                            let noise = median(&self.best_power) / std::f32::consts::LN_2;
                            let score_db = 10. * (peak / noise.max(f32::MIN_POSITIVE)).log10();
                            // A tone's correlation magnitude is nearly flat;
                            // require a localized code peak before ranking it.
                            if score_db < 10. {
                                continue;
                            }
                            best = Some(CodeMatch {
                                ipp_us: *ipp,
                                mode: *mode,
                                carrier_offset_hz: shift as f64 * self.sr / self.n as f64,
                                lag_s: lag as f64 / self.sr,
                                score_db,
                                coherence,
                                all_polarizations: *all,
                                fine_frequency_count: *fine,
                                multiplex_block_pulses: *block,
                                polarization_interleaved: *order,
                            });
                        }
                    }
                }
            }
        }
        best
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn polarization_first_order_is_identified_on_multiplexed_codes() {
        for order in [false, true] {
            // Independent physical pulse assembly: fine-frequency pairs alternate;
            // within a selected four-pulse group, repeat A then B for O/X-first.
            let legacy = template_program(16000, 100e3, 10000, 3, true, 2, 2);
            let mut physical = vec![Z::default(); 16000];
            for p in 0..16 {
                if (p / 2) % 2 != 0 {
                    continue;
                }
                let selected = (p / 4) * 2 + p % 2;
                let logical = if order {
                    (selected / 4) * 4 + [0, 2, 1, 3][selected % 4]
                } else {
                    selected
                };
                let source = (logical / 2) * 4 + logical % 2;
                physical[p * 1000..(p + 1) * 1000]
                    .copy_from_slice(&legacy[source * 1000..(source + 1) * 1000]);
            }
            let m = CodeSearch::with_orders(16000, 100e3, &[10000], &[(2, 2)], &[false, true])
                .seek(&physical, 0., 5000.)
                .unwrap();
            assert_eq!(m.polarization_interleaved, order, "{m:?}");
            // Independently copied fractional-chip pulses can differ by one
            // 100 kHz sample at a chip boundary; require a strong, correctly
            // ordered zero-delay result and separation from the wrong order.
            assert!(m.coherence > 0.95 && m.lag_s < 0.00002, "{m:?}");
            let wrong = CodeSearch::with_orders(16000, 100e3, &[10000], &[(2, 2)], &[!order])
                .seek(&physical, 0., 5000.);
            assert!(
                wrong.is_none_or(|w| w.coherence < m.coherence * 0.8),
                "wrong order not separated"
            );
        }
    }
    #[test]
    fn multiplexed_codeword_delay_does_not_fold_at_the_physical_pri() {
        let n = 16000;
        let delay = 2441;
        for block in [1, 2, 4] {
            let code = template_program(n, 100e3, 10000, 3, false, 2, block);
            let input: Vec<_> = (0..n).map(|i| code[(i + n - delay) % n]).collect();
            let m = CodeSearch::with_programs(n, 100e3, &[10000], &[(2, 1), (2, 2), (2, 4)])
                .seek(&input, 0., 5000.)
                .unwrap();
            let error = (m.lag_s - delay as f64 / 100e3 + 0.04).rem_euclid(0.08) - 0.04;
            assert!(
                m.coherence > 0.6 && error.abs() < 2e-5,
                "block={block}: {m:?}"
            );
        }
    }
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
    fn custom_bank_recovers_multiplexed_carrier_pulse_spacing() {
        for ipp in [20000, 40000] {
            let z = template_polarizations(16000, 100e3, ipp, 0, false);
            let m = CodeSearch::with_ipps(16000, 100e3, &[5000, 10000, 20000, 40000])
                .seek(&z, 0., 5000.)
                .unwrap();
            assert_eq!(m.ipp_us, ipp, "{m:?}");
            assert!(m.coherence > 0.99, "{m:?}");
            assert!(m.lag_s.abs() < 0.00002, "{m:?}");
        }
    }
    #[test]
    fn an_unmodulated_carrier_is_not_a_complementary_code_detection() {
        for carrier in [0., 5000.] {
            let z: Vec<_> = (0..16000)
                .map(|i| {
                    Z::from_polar(
                        1.,
                        (std::f64::consts::TAU * carrier * i as f64 / 100000.) as f32,
                    )
                })
                .collect();
            let m = CodeSearch::new(16000, 100e3).seek_with_doppler(&z, 10000., 5000., 2);
            assert!(m.is_none(), "carrier={carrier}, false match={m:?}");
        }
    }
    #[test]
    fn a_code_train_survives_a_stronger_stationary_carrier() {
        let code = template(16000, 100e3, 10000, 3);
        for carrier in [0., 5000.] {
            let z: Vec<_> = code
                .iter()
                .enumerate()
                .map(|(i, &v)| {
                    v + Z::from_polar(
                        10.,
                        (std::f64::consts::TAU * carrier * i as f64 / 100000.) as f32,
                    )
                })
                .collect();
            let m = CodeSearch::new(16000, 100e3)
                .seek(&z, 10000., 5000.)
                .unwrap();
            assert!(
                m.score_db > 10. && m.coherence > 0.1,
                "carrier={carrier} {m:?}"
            );
            assert!(
                m.carrier_offset_hz.abs() < 20. && m.lag_s < 0.00002,
                "carrier={carrier} {m:?}"
            );
        }
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

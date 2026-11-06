//! PINS-style pulse-folded dynamic-spectrum discovery (Juha Vierinen, 2019).
//! These are pulsed-signal candidates, not transmitter identifications. The
//! reported RF bins can be spectral sidelobes after whitening; refine carriers
//! and code modes before configuring a receiver.
use num_complex::Complex32 as Z;
use rustfft::{Fft, FftPlanner};
use std::sync::{Arc, OnceLock};
type Plans = (Arc<dyn Fft<f32>>, Arc<dyn Fft<f32>>);
static WHITENING: OnceLock<Plans> = OnceLock::new();

#[derive(Clone, Debug)]
pub struct Candidate {
    pub frequency_hz: f64,
    pub ipp_us: u32,
    pub phase_s: f64,
    pub contrast: f32,
    pub mean_power: f32,
    pub effective_pulses: f32,
}
pub struct PulseSearch {
    nfft: usize,
    sr: f64,
    center: f64,
    fft: Arc<dyn Fft<f32>>,
    work: Vec<Z>,
    scratch: Vec<Z>,
    window: Vec<f32>,
    folded: Vec<f32>,
    folded_squared: Vec<f32>,
    whitened: Vec<Z>,
    wide_scratch: Vec<Z>,
    amplitudes: Vec<f32>,
}
impl PulseSearch {
    pub fn new(sr: f64, center: f64) -> Self {
        // 5 kHz RF bins, 200 us time bins. Both 5 and 10 ms IPPs are exact.
        assert_eq!(sr, 25e6);
        let nfft = 5000;
        let fft = FftPlanner::new().plan_fft_forward(nfft);
        Self {
            nfft,
            sr,
            center,
            scratch: vec![Z::default(); fft.get_inplace_scratch_len()],
            fft,
            work: vec![Z::default(); nfft],
            folded: vec![0.; 50 * nfft],
            folded_squared: vec![0.; 50 * nfft],
            whitened: vec![],
            wide_scratch: vec![],
            amplitudes: vec![],
            window: (0..nfft)
                .map(|i| {
                    (0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / nfft as f64).cos()) as f32
                })
                .collect(),
        }
    }
    pub fn seek(&mut self, raw: &[Z], threshold: f32, max_candidates: usize) -> Vec<Candidate> {
        self.seek_band(raw, threshold, max_candidates, 0.7e6, 24.5e6)
    }
    pub fn seek_band(
        &mut self,
        raw: &[Z],
        threshold: f32,
        max_candidates: usize,
        min_hz: f64,
        max_hz: f64,
    ) -> Vec<Candidate> {
        assert!(
            min_hz.is_finite()
                && max_hz.is_finite()
                && min_hz >= 0.7e6
                && max_hz <= 24.5e6
                && min_hz < max_hz
        );
        assert!(raw.len() >= self.nfft * 50 && (raw.len() / self.nfft) % 50 == 0);
        self.folded.fill(0.);
        self.folded_squared.fill(0.);
        // The PINS search limits narrow spectral peaks before pulse folding.
        // Preserve phase and reduce amplitudes above 10*median to the median,
        // matching freq_rfi_clip's soft-clipping convention.
        let (forward, inverse) = WHITENING.get_or_init(|| {
            let mut p = FftPlanner::new();
            (p.plan_fft_forward(4_000_000), p.plan_fft_inverse(4_000_000))
        });
        assert_eq!(raw.len(), 4_000_000);
        self.whitened.clear();
        self.whitened.extend_from_slice(raw);
        self.wide_scratch.resize(
            forward
                .get_inplace_scratch_len()
                .max(inverse.get_inplace_scratch_len()),
            Z::default(),
        );
        forward.process_with_scratch(&mut self.whitened, &mut self.wide_scratch);
        self.amplitudes.clear();
        self.amplitudes
            .extend(self.whitened.iter().map(|z| z.norm()));
        let mid = self.amplitudes.len() / 2;
        let floor = *self
            .amplitudes
            .select_nth_unstable_by(mid, f32::total_cmp)
            .1;
        for z in &mut self.whitened {
            let amp = z.norm();
            if amp > 10. * floor {
                *z *= floor / amp;
            }
        }
        inverse.process_with_scratch(&mut self.whitened, &mut self.wide_scratch);
        let norm = raw.len() as f32;
        for z in &mut self.whitened {
            *z /= norm;
        }
        for (t, chunk) in self.whitened.chunks_exact(self.nfft).enumerate() {
            for ((out, &z), &w) in self.work.iter_mut().zip(chunk).zip(&self.window) {
                *out = z * w;
            }
            self.fft
                .process_with_scratch(&mut self.work, &mut self.scratch);
            let row = &mut self.folded[(t % 50) * self.nfft..(t % 50 + 1) * self.nfft];
            for (k, (sum, z)) in row.iter_mut().zip(&self.work).enumerate() {
                let power = z.norm_sqr();
                *sum += power;
                self.folded_squared[(t % 50) * self.nfft + k] += power * power;
            }
        }
        let mut points = Vec::new();
        for j in 140..self.nfft - 100 {
            let frequency = j as f64 * self.sr / self.nfft as f64;
            if frequency < min_hz || frequency > max_hz {
                continue;
            }
            // 0.7--24.5 MHz, avoiding DC/Nyquist edges.
            let k = (j + self.nfft / 2) % self.nfft;
            let mean = (0..50).map(|t| self.folded[t * self.nfft + k]).sum::<f32>();
            if !mean.is_finite() || mean <= 0. {
                continue;
            }
            let mut best = (0f32, 0usize, 0usize);
            for period in [25usize, 50] {
                for phase in 0..period {
                    let sum = (0..3)
                        .map(|dt| {
                            let p = (phase + dt) % period;
                            self.folded[p * self.nfft + k]
                                + if period == 25 {
                                    self.folded[(p + 25) * self.nfft + k]
                                } else {
                                    0.
                                }
                        })
                        .sum::<f32>();
                    let ratio = sum * period as f32 / (mean * 3.);
                    if ratio > best.0 {
                        best = (ratio, period, phase);
                    }
                }
            }
            let sum = (0..3)
                .map(|dt| {
                    let p = (best.2 + dt) % best.1;
                    self.folded[p * self.nfft + k]
                        + if best.1 == 25 {
                            self.folded[(p + 25) * self.nfft + k]
                        } else {
                            0.
                        }
                })
                .sum::<f32>();
            let squared = (0..3)
                .map(|dt| {
                    let p = (best.2 + dt) % best.1;
                    self.folded_squared[p * self.nfft + k]
                        + if best.1 == 25 {
                            self.folded_squared[(p + 25) * self.nfft + k]
                        } else {
                            0.
                        }
                })
                .sum::<f32>();
            let effective = sum * sum / squared.max(f32::MIN_POSITIVE);
            if best.0 >= threshold && effective >= 8. {
                points.push(Candidate {
                    frequency_hz: self.center
                        + (j as f64 - self.nfft as f64 / 2.) * self.sr / self.nfft as f64,
                    ipp_us: if best.1 == 25 { 5000 } else { 10000 },
                    phase_s: (best.2 as f64 + 1.5) * self.nfft as f64 / self.sr,
                    contrast: best.0,
                    mean_power: mean / (raw.len() / self.nfft) as f32,
                    effective_pulses: effective,
                });
            }
        }
        // Require repeated energy rather than rejecting strong signals merely
        // because their spectral sidelobes occupy much of the band.
        points.sort_by(|a, b| {
            ((b.contrast - 1.) * b.mean_power).total_cmp(&((a.contrast - 1.) * a.mean_power))
        });
        let mut selected: Vec<Candidate> = vec![];
        for p in points {
            if selected
                .iter()
                .all(|q| (p.frequency_hz - q.frequency_hz).abs() > 60e3)
            {
                selected.push(p);
                if selected.len() == max_candidates {
                    break;
                }
            }
        }
        selected
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn periodic_pulses_survive_a_strong_continuous_carrier() {
        let sr = 25e6;
        let step = Z::from_polar(1., (std::f64::consts::TAU * (-7.5e6) / sr) as f32);
        let mut phase = Z::new(1., 0.);
        let mut seed = 7u64;
        let a = [
            1., 1., -1., 1., 1., 1., 1., -1., -1., 1., 1., 1., -1., 1., -1., -1.,
        ];
        let raw: Vec<_> = (0..4_000_000)
            .map(|i| {
                phase *= step;
                if i % 4096 == 0 {
                    phase /= phase.norm();
                }
                let t = (i % 125000) as f64 / sr;
                let pulse = if t < 16. / 30e3 {
                    phase * a[(t * 30e3) as usize]
                } else {
                    Z::default()
                };
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                let re = ((seed >> 32) as u32 as f64 / u32::MAX as f64 - 0.5) as f32;
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                let im = ((seed >> 32) as u32 as f64 / u32::MAX as f64 - 0.5) as f32;
                pulse + Z::new(10., 0.) + Z::new(re, im) * 0.05 // Strong continuous RFI plus noise.
            })
            .collect();
        let found = PulseSearch::new(sr, 12.5e6).seek(&raw, 1.8, 4);
        assert!(
            found
                .iter()
                .any(|p| (p.frequency_hz - 5e6).abs() <= 2. * 30e3),
            "{found:?}"
        );
        assert!(!found.iter().any(|p| (p.frequency_hz - 12.5e6).abs() < 40e3));
    }
    #[test]
    fn a_continuous_carrier_is_not_a_pulsed_sounder() {
        let found = PulseSearch::new(25e6, 12.5e6).seek(&vec![Z::new(1., 0.); 4_000_000], 1.8, 4);
        assert!(found.is_empty(), "{found:?}");
    }
    #[test]
    fn one_broadband_impulse_is_not_a_repeated_pulse_train() {
        let mut raw = vec![Z::default(); 4_000_000];
        raw[124567] = Z::new(1., 0.);
        assert!(PulseSearch::new(25e6, 12.5e6).seek(&raw, 1.8, 4).is_empty());
    }
}

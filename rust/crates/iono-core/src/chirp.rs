//! Whitened, Hann-windowed LFM matched filter bank used by the live detector.
use num_complex::Complex32;
use rustfft::{Fft, FftPlanner};
use std::sync::Arc;
#[derive(Debug, Clone)]
pub struct Detection {
    pub frequency_hz: f64,
    pub rate_hz_s: f64,
    pub snr: f32,
}
pub struct Detector {
    n: usize,
    sr: f64,
    cf: f64,
    rates: Vec<f64>,
    window: Vec<f32>,
    kernels: Vec<Vec<Complex32>>,
    fft: Arc<dyn Fft<f32>>,
    ifft: Arc<dyn Fft<f32>>,
    scratch: Vec<Complex32>,
}
impl Detector {
    pub fn new(n: usize, sr: f64, cf: f64, rates: Vec<f64>) -> Self {
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(n);
        let ifft = planner.plan_fft_inverse(n);
        let window: Vec<_> = (0..n)
            .map(|i| (0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / (n - 1) as f64).cos()) as f32)
            .collect();
        let kernels = rates
            .iter()
            .map(|&rate| {
                (0..n)
                    .map(|i| {
                        let t = i as f64 / sr;
                        let phase = -std::f64::consts::PI * rate * t * t;
                        Complex32::new(phase.cos() as f32, phase.sin() as f32) * window[i]
                    })
                    .collect()
            })
            .collect();
        let scratch = vec![
            Complex32::new(0., 0.);
            fft.get_inplace_scratch_len()
                .max(ifft.get_inplace_scratch_len())
        ];
        Self {
            n,
            sr,
            cf,
            rates,
            window,
            kernels,
            fft,
            ifft,
            scratch,
        }
    }
    pub fn seek(
        &mut self,
        raw: &[Complex32],
        threshold: f32,
        max_detections: usize,
        spacing_hz: f64,
    ) -> Vec<Detection> {
        self.seek_with_spectra(raw, threshold, max_detections, spacing_hz, |_, _| {})
    }
    /// Project already-computed matched-filter FFTs without rereading voltage
    /// or running another transform. The callback sees one rate at a time.
    pub fn seek_with_spectra<F: FnMut(f64, &[Complex32])>(
        &mut self,
        raw: &[Complex32],
        threshold: f32,
        max_detections: usize,
        spacing_hz: f64,
        mut project: F,
    ) -> Vec<Detection> {
        assert_eq!(raw.len(), self.n);
        let mut white: Vec<_> = raw.iter().zip(&self.window).map(|(&z, &w)| z * w).collect();
        self.fft.process_with_scratch(&mut white, &mut self.scratch);
        for z in &mut white {
            *z /= z.norm() + 1e-9
        }
        self.ifft
            .process_with_scratch(&mut white, &mut self.scratch);
        for z in &mut white {
            *z /= self.n as f32
        }
        let mut best = vec![0f32; self.n];
        let mut which = vec![0u8; self.n];
        let mut work = vec![Complex32::new(0., 0.); self.n];
        for (ri, kernel) in self.kernels.iter().enumerate() {
            for i in 0..self.n {
                work[i] = white[i] * kernel[i]
            }
            self.fft.process_with_scratch(&mut work, &mut self.scratch);
            project(self.rates[ri], &work);
            for (i, z) in work.iter().enumerate() {
                let p = z.norm_sqr();
                if p > best[i] {
                    best[i] = p;
                    which[i] = ri as u8
                }
            }
        }
        let mut detections = Vec::new();
        let spacing = (spacing_hz * self.n as f64 / self.sr).ceil() as isize;
        for _ in 0..max_detections {
            let (k, &power) = best
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.total_cmp(b.1))
                .unwrap();
            if power <= threshold {
                break;
            }
            let f = if k < self.n / 2 {
                k as f64
            } else {
                k as f64 - self.n as f64
            };
            detections.push(Detection {
                frequency_hz: self.cf + f * self.sr / self.n as f64,
                rate_hz_s: self.rates[which[k] as usize],
                snr: power,
            });
            for dk in -spacing..=spacing {
                best[(k as isize + dk).rem_euclid(self.n as isize) as usize] = 0.;
            }
        }
        detections
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identifies_a_chirp_without_changing_its_epoch() {
        let n = 25000;
        let sr = 250000.;
        let f = 30000.;
        let rate = 100000.;
        let raw: Vec<_> = (0..n)
            .map(|i| {
                let t = i as f64 / sr;
                Complex32::from_polar(
                    1.,
                    (std::f64::consts::TAU * (f * t + 0.5 * rate * t * t)) as f32,
                )
            })
            .collect();
        let mut d = Detector::new(n, sr, 125000., vec![100000., 125000.]);
        let v = d.seek(&raw, 13., 1, 2000.);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].rate_hz_s, rate);
        assert!((v[0].frequency_hz - (125000. + f)).abs() <= sr / n as f64);
    }
}

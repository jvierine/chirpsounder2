//! Periodic pseudorandom-code receiver. Voltage, correlation and FFT arithmetic
//! are complex64 (f32 pairs); oscillator epochs remain f64/integer sample counts.
//! Code generation matches NumPy RandomState(seed), not the newer default_rng.
use crate::{dsp::fir_taps, median, simd};
use anyhow::{ensure, Result};
use num_complex::Complex32 as Z;
use rustfft::{Fft, FftPlanner};
use std::{f64::consts::TAU, sync::Arc};

struct Mt {
    state: [u32; 624],
    index: usize,
}
impl Mt {
    fn new(seed: u32) -> Self {
        let mut state = [0; 624];
        state[0] = seed;
        for i in 1..624 {
            state[i] = 1812433253u32
                .wrapping_mul(state[i - 1] ^ (state[i - 1] >> 30))
                .wrapping_add(i as u32);
        }
        Self { state, index: 624 }
    }
    fn next(&mut self) -> u32 {
        if self.index == 624 {
            for i in 0..624 {
                let v = (self.state[i] & 0x80000000) | (self.state[(i + 1) % 624] & 0x7fffffff);
                self.state[i] = self.state[(i + 397) % 624]
                    ^ (v >> 1)
                    ^ if v & 1 != 0 { 0x9908b0df } else { 0 };
            }
            self.index = 0;
        }
        let mut y = self.state[self.index];
        self.index += 1;
        y ^= y >> 11;
        y ^= (y << 7) & 0x9d2c5680;
        y ^= (y << 15) & 0xefc60000;
        y ^= y >> 18;
        y
    }
    fn uniform(&mut self) -> f64 {
        let a = (self.next() >> 5) as u64;
        let b = (self.next() >> 6) as u64;
        ((a << 26) + b) as f64 / 9007199254740992.
    }
}
pub fn code(seed: u32, n: usize, pulse: isize, ipp: usize) -> Result<Vec<Z>> {
    ensure!(
        (16..=100000).contains(&n) && ipp > 0,
        "invalid PRN length/IPP"
    );
    ensure!(
        pulse == -1 || (pulse > 0 && pulse as usize <= ipp && n % ipp == 0),
        "invalid pulse gate"
    );
    let mut rng = Mt::new(seed);
    Ok((0..n)
        .map(|i| {
            let p = TAU * rng.uniform();
            if pulse > 0 && i % ipp >= pulse as usize {
                Z::default()
            } else {
                Z::new(p.cos() as f32, p.sin() as f32)
            }
        })
        .collect())
}

/// A 10x mixer/boxcar followed by a centered 191-tap FIR and 25x decimation.
/// Call with 1000 input samples of real overlap on either side and discard
/// four output samples on each side. No synthetic padding across file gaps.
pub struct Channelizer {
    taps: Vec<Z>,
    phase: Vec<Z>,
    offset: f64,
    bandwidth: f64,
}
impl Channelizer {
    pub fn new(offset: f64, bandwidth: f64) -> Result<Self> {
        ensure!(
            offset.is_finite() && bandwidth.is_finite() && bandwidth > 0. && bandwidth <= 100000.,
            "invalid passband"
        );
        let phase = (0..4000)
            .map(|i| {
                let p = -TAU * offset * i as f64 / 25e6;
                Z::new(p.cos() as f32, p.sin() as f32)
            })
            .collect();
        Ok(Self {
            taps: fir_taps(191, (bandwidth / 2.).min(45000.) / 2.5e6, 8.6)
                .into_iter()
                .map(|h| Z::new(h, h))
                .collect(),
            phase,
            offset,
            bandwidth,
        })
    }
    pub fn process(&self, raw: &[Z], relative_sample: i64) -> Vec<Z> {
        let mut stage = Vec::with_capacity(raw.len() / 10);
        let mut mixed = vec![Z::default(); 4000];
        let mut oscillator = mixed.clone();
        let mut rotation = mixed.clone();
        for (b, chunk) in raw.chunks(4000).enumerate() {
            let phase = (-TAU * self.offset * (relative_sample + b as i64 * 4000) as f64 / 25e6)
                .rem_euclid(TAU);
            rotation[..chunk.len()].fill(Z::new(phase.cos() as f32, phase.sin() as f32));
            simd::multiply(
                &self.phase[..chunk.len()],
                &rotation[..chunk.len()],
                &mut oscillator[..chunk.len()],
            );
            simd::multiply(chunk, &oscillator[..chunk.len()], &mut mixed[..chunk.len()]);
            for v in mixed[..chunk.len()].chunks_exact(10) {
                stage.push(v.iter().copied().sum::<Z>() * 0.1);
            }
        }
        let mut out = vec![Z::default(); raw.len() / 250];
        for (i, z) in out.iter_mut().enumerate() {
            let center = i as isize * 25;
            if center >= 95 && center as usize + 96 <= stage.len() {
                *z = simd::weighted_sum(
                    &stage[center as usize - 95..center as usize + 96],
                    &self.taps,
                );
                continue;
            }
            for (k, &h) in self.taps.iter().enumerate() {
                let j = center + k as isize - 95;
                if j >= 0 && (j as usize) < stage.len() {
                    *z += stage[j as usize] * h.re;
                }
            }
        }
        out
    }
    pub fn bandwidth(&self) -> f64 {
        self.bandwidth
    }
}

/// FFT matched correlation, then Doppler FFT across code periods. A bounded
/// matched filter replaces the old gigabyte-scale dense estimation matrix.
/// It retains PRN sidelobes; this is deliberately not labelled least squares.
pub struct Decoder {
    n: usize,
    kernel: Vec<Z>,
    energy: f32,
    fft: Arc<dyn Fft<f32>>,
    ifft: Arc<dyn Fft<f32>>,
    scratch: Vec<Z>,
    work: Vec<Z>,
    decoded: Vec<Vec<Z>>,
    nr: usize,
}
impl Decoder {
    pub fn new(code: &[Z], nr: usize) -> Result<Self> {
        ensure!(nr > 1 && nr <= code.len(), "invalid delay extent");
        let energy = code.iter().map(|z| z.norm_sqr()).sum::<f32>();
        ensure!(energy > 0., "empty transmit code");
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(code.len());
        let ifft = planner.plan_fft_inverse(code.len());
        let mut kernel = code.to_vec();
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
            n: code.len(),
            kernel,
            energy,
            fft,
            ifft,
            scratch,
            work: vec![Z::default(); code.len()],
            decoded: vec![],
            nr,
        })
    }
    pub fn push(&mut self, input: &[Z]) -> Result<()> {
        ensure!(
            input.len() == self.n && self.decoded.len() < 256,
            "invalid code block count/length"
        );
        self.work.copy_from_slice(input);
        // Reject DC broadcast leakage without subtracting any range profile.
        let mean = self.work.iter().copied().sum::<Z>() / self.n as f32;
        for z in &mut self.work {
            *z -= mean;
        }
        self.fft
            .process_with_scratch(&mut self.work, &mut self.scratch);
        let mut spectrum = vec![Z::default(); self.n];
        simd::multiply(&self.work, &self.kernel, &mut spectrum);
        self.ifft
            .process_with_scratch(&mut spectrum, &mut self.scratch);
        let scale = 1. / (self.n as f32 * self.energy);
        self.decoded
            .push(spectrum[..self.nr].iter().map(|z| *z * scale).collect());
        Ok(())
    }
    pub fn finish(&self) -> Result<Vec<f32>> {
        Ok(self.finish_with_score()?.0)
    }
    /// Same energy normalization for every seed. Range-median-normalized SNR
    /// alone is unsuitable for code identification with sparse pulse trains.
    pub fn finish_with_score(&self) -> Result<(Vec<f32>, f32)> {
        let nt = self.decoded.len();
        ensure!(nt >= 4, "at least four complete periods required");
        let fft = FftPlanner::new().plan_fft_forward(nt);
        let mut scratch = vec![Z::default(); fft.get_inplace_scratch_len()];
        let mut bins = vec![Z::default(); nt];
        let mut power = vec![0f32; nt * self.nr];
        for r in 0..self.nr {
            for (t, v) in bins.iter_mut().enumerate() {
                *v = if t == 0 || t == nt - 1 {
                    Z::default()
                } else {
                    self.decoded[t][r]
                };
            }
            fft.process_with_scratch(&mut bins, &mut scratch);
            for (t, z) in bins.iter().enumerate() {
                power[t * self.nr + r] = z.norm_sqr();
            }
        }
        let mut result = vec![0f32; self.nr];
        for row in power.chunks_exact(self.nr) {
            // Exponential power's population median is ln(2)*mean. This is an
            // empirical interference+sidelobe floor, not calibrated thermal SNR.
            let noise = median(row) / std::f32::consts::LN_2;
            if noise.is_finite() && noise > 0. {
                for (out, &p) in result.iter_mut().zip(row) {
                    *out = out.max((p - noise) / noise);
                }
            }
        }
        Ok((result, power.iter().copied().fold(0., f32::max)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn numpy_randomstate_seed_one_and_pulse_gate() {
        let mut rng = Mt::new(1);
        for expected in [
            0.417022004702574,
            0.7203244934421581,
            0.00011437481734488664,
        ] {
            assert!((rng.uniform() - expected).abs() < 1e-15);
        }
        let a = code(1, 10000, -1, 500).unwrap();
        let b = code(1, 10000, 10, 500).unwrap();
        assert_eq!(b.iter().filter(|z| z.norm_sqr() > 0.).count(), 200);
        assert_eq!(a[500], b[500]);
        assert_eq!(b[10], Z::default());
    }
    #[test]
    fn known_delay_survives_doppler_and_wrong_seed_does_not_match() {
        for pulse in [-1, 10] {
            let a = code(1, 10000, pulse, 500).unwrap();
            let wrong = code(2, 10000, pulse, 500).unwrap();
            let mut right = Decoder::new(&a, 2000).unwrap();
            let mut control = Decoder::new(&wrong, 2000).unwrap();
            for t in 0..20 {
                let ph = Z::from_polar(1., t as f32 * 0.7);
                let v: Vec<_> = (0..a.len())
                    .map(|i| a[(i + a.len() - 367) % a.len()] * ph)
                    .collect();
                right.push(&v).unwrap();
                control.push(&v).unwrap();
            }
            let p = right.finish().unwrap();
            let q = control.finish().unwrap();
            assert_eq!(
                p.iter()
                    .enumerate()
                    .max_by(|a, b| a.1.total_cmp(b.1))
                    .unwrap()
                    .0,
                367
            );
            assert!(p[367] > q.iter().copied().fold(0., f32::max) * 10.);
            let target = right.finish_with_score().unwrap().1;
            let cross_code = control.finish_with_score().unwrap().1;
            assert!(10. * (target / cross_code).log10() > 10.);
        }
    }
    #[test]
    fn channelizer_preserves_tone_phase_across_chunks() {
        let offset = 4_350_000.;
        let ch = Channelizer::new(offset, 100000.).unwrap();
        for start in [0i64, 1234567, -1000] {
            let v: Vec<_> = (0..27000)
                .map(|i| {
                    let p = TAU * offset * (start + i) as f64 / 25e6;
                    Z::new(p.cos() as f32, p.sin() as f32)
                })
                .collect();
            let out = ch.process(&v, start);
            assert!(out[4..out.len() - 4]
                .iter()
                .all(|z| (*z - Z::new(1., 0.)).norm() < 3e-5));
        }
    }
}

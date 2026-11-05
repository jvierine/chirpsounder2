//! Exact recursive chirp mixing and inverse-MAD-weighted sweep spectra.
use num_complex::{Complex32, Complex64};
use rustfft::FftPlanner;
use std::f64::consts::{PI, TAU};
pub fn fir_window(dec: usize) -> Vec<f32> {
    (0..2 * dec)
        .map(|k| {
            let m = k as f64 - dec as f64 + 1e-6;
            let hann = 0.5 - 0.5 * (TAU * k as f64 / (2 * dec - 1) as f64).cos();
            (hann * (TAU / dec as f64 * m).sin() / (PI * m)) as f32
        })
        .collect()
}
pub fn dechirp(
    raw: &[Complex32],
    sample_offset: u64,
    n_out: usize,
    dec: usize,
    sr: f64,
    cf: f64,
    rate: f64,
) -> Vec<Complex32> {
    let w = fir_window(dec);
    let dt = 1. / sr;
    let accel = Complex64::from_polar(1., -TAU * rate * dt * dt);
    let mut result = vec![Complex32::new(0., 0.); n_out];
    for (i, z) in result.iter_mut().enumerate() {
        let t = (sample_offset + i as u64 * dec as u64) as f64 * dt;
        let f0 = -cf;
        let mut p = Complex64::from_polar(1., -TAU * (f0 + 0.5 * rate * t) * t);
        let mut step =
            Complex64::from_polar(1., -TAU * ((f0 + rate * t) * dt + 0.5 * rate * dt * dt));
        for k in 0..2 * dec {
            let v = raw[i * dec + k] * w[k];
            let pf = Complex32::new(p.re as f32, p.im as f32);
            *z += v * pf;
            p *= step;
            step *= accel;
        }
    }
    result
}
fn median(v: &[f64]) -> f64 {
    let mut a = v.to_vec();
    let n = a.len();
    let (_, p, _) = a.select_nth_unstable_by(n / 2, f64::total_cmp);
    let upper = *p;
    if n % 2 == 0 {
        let lower = a[..n / 2].iter().copied().fold(f64::NEG_INFINITY, f64::max);
        (lower + upper) * 0.5
    } else {
        upper
    }
}
pub fn spectra(z: &[Complex32], nfft: usize, hop: usize, indices: &[usize]) -> Vec<f32> {
    let fft = FftPlanner::<f64>::new().plan_fft_forward(nfft);
    let w: Vec<_> = (0..nfft)
        .map(|i| 0.5 - 0.5 * (TAU * i as f64 / (nfft - 1) as f64).cos())
        .collect();
    let mut work = vec![Complex64::new(0., 0.); nfft];
    let mut out = vec![0f32; indices.len() * nfft];
    let sub = hop / 13;
    for (row, &i) in indices.iter().enumerate() {
        let mut accum = vec![0f64; nfft];
        let mut weights = 0.;
        for k in -6isize..=6 {
            let start = i as isize * hop as isize + k * sub as isize;
            if start < 0 || start as usize + nfft >= z.len() {
                continue;
            }
            for j in 0..nfft {
                let v = z[start as usize + j].conj();
                work[j] = Complex64::new(v.re as f64, v.im as f64) * w[j]
            }
            fft.process(&mut work);
            let power: Vec<_> = (0..nfft)
                .map(|j| work[(j + nfft / 2) % nfft].norm_sqr())
                .collect();
            let m = median(&power);
            let deviations: Vec<_> = power.iter().map(|v| (v - m).abs()).collect();
            let mad = median(&deviations);
            if mad > 0. {
                let weight = 1. / (mad * mad);
                for j in 0..nfft {
                    accum[j] += weight * power[j]
                }
                weights += weight;
            }
        }
        for j in 0..nfft {
            out[row * nfft + j] = if weights > 0. {
                (accum[j] / weights) as f32
            } else {
                f32::NAN
            }
        }
    }
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recursive_mixing_has_expected_beat() {
        let dec = 50;
        let n = 200;
        let sr = 1e6;
        let rate = 100000.;
        let cf = 500000.;
        let offset = 1000.;
        let raw: Vec<_> = (0..n * dec + 2 * dec)
            .map(|i| {
                let t = i as f64 / sr;
                let phase = TAU * ((-cf + offset) * t + 0.5 * rate * t * t);
                Complex32::new(phase.cos() as f32, phase.sin() as f32)
            })
            .collect();
        let z = dechirp(&raw, 0, n, dec, sr, cf, rate);
        let phase = (z[100] * z[99].conj()).arg() as f64;
        assert!((phase - TAU * offset * dec as f64 / sr).abs() < 1e-4);
    }
}

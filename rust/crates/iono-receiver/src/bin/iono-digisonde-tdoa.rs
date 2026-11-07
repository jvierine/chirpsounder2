//! Cross-receiver delay/Doppler evidence; source labels are never fitted here.
use anyhow::{ensure, Result};
use hdf5::File;
use num_complex::Complex32 as Z;
use rustfft::FftPlanner;
use std::{f64::consts::TAU, path::Path};
fn ambiguity(
    a: &[Z],
    b: &[Z],
    sr: f64,
    max_delay: usize,
    fd_limit: i32,
    fd_step: i32,
) -> (Vec<f32>, Vec<f32>) {
    assert_eq!(a.len(), b.len());
    assert!(max_delay < a.len() / 2);
    let n = (a.len() * 2).next_power_of_two();
    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(n);
    let ifft = planner.plan_fft_inverse(n);
    let mean = |v: &[Z]| v.iter().copied().sum::<Z>() / v.len() as f32;
    let ma = mean(a);
    let mb = mean(b);
    let mut fa = vec![Z::default(); n];
    for (v, x) in fa.iter_mut().zip(a) {
        *v = *x - ma;
    }
    fft.process(&mut fa);
    // Limit the same RF passband and suppress strong unresolved carriers.
    fn clip(v: &mut [Z], sr: f64) {
        let n = v.len();
        let amps: Vec<_> = v
            .iter()
            .enumerate()
            .filter(|(i, _)| (*i as f64).min((n - *i) as f64) * sr / n as f64 <= 15000.)
            .map(|(_, z)| z.norm())
            .collect();
        let floor = iono_core::median(&amps);
        for (i, z) in v.iter_mut().enumerate() {
            if (i as f64).min((n - i) as f64) * sr / n as f64 > 15000. {
                *z = Z::default();
            } else if z.norm() > 6. * floor {
                *z *= floor / z.norm();
            }
        }
    }
    clip(&mut fa, sr);
    let ea = fa.iter().map(|z| z.norm_sqr() as f64).sum::<f64>() / n as f64;
    let mut fb = vec![Z::default(); n];
    let mut cross = fb.clone();
    let mut best = vec![0f32; 2 * max_delay + 1];
    let mut doppler = best.clone();
    for fd in (-fd_limit..=fd_limit).step_by(fd_step as usize) {
        fb.fill(Z::default());
        for (i, (v, x)) in fb.iter_mut().zip(b).enumerate() {
            *v = (*x - mb) * Z::from_polar(1., (-TAU * fd as f64 * i as f64 / sr) as f32);
        }
        fft.process(&mut fb);
        clip(&mut fb, sr);
        let eb = fb.iter().map(|z| z.norm_sqr() as f64).sum::<f64>() / n as f64;
        for ((o, x), y) in cross.iter_mut().zip(&fa).zip(&fb) {
            *o = *x * y.conj();
        }
        ifft.process(&mut cross);
        for (j, p) in best.iter_mut().enumerate() {
            let lag = j as isize - max_delay as isize;
            let index = lag.rem_euclid(n as isize) as usize;
            let score = (cross[index].norm_sqr() as f64
                / (n as f64 * n as f64 * ea * eb).max(1e-30)) as f32;
            if score > *p {
                *p = score;
                doppler[j] = fd as f32;
            }
        }
    }
    (best, doppler)
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    ensure!(
        (5..=6).contains(&args.len()) && args.get(5).is_none_or(|s|s=="--phase-null"),
        "usage: iono-digisonde-tdoa RECEIVER_A.h5 RECEIVER_B.h5 CARRIER_HZ OUTPUT.h5 [--phase-null]"
    );
    let carrier: f64 = args[3].parse()?;
    let a = iono_core::digisonde_iq::read(Path::new(&args[1]), carrier)?;
    let b = iono_core::digisonde_iq::read(Path::new(&args[2]), carrier)?;
    let start = a.first_sample.max(b.first_sample);
    let ia = ((start - a.first_sample) as f64 / 250.).round() as usize;
    let ib = ((start - b.first_sample) as f64 / 250.).round() as usize;
    ensure!(
        ia < a.data.len() && ib < b.data.len(),
        "no shared UTC window"
    );
    let length = (a.data.len() - ia).min(b.data.len() - ib);
    ensure!(length >= 16000, "insufficient shared IQ");
    let (maximum, fd) = ambiguity(
        &a.data[ia..ia + length],
        &b.data[ib..ib + length],
        100000.,
        4000,
        40,
        2,
    );
    let lag: Vec<_> = (0..maximum.len())
        .map(|i| (i as f64 - 4000.) / 100000.)
        .collect();
    let mut null_maxima = Vec::new();
    if args.iter().any(|s| s == "--phase-null") {
        let mut seed = 7u64;
        for _ in 0..32 {
            let mut scrambled = b.data[ib..ib + length].to_vec();
            for block in scrambled.chunks_mut(1000) {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                let phase = Z::from_polar(1., (TAU * ((seed >> 32) % 4) as f64 / 4.) as f32);
                for v in block {
                    *v *= phase;
                }
            }
            let (p, _) = ambiguity(&a.data[ia..ia + length], &scrambled, 100000., 4000, 40, 2);
            null_maxima.push(p.into_iter().fold(0., f32::max));
        }
        let observed = maximum.iter().copied().fold(0., f32::max);
        println!("phase-scramble checks >= observed: {}/{} (diagnostic, not a calibrated false-alarm probability)",null_maxima.iter().filter(|v|**v>=observed).count(),null_maxima.len());
    }
    let h = File::create_excl(&args[4])?;
    h.new_dataset_builder()
        .with_data(&lag)
        .create("receiver_a_minus_b_delay_s")?;
    h.new_dataset_builder()
        .with_data(&maximum)
        .create("normalized_cross_power")?;
    h.new_dataset_builder()
        .with_data(&fd)
        .create("receiver_b_doppler_compensation_hz")?;
    h.new_dataset_builder()
        .with_data(&null_maxima)
        .create("phase_scramble_maxima")?;
    h.new_dataset::<f64>()
        .create("carrier_hz")?
        .write_scalar(&carrier)?;
    h.new_dataset::<u64>()
        .create("first_common_sample")?
        .write_scalar(&start)?;
    let mut peaks: Vec<_> = (1..maximum.len() - 1)
        .filter(|&i| maximum[i] > maximum[i - 1] && maximum[i] >= maximum[i + 1])
        .collect();
    peaks.sort_by(|&a, &b| maximum[b].total_cmp(&maximum[a]));
    let mut selected: Vec<usize> = vec![];
    for i in peaks {
        if selected.iter().all(|&j| i.abs_diff(j) > 50) {
            selected.push(i);
            println!(
                "lag={:.3} ms fd={:.1} Hz cross-power={:.5}",
                lag[i] * 1000.,
                fd[i],
                maximum[i]
            );
            if selected.len() == 10 {
                break;
            }
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn known_delay_and_doppler_are_recovered() {
        let n = 4096;
        let mut state = 12u64;
        let mut a = vec![];
        for _ in 0..n {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            a.push(Z::new(
                ((state >> 32) as u32) as f32 / u32::MAX as f32 - 0.5,
                ((state >> 16) as u32) as f32 / u32::MAX as f32 - 0.5,
            ));
        }
        let mut b = vec![Z::default(); n];
        for i in 237..n {
            b[i] = a[i - 237] * Z::from_polar(1., (TAU * 6. * i as f64 / 10000.) as f32);
        }
        let (p, f) = ambiguity(&a, &b, 10000., 500, 10, 2);
        let k = (0..p.len()).max_by(|&a, &b| p[a].total_cmp(&p[b])).unwrap();
        assert_eq!(k as isize - 500, -237);
        assert_eq!(f[k], 6.);
        assert!(p[k] > 0.5);
    }
    #[test]
    fn distinct_source_delays_remain_separate() {
        let n = 4096;
        let mut state = 19u64;
        let mut u = Vec::new();
        let mut v = Vec::new();
        for _ in 0..n {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            u.push(Z::new(
                (state >> 32) as u32 as f32 / u32::MAX as f32 - 0.5,
                0.,
            ));
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            v.push(Z::new(
                (state >> 32) as u32 as f32 / u32::MAX as f32 - 0.5,
                0.,
            ));
        }
        let a: Vec<_> = u.iter().zip(&v).map(|(a, b)| *a + *b).collect();
        let mut b = vec![Z::default(); n];
        for i in 0..n {
            if i >= 100 {
                b[i] += u[i - 100];
            }
            if i + 180 < n {
                b[i] += v[i + 180];
            }
        }
        let (p, _) = ambiguity(&a, &b, 10000., 500, 0, 2);
        assert!(p[400] > 0.1, "missing delayed first source");
        assert!(p[680] > 0.1, "missing advanced second source");
    }
}

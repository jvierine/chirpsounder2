//! Overlapping-tree experiment. Inputs are shared even in the direct baseline:
//! speedups cannot be attributed to fictitious duplicate disk reads.
use anyhow::Result;
use num_complex::Complex32 as Z;
use std::{f64::consts::TAU, hint::black_box, time::Instant};
mod channel_tree;
mod dsp;
mod prn;
mod simd;
use channel_tree::{OverlappingTree, Request};
#[repr(C)]
struct Ts {
    sec: i64,
    ns: i64,
}
unsafe extern "C" {
    fn clock_gettime(clock: i32, time: *mut Ts) -> i32;
}
fn cpu() -> f64 {
    let mut t = Ts { sec: 0, ns: 0 };
    assert_eq!(unsafe { clock_gettime(3, &mut t) }, 0);
    t.sec as f64 + t.ns as f64 * 1e-9
}
fn tone(f: f64, n: i64) -> Z {
    let p = TAU * f * n as f64 / 25e6;
    Z::new(p.cos() as f32, p.sin() as f32)
}
fn validate() -> Result<()> {
    // Exactly on nominal binary boundaries, plus both sides of each boundary.
    // A 40 dB stronger out-of-band tone tests aliasing, not just passband gain.
    for f in [-6_250_000., -3_125_000., 0., 3_125_000., 6_250_000.] {
        for delta in [-25_000., 0., 25_000.] {
            let f = f + delta;
            let first = 1_234_567;
            let raw: Vec<_> = (0..250_000)
                .map(|i| tone(f + 12_000., first + i) + tone(f + 180_000., first + i) * 100.)
                .collect();
            let tree = OverlappingTree::new(&[Request {
                offset_hz: f,
                bandwidth_hz: 80_000.,
            }])?;
            let out = tree.process(&raw, first).remove(0);
            let mut target = Z::default();
            let mut alias = Z::default();
            let mut count = 0.;
            let mut square = 0.;
            for (i, &z) in out.values.iter().enumerate() {
                if z.re.is_finite() && z.im.is_finite() {
                    let n = out.first_sample + i as i64 * 250;
                    target += z * tone(-12_000., n);
                    alias += z * tone(20_000., n);
                    count += 1.;
                    square += (z - tone(12_000., n)).norm_sqr() as f64;
                }
            }
            let gain = target / count as f32;
            let error = (square / count).sqrt();
            assert!(
                (gain - Z::new(1., 0.)).norm() < 0.003,
                "passband/phase f={f}: {gain}"
            );
            assert!(error < 0.01, "strong-interferer leakage f={f}: rms={error}");
            println!("{{\"test\":\"boundary\",\"offset_hz\":{f},\"gain_re\":{},\"gain_im\":{},\"residual_rms\":{error},\"valid_samples\":{count}}}",gain.re,gain.im);
            black_box(alias);
        }
    }
    // Known BPSK chips with a seven-output-sample delay across a split. The
    // channelizer may smooth chip edges, but must not move the correlation peak.
    let carrier = 6_250_000.;
    let delay = 7i64;
    let first = 250_000;
    let bits = [
        1., -1., 1., 1., -1., -1., -1., 1., -1., 1., -1., -1., 1., 1., 1., -1., 1., -1., 1., 1.,
        1., -1., -1., 1., -1., -1., 1., -1., 1., -1., 1.,
    ];
    let chip = |n: i64| bits[n.div_euclid(5000).rem_euclid(bits.len() as i64) as usize];
    let raw: Vec<_> = (0..500_000)
        .map(|i| {
            let n = first + i;
            tone(carrier, n) * chip(n - delay * 250)
        })
        .collect();
    let coded = OverlappingTree::new(&[Request {
        offset_hz: carrier,
        bandwidth_hz: 80_000.,
    }])?
    .process(&raw, first)
    .remove(0);
    let mut peaks = vec![];
    for lag in -12..=12 {
        let mut sum = Z::default();
        let mut count = 0.;
        for (i, &z) in coded.values.iter().enumerate() {
            if z.re.is_finite() {
                let n = coded.first_sample + i as i64 * 250;
                sum += z * chip(n - lag * 250);
                count += 1.;
            }
        }
        peaks.push((sum.norm() / count, lag));
    }
    let peak = peaks
        .into_iter()
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .unwrap();
    assert_eq!(peak.1, delay, "coded delay moved");
    println!("{{\"test\":\"coded_delay\",\"expected_samples\":{delay},\"measured_samples\":{},\"correlation\":{}}}",peak.1,peak.0);
    // A broadband chirp crossing several kHz checks more than a single tone.
    let f = 6_250_000.;
    let first = 0;
    let n = 500_000;
    let phase = |i: i64| {
        TAU * (f * i as f64 / 25e6 + 10_000. * (i as f64 / 25e6).powi(2) / 0.02
            - 10_000. * i as f64 / 25e6)
    };
    let raw: Vec<_> = (0..n)
        .map(|i| Z::new(phase(i).cos() as f32, phase(i).sin() as f32))
        .collect();
    let out = OverlappingTree::new(&[Request {
        offset_hz: f,
        bandwidth_hz: 80_000.,
    }])?
    .process(&raw, first)
    .remove(0);
    let mut error = 0.;
    let mut count = 0.;
    for (i, &z) in out.values.iter().enumerate() {
        if z.re.is_finite() {
            let at = out.first_sample + i as i64 * 250;
            let refphase = phase(at) - TAU * f * at as f64 / 25e6;
            let expected = Z::new(refphase.cos() as f32, refphase.sin() as f32);
            error += ((z - expected).norm_sqr()) as f64;
            count += 1.;
        }
    }
    let rms = (error / count).sqrt();
    assert!(rms < 0.003, "chirp distortion {rms}");
    println!("{{\"test\":\"boundary_chirp\",\"rms_error\":{rms}}}");
    Ok(())
}
fn main() -> Result<()> {
    validate()?;
    if std::env::args().any(|s| s == "--validate-only") {
        return Ok(());
    }
    let raw: Vec<_> = (0..2_502_000)
        .map(|i| Z::new((i as f32 * 0.031).sin(), (i as f32 * 0.071).cos()))
        .collect();
    for layout in ["clustered", "spread"] {
        for channels in [1, 2, 4, 8, 16, 32] {
            let requests: Vec<_> = (0..channels)
                .map(|i| Request {
                    offset_hz: if layout == "clustered" {
                        2_000_000. + (i as f64 - (channels - 1) as f64 / 2.) * 20_000.
                    } else {
                        if channels == 1 {
                            2_000_000.
                        } else {
                            -11_000_000. + 22_000_000. * i as f64 / (channels - 1) as f64
                        }
                    },
                    bandwidth_hz: 80_000.,
                })
                .collect();
            let start = Instant::now();
            let direct: Vec<_> = requests
                .iter()
                .map(|r| prn::Channelizer::new(r.offset_hz, r.bandwidth_hz).unwrap())
                .collect();
            let direct_setup = start.elapsed().as_secs_f64();
            let start = Instant::now();
            let tree = OverlappingTree::new(&requests)?;
            let tree_setup = start.elapsed().as_secs_f64();
            println!("{{\"test\":\"plan\",\"layout\":\"{layout}\",\"channels\":{channels},\"nodes\":{},\"depths\":{:?},\"direct_setup_s\":{direct_setup},\"tree_setup_s\":{tree_setup}}}",tree.node_count(),tree.depths());
            black_box(tree.process(&raw, 0));
            for c in &direct {
                black_box(c.process(&raw, 0));
            }
            for trial in 0..7 {
                for implementation in if trial % 2 == 0 { [0, 1] } else { [1, 0] } {
                    let start = Instant::now();
                    let c = cpu();
                    if implementation == 0 {
                        for c in &direct {
                            black_box(c.process(black_box(&raw), 0));
                        }
                    } else {
                        black_box(tree.process(black_box(&raw), 0));
                    }
                    let cpu_s = cpu() - c;
                    let wall_s = start.elapsed().as_secs_f64();
                    println!("{{\"test\":\"timing\",\"layout\":\"{layout}\",\"channels\":{channels},\"trial\":{trial},\"implementation\":\"{}\",\"cpu_s\":{cpu_s},\"wall_s\":{wall_s}}}",if implementation==0 {"direct"}else{"tree"});
                }
            }
        }
    }
    Ok(())
}

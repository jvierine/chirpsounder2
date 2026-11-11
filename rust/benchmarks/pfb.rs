//! End-to-end shared PFB + per-request narrow extraction, versus the current
//! SIMD direct channelizer. Same in-memory input; no assumed disk-I/O savings.
use anyhow::Result;
use num_complex::Complex32 as Z;
use rustfft::FftPlanner;
use std::{f64::consts::TAU, hint::black_box, time::Instant};
mod dsp;
mod pfb;
mod prn;
mod simd;
use pfb::{Pfb, Request};
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
fn validate(m: usize) -> Result<()> {
    let spacing = 25e6 / m as f64;
    let f = spacing / 2. + 1.;
    let first = 1_234_567;
    let p = Pfb::new(
        m,
        &[Request {
            offset_hz: f,
            bandwidth_hz: 100_000.,
        }],
    )?;
    let h = p.prototype();
    let mut spectrum = vec![Z::default(); 65536];
    for (z, &h) in spectrum.iter_mut().zip(&h) {
        z.re = h;
    }
    FftPlanner::new()
        .plan_fft_forward(spectrum.len())
        .process(&mut spectrum);
    let mut ripple = 0f32;
    let mut stop = 0f32;
    for (i, z) in spectrum[..32769].iter().enumerate() {
        let freq = i as f64 * 25e6 / 65536.;
        if freq <= p.flat_half_hz() {
            ripple = ripple.max((z.norm() - 1.).abs());
        }
        if freq >= p.stop_hz() {
            stop = stop.max(z.norm());
        }
    }
    assert!(ripple < 0.001, "M={m} passband ripple {ripple}");
    assert!(stop < 0.0001, "M={m} alias stop {stop}");
    // Worst half-bin assignment; the wanted band extends another 50 kHz outward.
    // The +Fs_out interferer would alias directly onto it without proper filtering.
    let wanted = f - 50_000.;
    let raw: Vec<_> = (0..250_000)
        .map(|i| tone(wanted, first + i) + tone(wanted + 50e6 / m as f64, first + i) * 100.)
        .collect();
    let c = p.coarse(&raw, first);
    let center = (f / spacing).round() * spacing;
    let mut err = 0.;
    let mut gain = Z::default();
    for (i, &z) in c.bins[0].iter().enumerate() {
        let n = c.first_sample + i as i64 * c.stride as i64;
        let expected = tone(wanted - center, n);
        err += (z - expected).norm_sqr() as f64;
        gain += z * expected.conj();
    }
    let rms = (err / c.bins[0].len() as f64).sqrt();
    gain /= c.bins[0].len() as f32;
    assert!(rms < 0.01, "M={m} worst-boundary/40dB-interferer rms={rms}");
    // Full-rate frame origins, odd FFT bins and fractional final sample positions.
    for carrier in [f, -f, 12_400_000., -12_400_000.] {
        let p = Pfb::new(
            m,
            &[Request {
                offset_hz: carrier,
                bandwidth_hz: 80_000.,
            }],
        )?;
        let raw: Vec<_> = (0..250_000)
            .map(|i| tone(carrier + 12_000., first + i))
            .collect();
        let out = p.process(&raw, first).remove(0);
        let mut gain = Z::default();
        let mut count = 0.;
        for (i, &z) in out.values.iter().enumerate() {
            if z.re.is_finite() {
                let n = out.first_sample + i as i64 * 250;
                gain += z * tone(-12_000., n);
                count += 1.;
            }
        }
        gain /= count as f32;
        assert!(
            (gain - Z::new(1., 0.)).norm() < 0.03,
            "M={m} phase/gain at {carrier}: {gain}"
        );
    }
    // Inject a known 70 us coded delay on a coarse-bin boundary.
    let bits = [
        1., -1., 1., 1., -1., -1., -1., 1., -1., 1., -1., -1., 1., 1., 1., -1., 1., -1., 1., 1.,
        1., -1., -1., 1., -1., -1., 1., -1., 1., -1., 1.,
    ];
    // Smooth chip edges analytically before either channelizer. A discontinuous
    // reference has ambiguous equal peaks after anti-alias filtering.
    let chip = |n: i64| {
        let j = n.div_euclid(5000);
        ((j - 1)..=(j + 1))
            .map(|k| {
                let a = ((n - k * 5000) as f64 / 500.).tanh();
                let b = ((n - (k + 1) * 5000) as f64 / 500.).tanh();
                bits[k.rem_euclid(bits.len() as i64) as usize] * ((a - b) * 0.5) as f32
            })
            .sum::<f32>()
    };
    let origin = 250_000;
    let raw: Vec<_> = (0..500_000)
        .map(|i| {
            let n = origin + i;
            tone(f, n) * chip(n - 1750)
        })
        .collect();
    let out = Pfb::new(
        m,
        &[Request {
            offset_hz: f,
            bandwidth_hz: 80_000.,
        }],
    )?
    .process(&raw, origin)
    .remove(0);
    let mut peaks = vec![];
    for lag in -12..=12 {
        let mut sum = Z::default();
        let mut count = 0.;
        for (i, &z) in out.values.iter().enumerate() {
            if z.re.is_finite() {
                let reference = chip(out.first_sample + i as i64 * 250 - lag * 250);
                sum += z * reference;
                count += reference * reference;
            }
        }
        peaks.push((sum.norm() / count.sqrt(), lag));
    }
    let peak = peaks
        .into_iter()
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .unwrap();
    assert_eq!(peak.1, 7, "M={m} coded delay");
    println!("{{\"test\":\"validation\",\"bins\":{m},\"prototype_taps\":{},\"flat_width_hz\":{},\"passband_max_error\":{ripple},\"stop_amplitude\":{stop},\"boundary_interferer_rms\":{rms},\"coded_delay_samples\":{}}}",h.len(),2.*p.flat_half_hz(),peak.1);
    Ok(())
}
fn main() -> Result<()> {
    let sizes: Vec<usize> = std::env::var("PFB_SIZES")
        .unwrap_or("8,12,16,20,24,32,48,50,64,96,100,128,160,192".into())
        .split(',')
        .map(|s| s.parse().unwrap())
        .collect();
    for &m in &sizes {
        validate(m)?;
    }
    if std::env::args().any(|s| s == "--validate-only") {
        return Ok(());
    }
    let raw: Vec<_> = (0..2_502_000)
        .map(|i| Z::new((i as f32 * 0.031).sin(), (i as f32 * 0.071).cos()))
        .collect();
    let trials = std::env::var("PFB_TRIALS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(7);
    for layout in ["clustered", "spread"] {
        for channels in [1, 4, 8, 16, 32] {
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
            for &m in &sizes {
                let start = Instant::now();
                let p = Pfb::new(m, &requests)?;
                let setup = start.elapsed().as_secs_f64();
                println!("{{\"test\":\"plan\",\"layout\":\"{layout}\",\"channels\":{channels},\"bins\":{m},\"spacing_hz\":{},\"flat_width_hz\":{},\"coarse_rate_hz\":{},\"prototype_taps\":{},\"active_bins\":{},\"direct_setup_s\":{direct_setup},\"pfb_setup_s\":{setup}}}",p.spacing_hz(),p.flat_half_hz()*2.,25e6/p.hop() as f64,p.prototype().len(),p.active_bins());
                black_box(p.process(&raw, 0));
                for c in &direct {
                    black_box(c.process(&raw, 0));
                }
                for trial in 0..trials {
                    for implementation in if (trial + m / 4) % 2 == 0 {
                        [0, 1]
                    } else {
                        [1, 0]
                    } {
                        let start = Instant::now();
                        let c = cpu();
                        if implementation == 0 {
                            for c in &direct {
                                black_box(c.process(black_box(&raw), 0));
                            }
                        } else {
                            black_box(p.process(black_box(&raw), 0));
                        }
                        let cpu_s = cpu() - c;
                        let wall_s = start.elapsed().as_secs_f64();
                        println!("{{\"test\":\"timing\",\"layout\":\"{layout}\",\"channels\":{channels},\"bins\":{m},\"trial\":{trial},\"implementation\":\"{}\",\"cpu_s\":{cpu_s},\"wall_s\":{wall_s}}}",if implementation==0{"direct"}else{"pfb"});
                    }
                }
            }
        }
    }
    Ok(())
}

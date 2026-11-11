//! Full-rate synthetic RF -> shared PFB -> unchanged sounder decoders.
use iono_core::{
    dsp::Digisonde,
    prn::{self, Channelizer, Decoder},
    pulsed::{resample_code, PulseDecoder},
    shared_pfb::Cache,
};
use num_complex::Complex32 as Z;
use std::f64::consts::TAU;
fn both(waveform: &[Z], delay: usize, offset: f64, count: usize) -> (Vec<Z>, Vec<Z>) {
    let origin = 44_867_654_321_024_000u64;
    let make = |s: u64, n: usize| -> anyhow::Result<Vec<Z>> {
        Ok((0..n)
            .map(|i| {
                let t = (s as i128 + i as i128 - origin as i128) as i64;
                let j = t.div_euclid(250) - delay as i64;
                let z = waveform[j.rem_euclid(waveform.len() as i64) as usize];
                let a = TAU * offset * t as f64 / 25e6;
                z * Z::new(a.cos() as f32, a.sin() as f32)
            })
            .collect())
    };
    let raw = make(origin - 4000, count * 250 + 8000).unwrap();
    let bandwidth = if waveform.len() == 10000 {
        100e3
    } else {
        62500.
    };
    let direct = Channelizer::new(offset, bandwidth)
        .unwrap()
        .process(&raw, -4000)[16..16 + count]
        .to_vec();
    let shared = Cache::new(8)
        .unwrap()
        .read(origin, count, origin, offset, bandwidth, make)
        .unwrap();
    (direct, shared)
}
fn max_at(s: &[f32]) -> usize {
    s.iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .unwrap()
        .0
}
#[test]
fn kiruna_uppsala_montelibretti_cached_inverse_preserves_echo_and_code_identity() {
    for (seed, pulse) in [(4, 10), (2, 10), (10, 400)] {
        let code = prn::code(seed, 10000, pulse, 400).unwrap();
        let control = prn::code(1, 10000, pulse, 400).unwrap();
        let (a, b) = both(&code, 47, -9_375_000. + 390_625., 10000);
        let mut powers = vec![];
        for z in [a, b] {
            let mut d = Decoder::new_matrix(&code, 0, 128).unwrap();
            let mut ctl = Decoder::new(&control, 128).unwrap();
            for p in 0..8 {
                let rotation = Z::from_polar(1., p as f32 * 0.4);
                let v: Vec<_> = z.iter().map(|v| *v * rotation).collect();
                d.push(&v).unwrap();
                ctl.push(&v).unwrap();
            }
            let profiles = d.range_estimates().unwrap();
            let phase = (profiles[1][47] * profiles[0][47].conj()).arg();
            assert!((phase - 0.4).abs() < 1e-4, "Doppler changed");
            let (s, peak) = d.finish_with_score().unwrap();
            let negative = ctl.finish_with_score().unwrap().1;
            assert!(max_at(&s).abs_diff(47) <= 1, "seed{seed} lost echo");
            assert!(
                10. * (peak / negative).log10() > 6.,
                "wrong-code acceptance"
            );
            powers.push(s[47]);
        }
        assert!(
            (10. * (powers[1] / powers[0]).log10()).abs() < 1.,
            "seed{seed} sensitivity changed"
        );
    }
}
#[test]
fn digisonde_all_pulse_modes_and_ipps_keep_single_peak() {
    for ipp in [5000usize, 10000, 20000, 40000] {
        let nr = ipp / 10;
        for mode in 0..4 {
            let waveform = iono_core::digisonde_match::template(nr * 8, 100e3, ipp as u32, mode);
            // Build high-rate pulse signal independently from the decoder kernels.
            let origin = 50_000_000u64;
            let lag = 73usize;
            let offset = 4_296_875.;
            let mut cache = Cache::new(4).unwrap();
            let d = Digisonde::new(ipp);
            let mut decoded = vec![];
            for pi in 0..8 {
                let start = origin + (pi * nr * 250) as u64;
                let z = cache
                    .read(start, nr, origin, offset, 30000., |s, n| {
                        Ok((0..n)
                            .map(|i| {
                                let t = (s as i128 + i as i128 - origin as i128) as i64;
                                let j = t.div_euclid(250) - lag as i64;
                                let v = if j >= 0 && j < waveform.len() as i64 {
                                    waveform[j as usize]
                                } else {
                                    Z::default()
                                };
                                let a = TAU * offset * t as f64 / 25e6;
                                v * Z::new(a.cos() as f32, a.sin() as f32)
                            })
                            .collect())
                    })
                    .unwrap();
                decoded.push(d.decode_baseband_ipp(z, pi, None));
            }
            let power = d.power(&decoded);
            let p = max_at(&power[0]);
            assert!(p.abs_diff(lag) <= 1, "IPP{ipp} mode{mode} peak{p}");
            // Complementary-code ghost peaks must not rival the compressed echo.
            let outside = power[0]
                .iter()
                .enumerate()
                .filter(|(i, _)| i.abs_diff(p) > 8)
                .map(|(_, v)| *v)
                .fold(0., f32::max);
            assert!(
                outside < power[0][p] * 0.12,
                "IPP{ipp} mode{mode} doubled trace"
            );
        }
    }
}
#[test]
fn verso_matched_and_regularized_decoding_preserve_range() {
    let native: Vec<_> = include_bytes!("../../../deploy/verso/SGO_bauds_complex128.bin")
        .chunks_exact(16)
        .map(|b| {
            Z::new(
                f64::from_le_bytes(b[..8].try_into().unwrap()) as f32,
                f64::from_le_bytes(b[8..].try_into().unwrap()) as f32,
            )
        })
        .collect();
    let code = resample_code(&native, 62500, 100000).unwrap();
    let control: Vec<_> = code.iter().rev().map(|v| v.conj()).collect();
    for inverse in [false, true] {
        let mut d = if inverse {
            PulseDecoder::regularized(&code, 128, 0.01).unwrap()
        } else {
            PulseDecoder::new(&code, 128).unwrap()
        };
        let prefix = d.input_prefix();
        let n = d.input_len();
        let lag = 47;
        let mut waveform = vec![Z::default(); n];
        waveform[prefix + lag..prefix + lag + code.len()].copy_from_slice(&code);
        let (_, z) = both(&waveform, 0, 2_734_375., n);
        let mut ctl = PulseDecoder::new(&control, 128).unwrap();
        for _ in 0..8 {
            d.push(&z).unwrap();
            ctl.push(&z[prefix..prefix + ctl.input_len()]).unwrap();
        }
        let (s, peak) = d.finish().unwrap();
        let negative = ctl.finish().unwrap().1;
        assert!(max_at(&s).abs_diff(lag) <= 1, "VERSO echo moved");
        assert!(10. * (peak / negative).log10() > 6.);
    }
}
#[test]
fn digisonde_fine_frequency_multiplexing_does_not_double_echo() {
    let (nr, lag, origin, offset) = (1000usize, 73usize, 50_000_000u64, -8_984_375.);
    let wave = iono_core::digisonde_match::template(8 * nr, 100e3, 10000, 3);
    for block in [1, 2, 4] {
        let mut cache = Cache::new(8).unwrap();
        let decoder = Digisonde::new(10000);
        let mut decoded = vec![];
        for p in 0..8 {
            let physical = p / block * (2 * block) + p % block;
            let start = origin + (physical * nr * 250) as u64;
            let z = cache
                .read(start, nr, origin, offset, 30000., |s, n| {
                    Ok((0..n)
                        .map(|i| {
                            let t = (s as i128 + i as i128 - origin as i128) as i64;
                            let j = t.div_euclid(250) - lag as i64;
                            if j < 0 {
                                return Z::default();
                            }
                            let pulse = j as usize / nr;
                            let selected = pulse / (2 * block) * block + pulse % block;
                            let code = wave
                                .get(selected * nr + j as usize % nr)
                                .copied()
                                .unwrap_or_default();
                            let a = TAU
                                * (offset + if pulse / block % 2 == 0 { 0. } else { 50e3 })
                                * t as f64
                                / 25e6;
                            code * Z::new(a.cos() as f32, a.sin() as f32)
                        })
                        .collect())
                })
                .unwrap();
            decoded.push(decoder.decode_baseband_ipp(z, p, None));
        }
        let power = decoder.power(&decoded);
        let peak = max_at(&power[0]);
        assert!(peak.abs_diff(lag) <= 1, "block{block} peak{peak}");
        let ghost = power[0]
            .iter()
            .enumerate()
            .filter(|(i, _)| i.abs_diff(peak) > 8)
            .map(|(_, v)| *v)
            .fold(0., f32::max);
        assert!(
            ghost < power[0][peak] * 0.12,
            "multiplex decoder doubled trace"
        );
    }
}

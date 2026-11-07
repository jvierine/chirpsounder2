//! Offline pulse-pair audit of bounded 100 ksample/s IQ caches. No clock edits.
use anyhow::{ensure, Result};
use hdf5::File;
use iono_core::{dsp::Digisonde, median};
use num_complex::Complex32 as Z;
#[derive(Clone, Debug)]
struct Model {
    block: usize,
    order: bool,
    phase: usize,
    groups: usize,
    snr: f32,
    pslr: f32,
    delay: usize,
    peak: f32,
    profile: Vec<f32>,
}
#[cfg(test)]
fn audit(z: &[Z], block: usize, order: bool, phase: usize) -> Option<Model> {
    let dsp = Digisonde::new(10000);
    let pairs: Vec<_> = z
        .chunks_exact(1000)
        .enumerate()
        .map(|(_p, a)| {
            [
                dsp.decode_baseband_ipp(a.to_vec(), 0, Some(6.)),
                dsp.decode_baseband_ipp(a.to_vec(), 1, Some(6.)),
            ]
        })
        .collect();
    audit_pairs(&dsp, &pairs, block, order, phase)
}
fn audit_pairs(
    dsp: &Digisonde,
    pairs: &[[Vec<Z>; 2]],
    block: usize,
    order: bool,
    phase: usize,
) -> Option<Model> {
    let complete =
        iono_core::digisonde_match::multiplex_quartets(pairs.len(), block, 2, phase, order);
    if complete.len() < 2 {
        return None;
    }
    let mut decoded = vec![];
    for group in &complete {
        for item in group.iter() {
            let (p, logical) = *item;
            let sign = if logical % 8 >= 4 { -1. } else { 1. };
            decoded.push(pairs[p][logical % 2].iter().map(|v| *v * sign).collect());
        }
    }
    let powers = dsp.power(&decoded);
    let profile: Vec<_> = powers[0]
        .iter()
        .zip(&powers[1])
        .map(|(a, b)| a.max(*b) / (complete.len() * complete.len()) as f32)
        .collect();
    let (delay, &peak) = profile
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))?;
    let floor = median(&profile).max(f32::MIN_POSITIVE);
    let side = profile
        .iter()
        .enumerate()
        .filter(|(i, _)| i.abs_diff(delay).min(1000 - i.abs_diff(delay)) > 8)
        .map(|(_, v)| *v)
        .fold(f32::MIN_POSITIVE, f32::max);
    Some(Model {
        block,
        order,
        phase,
        groups: complete.len(),
        snr: 10. * (peak / floor).log10(),
        pslr: 10. * (peak / side).log10(),
        delay,
        peak,
        profile,
    })
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    ensure!(
        (4..=6).contains(&args.len())
            && args[4..]
                .iter()
                .all(|s| s == "--no-clip" || s.starts_with("--skip-ipps=")),
        "usage: iono-digisonde-pulse-audit IQ.h5 CARRIER_HZ OUTPUT.h5 [--no-clip] [--skip-ipps=N]"
    );
    let carrier: f64 = args[2].parse()?;
    let channel = iono_core::digisonde_iq::read(std::path::Path::new(&args[1]), carrier)?;
    let skip: usize = args
        .iter()
        .find_map(|s| s.strip_prefix("--skip-ipps="))
        .unwrap_or("0")
        .parse()?;
    ensure!(
        skip < 32 && skip * 1000 < channel.data.len(),
        "invalid IPP skip"
    );
    let first = channel.first_sample + skip as u64 * 250000;
    let z = &channel.data[skip * 1000..];
    let dsp = Digisonde::new(10000);
    let clip = if args.iter().any(|s| s == "--no-clip") {
        None
    } else {
        Some(6.)
    };
    let pairs: Vec<_> = z
        .chunks_exact(1000)
        .map(|a| {
            [
                dsp.decode_baseband_ipp(a.to_vec(), 0, clip),
                dsp.decode_baseband_ipp(a.to_vec(), 1, clip),
            ]
        })
        .collect();
    let mut models = vec![];
    for block in [1, 2, 4, 8, 16] {
        for order in [false, true] {
            for phase in 0..32 {
                if let Some(m) = audit_pairs(&dsp, &pairs, block, order, phase) {
                    models.push(m)
                }
            }
        }
    }
    models.sort_by(|a, b| b.peak.total_cmp(&a.peak));
    let out = File::create_excl(&args[3])?;
    for (code, name) in [(0, "pulse_a_complex32"), (1, "pulse_b_complex32")] {
        let z: Vec<[f32; 2]> = pairs
            .iter()
            .flat_map(|p| p[code].iter().map(|z| [z.re, z.im]))
            .collect();
        out.new_dataset::<[f32; 2]>()
            .shape([pairs.len(), 1000])
            .chunk([1, 1000])
            .deflate(4)
            .create(name)?
            .write_raw(&z)?;
    }
    let parameters: Vec<f64> = models
        .iter()
        .flat_map(|m| {
            [
                m.block as f64,
                u8::from(m.order) as f64,
                m.phase as f64,
                m.groups as f64,
                m.snr as f64,
                m.pslr as f64,
                m.delay as f64 * 10.,
                m.peak as f64,
            ]
        })
        .collect();
    out.new_dataset::<f64>()
        .shape([models.len(), 8])
        .create("models_block_order_phase_groups_snr_pslr_delay_us_power")?
        .write_raw(&parameters)?;
    let profiles: Vec<_> = models
        .iter()
        .flat_map(|m| m.profile.iter().copied())
        .collect();
    out.new_dataset::<f32>()
        .shape([models.len(), 1000])
        .chunk([1, 1000])
        .deflate(4)
        .create("compressed_power")?
        .write_raw(&profiles)?;
    out.new_dataset::<u64>()
        .create("iq_first_sample")?
        .write_scalar(&first)?;
    out.new_dataset::<f64>()
        .create("carrier_hz")?
        .write_scalar(&carrier)?;
    for m in models.iter().take(8) {
        println!("block={} pol-first={} phase={} groups={} peak-lag={}us SNR={:.1}dB peak/secondary={:.1}dB",m.block,m.order,m.phase,m.groups,m.delay*10,m.snr,m.pslr);
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    const A: [f32; 16] = [
        1., 1., -1., 1., 1., 1., 1., -1., -1., 1., 1., 1., -1., 1., -1., -1.,
    ];
    const B: [f32; 16] = [
        -1., -1., 1., -1., -1., -1., -1., 1., -1., 1., 1., 1., -1., 1., -1., -1.,
    ];
    fn target(order: bool, delays: &[(usize, f32)]) -> Vec<Z> {
        let mut z = vec![Z::default(); 32000];
        for physical in 0..32 {
            if physical / 4 % 2 != 0 {
                continue;
            }
            // Independent transmitter sequence: AX/BX/AO/BO or AX/AO/BX/BO.
            // Do not call the receiver's index mapping in this waveform fixture.
            let code_a = if order {
                [true, true, false, false][physical % 4]
            } else {
                [true, false, true, false][physical % 4]
            };
            let chips = if code_a { A } else { B };
            let sign = if physical / 8 % 2 == 1 { -1. } else { 1. };
            for &(delay, amplitude) in delays {
                for sample in 0..54 {
                    let bit = (sample as f64 * 0.3).floor() as usize;
                    if bit < 16 {
                        let k = physical * 1000 + 54 + sample + delay;
                        if k < z.len() {
                            z[k] += Z::new(chips[bit] * sign * amplitude, 0.);
                        }
                    }
                }
            }
        }
        z
    }
    #[test]
    fn correct_pairing_compresses_an_independent_point_target() {
        for order in [false, true] {
            let z = target(order, &[(230, 1.)]);
            let right = audit(&z, 4, order, 0).unwrap();
            let wrong = audit(&z, 4, !order, 0).unwrap();
            assert!(right.delay.abs_diff(230) <= 2, "{right:?}");
            assert!(
                right.peak > wrong.peak * 1.4,
                "correct {} wrong {}",
                right.peak,
                wrong.peak
            );
            assert!(right.pslr > 6., "{}", right.pslr);
        }
    }
    #[test]
    fn two_physical_echoes_are_not_forced_to_one_peak() {
        let z = target(false, &[(230, 1.), (460, 0.7)]);
        let m = audit(&z, 4, false, 0).unwrap();
        let second = m.profile[458..463].iter().copied().fold(0., f32::max);
        assert!(second > m.peak * 0.25);
    }
    #[test]
    fn shifted_window_keeps_code_identity_and_complete_polarization_pairs() {
        for order in [false, true] {
            let z = target(order, &[(230, 1.)]);
            // Keep the range origin fixed within each IPP, but begin the dwell
            // one physical pulse later, as happens after a geometric read shift.
            let shifted = &z[1000..];
            let right = audit(shifted, 4, order, 1).unwrap();
            let wrong = audit(shifted, 4, order, 0).unwrap();
            assert!(right.delay.abs_diff(230) <= 2);
            if !order {
                assert!(right.peak > wrong.peak * 2.);
            }
            assert!(right.pslr > 20.);
            assert!(
                right.pslr > wrong.pslr + 10.,
                "order={order} right={} wrong={}",
                right.pslr,
                wrong.pslr
            );
        }
    }
}

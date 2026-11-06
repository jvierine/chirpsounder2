//! Supplied-waveform, actual-channelizer compression response; no receiver I/O.
use anyhow::{ensure, Result};
use iono_core::{
    prn::Channelizer,
    pulsed::{resample_code, PulseDecoder},
};
use num_complex::Complex32 as Z;
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    ensure!(args.len() == 2, "usage: iono-verso-filter-check result.h5");
    let bytes = include_bytes!("../../../../deploy/verso/SGO_bauds_complex128.bin");
    let native: Vec<_> = bytes
        .chunks_exact(16)
        .map(|v| {
            Z::new(
                f64::from_le_bytes(v[..8].try_into().unwrap()) as f32,
                f64::from_le_bytes(v[8..].try_into().unwrap()) as f32,
            )
        })
        .collect();
    let code = resample_code(&native, 62500, 100000)?;
    let nr = 4096;
    let center = 2048;
    let probe = PulseDecoder::regularized(&code, nr, 0.01)?;
    let prefix = probe.input_prefix();
    let mut raw = vec![Z::default(); probe.input_len() * 250 + 2000];
    for (i, &v) in native.iter().enumerate() {
        raw[(prefix + center) * 250 + 1000 + i * 400
            ..(prefix + center) * 250 + 1000 + (i + 1) * 400]
            .fill(v);
    }
    let z = Channelizer::new(0., 62500.)?.process(&raw, 0);
    let h = hdf5::File::create(&args[1])?;
    h.new_dataset_builder()
        .with_data(
            &(0..nr)
                .map(|i| (i as f64 - center as f64) * iono_core::C / 100000. / 1000.)
                .collect::<Vec<_>>(),
        )
        .create("range_offset_km")?;
    for (i, lambda) in [0., 0.1, 0.03, 0.01, 0.003].into_iter().enumerate() {
        let mut decoder = if lambda == 0. {
            PulseDecoder::new(&code, nr)?
        } else {
            PulseDecoder::regularized(&code, nr, lambda)?
        };
        let offset = 4 + prefix - decoder.input_prefix();
        let response = decoder.decode_pulse(&z[offset..offset + decoder.input_len()])?;
        let peak = response[center].norm();
        let psl = 20.
            * (response
                .iter()
                .enumerate()
                .filter(|(j, _)| j.abs_diff(center) > 4)
                .map(|(_, z)| z.norm())
                .fold(0., f32::max)
                / peak)
                .log10();
        let width = response
            .iter()
            .filter(|z| z.norm() >= peak * std::f32::consts::FRAC_1_SQRT_2)
            .count();
        let g = h.create_group(&format!("filter_{i}"))?;
        for (name, value) in [
            ("regularization", lambda),
            ("white_noise_penalty_db", decoder.noise_gain_db()),
            ("peak_sidelobe_db", psl),
            ("half_power_samples", width as f32),
        ] {
            g.new_dataset::<f32>().create(name)?.write_scalar(&value)?;
        }
        g.new_dataset_builder()
            .with_data(
                &response
                    .iter()
                    .map(|z| 20. * (z.norm() / peak).max(1e-6).log10())
                    .collect::<Vec<_>>(),
            )
            .create("relative_amplitude_db")?;
        println!("lambda {lambda}: filtered-waveform PSL {psl:.2} dB, white-noise penalty {:.2} dB, half-power width {width} samples",decoder.noise_gain_db());
        for doppler in [5., 10., 25.] {
            let shifted: Vec<_> = z[offset..offset + decoder.input_len()]
                .iter()
                .enumerate()
                .map(|(j, &v)| {
                    v * Z::from_polar(1., std::f32::consts::TAU * doppler * j as f32 / 100000.)
                })
                .collect();
            let v = decoder.decode_pulse(&shifted)?;
            let peak_d = v[center].norm();
            let sidelobe = v
                .iter()
                .enumerate()
                .filter(|(j, _)| j.abs_diff(center) > 4)
                .map(|(_, z)| z.norm())
                .fold(0., f32::max);
            let d = g.create_group(&format!("doppler_{}", doppler as u32))?;
            d.new_dataset::<f32>()
                .create("peak_sidelobe_db")?
                .write_scalar(&(20. * (sidelobe / peak_d).log10()))?;
            d.new_dataset::<f32>()
                .create("peak_loss_db")?
                .write_scalar(&(20. * (peak_d / peak).log10()))?;
            println!(
                "  {doppler} Hz: PSL {:.2} dB, peak change {:.2} dB",
                20. * (sidelobe / peak_d).log10(),
                20. * (peak_d / peak).log10()
            );
        }
    }
    Ok(())
}

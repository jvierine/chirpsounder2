//! Bounded receiver-local IQ caches used for Digisonde commissioning.
use anyhow::{ensure, Context, Result};
use hdf5::File;
use num_complex::Complex32 as Z;
use std::{f64::consts::TAU, path::Path};
pub struct Channel {
    pub first_sample: u64,
    pub data: Vec<Z>,
}
pub fn read(path: &Path, carrier: f64) -> Result<Channel> {
    let h = File::open(path)?;
    let samples = h.dataset("iq/sample_index")?.read_raw::<u64>()?;
    let carriers = h.dataset("iq/carrier_hz")?.read_raw::<f64>()?;
    ensure!(
        samples.len() == carriers.len() && samples.len() <= 512,
        "invalid IQ axes"
    );
    let d = h.dataset("iq/baseband_complex32")?;
    ensure!(
        d.shape() == [samples.len(), 16000] || d.shape() == [samples.len(), 16000, 2],
        "invalid bounded IQ cache"
    );
    let iq: Vec<[f32; 2]> = if d.shape().len() == 2 {
        d.read_raw()?
    } else {
        d.read_raw::<f32>()?
            .chunks_exact(2)
            .map(|v| [v[0], v[1]])
            .collect()
    };
    let selected: Vec<_> = carriers
        .iter()
        .enumerate()
        .filter(|(_, f)| (**f - carrier).abs() < 1.)
        .map(|(i, _)| i)
        .collect();
    let first = selected
        .iter()
        .map(|&i| samples[i])
        .min()
        .context("carrier absent")?;
    let end = selected
        .iter()
        .map(|&i| ((samples[i] - first) as f64 / 250.).round() as usize + 16000)
        .max()
        .unwrap();
    ensure!(end <= 100000, "IQ span exceeds one second");
    let mut data = vec![Z::default(); end];
    let mut count = vec![0u32; end];
    for i in selected {
        let delta = samples[i] - first;
        let start = (delta as f64 / 250.).round() as usize;
        let phase = Z::from_polar(
            1.,
            (-TAU * ((carrier - 12.5e6) * (delta % 25_000_000) as f64 / 25e6).rem_euclid(1.))
                as f32,
        );
        for (j, a) in iq[i * 16000..(i + 1) * 16000].iter().enumerate() {
            data[start + j] += Z::new(a[0], a[1]) * phase;
            count[start + j] += 1;
        }
    }
    ensure!(
        count.iter().all(|n| *n > 0),
        "unobserved IQ gap; do not synthesize voltage"
    );
    for (v, n) in data.iter_mut().zip(&count) {
        *v /= *n as f32;
    }
    Ok(Channel {
        first_sample: first,
        data,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn overlapping_windows_keep_physical_oscillator_phase() {
        let path =
            std::env::temp_dir().join(format!("digisonde-iq-phase-{}.h5", std::process::id()));
        let first = 44_000_000_000_000_000u64;
        let delta = 1_000_004u64;
        let carrier = 9_000_000.;
        let rotation = Z::from_polar(
            1.,
            (TAU * ((carrier - 12.5e6) * (delta % 25_000_000) as f64 / 25e6).rem_euclid(1.)) as f32,
        );
        {
            let h = File::create(&path).unwrap();
            h.create_group("iq").unwrap();
            h.new_dataset_builder()
                .with_data(&[first, first + delta])
                .create("iq/sample_index")
                .unwrap();
            h.new_dataset_builder()
                .with_data(&[carrier, carrier])
                .create("iq/carrier_hz")
                .unwrap();
            let mut raw = vec![[1f32, 0.]; 16000];
            raw.extend(vec![[rotation.re, rotation.im]; 16000]);
            h.new_dataset::<[f32; 2]>()
                .shape([2, 16000])
                .create("iq/baseband_complex32")
                .unwrap()
                .write_raw(&raw)
                .unwrap();
        }
        let c = read(&path, carrier).unwrap();
        assert_eq!(c.first_sample, first);
        assert_eq!(c.data.len(), 20000);
        assert!(c.data.iter().all(|v| (*v - Z::new(1., 0.)).norm() < 1e-5));
        std::fs::remove_file(path).unwrap();
    }
}

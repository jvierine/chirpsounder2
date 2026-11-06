//! Sparse diagnostic ionograms from the detector's existing matched chirp FFTs.
//! Sources are provisional range-gate associations; no new voltage scans/FFTs.
use super::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Deserialize, Serialize)]
pub struct Source {
    pub name: String,
    pub range_gate_km: [f64; 2],
}
pub struct Column {
    source: String,
    gate: [f64; 2],
    epoch: i64,
    time: f64,
    rate: f64,
    frequency: f64,
    range_step: f64,
    points: Vec<(usize, f32)>,
}
fn frequency(k: usize, n: usize, sr: f64, cf: f64) -> f64 {
    cf + (if k < n / 2 {
        k as f64
    } else {
        k as f64 - n as f64
    }) * sr
        / n as f64
}
pub fn project(c: &Config, t: f64, rate: f64, fft: &[Complex32]) -> Vec<Column> {
    let n = fft.len();
    let noise = median(
        &fft.iter()
            .step_by((n / 4096).max(1))
            .map(|z| z.norm_sqr())
            .collect::<Vec<_>>(),
    );
    if !noise.is_finite() || noise <= 0. {
        return vec![];
    }
    let mut peaks: Vec<usize> = vec![];
    for _ in 0..2 {
        let peak = fft
            .iter()
            .enumerate()
            .filter(|(k, _)| {
                peaks.iter().all(|&j| {
                    let d = k.abs_diff(j);
                    d.min(n - d) as f64 * c.sample_rate / n as f64 > 200000.
                })
            })
            .max_by(|a, b| a.1.norm_sqr().total_cmp(&b.1.norm_sqr()))
            .map(|(k, _)| k);
        let Some(k) = peak else {
            break;
        };
        if (fft[k].norm_sqr() - noise) / noise < 10f32.powf(13. / 10.) {
            break;
        }
        peaks.push(k);
    }
    let step = C * c.sample_rate / (rate * n as f64) / 1000.;
    let mut columns = vec![];
    let mut used = BTreeSet::new();
    for k in peaks {
        let f = frequency(k, n, c.sample_rate, c.center_freq);
        if !(500000. ..25e6).contains(&f) {
            continue;
        }
        let received_epoch = t - f / rate;
        let epoch = received_epoch.floor() as i64;
        let peak_range = (received_epoch - epoch as f64) * C / 1000.;
        for source in &c.radar_sources {
            let [lo, hi] = source.range_gate_km;
            if peak_range < lo || peak_range > hi || !used.insert((source.name.clone(), epoch)) {
                continue;
            }
            let center = rate * (t - epoch as f64);
            let nr = ((hi - lo) / step).ceil() as usize + 1;
            if nr < 2 || nr > 4096 {
                continue;
            }
            let mut points = vec![];
            for r in 0..nr {
                let expected = center - (lo + r as f64 * step) * 1000. * rate / C;
                let bin = (((expected - c.center_freq) / c.sample_rate * n as f64).round() as i64)
                    .rem_euclid(n as i64) as usize;
                let snr = (fft[bin].norm_sqr() - noise) / noise;
                if snr.is_finite() && snr >= 10f32.powf(13. / 10.) {
                    points.push((r, snr));
                }
            }
            if !points.is_empty() {
                columns.push(Column {
                    source: source.name.clone(),
                    gate: [lo, hi],
                    epoch,
                    time: t,
                    rate,
                    frequency: rate * (t + n as f64 / c.sample_rate / 2. - epoch as f64),
                    range_step: step,
                    points,
                });
            }
        }
    }
    columns
}
struct Image {
    gate: [f64; 2],
    step: f64,
    points: BTreeMap<u32, f32>,
    frequencies: BTreeSet<usize>,
    last_time: f64,
    last_save: f64,
    dirty: bool,
}
#[derive(Default)]
pub struct Accumulator {
    images: BTreeMap<(String, i64, i64), Image>,
}
impl Accumulator {
    pub fn update(&mut self, c: &Config, columns: Vec<Column>, now: f64) -> Result<()> {
        for column in columns {
            let f = (column.frequency / 50000.).floor() as usize;
            if f >= 500 {
                continue;
            }
            let nr = ((column.gate[1] - column.gate[0]) / column.range_step).ceil() as usize + 1;
            let image = self
                .images
                .entry((column.source, column.rate as i64, column.epoch))
                .or_insert_with(|| Image {
                    gate: column.gate,
                    step: column.range_step,
                    points: BTreeMap::new(),
                    frequencies: BTreeSet::new(),
                    last_time: now,
                    last_save: 0.,
                    dirty: false,
                });
            image.last_time = column.time;
            image.frequencies.insert(f);
            image.dirty = true;
            for (r, snr) in column.points {
                let entry = image.points.entry((f * nr + r) as u32).or_insert(snr);
                *entry = entry.max(snr);
            }
        }
        for ((source, rate, epoch), image) in &mut self.images {
            if image.dirty && image.frequencies.len() >= 3 && now - image.last_save >= 30. {
                save(c, source, *rate, *epoch, image)?;
                image.last_save = now;
                image.dirty = false;
            }
        }
        self.images
            .retain(|(_, rate, epoch), _| now - (*epoch as f64) < 25e6 / (*rate as f64) + 120.);
        while self.images.len() > 12 {
            let key = self.images.keys().next().unwrap().clone();
            self.images.remove(&key);
        }
        Ok(())
    }
}
fn save(c: &Config, source: &str, rate: i64, epoch: i64, image: &Image) -> Result<()> {
    let path = output(
        c,
        &format!(
            "lfm_ionogram-{source}-{}-{}-{}",
            c.station,
            c.channel,
            rate / 1000
        ),
        epoch as f64,
    )?;
    let tmp = path.with_extension("partial.h5");
    let nr = ((image.gate[1] - image.gate[0]) / image.step).ceil() as usize + 1;
    let h = File::create(&tmp)?;
    iono_core::receiver_timing::write_metadata(&h)?;
    let freqs: Vec<_> = (0..500).map(|i| (i as f64 + 0.5) * 50000.).collect();
    let ranges: Vec<_> = (0..nr)
        .map(|i| (image.gate[0] + i as f64 * image.step) * 1000.)
        .collect();
    h.new_dataset_builder().with_data(&freqs).create("freqs")?;
    h.new_dataset_builder()
        .with_data(&ranges)
        .create("ranges")?;
    let indices: Vec<_> = image.points.keys().copied().collect();
    let snr: Vec<_> = image.points.values().copied().collect();
    h.new_dataset_builder()
        .with_data(&indices)
        .deflate(3)
        .create("SNR_sparse_indices")?;
    h.new_dataset_builder()
        .with_data(&snr)
        .deflate(3)
        .create("SNR_sparse_values")?;
    for (name, v) in [
        ("t0", epoch as f64),
        ("rate", rate as f64),
        ("range_offset_applied", 1.),
        ("last_observation_unix", image.last_time),
        ("source_assumed", 1.),
        ("snr_threshold_db", 13.),
    ] {
        dataset(&h, name, v)?;
    }
    text(&h, "station_name", &c.station)?;
    text(&h, "txname", source)?;
    text(&h, "processor", "iono-receiver/sparse-matched-chirp-v1")?;
    text(&h,"source_identification","assumed from virtual-range gate and integer-second transmit epoch; not a verified transmitter identity")?;
    text(
        &h,
        "snr_reference",
        "matched chirp FFT after spectral whitening; median noise over the recorded band",
    )?;
    h.flush()?;
    drop(h);
    std::fs::rename(&tmp, &path)?;
    eprintln!(
        "sparse assumed {source} ionogram: {} echoes in {} measured frequency bins",
        snr.len(),
        image.frequencies.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn known_delayed_chirp_maps_to_physical_gate_without_filling_noise() {
        let c: Config = serde_json::from_str(r#"{"station":"KREA","data_dir":"x","output_dir":"y","sample_rate":2500000,"center_freq":1250000,"radar_sources":[{"name":"Cyprus","range_gate_km":[5240.62,7090.26]},{"name":"Australia","range_gate_km":[6429.61,8698.89]}]}"#).unwrap();
        let mut fft = vec![Complex32::new(1., 0.); 250000];
        // A 20 ms propagation delay at 100 kHz/s shifts 1 MHz to 998 kHz.
        fft[224800] = Complex32::new(100., 0.);
        let columns = project(&c, 1800000010., 100000., &fft);
        assert_eq!(columns.len(), 1);
        let column = &columns[0];
        assert_eq!(column.source, "Cyprus");
        assert_eq!(column.epoch, 1800000000);
        assert_eq!(column.points.len(), 1);
        let distance = column.gate[0] + column.points[0].0 as f64 * column.range_step;
        assert!((distance - C * 0.02 / 1000.).abs() <= column.range_step);
        assert!(column.points[0].1 > 1000.);
    }
}

//! Screen configured schedules for simultaneous RF occupancy; not TX identification.
use anyhow::{ensure, Context, Result};
use hdf5::File;
use serde_json::Value;
use std::path::Path;
fn v(c: &Value, k: &str, d: f64) -> f64 {
    c[k].as_f64().unwrap_or(d)
}
fn dwell(c: &Value) -> f64 {
    v(c, "n_ipp", 64.) * v(c, "ipp_us", 10000.) / 1e6 + v(c, "inter_frequency_gap_s", 0.)
}
fn active(c: &Value, t: f64) -> bool {
    let h = t.rem_euclid(86400.) / 3600.;
    let a = c["sounding_hours"][0].as_f64().unwrap_or(0.);
    let b = c["sounding_hours"][1].as_f64().unwrap_or(24.);
    if a <= b {
        h >= a && h < b
    } else {
        h >= a || h < b
    }
}
fn overlap(a: f64, b: f64, c: f64, d: f64) -> f64 {
    (b.min(d) - a.max(c)).max(0.)
}
fn main() -> Result<()> {
    let a: Vec<_> = std::env::args().collect();
    ensure!(
        a.len() == 5,
        "usage: iono-digisonde-overlap CONFIG_DIRECTORY TARGET.json SOUNDING_EPOCH OUTPUT.h5"
    );
    let target: Value = serde_json::from_slice(&std::fs::read(&a[2])?)?;
    let epoch: f64 = a[3].parse()?;
    ensure!(epoch.is_finite(), "invalid epoch");
    let mut files: Vec<_> = std::fs::read_dir(&a[1])?
        .map(|p| p.map(|p| p.path()))
        .collect::<std::io::Result<_>>()?;
    files.sort();
    let mut rows = Vec::<[f64; 7]>::new();
    let mut labels = Vec::new();
    for path in files {
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        let c: Value = serde_json::from_slice(&std::fs::read(&path)?)?;
        if c["freq_start_hz"].is_null()
            || c["freq_step_hz"].is_null()
            || c["transmitter"].is_null()
            || c["transmitter"] == target["transmitter"]
        {
            continue;
        }
        let period = v(&c, "interval_s", 0.);
        if period <= 0. {
            continue;
        }
        let label = format!(
            "{} ({})",
            c["transmitter"].as_str().context("transmitter string")?,
            path.file_name().unwrap().to_string_lossy()
        );
        let index = labels.len();
        labels.push(label.clone());
        let mut n = 0;
        let nf = ((v(&target, "freq_stop_hz", 0.) - v(&target, "freq_start_hz", 0.))
            / v(&target, "freq_step_hz", 1.))
        .round() as usize;
        ensure!(nf <= 4000, "frequency grid too large");
        for i in 0..nf {
            let f = v(&target, "freq_start_hz", 0.) + i as f64 * v(&target, "freq_step_hz", 0.);
            let t = epoch + i as f64 * dwell(&target) + v(&target, "transmit_delay_us", 0.) / 1e6;
            let j0 = ((f - v(&c, "freq_start_hz", 0.)) / v(&c, "freq_step_hz", 1.)).round() as i64;
            for j in j0 - 1..=j0 + 1 {
                let rf = v(&c, "freq_start_hz", 0.) + j as f64 * v(&c, "freq_step_hz", 0.);
                if j < 0 || rf >= v(&c, "freq_stop_hz", 0.) || (rf - f).abs() > 30000. {
                    continue;
                }
                let step = j as f64 * dwell(&c) + v(&c, "transmit_delay_us", 0.) / 1e6;
                let base = ((t - step - v(&c, "start_offset_s", 0.)) / period).floor() * period
                    + v(&c, "start_offset_s", 0.);
                for start in [base, base + period] {
                    if !active(&c, start) {
                        continue;
                    }
                    let arrival = start + step;
                    // Conservative unknown one-way propagation of 0..50 ms
                    // for both sounders; occupancy is not code correlation.
                    let seconds = overlap(
                        t,
                        t + dwell(&target) + 0.05,
                        arrival,
                        arrival + dwell(&c) + 0.05,
                    );
                    if seconds > 0. {
                        rows.push([index as f64, f, t, rf, arrival, seconds, rf - f]);
                        n += 1;
                    }
                }
            }
        }
        println!("{label}: {n} possible cochannel step overlaps");
    }
    let h = File::create_excl(Path::new(&a[4]))?;
    let flat: Vec<_> = rows.iter().flatten().copied().collect();
    h.new_dataset::<f64>()
        .shape([rows.len(), 7])
        .create("source_index_target_hz_target_utc_source_hz_source_utc_overlap_s_delta_hz")?
        .write_raw(&flat)?;
    let bytes = serde_json::to_vec(&labels)?;
    h.new_dataset_builder()
        .with_data(&bytes)
        .create("source_labels_json_utf8")?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn endpoints_and_midnight() {
        assert_eq!(overlap(0., 1., 1., 2.), 0.);
        assert_eq!(overlap(0., 2., 1., 3.), 1.);
        let c = serde_json::json!({"sounding_hours":[21.,5.5]});
        assert!(active(&c, 23. * 3600.));
        assert!(active(&c, 3. * 3600.));
        assert!(!active(&c, 12. * 3600.));
    }
}

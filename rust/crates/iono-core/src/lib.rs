//! HDF5 product semantics shared by the receiver and indexed archive service.
use anyhow::{bail, ensure, Context, Result};
use hdf5::File;
use serde::{Deserialize, Serialize};
use std::path::Path;
#[cfg(feature = "dsp")]
pub mod digisonde_match;
#[cfg(feature = "dsp")]
pub mod digisonde_search;
#[cfg(feature = "dsp")]
pub mod prn;
#[cfg(feature = "dsp")]
pub mod prn_matrix;
#[cfg(feature = "dsp")]
pub mod pulsed;
#[cfg(feature = "dsp")]
pub mod receiver_timing;
#[cfg(feature = "dsp")]
pub mod simd;
pub const C: f64 = 299_792_458.;
pub mod lfm_geolocation;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Grid {
    pub id: String,
    pub station: String,
    pub transmitter: String,
    pub kind: String,
    pub mode: String,
    pub timestamp: f64,
    pub nx: usize,
    pub ny: usize,
    pub x: Vec<f64>,
    pub y: Vec<f64>,
    pub xlabel: String,
    pub ylabel: String,
    pub color_label: String,
    pub low: f32,
    pub high: f32,
    #[serde(skip)]
    pub values: Vec<f32>,
}
pub fn scalar(h: &File, key: &str, default: f64) -> f64 {
    h.dataset(key)
        .and_then(|d| d.read_scalar::<f64>())
        .unwrap_or(default)
}
pub fn text(h: &File, key: &str, default: &str) -> String {
    let Ok(d) = h.dataset(key) else {
        return default.into();
    };
    if let Ok(s) = d.read_scalar::<hdf5::types::VarLenUnicode>() {
        return s.as_str().into();
    }
    if let Ok(s) = d.read_scalar::<hdf5::types::VarLenAscii>() {
        return s.as_str().into();
    }
    if let Ok(s) = d.read_scalar::<hdf5::types::FixedAscii<256>>() {
        return s.as_str().into();
    }
    default.into()
}
pub fn median(values: &[f32]) -> f32 {
    let mut v: Vec<_> = values.iter().copied().filter(|v| v.is_finite()).collect();
    if v.is_empty() {
        return f32::NAN;
    }
    v.sort_unstable_by(f32::total_cmp);
    let n = v.len();
    if n % 2 == 0 {
        (v[n / 2 - 1] + v[n / 2]) * 0.5
    } else {
        v[n / 2]
    }
}
/// Finite-data Q25/Q75 limits; gaps never enter the color statistics.
pub fn iqr_limits(values: &[f32]) -> (f32, f32) {
    let mut v: Vec<_> = values.iter().copied().filter(|x| x.is_finite()).collect();
    if v.is_empty() {
        return (-1., 1.);
    }
    v.sort_unstable_by(f32::total_cmp);
    let q = |p: f64| {
        let k = p * (v.len() - 1) as f64;
        let i = k.floor() as usize;
        v[i] + (v[k.ceil() as usize] - v[i]) * (k - i as f64) as f32
    };
    let (a, b) = (q(0.25), q(0.75));
    if b - a < 0.001 {
        (a - 0.5, b + 0.5)
    } else {
        (a, b)
    }
}
pub fn db(v: f32) -> f32 {
    if v.is_finite() && v > 0. {
        10. * v.log10()
    } else {
        f32::NAN
    }
}
/// Preserve every accepted finite SNR cell, including both Digisonde modes.
pub fn write_sparse_snr(h: &hdf5::File, shape: &[usize], values: &[f32]) -> Result<()> {
    ensure!(matches!(shape.len(), 2 | 3), "invalid sparse SNR rank");
    let n = shape
        .iter()
        .try_fold(1usize, |n, &v| n.checked_mul(v))
        .context("sparse shape overflow")?;
    ensure!(
        n == values.len() && n <= 8_000_000,
        "invalid sparse SNR size"
    );
    let mut ids = vec![];
    let mut kept = vec![];
    for (i, &v) in values.iter().enumerate() {
        if v.is_finite() && v > 0. {
            ids.push(i as u32);
            kept.push(v);
        }
    }
    h.new_dataset_builder()
        .with_data(&shape.iter().map(|&v| v as u64).collect::<Vec<_>>())
        .create("SNR_sparse_shape")?;
    if ids.is_empty() {
        h.new_dataset_builder()
            .with_data(&ids)
            .create("SNR_sparse_indices")?;
        h.new_dataset_builder()
            .with_data(&kept)
            .create("SNR_sparse_values")?;
    } else {
        h.new_dataset::<u32>()
            .shape(ids.len())
            .chunk(ids.len().min(4096))
            .deflate(3)
            .create("SNR_sparse_indices")?
            .write_raw(&ids)?;
        h.new_dataset::<f32>()
            .shape(kept.len())
            .chunk(kept.len().min(4096))
            .deflate(3)
            .create("SNR_sparse_values")?
            .write_raw(&kept)?;
    }
    Ok(())
}
pub fn read_products(path: &Path) -> Result<Vec<Grid>> {
    let h = File::open(path).context("open product")?;
    if h.link_exists("data") {
        return read_detections(&h, path);
    }
    if h.link_exists("power") && h.link_exists("time_unix") {
        return read_spectrum(&h);
    }
    if !h.link_exists("freqs") || !h.link_exists("ranges") {
        bail!("not a frequency/range product")
    }
    let freqs = h.dataset("freqs")?.read_raw::<f64>()?;
    let ranges = h.dataset("ranges")?.read_raw::<f64>()?;
    ensure!(freqs.len() > 1 && ranges.len() > 1, "empty axes");
    let sparse = h.link_exists("SNR_sparse_values");
    let (shape, mut values) = if sparse {
        let shape = if h.link_exists("SNR_sparse_shape") {
            h.dataset("SNR_sparse_shape")?
                .read_raw::<u64>()?
                .into_iter()
                .map(|v| usize::try_from(v).context("sparse dimension overflow"))
                .collect::<Result<Vec<_>>>()?
        } else {
            vec![freqs.len(), ranges.len()]
        };
        ensure!(
            shape == [freqs.len(), ranges.len()] || shape == [2, freqs.len(), ranges.len()],
            "invalid sparse SNR axes"
        );
        let n = shape
            .iter()
            .try_fold(1usize, |n, &v| n.checked_mul(v))
            .context("sparse shape overflow")?;
        ensure!(n <= 8_000_000, "sparse raster exceeds bounded import size");
        let ids = h.dataset("SNR_sparse_indices")?;
        let power = h.dataset("SNR_sparse_values")?;
        ensure!(
            ids.size() == power.size() && ids.size() <= n,
            "invalid sparse entries"
        );
        let ids = ids.read_raw::<u32>()?;
        let power = power.read_raw::<f32>()?;
        ensure!(
            ids.windows(2).all(|p| p[0] < p[1]),
            "unordered or repeated sparse indices"
        );
        let mut values = vec![f32::NAN; n];
        for (&i, &v) in ids.iter().zip(&power) {
            ensure!(
                (i as usize) < n && v.is_finite() && v > 0.,
                "invalid sparse echo"
            );
            values[i as usize] = v;
        }
        (shape, values)
    } else {
        let dataset = if h.link_exists("SNR") {
            h.dataset("SNR")?
        } else {
            h.dataset("S")?
        };
        ensure!(
            dataset.size() <= 8_000_000,
            "product exceeds bounded import size"
        );
        (dataset.shape(), dataset.read_raw::<f32>()?)
    };
    let digi = shape.len() == 3;
    let (modes, nf, nr) = if digi {
        (shape[0], shape[1], shape[2])
    } else {
        ensure!(shape.len() == 2, "invalid raster rank");
        (1, shape[0], shape[1])
    };
    ensure!(
        nf == freqs.len() && nr == ranges.len() && modes <= 2,
        "axis/data shape mismatch"
    );
    let decoder = text(&h, "decoder", "");
    let prn = decoder.starts_with("complex64-fft-matched")
        || decoder.starts_with("complex64-fft-inverse-normal-matrix-")
        || decoder.starts_with("complex64-fft-factor-normal-matrix-")
        || decoder.starts_with("complex64-finite-regularized-inverse-verso-");
    if prn && !h.link_exists("code_identity_margin_db") {
        // Early commissioning products lack the seed-discrimination control.
        // Retain their scientific arrays but exclude them from interpretations.
        values.fill(f32::NAN);
    } else if prn {
        let margins = h.dataset("code_identity_margin_db")?.read_raw::<f32>()?;
        ensure!(margins.len() == nf, "code identity axis mismatch");
        let threshold = scalar(&h, "code_identity_required_db", 3.) as f32;
        // Raw SNR remains in HDF5. Display and band-limit reduction require a
        // positive identification against other known codes on the same data.
        for (f, &margin) in margins.iter().enumerate() {
            if !margin.is_finite() || margin < threshold {
                values[f * nr..(f + 1) * nr].fill(f32::NAN);
            }
        }
    }
    if !h.link_exists("SNR") && !sparse {
        for row in values.chunks_mut(nr) {
            let noise = median(row);
            for v in row {
                *v = if noise > 0. {
                    (*v - noise) / noise
                } else {
                    f32::NAN
                }
            }
        }
    }
    let t = scalar(&h, "t0", 0.);
    let station = text(
        &h,
        if digi { "receiver" } else { "station_name" },
        "unknown",
    );
    let tx = text(&h, if digi { "transmitter" } else { "txname" }, "unknown");
    let offset = if digi {
        scalar(&h, "offset_us", 0.) * 1e-6 * C
    } else if scalar(&h, "range_offset_applied", 0.) != 0. {
        0.
    } else {
        t.fract() * C
    };
    // Historical files keep their original timing. New products carry the
    // calibrated reference in transmit_delay_us; no silent retrospective shift.
    let x = freqs.iter().map(|f| f / 1e6).collect::<Vec<_>>();
    let y = ranges
        .iter()
        .map(|r| (r + offset) / 1000.)
        .collect::<Vec<_>>();
    let mut grids = Vec::new();
    for mode in 0..modes {
        let mut raster = vec![f32::NAN; nf * nr];
        for f in 0..nf {
            for r in 0..nr {
                raster[r * nf + f] = db(values[(mode * nf + f) * nr + r]);
            }
        }
        grids.push(Grid {
            id: String::new(),
            station: station.clone(),
            transmitter: tx.clone(),
            kind: if prn {
                "prn"
            } else if digi {
                "digisonde"
            } else {
                "lfm"
            }
            .into(),
            mode: if digi {
                if mode == 0 {
                    "O"
                } else {
                    "X"
                }
            } else {
                ""
            }
            .into(),
            timestamp: t,
            nx: nf,
            ny: nr,
            x: x.clone(),
            y: y.clone(),
            xlabel: "Frequency (MHz)".into(),
            ylabel: "One-way range (km)".into(),
            color_label: if prn {
                "Correlation SNR (dB)"
            } else {
                "SNR (dB)"
            }
            .into(),
            low: 0.,
            high: 20.,
            values: raster,
        });
    }
    Ok(grids)
}
fn read_spectrum(h: &File) -> Result<Vec<Grid>> {
    let ds = h.dataset("power")?;
    let shape = ds.shape();
    ensure!(
        shape.len() == 2 && ds.size() <= 8_000_000,
        "invalid spectrum"
    );
    let (nt, nf) = (shape[0], shape[1]);
    let p = ds.read_raw::<f32>()?;
    let mut v = vec![f32::NAN; nt * nf];
    for f in 0..nf {
        let mut sum = 0f64;
        let mut n = 0;
        for t in 0..nt {
            let z = p[t * nf + f];
            if z.is_finite() && z > 0. {
                sum += z as f64;
                n += 1
            }
        }
        let avg = (sum / n.max(1) as f64) as f32;
        for t in 0..nt {
            v[f * nt + t] = db(p[t * nf + f] / avg)
        }
    }
    let times = h.dataset("time_unix")?.read_raw::<f64>()?;
    let freq = h.dataset("frequency_hz")?.read_raw::<f64>()?;
    ensure!(
        nt > 1 && nf > 1 && times.len() == nt && freq.len() == nf,
        "spectrum axis/data shape mismatch"
    );
    let station = h
        .attr("station")
        .ok()
        .and_then(|a| a.read_scalar::<hdf5::types::VarLenUnicode>().ok())
        .map(|s| s.as_str().to_string())
        .unwrap_or_else(|| "TGO".into());
    let (low, high) = iqr_limits(&v);
    Ok(vec![Grid {
        id: String::new(),
        station,
        transmitter: String::new(),
        kind: "spectrum".into(),
        mode: "".into(),
        timestamp: scalar(h, "minute_start_unix", times[0]),
        nx: nt,
        ny: nf,
        x: times,
        y: freq.iter().map(|f| f / 1e6).collect(),
        xlabel: "Time (UTC)".into(),
        ylabel: "Frequency (MHz)".into(),
        color_label: "Relative power (dB)".into(),
        low,
        high,
        values: v,
    }])
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_stays_missing() {
        assert!(db(f32::NAN).is_nan());
        assert!(db(-1.).is_nan());
        assert!((db(100.) - 20.).abs() < 1e-6)
    }
    #[test]
    fn finite_interquartile_limits() {
        assert_eq!(iqr_limits(&[0., 1., 2., 3., 4., f32::NAN]), (1., 3.));
        let (a, b) = iqr_limits(&[7., 7.]);
        assert!(a < 7. && b > 7.);
    }
    #[test]
    fn median_ignores_gaps() {
        assert_eq!(median(&[1., f32::NAN, 3., 2.]), 2.)
    }
}

#[cfg(feature = "dsp")]
pub mod dsp;

fn read_detections(h: &File, path: &Path) -> Result<Vec<Grid>> {
    let d = h.dataset("data")?;
    let shape = d.shape();
    ensure!(
        shape.len() == 2 && shape[1] == 5 && d.size() < 1_000_000,
        "invalid detection list"
    );
    let a = d.read_raw::<f64>()?;
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let fields: Vec<_> = stem.split('-').collect();
    let station = fields.get(1).unwrap_or(&"unknown").to_string();
    let start = fields
        .last()
        .and_then(|x| x.parse::<f64>().ok())
        .unwrap_or(0.);
    let nt = 180;
    let nr = 1000;
    let mut result = Vec::new();
    for rate in [100000., 125000.] {
        let mut raster = vec![f32::NAN; nt * nr];
        for row in a.chunks_exact(5) {
            if (row[3] - rate).abs() > 1. {
                continue;
            }
            let t = ((row[1] - start) / 5.).floor() as isize;
            let r = (row[0].rem_euclid(1.) * 1000.).floor() as usize;
            if t >= 0 && (t as usize) < nt && r < nr {
                let v = db(row[4] as f32);
                let index = r * nt + t as usize;
                if !raster[index].is_finite() || v > raster[index] {
                    raster[index] = v
                }
            }
        }
        result.push(Grid {
            id: String::new(),
            station: station.clone(),
            transmitter: format!("{:.0} kHz/s", rate / 1000.),
            kind: "detections".into(),
            mode: String::new(),
            timestamp: start,
            nx: nt,
            ny: nr,
            x: (0..nt).map(|i| start + (i as f64 + 0.5) * 5.).collect(),
            y: (0..nr).map(|i| i as f64 + 0.5).collect(),
            xlabel: "Detection time (UTC)".into(),
            ylabel: "Sweep epoch modulo 1 s (ms)".into(),
            color_label: "Detection SNR (dB)".into(),
            low: 10.,
            high: 25.,
            values: raster,
        });
    }
    Ok(result)
}

#[cfg(feature = "dsp")]
pub mod chirp;

#[cfg(feature = "dsp")]
pub mod lfm;

#[cfg(test)]
mod sparse_tests {
    use super::*;
    #[test]
    fn sparse_digisonde_preserves_both_modes_and_every_accepted_echo() {
        let values = [4., f32::NAN, 9., f32::NAN, 16., f32::NAN, 25., f32::NAN];
        let base = std::env::temp_dir().join(format!("iono-modes-{}", std::process::id()));
        let paths = [
            base.with_extension("dense.h5"),
            base.with_extension("sparse.h5"),
        ];
        for (i, path) in paths.iter().enumerate() {
            let h = File::create(path).unwrap();
            h.new_dataset_builder()
                .with_data(&[2e6, 3e6])
                .create("freqs")
                .unwrap();
            h.new_dataset_builder()
                .with_data(&[300e3, 600e3])
                .create("ranges")
                .unwrap();
            if i == 0 {
                h.new_dataset::<f32>()
                    .shape([2, 2, 2])
                    .create("SNR")
                    .unwrap()
                    .write_raw(&values)
                    .unwrap();
            } else {
                write_sparse_snr(&h, &[2, 2, 2], &values).unwrap();
            }
        }
        let a = read_products(&paths[0]).unwrap();
        let b = read_products(&paths[1]).unwrap();
        assert_eq!(a.len(), 2);
        assert_eq!(b.len(), 2);
        for (a, b) in a.iter().zip(&b) {
            assert_eq!(a.mode, b.mode);
            assert_eq!(a.x, b.x);
            assert_eq!(a.y, b.y);
            for (x, y) in a.values.iter().zip(&b.values) {
                assert!(x == y || (x.is_nan() && y.is_nan()));
            }
        }
        for p in paths {
            std::fs::remove_file(p).unwrap();
        }
    }
    #[test]
    fn prn_cross_code_rejection_changes_display_but_preserves_source() {
        for tag in [
            "complex64-fft-matched-filter-doppler-v1",
            "complex64-fft-inverse-normal-matrix-least-squares-doppler-v1",
            "complex64-fft-factor-normal-matrix-least-squares-doppler-v1",
            "complex64-finite-regularized-inverse-verso-v3",
        ] {
            let path = std::env::temp_dir().join(format!("iono-prn-{}.h5", std::process::id()));
            {
                let h = File::create(&path).unwrap();
                h.new_dataset_builder()
                    .with_data(&[2e6, 3e6])
                    .create("freqs")
                    .unwrap();
                h.new_dataset_builder()
                    .with_data(&[300e3, 600e3])
                    .create("ranges")
                    .unwrap();
                h.new_dataset::<f32>()
                    .shape((2, 2))
                    .create("SNR")
                    .unwrap()
                    .write_raw(&[100f32; 4])
                    .unwrap();
                let decoder: hdf5::types::VarLenUnicode = tag.parse().unwrap();
                h.new_dataset::<hdf5::types::VarLenUnicode>()
                    .create("decoder")
                    .unwrap()
                    .write_scalar(&decoder)
                    .unwrap();
                h.new_dataset_builder()
                    .with_data(&[-5f32, 8.])
                    .create("code_identity_margin_db")
                    .unwrap();
            }
            let grids = read_products(&path).unwrap();
            let g = &grids[0];
            assert_eq!(g.kind, "prn");
            assert!(g.values[0].is_nan() && g.values[2].is_nan());
            assert_eq!(g.values[1], 20.);
            assert_eq!(g.values[3], 20.);
            assert_eq!(
                File::open(&path)
                    .unwrap()
                    .dataset("SNR")
                    .unwrap()
                    .read_raw::<f32>()
                    .unwrap(),
                vec![100.; 4]
            );
            fs_remove(&path);
        }
    }
    fn fs_remove(path: &Path) {
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn sparse_hdf5_keeps_echo_geometry_and_never_fills_unmeasured_cells() {
        let path = std::env::temp_dir().join(format!("iono-sparse-{}.h5", std::process::id()));
        let h = File::create(&path).unwrap();
        h.new_dataset_builder()
            .with_data(&[1e6, 2e6])
            .create("freqs")
            .unwrap();
        h.new_dataset_builder()
            .with_data(&[5e6, 6e6, 7e6])
            .create("ranges")
            .unwrap();
        h.new_dataset_builder()
            .with_data(&[1u32, 5])
            .create("SNR_sparse_indices")
            .unwrap();
        h.new_dataset_builder()
            .with_data(&[100f32, 20.])
            .create("SNR_sparse_values")
            .unwrap();
        h.new_dataset::<f64>()
            .create("range_offset_applied")
            .unwrap()
            .write_scalar(&1.)
            .unwrap();
        h.flush().unwrap();
        drop(h);
        let plots = read_products(&path).unwrap();
        assert_eq!(plots[0].values.iter().filter(|v| v.is_finite()).count(), 2);
        assert_eq!(plots[0].values[2], 20.);
        assert!((plots[0].values[5] - 10. * 20f32.log10()).abs() < 1e-4);
        assert!(plots[0].values[0].is_nan());
        let h = File::open_rw(&path).unwrap();
        h.dataset("SNR_sparse_indices")
            .unwrap()
            .write_raw(&[1u32, 7])
            .unwrap();
        h.flush().unwrap();
        drop(h);
        assert!(read_products(&path).is_err());
        std::fs::remove_file(path).unwrap();
    }
}

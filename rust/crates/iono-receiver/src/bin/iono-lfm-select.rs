//! Compact offline timing export from fixed 15-minute HDF5 detection buckets.
//! Does not enumerate the archive, open voltage, or alter station processing.
use anyhow::{ensure, Context, Result};
use std::path::Path;

fn matches(epoch: f64, period: f64, phase: f64, half_width: f64) -> bool {
    let d = (epoch - phase).rem_euclid(period);
    d.min(period - d) <= half_width
}
fn main() -> Result<()> {
    let a: Vec<_> = std::env::args().collect();
    ensure!(
        a.len() == 8,
        "usage: iono-lfm-select DAY_DIRECTORY STATION DAY_START PERIOD PHASE HALF_WIDTH OUTPUT.h5"
    );
    let station = &a[2];
    ensure!(
        !station.is_empty() && station.bytes().all(|x| x.is_ascii_alphanumeric()),
        "invalid station"
    );
    let start: u64 = a[3].parse()?;
    let period: f64 = a[4].parse()?;
    let phase: f64 = a[5].parse()?;
    let width: f64 = a[6].parse()?;
    ensure!(
        start % 86400 == 0
            && period.is_finite()
            && (1. ..=3600.).contains(&period)
            && phase.is_finite()
            && (0. ..period).contains(&phase)
            && width.is_finite()
            && (0.001..=0.5).contains(&width),
        "invalid bounded selection"
    );
    let mut rows = vec![];
    let mut input_rows = 0u64;
    let mut files = 0u64;
    let mut sources = String::new();
    for epoch in (start..start + 86400).step_by(900) {
        let name = format!("cdetections-{station}-{epoch}.h5");
        let path = Path::new(&a[1]).join(&name);
        if !path.is_file() {
            continue;
        }
        let h = hdf5::File::open(&path).with_context(|| name.clone())?;
        let d = h.dataset("data")?;
        ensure!(
            d.shape().len() == 2 && d.shape()[1] == 5 && d.size() <= 500000,
            "invalid detection table"
        );
        let values = d.read_raw::<f64>()?;
        input_rows += values.len() as u64 / 5;
        files += 1;
        sources.push_str(&name);
        sources.push('\n');
        for row in values.chunks_exact(5) {
            if row.iter().all(|x| x.is_finite())
                && (start as f64..(start + 86400) as f64).contains(&row[0])
                && matches(row[0], period, phase, width)
            {
                ensure!(
                    rows.len() + 5 <= 1000000,
                    "selected timing export exceeds bound"
                );
                rows.extend_from_slice(row);
            }
        }
    }
    ensure!(
        rows.len() <= 1000000,
        "selected timing export exceeds bound"
    );
    let target = Path::new(&a[7]);
    let tmp = target.with_extension("h5.partial");
    {
        let h = hdf5::File::create(&tmp)?;
        let d = if rows.is_empty() {
            h.new_dataset::<f64>().shape([0, 5]).create("data")?
        } else {
            h.new_dataset::<f64>()
                .shape([rows.len() / 5, 5])
                .deflate(3)
                .create("data")?
        };
        d.write_raw(&rows)?;
        for (name, value) in [
            ("input_rows", input_rows as f64),
            ("input_files", files as f64),
            ("period_s", period),
            ("phase_s", phase),
            ("half_width_s", width),
            ("day_start_unix", start as f64),
        ] {
            h.new_dataset::<f64>().create(name)?.write_scalar(&value)?;
        }
        let sources: hdf5::types::VarLenUnicode = sources.parse()?;
        h.new_dataset::<hdf5::types::VarLenUnicode>()
            .create("source_buckets")?
            .write_scalar(&sources)?;
        h.flush()?;
    }
    std::fs::rename(tmp, target)?;
    println!(
        "{station}: {files} buckets, {input_rows} input detections, {} selected",
        rows.len() / 5
    );
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn phase_gate_handles_epoch_and_period_boundary_without_rounding_seconds() {
        assert!(matches(1793923200. + 651.021, 720., 651., 0.15));
        assert!(matches(1793923200. + 1371.021, 720., 651., 0.15));
        assert!(!matches(1793923200. + 652., 720., 651., 0.15));
        assert!(matches(1793923200. + 719.99, 720., 0., 0.02));
    }
}

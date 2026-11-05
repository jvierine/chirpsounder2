//! Receiver-produced numeric display products. No images or server reductions.
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{io::Write, path::Path};
pub const SCHEMA: &str = "receiver-browser-day-v1";
pub const MAX_CELLS: usize = 1440 * 512;
pub const RECEIVERS: &[&str] = &["TGO", "DOB", "DMB", "KHO", "W2NAF", "KREA"];
#[derive(Clone, Serialize, Deserialize)]
pub struct Product {
    #[serde(flatten)]
    pub grid: iono_core::Grid,
    pub quantity: String,
    pub palette: String,
    pub occupied_minutes: usize,
}
#[derive(Clone, Serialize, Deserialize, Default)]
pub struct Evidence {
    pub transmitter: String,
    pub receiver: String,
    pub time: f64,
    pub frequency_mhz: Vec<f64>,
    pub snr_db: Vec<Option<f32>>,
    pub range_gate_km: [f64; 2],
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Bundle {
    pub schema: String,
    pub station: String,
    pub day: String,
    pub generated_unix: f64,
    pub products: Vec<Product>,
    pub evidence: Vec<Evidence>,
    /// Receiver-computed envelopes for provisional, range-gated sweep sources.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assumed_paths: Vec<serde_json::Value>,
    pub health: serde_json::Value,
}
/// Hash is over array bytes and display metadata, so unchanged plots retain URLs.
pub fn identify(p: &mut Product) {
    p.grid.id.clear();
    let mut h = Sha256::new();
    h.update(serde_json::to_vec(p).unwrap());
    for value in &p.grid.values {
        h.update(value.to_le_bytes());
    }
    p.grid.id = format!("{:x}", h.finalize())[..24].into();
}
pub fn validate(bundle: &Bundle) -> Result<()> {
    ensure!(
        bundle.schema == SCHEMA && bundle.products.len() <= 64,
        "unsupported bundle"
    );
    ensure!(
        RECEIVERS.contains(&bundle.station.as_str()),
        "unknown station"
    );
    for p in &bundle.products {
        let g = &p.grid;
        ensure!(
            g.station == bundle.station
                && g.id.len() == 24
                && g.id.bytes().all(|b| b.is_ascii_hexdigit()),
            "invalid product identity"
        );
        ensure!(
            g.nx >= 2 && g.ny >= 2 && g.nx.checked_mul(g.ny).is_some_and(|n| n <= MAX_CELLS),
            "display array too large"
        );
        ensure!(
            g.x.len() == g.nx && g.y.len() == g.ny,
            "axis shape mismatch"
        );
        ensure!(
            g.x.iter().chain(g.y.iter()).all(|v| v.is_finite())
                && g.x.windows(2).all(|v| v[1] > v[0])
                && g.y.windows(2).all(|v| v[1] > v[0]),
            "nonmonotonic axes"
        );
        ensure!(
            g.low.is_finite() && g.high.is_finite() && g.high > g.low,
            "invalid scale"
        );
    }
    ensure!(bundle.evidence.len() <= 20000, "too many frequency records");
    ensure!(
        bundle.assumed_paths.len() <= 20000,
        "too many assumed paths"
    );
    Ok(())
}
pub fn write(path: &Path, bundle: &mut Bundle) -> Result<()> {
    // Display precision is finer than the 256-level color scale and native
    // sampling. Original scientific products retain their full precision.
    for p in &mut bundle.products {
        for v in &mut p.grid.values {
            if v.is_finite() {
                *v = (*v * 100.).round() / 100.;
            }
        }
        identify(p);
    }
    validate(bundle)?;
    let temporary = path.with_extension("partial.h5");
    {
        let h = hdf5::File::create(&temporary)?;
        let evidence = std::mem::take(&mut bundle.evidence);
        let metadata: hdf5::types::VarLenUnicode = serde_json::to_string(bundle)?.parse()?;
        h.new_dataset::<hdf5::types::VarLenUnicode>()
            .create("browser_metadata")?
            .write_scalar(&metadata)?;
        let mut groups = std::collections::BTreeMap::<String, Vec<&Evidence>>::new();
        for e in &evidence {
            groups
                .entry(serde_json::to_string(&(
                    &e.transmitter,
                    &e.receiver,
                    &e.frequency_mhz,
                    e.range_gate_km,
                ))?)
                .or_default()
                .push(e);
        }
        let root = h.create_group("frequency_evidence")?;
        for (i, rows) in groups.values().enumerate() {
            let g = root.create_group(&i.to_string())?;
            let first = rows[0];
            for (name, text) in [
                ("transmitter", &first.transmitter),
                ("receiver", &first.receiver),
            ] {
                let v: hdf5::types::VarLenUnicode = text.parse()?;
                g.new_dataset::<hdf5::types::VarLenUnicode>()
                    .create(name)?
                    .write_scalar(&v)?;
            }
            g.new_dataset_builder()
                .with_data(&first.frequency_mhz)
                .create("frequency_mhz")?;
            g.new_dataset_builder()
                .with_data(&first.range_gate_km)
                .create("range_gate_km")?;
            g.new_dataset_builder()
                .with_data(&rows.iter().map(|e| e.time).collect::<Vec<_>>())
                .create("time_unix")?;
            let values: Vec<f32> = rows
                .iter()
                .flat_map(|e| e.snr_db.iter().map(|v| v.unwrap_or(f32::NAN)))
                .collect();
            ensure!(
                values.len() == rows.len() * first.frequency_mhz.len(),
                "evidence axes mismatch"
            );
            g.new_dataset::<f32>()
                .shape([rows.len(), first.frequency_mhz.len()])
                .chunk((rows.len().min(32), first.frequency_mhz.len().min(64)))
                .deflate(3)
                .create("snr_db")?
                .write_raw(&values)?;
        }
        bundle.evidence = evidence;
        for p in &bundle.products {
            ensure!(
                p.grid.values.len() == p.grid.nx * p.grid.ny,
                "data shape mismatch"
            );
            h.new_dataset::<f32>()
                .shape([p.grid.ny, p.grid.nx])
                .chunk((p.grid.ny.min(32), p.grid.nx.min(180)))
                .deflate(3)
                .create(p.grid.id.as_str())?
                .write_raw(&p.grid.values)?;
            let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
            for value in &p.grid.values {
                gz.write_all(&value.to_le_bytes())?;
            }
            let bytes = gz.finish()?;
            h.new_dataset_builder()
                .with_data(&bytes)
                .create(format!("{}_gzip", p.grid.id).as_str())?;
        }
    }
    std::fs::rename(temporary, path)?;
    Ok(())
}
pub fn read_evidence(h: &hdf5::File) -> Result<Vec<Evidence>> {
    let mut result = Vec::new();
    let root = h.group("frequency_evidence")?;
    for name in root.member_names()? {
        let g = root.group(&name)?;
        let frequency_mhz = g.dataset("frequency_mhz")?.read_raw::<f64>()?;
        let times = g.dataset("time_unix")?.read_raw::<f64>()?;
        let gate = g.dataset("range_gate_km")?.read_raw::<f64>()?;
        let d = g.dataset("snr_db")?;
        ensure!(
            d.shape() == [times.len(), frequency_mhz.len()]
                && d.size() <= 8_000_000
                && gate.len() == 2
                && !frequency_mhz.is_empty(),
            "invalid frequency evidence"
        );
        let values = d.read_raw::<f32>()?;
        let transmitter = g
            .dataset("transmitter")?
            .read_scalar::<hdf5::types::VarLenUnicode>()?
            .to_string();
        let receiver = g
            .dataset("receiver")?
            .read_scalar::<hdf5::types::VarLenUnicode>()?
            .to_string();
        for (i, time) in times.into_iter().enumerate() {
            result.push(Evidence {
                transmitter: transmitter.clone(),
                receiver: receiver.clone(),
                time,
                frequency_mhz: frequency_mhz.clone(),
                snr_db: values[i * frequency_mhz.len()..(i + 1) * frequency_mhz.len()]
                    .iter()
                    .map(|&v| v.is_finite().then_some(v))
                    .collect(),
                range_gate_km: [gate[0], gate[1]],
            });
        }
    }
    Ok(result)
}
/// Experimental contiguous-band echo detector, run by the API server on small
/// receiver-reduced frequency profiles. Endpoints are sampled, not extrapolated.
pub fn limits(e: &Evidence, threshold: f32, support: usize) -> serde_json::Value {
    let support = support.max(2);
    let mut bands = Vec::new();
    let mut run: Vec<usize> = Vec::new();
    let flush = |run: &mut Vec<usize>, bands: &mut Vec<(f64, f64, usize)>| {
        if run.len() >= support {
            bands.push((
                e.frequency_mhz[run[0]],
                e.frequency_mhz[*run.last().unwrap()],
                run.len(),
            ));
        }
        run.clear();
    };
    for i in 0..e.frequency_mhz.len().min(e.snr_db.len()) {
        let accepted = e.frequency_mhz[i].is_finite()
            && e.snr_db[i].is_some_and(|v| v.is_finite() && v >= threshold);
        if !accepted
            || run.last().is_some_and(|&j| {
                e.frequency_mhz[i] <= e.frequency_mhz[j]
                    || e.frequency_mhz[i] - e.frequency_mhz[j] > 0.5
            })
        {
            flush(&mut run, &mut bands);
        }
        if accepted {
            run.push(i);
        }
    }
    flush(&mut run, &mut bands);
    let low = bands.first().map(|b| b.0);
    let high = bands.last().map(|b| b.1);
    serde_json::json!({"transmitter":e.transmitter,"receiver":e.receiver,"time":e.time,"lower_mhz":low,"upper_mhz":high,"bands":bands,"support_bins":bands.iter().map(|b|b.2).sum::<usize>(),"threshold_db":threshold,"range_gate_km":e.range_gate_km,"method":"contiguous-echo-band-v1","experimental":true,"lower_censored":low.is_some()&&low==e.frequency_mhz.first().copied(),"upper_censored":high.is_some()&&high==e.frequency_mhz.last().copied(),"observed_band_mhz":[e.frequency_mhz.first(),e.frequency_mhz.last()]})
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounds_require_adjacent_support_and_do_not_bridge_gaps() {
        let mut e = Evidence {
            frequency_mhz: vec![1., 1.1, 1.2, 2., 2.1, 2.2, 5.],
            snr_db: vec![
                Some(15.),
                Some(16.),
                Some(14.),
                None,
                Some(18.),
                Some(16.),
                Some(20.),
            ],
            ..Default::default()
        };
        let v = limits(&e, 13., 3);
        assert_eq!(v["lower_mhz"], 1.);
        assert_eq!(v["upper_mhz"], 1.2);
        e.snr_db = vec![None; 7];
        assert!(limits(&e, 13., 3)["lower_mhz"].is_null());
    }
}

//! Compare candidate sites against timing evidence without assigning identities.
use anyhow::{ensure, Result};
use iono_core::lfm_geolocation as geo;
use serde_json::{json, Value};
fn main() -> Result<()> {
    let a: Vec<_> = std::env::args().collect();
    ensure!(
        a.len() == 4,
        "usage: iono-lfm-site-check timing.h5 positions.json output.h5"
    );
    let input = hdf5::File::open(&a[1])?;
    let manifest: hdf5::types::VarLenUnicode = input.dataset("manifest")?.read_scalar()?;
    let c: geo::Settings = serde_json::from_str(manifest.as_str())?;
    let positions: Value = serde_json::from_str(&std::fs::read_to_string(&a[2])?)?;
    let sites: Vec<_> = positions
        .as_object()
        .unwrap()
        .iter()
        .filter(|(id, _)| ["ROTHR1", "ROTHR2", "ROTHR3"].contains(&id.as_str()))
        .collect();
    ensure!(sites.len() == 3, "need three ROTHR candidate sites");
    for (_, site) in &sites {
        ensure!(
            site["lat"]
                .as_f64()
                .is_some_and(|x| x.is_finite() && x.abs() <= 90.)
                && site["lon"]
                    .as_f64()
                    .is_some_and(|x| x.is_finite() && x.abs() <= 180.),
            "invalid candidate coordinates"
        );
    }
    let output = hdf5::File::create(&a[3])?;
    let text = |g: &hdf5::Group, name: &str, s: &str| -> Result<()> {
        let v: hdf5::types::VarLenUnicode = s.parse()?;
        g.new_dataset::<hdf5::types::VarLenUnicode>()
            .create(name)?
            .write_scalar(&v)?;
        Ok(())
    };
    text(&output, "manifest", manifest.as_str())?;
    text(&output, "candidate_sites", &serde_json::to_string(&sites)?)?;
    text(&output,"interpretation","Candidate-site timing residuals only, not identity probabilities or calibrated position errors. Same-height minimum-hop spherical proxies; unmodelled receiver-specific propagation modes can bias ranking.")?;
    for name in input
        .member_names()?
        .into_iter()
        .filter(|s| s.starts_with("family_"))
    {
        let g = input.group(&name)?;
        let meta: hdf5::types::VarLenUnicode = g.dataset("metadata")?.read_scalar()?;
        let f: geo::Family = serde_json::from_str(meta.as_str())?;
        let heights = g.dataset("virtual_height_km")?.read_raw::<f64>()?;
        let out = output.create_group(&name)?;
        let mut med = vec![];
        let mut per_event = vec![];
        for delays in std::iter::once(&f.delay_s).chain(f.events.iter().map(|e| &e.delay_s)) {
            ensure!(
                delays.len() == f.receivers.len() && delays.len() >= 3,
                "invalid family dimensions"
            );
            let mut values = vec![];
            for (_, site) in &sites {
                let paths: Vec<_> = f
                    .receivers
                    .iter()
                    .map(|&i| {
                        geo::distance_km(
                            site["lat"].as_f64().unwrap(),
                            site["lon"].as_f64().unwrap(),
                            &c.receivers[i],
                        )
                    })
                    .collect();
                for &height in &heights {
                    let reference = geo::path_km(paths[0], height);
                    let rms = (paths
                        .iter()
                        .skip(1)
                        .zip(delays.iter().skip(1))
                        .map(|(d, t)| {
                            (geo::path_km(*d, height) - reference - t * 299792.458).powi(2)
                        })
                        .sum::<f64>()
                        / (paths.len() - 1) as f64)
                        .sqrt();
                    values.push(rms);
                }
            }
            if med.is_empty() {
                med = values;
            } else {
                per_event.extend(values);
            }
        }
        out.new_dataset::<f64>()
            .shape([sites.len(), heights.len()])
            .create("family_rms_km")?
            .write_raw(&med)?;
        out.new_dataset::<f64>()
            .shape([f.events.len(), sites.len(), heights.len()])
            .create("event_rms_km")?
            .write_raw(&per_event)?;
        out.new_dataset_builder()
            .with_data(&heights)
            .create("virtual_height_km")?;
        text(&out, "metadata", meta.as_str())?;
        println!(
            "{}",
            json!({"family":name,"events":f.events.len(),"site_order":sites.iter().map(|(id,_)|id).collect::<Vec<_>>(),"rms_km":med.chunks(heights.len()).collect::<Vec<_>>()})
        );
    }
    Ok(())
}

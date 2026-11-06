//! Offline bounded network inference, using precomputed detector HDF5 only.
use anyhow::{ensure, Result};
use iono_core::lfm_geolocation as geo;
fn main() -> Result<()> {
    let a: Vec<_> = std::env::args().collect();
    ensure!(
        a.len() == 3,
        "usage: iono-lfm-locate manifest.json result.h5"
    );
    let config_text = std::fs::read_to_string(&a[1])?;
    let c: geo::Settings = serde_json::from_str(&config_text)?;
    let rows = geo::load(&c)?;
    let events = geo::events(&c, &rows);
    let families = geo::families(&events, c.minimum_events);
    let h = hdf5::File::create(&a[2])?;
    let text = |parent: &hdf5::Group, name: &str, s: &str| -> Result<()> {
        let v: hdf5::types::VarLenUnicode = s.parse()?;
        parent
            .new_dataset::<hdf5::types::VarLenUnicode>()
            .create(name)?
            .write_scalar(&v)?;
        Ok(())
    };
    text(&h, "manifest", &config_text)?;
    text(&h,"interpretation","Timing families and propagation-model sensitivity surfaces; no validated transmitter identities or calibrated confidence regions. Geometric and common-height minimum-hop spherical proxies only. Long paths, unequal heights, refraction, slope errors and mode ambiguity are not solved.")?;
    text(
        &h,
        "candidate_events_json",
        &serde_json::to_string(&events)?,
    )?;
    h.new_dataset::<u64>()
        .create("input_detection_count")?
        .write_scalar(&(rows.len() as u64))?;
    for (i, f) in families.iter().enumerate() {
        let g = h.create_group(&format!("family_{i:03}"))?;
        text(&g, "metadata", &serde_json::to_string(f)?)?;
        let s = geo::surface(&c, f);
        g.new_dataset_builder()
            .with_data(&s.latitude)
            .create("latitude_deg")?;
        g.new_dataset_builder()
            .with_data(&s.longitude)
            .create("longitude_deg")?;
        g.new_dataset_builder()
            .with_data(&s.heights)
            .create("virtual_height_km")?;
        g.new_dataset::<f64>()
            .shape([s.heights.len(), s.latitude.len(), s.longitude.len()])
            .deflate(3)
            .create("tdoa_rms_km")?
            .write_raw(&s.rms_km)?;
        g.new_dataset::<f64>()
            .shape([s.best.len(), 3])
            .create("model_minima_rms_lat_lon")?
            .write_raw(&s.best.concat())?;
        if let Some(minima) = &s.conditional_best {
            g.new_dataset::<f64>()
                .shape([minima.len(), 3])
                .create("conditional_integer_epoch_minima_rms_lat_lon")?
                .write_raw(&minima.concat())?;
            text(&g,"conditional_range_assumption","Assumes the selected anchor receiver's apparent range is actual propagation path length from an integer-second transmission epoch. This epoch is unvalidated; unrestricted TDOA surfaces are retained separately.")?;
            println!("conditional integer-epoch minima: {minima:?}");
        }
        println!(
            "family {i}: {} kHz/s, {} events, receivers {:?}, delays ms {:?}, model minima {:?}",
            f.rate / 1000,
            f.events.len(),
            f.receivers,
            f.delay_s.iter().map(|x| x * 1000.).collect::<Vec<_>>(),
            s.best
        );
    }
    println!(
        "{} input detections, {} candidate events, {} supported timing families",
        rows.len(),
        events.len(),
        families.len()
    );
    Ok(())
}

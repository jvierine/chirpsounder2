//! Standalone candidate surface, with source provenance retained in HDF5 only.
use anyhow::{ensure, Result};
use rustplotlib::{
    Axes, ColorMap, Colorbar, Figure, IntendedUse, Label, Layer, Line, Normalization, Panel, Rect,
    Scale,
};
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    ensure!(
        args.len() == 4,
        "usage: iono-lfm-location-plot result.h5 coastlines.json output.png"
    );
    let h = hdf5::File::open(&args[1])?;
    let names: Vec<_> = h
        .member_names()?
        .into_iter()
        .filter(|s| s.starts_with("family_"))
        .collect();
    ensure!(!names.is_empty(), "no supported timing families to plot");
    let coast: Vec<Option<f64>> = serde_json::from_str(&std::fs::read_to_string(&args[2])?)?;
    let mut fig = Figure::new(IntendedUse::Custom {
        width_mm: 250.,
        font_pt: 11.,
    });
    fig.latex.cache = std::path::Path::new(&args[3])
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .join(".latex-cache");
    fig.height_mm = names.len() as f64 * 116. + 10.;
    fig.raster_dpi = 140.;
    let scale = fig.scale(Scale::new(
        [0., 0.5],
        Normalization::Linear,
        ColorMap::turbo(),
    )?);
    for (k, name) in names.iter().enumerate() {
        let g = h.group(name)?;
        let m: hdf5::types::VarLenUnicode = g.dataset("metadata")?.read_scalar()?;
        let metadata: serde_json::Value = serde_json::from_str(m.as_str())?;
        let lat = g.dataset("latitude_deg")?.read_raw::<f64>()?;
        let lon = g.dataset("longitude_deg")?.read_raw::<f64>()?;
        let shape = g.dataset("tdoa_rms_km")?.shape();
        let raw = g.dataset("tdoa_rms_km")?.read_raw::<f64>()?;
        let n = lat.len() * lon.len();
        let values: Vec<_> = (0..n)
            .map(|i| {
                let v = (0..shape[0])
                    .map(|j| raw[j * n + i])
                    .fold(f64::INFINITY, f64::min)
                    / 299.792458;
                if v <= 0.5 {
                    v
                } else {
                    f64::NAN
                }
            })
            .collect();
        let top = 10. + k as f64 * 116.;
        let axes = Axes::new(
            [-180., 180.],
            [-90., 90.],
            "Longitude (deg)",
            "Latitude (deg)",
        );
        let mut p = Panel::new(Rect::new(21., top + 7., 200., 86.), axes);
        let step = lon[1] - lon[0];
        let xe: Vec<_> = (0..=lon.len())
            .map(|i| lon[0] - step / 2. + i as f64 * step)
            .collect();
        let ye: Vec<_> = (0..=lat.len())
            .map(|i| lat[0] - step / 2. + i as f64 * step)
            .collect();
        p.pcolormesh(xe, ye, values, scale)?;
        let mut segment = vec![];
        for xy in coast.chunks_exact(2) {
            if let (Some(x), Some(y)) = (xy[0], xy[1]) {
                segment.push([x, y]);
            } else if !segment.is_empty() {
                let mut l = Line::new(std::mem::take(&mut segment), "#444444");
                l.width_pt = 0.45;
                p.push(Layer::Line(l));
            }
        }
        if !segment.is_empty() {
            p.push(Layer::Line(Line::new(segment, "#444444")));
        }
        fig.panels.push(p);
        fig.colorbars.push(Colorbar::new(
            Rect::new(227., top + 7., 3., 86.),
            scale,
            "Timing residual (ms)",
            vec![0., 0.1, 0.2, 0.3, 0.4, 0.5],
        ));
        fig.labels.push(Label::new(
            format!(
                "{} kHz/s --- {} matched sweeps",
                metadata["rate"].as_u64().unwrap() / 1000,
                metadata["events"].as_array().unwrap().len()
            ),
            121.,
            top,
        ));
    }
    fig.save(std::path::Path::new(&args[3]))?;
    Ok(())
}

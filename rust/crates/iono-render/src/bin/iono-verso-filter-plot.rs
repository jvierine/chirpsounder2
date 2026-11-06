//! Receiver-filtered supplied-code response comparison, rendered natively.
use anyhow::{ensure, Result};
use rustplotlib::{
    Axes, Figure, IntendedUse, Label, Layer, Legend, LegendEntry, Line, Panel, Rect,
};
fn main() -> Result<()> {
    let a: Vec<_> = std::env::args().collect();
    ensure!(
        a.len() == 3,
        "usage: iono-verso-filter-plot response.h5 figure.png"
    );
    let h = hdf5::File::open(&a[1])?;
    let x = h.dataset("range_offset_km")?.read_raw::<f64>()?;
    let mut f = Figure::new(IntendedUse::Custom {
        width_mm: 200.,
        font_pt: 11.,
    });
    f.height_mm = 105.;
    f.raster_dpi = 150.;
    f.latex.cache = std::path::Path::new(&a[2])
        .parent()
        .unwrap()
        .join(".latex-cache");
    let mut p = Panel::new(
        Rect::new(19., 10., 174., 76.),
        Axes::new(
            [-600., 600.],
            [-60., 3.],
            "Range offset (km)",
            "Relative power (dB)",
        ),
    );
    for (key, color) in [("filter_0", "#0072B2"), ("filter_3", "#D55E00")] {
        let y = h
            .group(key)?
            .dataset("relative_amplitude_db")?
            .read_raw::<f32>()?;
        p.push(Layer::Line(Line::new(
            x.iter().zip(y).map(|(&x, y)| [x, y as f64]).collect(),
            color,
        )));
    }
    p.legend = Some(Legend::new(vec![
        LegendEntry::line("Matched filter", "#0072B2"),
        LegendEntry::line("Regularized inverse", "#D55E00"),
    ]));
    f.panels.push(p);
    f.labels
        .push(Label::new("VERSO pulse compression", 106., 3.));
    f.save(std::path::Path::new(&a[2]))?;
    Ok(())
}

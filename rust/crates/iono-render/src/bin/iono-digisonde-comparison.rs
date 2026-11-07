//! Same-voltage CC comparison at the internal memo's final inclusion size.
use anyhow::{ensure, Result};
use rustplotlib::{ArticleLayout, Axes, ColorMap, Colorbar, Normalization, Panel, Rect, Scale};
use std::path::Path;
fn edges(a: &[f64]) -> Vec<f64> {
    let d = (a[a.len() - 1] - a[0]) / (a.len() - 1) as f64;
    (0..=a.len()).map(|i| a[0] + (i as f64 - 0.5) * d).collect()
}
fn main() -> Result<()> {
    let a: Vec<_> = std::env::args().collect();
    ensure!(
        a.len() == 4,
        "usage: iono-digisonde-comparison BEFORE.h5 AFTER.h5 OUTPUT.pdf"
    );
    let mut f = ArticleLayout::new(179.324, 10.95)?.figure(1., 146.)?;
    f.raster_dpi = 160.;
    f.latex.cache = Path::new(&a[3]).parent().unwrap().join(".latex-cache");
    let scale = f.scale(Scale::new(
        [0., 20.],
        Normalization::Linear,
        ColorMap::new(vec![[1., 1., 1.], [0., 0., 0.]])?,
    )?);
    for (i, label) in ["Old alignment", "Aligned A/B"].iter().enumerate() {
        let g = iono_core::read_products(Path::new(&a[i + 1]))?
            .into_iter()
            .find(|g| g.mode == "O")
            .unwrap();
        let mut ax = Axes::new(
            [2., 20.],
            [3000., 6000.],
            if i == 1 { "Frequency (MHz)" } else { "" },
            "Apparent range (km)",
        );
        ax.xticks = vec![4., 8., 12., 16., 20.];
        ax.yticks = vec![3000., 4000., 5000., 6000.];
        if i == 0 {
            ax.show_x_axis = false;
        }
        let mut p = Panel::new(Rect::new(23., 4. + i as f64 * 66., 130., 53.), ax);
        p.pcolormesh(
            edges(&g.x),
            edges(&g.y),
            g.values.iter().map(|v| *v as f64).collect(),
            scale,
        )?;
        p.label_at(*label, [6., 3210.]);
        f.panels.push(p);
    }
    f.colorbars.push(Colorbar::new(
        Rect::new(157., 4., 2.5, 119.),
        scale,
        "SNR (dB)",
        vec![0., 5., 10., 15., 20.],
    ));
    let name = Path::new(&a[3]).file_name().unwrap().to_string_lossy();
    f.save_for_article(&a[3], &format!("figures/{name}"))?;
    Ok(())
}

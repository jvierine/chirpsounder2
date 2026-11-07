//! Compare code phase on the same cached voltage; range is relative, uncalibrated.
use anyhow::{ensure, Context, Result};
use hdf5::File;
use rustplotlib::{
    Axes, Figure, IntendedUse, Layer, Legend, LegendEntry, LegendPosition, Line, Panel, Rect,
};
fn main() -> Result<()> {
    let a: Vec<_> = std::env::args().collect();
    ensure!(
        a.len() == 3,
        "usage: iono-digisonde-phase-plot AUDIT.h5 OUTPUT.png"
    );
    let h = File::open(&a[1])?;
    let d = h.dataset("models_block_order_phase_groups_snr_pslr_delay_us_power")?;
    let pars = d.read_raw::<f64>()?;
    let power = h.dataset("compressed_power")?.read_raw::<f32>()?;
    let model = |phase: f64| {
        pars.chunks_exact(8)
            .position(|r| r[0] == 4. && r[1] == 0. && r[2] == phase)
            .context("model missing")
    };
    let right = model(1.)?;
    let wrong = model(0.)?;
    let center = pars[right * 8 + 6];
    let peak = power[right * 1000..(right + 1) * 1000]
        .iter()
        .copied()
        .fold(0f32, f32::max);
    let mut fig = Figure::new(IntendedUse::Custom {
        width_mm: 200.,
        font_pt: 13.,
    });
    fig.height_mm = 105.;
    fig.raster_dpi = 150.;
    fig.latex.cache = std::path::Path::new(&a[2])
        .parent()
        .unwrap()
        .join(".latex-cache");
    let mut ax = Axes::new(
        [-90., 90.],
        [-35., 3.],
        "Range offset (km)",
        "Relative power (dB)",
    );
    ax.xticks = vec![-80., -40., 0., 40., 80.];
    ax.yticks = vec![-30., -20., -10., 0.];
    let mut panel = Panel::new(Rect::new(23., 6., 169., 77.), ax);
    for (m, color) in [(wrong, "#b34a35"), (right, "#16718b")] {
        let points: Vec<_> = (0..1000)
            .map(|i| {
                [
                    (i as f64 * 10. - center) * 0.299792458,
                    10. * ((power[m * 1000 + i] / peak).max(1e-8) as f64).log10(),
                ]
            })
            .collect();
        panel.push(Layer::Line(Line::new(points, color)));
    }
    let mut legend = Legend::new(vec![
        LegendEntry::line("Wrong A/B phase", "#b34a35"),
        LegendEntry::line("Aligned A/B phase", "#16718b"),
    ]);
    legend.position = LegendPosition::LowerRight;
    panel.legend = Some(legend);
    fig.panels.push(panel);
    fig.save(&a[2])?;
    Ok(())
}

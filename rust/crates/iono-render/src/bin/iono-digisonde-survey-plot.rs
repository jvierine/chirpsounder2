//! Bounded overview of receiver-local pulse candidates and sampled code checks.
use anyhow::{ensure, Context, Result};
use hdf5::File;
use rustplotlib::{
    Axes, ColorMap, Colorbar, Figure, IntendedUse, Label, Normalization, Panel, Rect, Scale,
};
fn main() -> Result<()> {
    let a: Vec<_> = std::env::args().collect();
    ensure!(
        a.len() == 3,
        "usage: iono-digisonde-survey-plot survey.h5 figure.png"
    );
    let h = File::open(&a[1])?;
    let t = h.dataset("time_unix")?.read_raw::<f64>()?;
    let fr = h.dataset("frequency_hz")?.read_raw::<f64>()?;
    let contrast = h.dataset("fold_contrast")?.read_raw::<f32>()?;
    ensure!(
        t.len() == fr.len() && fr.len() == contrast.len(),
        "candidate axes disagree"
    );
    let band = h
        .dataset("searched_frequency_range_hz")?
        .read_raw::<f64>()?;
    ensure!(band.len() == 2, "RF band missing");
    let start = h.attr("first_planned_sample_index")?.read_scalar::<u64>()? as f64 / 25e6;
    let stop = h.attr("last_scanned_sample_index")?.read_scalar::<u64>()? as f64 / 25e6 + 0.16;
    ensure!(stop > start, "empty survey");
    let duration = stop - start;
    let dt = (duration / 1200.).max(0.64);
    let nx = (duration / dt).ceil() as usize;
    let ny = ((band[1] - band[0]) / 25000.).ceil() as usize;
    let mut z = vec![f64::NAN; nx * ny];
    for ((&time, &freq), &value) in t.iter().zip(&fr).zip(&contrast) {
        if !time.is_finite()
            || !freq.is_finite()
            || !value.is_finite()
            || time < start
            || freq < band[0]
            || freq > band[1]
        {
            continue;
        }
        let x = ((time - start) / dt).floor() as usize;
        let y = ((freq - band[0]) / 25000.).floor() as usize;
        if x < nx && y < ny {
            let cell = &mut z[y * nx + x];
            if !cell.is_finite() || value as f64 > *cell {
                *cell = value as f64;
            }
        }
    }
    let xe: Vec<_> = (0..=nx).map(|i| start + i as f64 * dt).collect();
    let ye: Vec<_> = (0..=ny)
        .map(|i| (band[0] + i as f64 * 25000.) / 1e6)
        .collect();
    let mut fig = Figure::new(IntendedUse::Custom {
        width_mm: 240.,
        font_pt: 11.,
    });
    fig.height_mm = 170.;
    fig.raster_dpi = 130.;
    fig.latex.cache = std::path::Path::new(&a[2])
        .parent()
        .unwrap()
        .join(".latex-cache");
    let cs = Scale::new([1.8, 8.], Normalization::Linear, ColorMap::plasma())?;
    let ms = Scale::new([0., 0.1], Normalization::Linear, ColorMap::turbo())?;
    let cid = fig.scale(cs.clone());
    let mid = fig.scale(ms.clone());
    let tick_step = if duration < 60. {
        5.
    } else if duration < 120. {
        20.
    } else if duration < 600. {
        60.
    } else {
        300.
    };
    let ticks: Vec<_> = (0..=(duration / tick_step).ceil() as usize)
        .map(|i| (start / tick_step).ceil() * tick_step + i as f64 * tick_step)
        .filter(|v| *v <= stop)
        .collect();
    let labels: Vec<_> = ticks
        .iter()
        .map(|&x| {
            chrono::DateTime::from_timestamp(x as i64, 0)
                .unwrap()
                .format(if duration < 120. { "%H:%M:%S" } else { "%H:%M" })
                .to_string()
        })
        .collect();
    for row in 0..2 {
        let mut axes = Axes::new(
            [start, stop],
            [ye[0], *ye.last().unwrap()],
            if row == 1 { "Time (UTC)" } else { "" },
            "Frequency (MHz)",
        );
        axes.xticks = ticks.clone();
        axes.xticklabels = Some(labels.clone());
        let mut panel = Panel::new(Rect::new(22., 12. + row as f64 * 72., 187., 60.), axes);
        if row == 0 {
            panel.pcolormesh(xe.clone(), ye.clone(), z.clone(), cid)?;
        } else {
            let ix = h.dataset("code_checks/sample_index")?.read_raw::<u64>()?;
            let frequency = h.dataset("code_checks/frequency_hz")?.read_raw::<f64>()?;
            let coherence = h.dataset("code_checks/coherence")?.read_raw::<f32>()?;
            ensure!(
                ix.len() == frequency.len() && ix.len() == coherence.len(),
                "code-check axes disagree"
            );
            let points: Vec<_> = ix
                .iter()
                .zip(frequency)
                .map(|(&i, f)| [i as f64 / 25e6, f / 1e6])
                .collect();
            panel.scatter_colored(
                &points,
                &coherence.iter().map(|&v| v as f64).collect::<Vec<_>>(),
                &ms,
                3.,
                0.9,
                "none",
            )?;
        }
        fig.panels.push(panel);
        fig.colorbars.push(Colorbar::new(
            Rect::new(213., 12. + row as f64 * 72., 3., 60.),
            if row == 0 { cid } else { mid },
            if row == 0 {
                "Pulse contrast"
            } else {
                "Code coherence"
            },
            if row == 0 {
                vec![2., 4., 6., 8.]
            } else {
                vec![0., 0.025, 0.05, 0.075, 0.1]
            },
        ));
    }
    let receiver = h
        .attr("receiver")?
        .read_scalar::<hdf5::types::VarLenUnicode>()?;
    let date = chrono::DateTime::from_timestamp(start as i64, 0)
        .context("UTC timestamp")?
        .format("%d %b %Y");
    fig.labels.push(Label::new(
        format!("{receiver} pulse survey --- {date}"),
        118.,
        5.,
    ));
    fig.save(&a[2])?;
    Ok(())
}

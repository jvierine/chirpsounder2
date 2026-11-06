//! Plot compact receiver-local code-check summaries; never reads cached voltage.
use anyhow::{ensure, Context, Result};
use hdf5::File;
use rustplotlib::{
    Axes, ColorMap, Colorbar, Figure, IntendedUse, Normalization, Panel, Rect, Scale,
};
fn main() -> Result<()> {
    let a: Vec<_> = std::env::args().collect();
    ensure!(
        a.len() == 3,
        "usage: iono-digisonde-timing-plot SUMMARY_DIRECTORY OUTPUT.png"
    );
    let mut files: Vec<_> = std::fs::read_dir(&a[1])?
        .map(|e| e.map(|e| e.path()))
        .collect::<std::io::Result<_>>()?;
    files.retain(|p| {
        p.extension().is_some_and(|s| s == "h5") && !p.to_string_lossy().ends_with("partial.h5")
    });
    files.sort();
    ensure!(
        !files.is_empty() && files.len() <= 256,
        "invalid summary file count"
    );
    let (mut start, mut stop) = (f64::INFINITY, f64::NEG_INFINITY);
    let mut records = vec![];
    let mut carriers = vec![];
    let mut intervals = vec![];
    for file in files {
        let h = File::open(file)?;
        let all = h.dataset("iq/sample_index")?.read_raw::<u64>()?;
        carriers.extend(h.dataset("iq/carrier_hz")?.read_raw::<f64>()?);
        intervals.extend(h.dataset("searched_ipp_candidates_us")?.read_raw::<u32>()?);
        for s in all {
            start = start.min(s as f64 / 25e6);
            stop = stop.max(s as f64 / 25e6 + 0.16);
        }
        let t = h.dataset("time_unix")?.read_raw::<f64>()?;
        let f = h.dataset("frequency_hz")?.read_raw::<f64>()?;
        let c = h.dataset("coherence")?.read_raw::<f32>()?;
        let p = h.dataset("ipp_us")?.read_raw::<u32>()?;
        ensure!(
            t.len() == f.len() && t.len() == c.len() && t.len() == p.len() && t.len() <= 512,
            "inconsistent bounded summary axes"
        );
        for i in 0..t.len() {
            if t[i].is_finite() && f[i].is_finite() && c[i].is_finite() {
                records.push((t[i], f[i], c[i] as f64, p[i] as f64 / 1000.));
            }
        }
    }
    ensure!(start.is_finite() && stop > start, "no coverage");
    carriers.sort_by(f64::total_cmp);
    carriers.dedup();
    ensure!(
        !carriers.is_empty()
            && carriers.len() <= 8
            && carriers.iter().all(|v| v.is_finite() && *v > 0.),
        "invalid monitored carriers"
    );
    intervals.sort();
    intervals.dedup();
    ensure!(!intervals.is_empty(), "missing tested pulse intervals");
    let mut rows = vec![vec![]; carriers.len()];
    for (t, f, c, p) in records {
        let band = carriers
            .iter()
            .enumerate()
            .min_by(|a, b| (a.1 - f).abs().total_cmp(&(b.1 - f).abs()))
            .unwrap()
            .0;
        if (carriers[band] - f).abs() < 50000. {
            rows[band].push((t, c, p));
        }
    }
    let ymax = rows
        .iter()
        .flatten()
        .map(|r| r.1 * 1.1)
        .fold(0.08, f64::max)
        .min(1.);
    let tick = 60.;
    let ticks: Vec<_> = (0..=(stop - start).div_euclid(tick) as usize + 1)
        .map(|i| (start / tick).ceil() * tick + i as f64 * tick)
        .filter(|t| *t <= stop)
        .collect();
    let labels: Vec<_> = ticks
        .iter()
        .map(|t| {
            chrono::DateTime::from_timestamp(*t as i64, 0)
                .unwrap()
                .format("%H:%M")
                .to_string()
        })
        .collect();
    let date = chrono::DateTime::from_timestamp(start as i64, 0)
        .unwrap()
        .format("%-d %b %Y")
        .to_string();
    let mut fig = Figure::new(IntendedUse::Custom {
        width_mm: 240.,
        font_pt: 14.,
    });
    fig.height_mm = 18. + carriers.len() as f64 * 65.;
    fig.raster_dpi = 130.;
    fig.latex.cache = std::path::Path::new(&a[2])
        .parent()
        .context("output parent")?
        .join(".latex-cache");
    let ticks_pri: Vec<_> = intervals.iter().map(|v| *v as f64 / 1000.).collect();
    let limits = if ticks_pri.len() == 1 {
        [ticks_pri[0] - 0.5, ticks_pri[0] + 0.5]
    } else {
        [ticks_pri[0], *ticks_pri.last().unwrap()]
    };
    let scale = Scale::new(limits, Normalization::Linear, ColorMap::turbo())?;
    let sid = fig.scale(scale.clone());
    for (band, data) in rows.iter().enumerate() {
        let mut axes = Axes::new(
            [start, stop],
            [0., ymax],
            if band + 1 == carriers.len() {
                format!("Time (UTC), {date}")
            } else {
                String::new()
            },
            format!("Code coherence ({:.1} MHz)", carriers[band] / 1e6),
        );
        axes.xticks = ticks.clone();
        axes.xticklabels = Some(labels.clone());
        let mut panel = Panel::new(Rect::new(27., 6. + band as f64 * 65., 174., 53.), axes);
        let points: Vec<_> = data.iter().map(|v| [v.0, v.1]).collect();
        let values: Vec<_> = data.iter().map(|v| v.2).collect();
        panel.scatter_colored(&points, &values, &scale, 3., 0.9, "none")?;
        fig.panels.push(panel);
    }
    fig.colorbars.push(Colorbar::new(
        Rect::new(213., 6., 3., carriers.len() as f64 * 65. - 12.),
        sid,
        "Tested pulse interval (ms)",
        ticks_pri,
    ));
    fig.save(&a[2])?;
    Ok(())
}

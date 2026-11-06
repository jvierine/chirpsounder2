//! Native compatibility PNGs for the original dashboard. Reads indexed arrays;
//! HDF5 sources and directory listings stay outside the rendering process.
use anyhow::{ensure, Context, Result};
use iono_core::Grid;
use rusqlite::Connection;
use rustplotlib::{
    Axes, ColorMap, Colorbar, Figure, IntendedUse, Label, Normalization, Panel, Rect, Scale,
};
use std::{collections::BTreeMap, fs, path::Path, process::Command, time::Duration};
fn edges(a: &[f64]) -> Vec<f64> {
    let d = (a[a.len() - 1] - a[0]) / (a.len() - 1) as f64;
    (0..=a.len()).map(|i| a[0] + (i as f64 - 0.5) * d).collect()
}
fn target(g: &Grid) -> Option<String> {
    let safe = |s: &str| s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    if !safe(&g.station)
        || (!["spectrum", "detections"].contains(&g.kind.as_str()) && !safe(&g.transmitter))
    {
        return None;
    }
    Some(match g.kind.as_str() {
        "spectrum" => format!("latest-rf-spectrum-{}.png", g.station),
        "detections" => format!("latest-rothr_jorn-{}.png", g.station),
        "digisonde" | "lfm" | "rti" if g.mode != "X" => {
            format!("latest-{}-{}-{}.png", g.kind, g.transmitter, g.station)
        }
        _ => return None,
    })
}
fn plot(grids: &[Grid], store: &Path, output: &Path) -> Result<()> {
    let mut f = Figure::new(IntendedUse::Custom {
        width_mm: 250.,
        font_pt: 11.,
    });
    f.height_mm = 35. + grids.len() as f64 * 115.;
    f.latex.cache = store.join("latex-cache");
    f.raster_dpi = 120.;
    for (i, g) in grids.iter().enumerate() {
        ensure!(g.x.len() > 1 && g.y.len() > 1, "empty axes");
        let xe = edges(&g.x);
        let ye = edges(&g.y);
        let scale = f.scale(Scale::new(
            [g.low as f64, g.high as f64],
            Normalization::Linear,
            if g.kind == "spectrum" {
                ColorMap::plasma()
            } else {
                ColorMap::new(vec![[1., 1., 1.], [0.02, 0.08, 0.13]])?
            },
        )?);
        let top = 15. + i as f64 * 115.;
        let mut axes = Axes::new(
            [xe[0], *xe.last().unwrap()],
            [ye[0], *ye.last().unwrap()],
            &g.xlabel,
            &g.ylabel,
        );
        if g.xlabel.contains("UTC") {
            let span = xe[xe.len() - 1] - xe[0];
            let step = if span > 3600. {
                21600.
            } else if span > 120. {
                300.
            } else {
                15.
            };
            let start = (xe[0] / step).ceil() * step;
            axes.xticks = (0..7)
                .map(|j| start + j as f64 * step)
                .filter(|&t| t <= *xe.last().unwrap())
                .collect();
            axes.xticklabels = Some(
                axes.xticks
                    .iter()
                    .map(|&t| {
                        chrono::DateTime::from_timestamp(t as i64, 0)
                            .unwrap()
                            .format(if span > 120. { "%H:%M" } else { "%H:%M:%S" })
                            .to_string()
                    })
                    .collect(),
            );
        }
        let mut p = Panel::new(Rect::new(24., top + 8., 190., 82.), axes);
        p.pcolormesh(xe, ye, g.values.iter().map(|&v| v as f64).collect(), scale)?;
        f.panels.push(p);
        f.colorbars.push(Colorbar::new(
            Rect::new(221., top + 8., 3., 82.),
            scale,
            &g.color_label,
            (0..5)
                .map(|j| g.low as f64 + j as f64 * (g.high - g.low) as f64 / 4.)
                .collect(),
        ));
        let date = chrono::DateTime::from_timestamp(g.timestamp as i64, 0)
            .context("timestamp")?
            .format("%Y-%m-%d %H:%M:%S UTC");
        let label = match g.kind.as_str() {
            "spectrum" => format!("{} full-band spectrum", g.station),
            "detections" => format!("{} {} sweeps", g.station, g.transmitter),
            _ => format!("{} to {} {} {}", g.transmitter, g.station, g.kind, g.mode),
        };
        f.labels
            .push(Label::new(format!("{label} --- {date}"), 125., top));
    }
    let temporary = output.with_extension("partial.png");
    f.save(&temporary)?;
    fs::rename(temporary, output)?;
    Ok(())
}
// Text glyphs include observation times, so keep this local cache bounded too.
fn prune_labels(store: &Path) -> Result<()> {
    let dir = store.join("latex-cache");
    let Ok(entries) = fs::read_dir(dir) else {
        return Ok(());
    };
    let mut rows: Vec<_> = entries
        .flatten()
        .filter_map(|e| {
            let m = e.metadata().ok()?;
            if m.is_file() {
                Some((m.modified().ok(), m.len(), e.path()))
            } else {
                None
            }
        })
        .collect();
    rows.sort_by_key(|r| r.0);
    let mut bytes: u64 = rows.iter().map(|r| r.1).sum();
    for (_, n, p) in rows {
        if bytes <= 16 * 1024 * 1024 {
            break;
        }
        fs::remove_file(p)?;
        bytes -= n;
    }
    Ok(())
}
fn run(store: &Path, output: &Path, completed: &mut BTreeMap<String, Vec<String>>) -> Result<()> {
    let db = Connection::open_with_flags(
        store.join("catalog.sqlite"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    db.busy_timeout(Duration::from_secs(3))?;
    let mut q=db.prepare("WITH latest AS (SELECT id,time,ROW_NUMBER() OVER(PARTITION BY stream ORDER BY time DESC,rowid DESC) n FROM products WHERE station='TGO') SELECT p.metadata FROM latest l JOIN products p ON l.id=p.id WHERE n=1 AND l.time>strftime('%s','now')-86400")?;
    let rows = q.query_map([], |r| r.get::<_, String>(0))?;
    let mut groups: BTreeMap<String, Vec<Grid>> = BTreeMap::new();
    for row in rows {
        let g: Grid = serde_json::from_str(&row?)?;
        if let Some(name) = target(&g) {
            groups.entry(name).or_default().push(g)
        }
    }
    drop(q);
    drop(db);
    for (name, mut grids) in groups {
        let ids: Vec<_> = grids.iter().map(|g| g.id.clone()).collect();
        if completed.get(&name) == Some(&ids) {
            continue;
        }
        let result = (|| -> Result<()> {
            for g in &mut grids {
                let cache = store.join("cache").join(format!("{}.bin", g.id));
                let bytes = match fs::read(&cache) {
                    Ok(b) => b,
                    Err(_) => {
                        let r = Command::new("curl")
                            .args([
                                "--fail",
                                "--silent",
                                "--max-time",
                                "30",
                                &format!("http://127.0.0.1:8796/api/data/{}", g.id),
                            ])
                            .output()?;
                        ensure!(r.status.success(), "data service unavailable");
                        r.stdout
                    }
                };
                ensure!(bytes.len() == g.nx * g.ny * 4, "array shape mismatch");
                g.values = bytes
                    .chunks_exact(4)
                    .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
                    .collect();
            }
            plot(&grids, store, &output.join(&name))
        })();
        match result {
            Ok(()) => {
                eprintln!("rendered {name}");
                completed.insert(name, ids);
            }
            Err(e) => eprintln!("{name}: {e:#}"),
        }
    }
    prune_labels(store)?;
    Ok(())
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).is_some_and(|a| a == "--product") {
        ensure!(
            args.len() == 4,
            "usage: iono-render --product INPUT.h5 OUTPUT.png"
        );
        let grids = iono_core::read_products(Path::new(&args[2]))?;
        ensure!(!grids.is_empty(), "no renderable grids");
        let output = Path::new(&args[3]);
        let parent = output
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        fs::create_dir_all(parent)?;
        return plot(&grids[..1], parent, output);
    }
    let store = std::env::var("IONO_STORE").unwrap_or_else(|_| "/var/lib/iono2".into());
    let output = std::env::var("IONO_PNG").unwrap_or_else(|_| "/var/www/html/iono".into());
    let once = std::env::args().any(|a| a == "--once");
    let mut completed = BTreeMap::new();
    loop {
        if let Err(e) = run(Path::new(&store), Path::new(&output), &mut completed) {
            eprintln!("render cycle: {e:#}")
        };
        if once {
            return Ok(());
        }
        std::thread::sleep(Duration::from_secs(30));
    }
}

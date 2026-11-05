use crate::catalog::Profile;
use anyhow::{ensure, Result};
use chrono::{TimeZone, Utc};
use image::{Rgba, RgbaImage};
use rustplotlib::{
    Axes, ColorMap, Colorbar, Figure, IntendedUse, Label, Layer, Normalization, Panel, Rect, Scale,
};
use std::{fs, path::Path};
const W: u32 = 1600;
const H: u32 = 420;
pub fn rainbow() -> ColorMap {
    ColorMap::rainbow()
}
fn viridis() -> ColorMap {
    ColorMap::viridis()
}
fn scale(limits: [f64; 2], colors: ColorMap) -> Scale {
    Scale::new(limits, Normalization::Linear, colors).unwrap()
}
fn figure(title: &str, n: usize, store: &Path) -> Figure {
    let mut f = Figure::new(IntendedUse::Custom {
        width_mm: 290.,
        font_pt: 16.,
    });
    f.height_mm = if n > 1 { 237. } else { 195. };
    f.raster_dpi = 140.;
    f.latex.cache = store.join("tex-cache");
    f.labels.push(Label::new(title, 132., 6.));
    f
}
fn stamp(t: f64, format: &str) -> String {
    Utc.timestamp_opt(t as i64, 0)
        .single()
        .unwrap()
        .format(format)
        .to_string()
}
fn time_axes(start: f64, end: f64, y: [f64; 2], ylabel: &str, bottom: bool) -> Axes {
    let mut a = Axes::new(
        [start, end],
        y,
        if bottom { "Time (UTC)" } else { "" },
        ylabel,
    );
    let step = if end - start > 30. * 3600. {
        21600.
    } else {
        14400.
    };
    let first = (start / step).ceil() * step;
    a.xticks = (0..20)
        .map(|i| first + i as f64 * step)
        .filter(|t| *t <= end)
        .collect();
    a.xticklabels = Some(
        a.xticks
            .iter()
            .map(|t| {
                if end - start <= 86401. && (*t - end).abs() < 0.1 {
                    "24:00".into()
                } else if end - start > 86401. && t.rem_euclid(86400.).abs() < 0.1 {
                    stamp(*t, "%d %b")
                } else {
                    stamp(*t, "%H:%M")
                }
            })
            .collect(),
    );
    if ylabel.contains("Frequency") {
        a.yticks = (0..=6)
            .map(|i| i as f64 * 5.)
            .filter(|v| *v >= y[0] && *v <= y[1])
            .collect();
    } else if y[1] - y[0] > 10000. {
        a.yticks = (-1..=8)
            .map(|i| i as f64 * 5000.)
            .filter(|v| *v >= y[0] && *v <= y[1])
            .collect();
    }
    a.show_x_axis = bottom;
    a
}
fn time_caption(start: f64, end: f64) -> String {
    let date = if stamp(start, "%Y-%m-%d") == stamp(end, "%Y-%m-%d") {
        stamp(start, "%d %b %Y")
    } else {
        format!("{}--{}", stamp(start, "%d %b"), stamp(end, "%d %b %Y"))
    };
    format!("Time (UTC), {date}")
}
fn panel(
    f: &mut Figure,
    img: RgbaImage,
    mut axes: Axes,
    rect: Rect,
    sc: Scale,
    label: &str,
) -> Result<()> {
    // Reserve the date row between clock ticks and the common axis label.
    if axes.show_x_axis {
        axes.xlabel.clear();
    }
    axes.yticklabels = Some(axes.yticks.iter().map(|v| format!("{v:.0}")).collect());
    let id = f.scale(sc.clone());
    let ticks = rustplotlib::figure::ticks(sc.limits, 5);
    let mut p = Panel::new(rect, axes);
    p.layers.push(Layer::Image(img));
    f.panels.push(p);
    let mut bar = Colorbar::new(Rect::new(241., rect.y, 3.5, rect.height), id, label, ticks);
    bar.tick_labels = Some(
        bar.ticks
            .iter()
            .map(|v| {
                format!("{v:.3}")
                    .trim_end_matches('0')
                    .trim_end_matches('.')
                    .to_string()
            })
            .collect(),
    );
    f.colorbars.push(bar);
    Ok(())
}
fn save(f: &Figure, path: &Path) -> Result<()> {
    let tmp = path.with_extension("partial.png");
    f.save(&tmp)?;
    fs::rename(tmp, path)?;
    Ok(())
}
fn pos(v: f64, lim: [f64; 2], n: u32) -> i32 {
    ((v - lim[0]) / (lim[1] - lim[0]) * n as f64).floor() as i32
}
fn point(
    img: &mut RgbaImage,
    x: f64,
    y: f64,
    xlim: [f64; 2],
    ylim: [f64; 2],
    color: [u8; 4],
    radius: i32,
) {
    let px = pos(x, xlim, img.width());
    let py = img.height() as i32 - 1 - pos(y, ylim, img.height());
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            let a = px + dx;
            let b = py + dy;
            if a >= 0 && a < img.width() as i32 && b >= 0 && b < img.height() as i32 {
                img.put_pixel(a as u32, b as u32, Rgba(color));
            }
        }
    }
}
fn rectangle(
    img: &mut RgbaImage,
    x: [f64; 2],
    y: [f64; 2],
    xlim: [f64; 2],
    ylim: [f64; 2],
    color: [u8; 4],
) {
    let x0 = pos(x[0], xlim, img.width()).clamp(0, img.width() as i32);
    let x1 = (pos(x[1], xlim, img.width()) + 1).clamp(0, img.width() as i32);
    let y0 =
        (img.height() as i32 - 1 - pos(y[1], ylim, img.height())).clamp(0, img.height() as i32);
    let y1 = (img.height() as i32 - pos(y[0], ylim, img.height())).clamp(0, img.height() as i32);
    for b in y0..y1 {
        for a in x0..x1 {
            img.put_pixel(a as u32, b as u32, Rgba(color));
        }
    }
}
pub fn rti(
    profiles: &[Profile],
    station: &str,
    start: f64,
    end: f64,
    store: &Path,
    output: &Path,
) -> Result<()> {
    ensure!(!profiles.is_empty(), "no RTI profiles");
    let ymin = profiles
        .iter()
        .map(|p| p.y0 - p.dy / 2.)
        .fold(f64::INFINITY, f64::min);
    let ymin = (ymin / 100.).floor() * 100.;
    let ymax = profiles
        .iter()
        .map(|p| p.y0 + (p.snr.len() as f64 - 0.5) * p.dy)
        .fold(f64::NEG_INFINITY, f64::max);
    let ymax = (ymax / 100.).ceil() * 100.;
    let maxfreq = profiles
        .iter()
        .flat_map(|p| p.freq.iter())
        .copied()
        .filter(|v| v.is_finite())
        .fold(0f32, f32::max)
        .ceil() as f64;
    let ry = [ymin, ymax];
    let fy = [0., maxfreq.max(1.)];
    let xr = [start, end];
    let gray = scale([0., 20.], ColorMap::new(vec![[1., 1., 1.], [0., 0., 0.]])?);
    let freq = scale(fy, rainbow());
    let range = scale(ry, viridis());
    let mut a = RgbaImage::from_pixel(W, H, Rgba([255; 4]));
    let mut b = a.clone();
    let mut c = a.clone();
    let mut strongest = vec![f32::NAN; (W * H) as usize];
    let mut dt: Vec<_> = profiles
        .windows(2)
        .map(|w| w[1].t - w[0].t)
        .filter(|v| *v > 0. && *v < 1800.)
        .collect();
    dt.sort_by(f64::total_cmp);
    let typical = dt.get(dt.len() / 2).copied().unwrap_or(60.);
    for (i, p) in profiles.iter().enumerate() {
        let left = (p.t - typical / 2.).max(if i > 0 {
            (profiles[i - 1].t + p.t) / 2.
        } else {
            start
        });
        let right = (p.t + typical / 2.).min(if i + 1 < profiles.len() {
            (p.t + profiles[i + 1].t) / 2.
        } else {
            end
        });
        let x0 = pos(left, xr, W).clamp(0, W as i32);
        let x1 = (pos(right, xr, W) + 1).clamp(0, W as i32);
        for (r, &v) in p.snr.iter().enumerate() {
            if !v.is_finite() {
                continue;
            }
            let y = p.y0 + r as f64 * p.dy;
            let y0 = (H as i32 - 1 - pos(y + p.dy / 2., ry, H)).clamp(0, H as i32);
            let y1 = (H as i32 - pos(y - p.dy / 2., ry, H)).clamp(0, H as i32);
            for py in y0..y1 {
                for px in x0..x1 {
                    let k = py as usize * W as usize + px as usize;
                    if !strongest[k].is_finite() || v > strongest[k] {
                        strongest[k] = v;
                        a.put_pixel(px as u32, py as u32, Rgba(gray.map(v as f64).unwrap()));
                        b.put_pixel(
                            px as u32,
                            py as u32,
                            Rgba(if v >= iono_core::db(20.) {
                                freq.map(p.freq[r] as f64).unwrap_or([255; 4])
                            } else {
                                [255; 4]
                            }),
                        );
                    }
                }
            }
            if v >= iono_core::db(20.) {
                if let Some(color) = range.map(y) {
                    point(&mut c, p.t, p.freq[r] as f64, xr, fy, color, 1);
                }
            }
        }
    }
    let title = format!("{} → {}", profiles[0].tx, station);
    let mut f = figure(&title.replace('→', r"$\rightarrow$"), 3, store);
    for (i, (img, y, label, sc)) in [
        (a, ry, "Virtual range (km)", gray),
        (b, ry, "Virtual range (km)", freq),
        (c, fy, "Frequency (MHz)", range),
    ]
    .into_iter()
    .enumerate()
    {
        let axes = time_axes(start, end, y, label, i == 2);
        panel(
            &mut f,
            img,
            axes,
            Rect::new(31., 16. + i as f64 * 70., 202., 60.),
            sc,
            if i == 0 {
                "SNR (dB)"
            } else if i == 1 {
                "Frequency (MHz)"
            } else {
                "Virtual range (km)"
            },
        )?;
    }
    f.labels
        .push(Label::new(time_caption(start, end), 132., 230.));
    save(&f, output)
}
pub fn ionogram(g: &iono_core::Grid, station: &str, store: &Path, output: &Path) -> Result<()> {
    let x = [g.x[0], g.x[g.nx - 1]];
    let y = [g.y[0], g.y[g.ny - 1]];
    let sc = scale([0., 20.], ColorMap::new(vec![[1., 1., 1.], [0., 0., 0.]])?);
    let mut img = RgbaImage::from_pixel(W, 850, Rgba([255; 4]));
    let dx = (x[1] - x[0]) / (g.nx - 1) as f64;
    let dy = (y[1] - y[0]) / (g.ny - 1) as f64;
    for r in 0..g.ny {
        for j in 0..g.nx {
            if let Some(color) = sc.map(g.values[r * g.nx + j] as f64) {
                rectangle(
                    &mut img,
                    [g.x[j] - dx / 2., g.x[j] + dx / 2.],
                    [g.y[r] - dy / 2., g.y[r] + dy / 2.],
                    x,
                    y,
                    color,
                );
            }
        }
    }
    let mut f = figure(
        &format!(
            "{} to {} --- {}",
            g.transmitter,
            station,
            stamp(g.timestamp, "%d %b %Y %H:%M UTC")
        ),
        1,
        store,
    );
    let a = Axes::new(x, y, "Frequency (MHz)", "Virtual range (km)");
    panel(
        &mut f,
        img,
        a,
        Rect::new(31., 16., 202., 157.),
        sc,
        "SNR (dB)",
    )?;
    f.labels.push(Label::new("Frequency (MHz)", 132., 187.));
    save(&f, output)
}
pub fn detections(
    rows: &[[f64; 5]],
    station: &str,
    start: f64,
    end: f64,
    store: &Path,
    output: &Path,
) -> Result<()> {
    ensure!(!rows.is_empty(), "no detections");
    let yr = [-5000., 39000.];
    let fr = [0., 25.];
    let xr = [start, end];
    let freq = scale([5., 25.], rainbow());
    let range = scale(yr, rainbow());
    let mut a = RgbaImage::from_pixel(W, 720, Rgba([255; 4]));
    let mut b = a.clone();
    let mut echoes = RgbaImage::new(W, 720);
    let bands: serde_json::Value =
        serde_json::from_str(include_str!("../../../web/transmitters.json"))?;
    let mut f = figure(&format!("JORN and ROTHR to {station}"), 2, store);
    let mut band_entries = Vec::new();
    if let Some(bands) = bands[station].as_array() {
        for band in bands {
            let low = band["min_km"].as_f64().unwrap();
            let high = band["max_km"].as_f64().unwrap();
            rectangle(&mut a, xr, [low, high], xr, yr, [235, 239, 241, 255]);
            let label =
                rustplotlib::latex::text(&band["label"].as_str().unwrap().replace("ROTHR ", ""));
            band_entries.push(rustplotlib::LegendEntry::line(
                format!("{label} ({:.0} km)", band["center_km"].as_f64().unwrap()),
                "#b4bec4",
            ));
        }
    }
    for rate in [100000., 125000.] {
        let selected = clustered(rows, rate);
        for p in selected {
            let delay = p[0].rem_euclid(1.);
            let r = (if delay > 0.5 { delay - 1. } else { delay }) * iono_core::C / 1000.;
            if let Some(color) = freq.map(p[2]) {
                point(&mut echoes, p[0], r, xr, yr, color, 1);
            }
            if let Some(color) = range.map(r) {
                point(&mut b, p[0], p[2], xr, fr, color, 1);
            }
        }
    }
    panel(
        &mut f,
        a,
        time_axes(start, end, yr, "Virtual range (km)", false),
        Rect::new(31., 16., 202., 96.),
        freq,
        "Frequency (MHz)",
    )?;
    panel(
        &mut f,
        b,
        time_axes(start, end, fr, "Frequency (MHz)", true),
        Rect::new(31., 122., 202., 96.),
        range,
        "Virtual range (km)",
    )?;
    f.panels[0].layers.push(Layer::Image(echoes));
    if !band_entries.is_empty() {
        let mut legend = rustplotlib::Legend::new(band_entries);
        legend.position = rustplotlib::LegendPosition::UpperLeft;
        legend.columns = 2;
        legend.avoid_layers = vec![1];
        f.panels[0].legend = Some(legend);
    }
    f.labels
        .push(Label::new(time_caption(start, end), 132., 232.));
    save(&f, output)
}
pub(crate) fn clustered(rows: &[[f64; 5]], rate: f64) -> Vec<&[f64; 5]> {
    let mut v: Vec<_> = rows
        .iter()
        .filter(|p| p.iter().all(|v| v.is_finite()) && (p[3] - rate).abs() < 1.)
        .collect();
    v.sort_by(|a, b| a[0].total_cmp(&b[0]));
    let mut bins: Vec<_> = v
        .iter()
        .map(|p| (p[0] / 0.1).round_ties_even() as i64)
        .collect();
    bins.dedup();
    let mut centers = Vec::<f64>::new();
    for bin in bins {
        let t = bin as f64 * 0.1;
        let lo = v.partition_point(|p| p[0] <= t - 0.1);
        let hi = v.partition_point(|p| p[0] < t + 0.1);
        if hi - lo < 5 {
            continue;
        }
        let mean = v[lo..hi].iter().map(|p| p[0]).sum::<f64>() / (hi - lo) as f64;
        let lo = v.partition_point(|p| p[0] <= mean - 0.02);
        let hi = v.partition_point(|p| p[0] < mean + 0.02);
        if hi - lo >= 5 {
            let mean = v[lo..hi].iter().map(|p| p[0]).sum::<f64>() / (hi - lo) as f64;
            if centers.iter().all(|c| (mean - c).abs() >= 0.1) {
                centers.push(mean);
            }
        }
    }
    v.into_iter()
        .filter(|p| centers.iter().any(|c| (p[0] - c).abs() < 0.02))
        .collect()
}
pub fn spectrum(
    profiles: &[Profile],
    station: &str,
    start: f64,
    end: f64,
    store: &Path,
    output: &Path,
) -> Result<()> {
    ensure!(!profiles.is_empty(), "no spectrum data");
    let nf = 500;
    let nt = 1440;
    let mut power = vec![f32::NAN; nt * nf];
    for p in profiles {
        let time = ((p.t - start) / 60.).floor() as isize;
        if time < 0 || time >= nt as isize {
            continue;
        }
        let mut sum = vec![0f64; nf];
        let mut count = vec![0; nf];
        for (i, &v) in p.snr.iter().enumerate() {
            let y = p.y0 + i as f64 * p.dy;
            let j = (y / 25. * nf as f64).floor() as isize;
            if j >= 0 && j < nf as isize && v.is_finite() && v > 0. {
                sum[j as usize] += v as f64;
                count[j as usize] += 1
            }
        }
        for j in 0..nf {
            if count[j] > 0 {
                power[time as usize * nf + j] = (sum[j] / count[j] as f64) as f32;
            }
        }
    }
    let mut relative = vec![f32::NAN; nt * nf];
    for j in 0..nf {
        let mut sum = 0f64;
        let mut n = 0;
        for i in 0..nt {
            let v = power[i * nf + j];
            if v.is_finite() && v > 0. {
                sum += v as f64;
                n += 1
            }
        }
        let mean = (sum / n.max(1) as f64) as f32;
        for i in 0..nt {
            relative[i * nf + j] = iono_core::db(power[i * nf + j] / mean);
        }
    }
    let (lo, hi) = percentile_limits(&relative);
    let sc = scale([lo as f64, hi as f64], ColorMap::plasma());
    let mut img = RgbaImage::from_pixel(W, 800, Rgba([255; 4]));
    for i in 0..nt {
        for j in 0..nf {
            if let Some(color) = sc.map(relative[i * nf + j] as f64) {
                rectangle(
                    &mut img,
                    [start + i as f64 * 60., start + (i + 1) as f64 * 60.],
                    [j as f64 * 25. / nf as f64, (j + 1) as f64 * 25. / nf as f64],
                    [start, end],
                    [0., 25.],
                    color,
                );
            }
        }
    }
    let mut f = figure(&format!("{station} full-band spectrum"), 1, store);
    f.height_mm = 198.;
    panel(
        &mut f,
        img,
        time_axes(start, end, [0., 25.], "Frequency (MHz)", true),
        Rect::new(31., 16., 202., 158.),
        sc,
        "Relative power (dB)",
    )?;
    f.labels
        .push(Label::new(time_caption(start, end - 1.), 132., 190.));
    save(&f, output)?;
    let product = output.with_extension("h5");
    let tmp = product.with_extension("partial.h5");
    {
        let h = hdf5::File::create(&tmp)?;
        h.new_dataset::<f32>()
            .shape([nt, nf])
            .chunk((60, 100))
            .deflate(3)
            .create("power")?
            .write_raw(&power)?;
        h.new_dataset::<f32>()
            .shape([nt, nf])
            .chunk((60, 100))
            .deflate(3)
            .create("relative_power_db")?
            .write_raw(&relative)?;
        h.new_attr::<f64>()
            .create("color_low_db")?
            .write_scalar(&(lo as f64))?;
        h.new_attr::<f64>()
            .create("color_high_db")?
            .write_scalar(&(hi as f64))?;
        let schema: hdf5::types::VarLenUnicode = "receiver-full-day-spectrum-v1".parse()?;
        h.new_attr::<hdf5::types::VarLenUnicode>()
            .create("schema")?
            .write_scalar(&schema)?;
        let t: Vec<_> = (0..nt).map(|i| start + (i as f64 + 0.5) * 60.).collect();
        let freq: Vec<_> = (0..nf)
            .map(|j| (j as f64 + 0.5) * 25e6 / nf as f64)
            .collect();
        h.new_dataset::<f64>()
            .shape(nt)
            .create("time_unix")?
            .write_raw(&t)?;
        h.new_dataset::<f64>()
            .shape(nf)
            .create("frequency_hz")?
            .write_raw(&freq)?;
    }
    fs::rename(tmp, product)?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn detector_clusters_keep_rates_separate() {
        let mut rows = Vec::new();
        for rate in [100000., 125000.] {
            for i in 0..4 {
                rows.push([
                    1792627200.21 + i as f64 * 0.001,
                    1792627200.,
                    10.,
                    rate,
                    100.,
                ]);
            }
        }
        assert!(clustered(&rows, 100000.).is_empty());
        rows.push([1792627200.214, 1792627200., 11., 100000., 100.]);
        assert_eq!(clustered(&rows, 100000.).len(), 5);
        assert!(clustered(&rows, 125000.).is_empty());
    }
    #[test]
    fn utc_ticks_mark_midnight_without_an_extra_row() {
        let start = 1792540800. + 3. * 3600.;
        let a = time_axes(start, start + 48. * 3600., [0., 1.], "", true);
        let labels = a.xticklabels.unwrap();
        assert_eq!(labels.len(), 8);
        assert_eq!(
            labels,
            vec!["06:00", "12:00", "18:00", "22 Oct", "06:00", "12:00", "18:00", "23 Oct"]
        );
        assert_eq!(labels[0], "06:00");
        assert_eq!(labels[3], "22 Oct");
    }
}

// Finite values only; absent minutes must never bias the colour limits.
pub(crate) fn percentile_limits(values: &[f32]) -> (f32, f32) {
    let mut v: Vec<_> = values.iter().copied().filter(|x| x.is_finite()).collect();
    if v.is_empty() {
        return (-1., 1.);
    }
    v.sort_unstable_by(f32::total_cmp);
    let q = |p: f64| {
        let k = p * (v.len() - 1) as f64;
        let i = k.floor() as usize;
        v[i] + (v[k.ceil() as usize] - v[i]) * (k - i as f64) as f32
    };
    let (a, b) = (q(0.05), q(0.95));
    if b - a < 0.001 {
        (a - 0.5, b + 0.5)
    } else {
        (a, b)
    }
}
#[cfg(test)]
mod percentile_test {
    use super::*;
    #[test]
    fn five_ninety_five_excludes_gaps() {
        let (a, b) = percentile_limits(&[0., 1., 2., 3., 4., f32::NAN]);
        assert!((a - 0.2).abs() < 1e-6);
        assert!((b - 3.8).abs() < 1e-6);
    }
}

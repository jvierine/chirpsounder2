//! Bounded display arrays and telemetry prepared beside the receiver's data.
use crate::catalog::{Catalog, FrequencyProfile, Profile};
use anyhow::Result;
use iono_browser::{Bundle, Evidence, Product, SCHEMA};
use iono_core::Grid;
use std::{fs, path::Path};
const NT: usize = 1440;
pub(crate) fn grid(
    station: &str,
    tx: &str,
    kind: &str,
    quantity: &str,
    palette: &str,
    start: f64,
    ny: usize,
    yr: [f64; 2],
    limits: [f32; 2],
    color: &str,
    values: Vec<f32>,
) -> Product {
    let occupied_minutes = (0..NT)
        .filter(|&x| (0..ny).any(|y| values[y * NT + x].is_finite()))
        .count();
    Product {
        grid: Grid {
            id: String::new(),
            station: station.into(),
            transmitter: tx.into(),
            kind: kind.into(),
            mode: "O".into(),
            timestamp: start,
            nx: NT,
            ny,
            x: (0..NT).map(|i| start + (i as f64 + 0.5) * 60.).collect(),
            y: (0..ny)
                .map(|i| yr[0] + (i as f64 + 0.5) * (yr[1] - yr[0]) / ny as f64)
                .collect(),
            xlabel: "Time (UTC)".into(),
            ylabel: if quantity == "range-frequency"
                || quantity == "spectrum"
                || quantity == "background-spectrum"
            {
                "Frequency (MHz)"
            } else {
                "Virtual range (km)"
            }
            .into(),
            color_label: color.into(),
            low: limits[0],
            high: limits[1],
            values,
        },
        quantity: quantity.into(),
        palette: palette.into(),
        occupied_minutes,
    }
}
fn minute(t: f64, start: f64) -> Option<usize> {
    let x = ((t - start) / 60.).floor();
    (x >= 0. && x < NT as f64).then_some(x as usize)
}
fn bin(v: f64, range: [f64; 2], n: usize) -> Option<usize> {
    let y = ((v - range[0]) / (range[1] - range[0]) * n as f64).floor();
    (y >= 0. && y < n as f64).then_some(y as usize)
}
fn configured_prn_window(station: &str, tx: &str) -> Option<[f64; 2]> {
    if !tx.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        return None;
    }
    let path = std::path::PathBuf::from(std::env::var_os("HOME")?)
        .join(".config/iono-prn")
        .join(format!("{tx}.json"));
    let c: serde_json::Value = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
    if c["station"] != station || c["transmitter"] != tx {
        return None;
    }
    let lo = c["range_min_km"].as_f64()?;
    let hi = c["range_max_km"].as_f64()?;
    (lo.is_finite() && hi.is_finite() && lo >= 0. && hi > lo && hi <= 10000.).then_some([lo, hi])
}
fn rti(
    rows: &[Profile],
    frequency_rows: &[FrequencyProfile],
    station: &str,
    tx: &str,
    start: f64,
) -> Vec<Product> {
    if rows.is_empty() {
        return vec![];
    }
    let lo = (rows
        .iter()
        .map(|p| p.y0 - p.dy / 2.)
        .fold(f64::INFINITY, f64::min)
        / 100.)
        .floor()
        * 100.;
    let hi = (rows
        .iter()
        .map(|p| p.y0 + (p.snr.len() as f64 - 0.5) * p.dy)
        .fold(f64::NEG_INFINITY, f64::max)
        / 100.)
        .ceil()
        * 100.;
    // A new receiver window also applies to older displayed profiles. Preserve
    // the original source arrays; do not let old broad axes expand this view.
    let yr = configured_prn_window(station, tx).unwrap_or([lo, hi.max(lo + 100.)]);
    let ny = 512;
    let mut peak = vec![f32::NAN; NT * ny];
    let mut frequency = peak.clone();
    let mut by_freq = vec![f32::NAN; NT * 256];
    let mut strength = by_freq.clone();
    for p in rows {
        let Some(x) = minute(p.t, start) else {
            continue;
        };
        for (r, &v) in p.snr.iter().enumerate() {
            if !v.is_finite() {
                continue;
            }
            let range = p.y0 + r as f64 * p.dy;
            if let Some(y) = bin(range, yr, ny) {
                let k = y * NT + x;
                if !peak[k].is_finite() || v > peak[k] {
                    peak[k] = v;
                    frequency[k] = if v >= iono_core::db(20.) {
                        p.freq[r]
                    } else {
                        f32::NAN
                    };
                }
            }
        }
    }
    for p in frequency_rows {
        let Some(x) = minute(p.t, start) else {
            continue;
        };
        for (f, &v) in p.snr.iter().enumerate() {
            if v.is_finite() && v >= iono_core::db(20.) && bin(p.range[f] as f64, yr, ny).is_some()
            {
                if let Some(y) = bin(p.frequency[f] as f64, [0., 25.], 256) {
                    let k = y * NT + x;
                    if !strength[k].is_finite() || v > strength[k] {
                        strength[k] = v;
                        by_freq[k] = p.range[f];
                    }
                }
            }
        }
    }
    vec![
        grid(
            station,
            tx,
            "rti",
            "snr",
            "greys",
            start,
            ny,
            yr,
            [0., 20.],
            "SNR (dB)",
            peak,
        ),
        grid(
            station,
            tx,
            "rti",
            "peak-frequency",
            "rainbow",
            start,
            ny,
            yr,
            [0., 25.],
            "Frequency (MHz)",
            frequency,
        ),
        grid(
            station,
            tx,
            "rti",
            "range-frequency",
            "viridis",
            start,
            256,
            [0., 25.],
            [lo as f32, hi as f32],
            "Virtual range (km)",
            by_freq,
        ),
    ]
}
fn spectrum(rows: &[Profile], station: &str, start: f64) -> Vec<Product> {
    if rows.is_empty() {
        return vec![];
    }
    let ny = 500;
    let mut power = vec![f32::NAN; NT * ny];
    for p in rows {
        let Some(x) = minute(p.t, start) else {
            continue;
        };
        let mut sum = vec![0f64; ny];
        let mut count = vec![0usize; ny];
        for (i, &v) in p.snr.iter().enumerate() {
            if v.is_finite() && v > 0. {
                if let Some(y) = bin(p.y0 + i as f64 * p.dy, [0., 25.], ny) {
                    sum[y] += v as f64;
                    count[y] += 1;
                }
            }
        }
        for y in 0..ny {
            if count[y] > 0 {
                power[y * NT + x] = (sum[y] / count[y] as f64) as f32;
            }
        }
    }
    // Interference monitoring must retain persistent carriers and the noise
    // floor. Average positive linear power before log conversion; never divide
    // this product by each frequency's daily mean. The receiver gain and FFT
    // normalization are not an absolute RF calibration, so do not label dBm.
    let background = power.iter().map(|&v| iono_core::db(v)).collect::<Vec<_>>();
    let (low, high) = crate::render::percentile_limits(&background);
    let background = grid(
        station,
        "",
        "spectrum",
        "background-spectrum",
        "plasma",
        start,
        ny,
        [0., 25.],
        [low, high],
        "Power (dB, uncalibrated)",
        background,
    );
    for row in power.chunks_mut(NT) {
        let sum: f64 = row
            .iter()
            .filter(|v| v.is_finite() && **v > 0.)
            .map(|&v| v as f64)
            .sum();
        let n = row.iter().filter(|v| v.is_finite() && **v > 0.).count();
        let mean = (sum / n.max(1) as f64) as f32;
        for v in row {
            *v = iono_core::db(*v / mean);
        }
    }
    let (low, high) = crate::render::percentile_limits(&power);
    vec![
        grid(
            station,
            "",
            "spectrum",
            "spectrum",
            "plasma",
            start,
            ny,
            [0., 25.],
            [low, high],
            "Relative power (dB)",
            power,
        ),
        background,
    ]
}
fn sweeps(rows: &[[f64; 5]], station: &str, start: f64) -> Vec<Product> {
    let mut products = vec![];
    for rate in [100000., 125000.] {
        let selected = crate::render::clustered(rows, rate);
        if selected.is_empty() {
            continue;
        }
        let mut frequency = vec![f32::NAN; NT * 512];
        let mut strength = frequency.clone();
        let mut ranges = vec![f32::NAN; NT * 256];
        let mut fsnr = ranges.clone();
        for p in selected {
            let Some(x) = minute(p[0], start) else {
                continue;
            };
            let delay = p[0].rem_euclid(1.);
            let range = if delay > 0.5 { delay - 1. } else { delay } * iono_core::C / 1000.;
            if let Some(y) = bin(range, [-5000., 39000.], 512) {
                let k = y * NT + x;
                if !strength[k].is_finite() || p[1] as f32 > strength[k] {
                    strength[k] = p[1] as f32;
                    frequency[k] = p[2] as f32;
                }
            }
            if let Some(y) = bin(p[2], [0., 25.], 256) {
                let k = y * NT + x;
                if !fsnr[k].is_finite() || p[1] as f32 > fsnr[k] {
                    fsnr[k] = p[1] as f32;
                    ranges[k] = range as f32;
                }
            }
        }
        let tx = format!("{} kHz/s", rate as i64 / 1000);
        products.push(grid(
            station,
            &tx,
            "detections",
            "peak-frequency",
            "rainbow",
            start,
            512,
            [-5000., 39000.],
            [5., 25.],
            "Frequency (MHz)",
            frequency,
        ));
        products.push(grid(
            station,
            &tx,
            "detections",
            "range-frequency",
            "rainbow",
            start,
            256,
            [0., 25.],
            [-5000., 39000.],
            "Virtual range (km)",
            ranges,
        ));
    }
    products
}
fn regularize_prn_frequency(g: &mut Grid) {
    if g.kind != "prn" || g.nx < 2 {
        return;
    }
    let span = g.x[g.nx - 1] - g.x[0];
    let step =
        g.x.windows(2)
            .map(|v| v[1] - v[0])
            .fold(f64::INFINITY, f64::min);
    if !step.is_finite() || step <= 0. || span <= 0. {
        return;
    }
    let nx = ((span / step).round() as usize + 1).min(512);
    if nx == g.nx && g.x.windows(2).all(|v| ((v[1] - v[0]) - step).abs() < 1e-6) {
        return;
    }
    let dx = span / (nx - 1) as f64;
    let mut values = vec![f32::NAN; nx * g.ny];
    for (f, &frequency) in g.x.iter().enumerate() {
        let x = ((frequency - g.x[0]) / dx).round() as usize;
        for y in 0..g.ny {
            let v = g.values[y * g.nx + f];
            let at = y * nx + x.min(nx - 1);
            if v.is_finite() && (!values[at].is_finite() || v > values[at]) {
                values[at] = v;
            }
        }
    }
    g.x = (0..nx).map(|i| g.x[0] + i as f64 * dx).collect();
    g.nx = nx;
    g.values = values;
}
fn finished_sounding(path: &Path) -> bool {
    if !path
        .file_name()
        .is_some_and(|n| n.to_string_lossy().starts_with("prn_ionogram-"))
    {
        return true;
    }
    let Ok(h) = hdf5::File::open(path) else {
        return false;
    };
    let Ok(cfg) =
        serde_json::from_str::<serde_json::Value>(&iono_core::text(&h, "configuration_json", ""))
    else {
        return false;
    };
    let Some(dwell) = cfg.get("dwell_s").and_then(|v| v.as_f64()) else {
        // VERSO's split TDM sections do not have one constant dwell. Its
        // explicit per-frequency completion flags identify a finished frame.
        return h
            .dataset("frequency_complete")
            .and_then(|d| d.read_raw::<u8>())
            .is_ok_and(|v| !v.is_empty() && v.iter().all(|&n| n == 1));
    };
    let Ok(freqs) = h.dataset("freqs") else {
        return false;
    };
    let end = iono_core::scalar(&h, "t0", f64::NAN) + dwell * freqs.size() as f64;
    dwell > 0. && iono_core::scalar(&h, "last_observation", 0.) >= end
}
fn latest_for_tx(catalog: &Catalog, station: &str, tx: &str, start: f64) -> Result<Vec<Product>> {
    let mut products = vec![];
    let candidates:Vec<String>=catalog.db.prepare("SELECT path FROM files WHERE kind='rti' AND tx=?1 AND time>=?2 AND time<?3 ORDER BY time DESC LIMIT 4")?
            .query_map(rusqlite::params![tx,start,start+86400.],|r|r.get(0))?.collect::<std::result::Result<_,_>>()?;
    // A newly started PRN sweep must not erase a complete ionogram with an
    // almost-empty frame. Its current measurements still enter the RTIs.
    let source = candidates
        .iter()
        .find(|p| finished_sounding(Path::new(p)))
        .or(candidates.first());
    if let Some(source) = source {
        let grids = iono_core::read_products(Path::new(&source))?;
        if let Some(mut g) = grids.into_iter().find(|g| g.mode != "X") {
            // The Swedish frequency tables have larger high-band steps.
            // Keep unmeasured frequencies blank on the WebGL uniform grid.
            regularize_prn_frequency(&mut g);
            // Peak-preserving display projection; native HDF5 stays authoritative.
            let sx = g.nx.div_ceil(512);
            let sy = g.ny.div_ceil(512);
            let nx = g.nx.div_ceil(sx);
            let ny = g.ny.div_ceil(sy);
            let mut values = vec![f32::NAN; nx * ny];
            for y in 0..g.ny {
                for x in 0..g.nx {
                    let v = g.values[y * g.nx + x];
                    let k = (y / sy) * nx + x / sx;
                    if v.is_finite() && (!values[k].is_finite() || v > values[k]) {
                        values[k] = v;
                    }
                }
            }
            g.x = (0..nx)
                .map(|i| g.x[(i * sx + (sx - 1) / 2).min(g.nx - 1)])
                .collect();
            g.y = (0..ny)
                .map(|i| g.y[(i * sy + (sy - 1) / 2).min(g.ny - 1)])
                .collect();
            g.nx = nx;
            g.ny = ny;
            g.values = values;
            g.station = station.into();
            products.push(Product {
                grid: g,
                quantity: "ionogram".into(),
                palette: "greys".into(),
                occupied_minutes: 0,
            });
        }
    }
    Ok(products)
}
pub fn export_latest(
    catalog: &Catalog,
    station: &str,
    start: f64,
    path: &Path,
    health: serde_json::Value,
) -> Result<usize> {
    let mut products = vec![];
    let mut evidence = vec![];
    for tx in catalog.transmitters()? {
        let found = latest_for_tx(catalog, station, &tx, start)?;
        if let Some(p) = found.first() {
            let payload:Option<String>=catalog.db.query_row(
                "SELECT payload FROM frequency_evidence WHERE tx=?1 AND time<=?2 ORDER BY time DESC LIMIT 1",
                rusqlite::params![tx,p.grid.timestamp],|r|r.get(0)).ok();
            if let Some(value) = payload {
                evidence.push(serde_json::from_str::<Evidence>(&value)?);
            }
        }
        products.extend(found);
    }
    let n = products.len();
    // The small publication lane also carries current radar associations.
    // Limited-bandwidth stations must not upload a full-day raster bundle
    // merely to update a few map rows. Use the local indexed detection table.
    let since = start.max(chrono::Utc::now().timestamp() as f64 - 1800.);
    let mut latest_paths = std::collections::BTreeMap::new();
    for row in crate::assumed::envelopes(&catalog.detections(since)?, station, start) {
        let id = row["transmitter"].as_str().unwrap().to_owned();
        if latest_paths.get(&id).is_none_or(|old: &serde_json::Value| {
            old["time"].as_f64().unwrap() < row["time"].as_f64().unwrap()
        }) {
            latest_paths.insert(id, row);
        }
    }
    let mut bundle = Bundle {
        schema: SCHEMA.into(),
        station: station.into(),
        day: chrono::DateTime::from_timestamp(start as i64, 0)
            .unwrap()
            .format("%Y-%m-%d")
            .to_string(),
        generated_unix: chrono::Utc::now().timestamp() as f64,
        partial: true,
        products,
        evidence,
        assumed_paths: latest_paths.into_values().collect(),
        health,
    };
    iono_browser::write_transport(path, &mut bundle)?;
    Ok(n)
}
pub fn export(
    catalog: &Catalog,
    station: &str,
    start: f64,
    path: &Path,
    health: serde_json::Value,
) -> Result<usize> {
    let mut products = vec![];
    for tx in catalog.transmitters()? {
        let rows: Vec<_> = catalog
            .profiles(&tx, start)?
            .into_iter()
            .filter(|p| p.t < start + 86400.)
            .collect();
        let frequency_rows = catalog.frequency_profiles(&tx, start, start + 86400.)?;
        products.extend(rti(&rows, &frequency_rows, station, &tx, start));
        products.extend(latest_for_tx(catalog, station, &tx, start)?);
    }
    products.extend(spectrum(
        &catalog
            .spectrum(start)?
            .into_iter()
            .filter(|p| p.t < start + 86400.)
            .collect::<Vec<_>>(),
        station,
        start,
    ));
    let detections = catalog
        .detections(start)?
        .into_iter()
        .filter(|p| p[0] < start + 86400.)
        .collect::<Vec<_>>();
    products.extend(sweeps(&detections, station, start));
    let mut q = catalog.db.prepare(
        "SELECT payload FROM frequency_evidence WHERE time>=?1 AND time<?2 ORDER BY time",
    )?;
    let mut evidence = vec![];
    for row in q.query_map([start, start + 86400.], |r| r.get::<_, String>(0))? {
        evidence.push(serde_json::from_str::<Evidence>(&row?)?);
    }
    // Only the live bundle carries the moving 48-hour display window. Existing
    // archive bundles and the limited-bandwidth DOB publisher remain unchanged.
    let now = chrono::Utc::now().timestamp() as f64;
    if (now / 86400.).floor() * 86400. == start {
        products.extend(crate::link_history::prepare(catalog, station, now)?);
    }
    let mut bundle = Bundle {
        schema: SCHEMA.into(),
        station: station.into(),
        day: chrono::DateTime::from_timestamp(start as i64, 0)
            .unwrap()
            .format("%Y-%m-%d")
            .to_string(),
        generated_unix: chrono::Utc::now().timestamp() as f64,
        partial: false,
        products,
        evidence,
        assumed_paths: crate::assumed::envelopes(&detections, station, start),
        health,
    };
    let n = bundle.products.len();
    iono_browser::write_transport(path, &mut bundle)?;
    Ok(n)
}
pub fn health(previous: &mut Option<(u64, u64)>) -> serde_json::Value {
    let cpu = fs::read_to_string("/proc/stat").unwrap_or_default();
    let logical_cpus = cpu
        .lines()
        .filter(|line| {
            line.strip_prefix("cpu")
                .is_some_and(|s| s.starts_with(|c: char| c.is_ascii_digit()))
        })
        .count()
        .max(1);
    let counts: Vec<u64> = cpu
        .lines()
        .next()
        .unwrap_or("")
        .split_whitespace()
        .skip(1)
        .take(8)
        .filter_map(|v| v.parse().ok())
        .collect();
    let total: u64 = counts.iter().sum();
    let idle = counts.get(3).copied().unwrap_or(0) + counts.get(4).copied().unwrap_or(0);
    let percent = previous.and_then(|(t, i)| {
        (total > t).then(|| 100. * (1. - idle.saturating_sub(i) as f64 / (total - t) as f64))
    });
    *previous = Some((total, idle));
    let load: Vec<f64> = fs::read_to_string("/proc/loadavg")
        .unwrap_or_default()
        .split_whitespace()
        .take(3)
        .filter_map(|v| v.parse().ok())
        .collect();
    let mut memory = serde_json::Map::new();
    for line in fs::read_to_string("/proc/meminfo")
        .unwrap_or_default()
        .lines()
    {
        let a: Vec<_> = line.split_whitespace().collect();
        if a.len() > 1 && ["MemTotal:", "MemAvailable:", "SwapTotal:", "SwapFree:"].contains(&a[0])
        {
            if let Ok(v) = a[1].parse::<u64>() {
                memory.insert(a[0].trim_end_matches(':').into(), (v * 1024).into());
            }
        }
    }
    let mut temperatures = vec![];
    if let Ok(chips) = fs::read_dir("/sys/class/hwmon") {
        for chip in chips.flatten().take(32) {
            let root = chip.path();
            let name = fs::read_to_string(root.join("name"))
                .unwrap_or_default()
                .trim()
                .to_string();
            if let Ok(entries) = fs::read_dir(&root) {
                for e in entries.flatten().take(128) {
                    let f = e.file_name().to_string_lossy().into_owned();
                    if f.starts_with("temp") && f.ends_with("_input") {
                        if let Ok(v) = fs::read_to_string(e.path())
                            .unwrap_or_default()
                            .trim()
                            .parse::<f64>()
                        {
                            if (-40_000. ..150_000.).contains(&v) {
                                let label =
                                    fs::read_to_string(root.join(f.replace("_input", "_label")))
                                        .unwrap_or_else(|_| f.clone())
                                        .trim()
                                        .to_string();
                                temperatures.push(serde_json::json!({"sensor":format!("{name} {label}"),"celsius":v/1000.}));
                            }
                        }
                    }
                }
            }
        }
    }
    serde_json::json!({"time":chrono::Utc::now().timestamp(),"cpu_percent":percent,"load_average":load,"logical_cpus":logical_cpus,"memory":memory,"temperatures":temperatures})
}
pub fn acquisition_health(
    health: &mut serde_json::Value,
    ring: &Path,
    output: &Path,
    newest_product_ns: Option<i64>,
) {
    let channel = ring.join("ch0");
    let mut age: Option<f64> = None;
    if let Ok(directories) = fs::read_dir(&channel) {
        for directory in directories.flatten().take(4) {
            if let Ok(files) = fs::read_dir(directory.path()) {
                for file in files.flatten().take(128) {
                    if file.file_name().to_string_lossy().starts_with("rf@") {
                        if let Ok(elapsed) = file
                            .metadata()
                            .and_then(|m| m.modified())
                            .and_then(|t| t.elapsed().map_err(std::io::Error::other))
                        {
                            let value = elapsed.as_secs_f64();
                            age = Some(age.map_or(value, |a| a.min(value)));
                        }
                    }
                }
            }
        }
    }
    health["generated_unix"] = health["time"].clone();
    health["ringbuffer"] = serde_json::json!({"digital_rf_ok":channel.join("drf_properties.h5").is_file()&&age.is_some(),"newest_age_s":age});
    // Product timestamps are already indexed; do not list science files again.
    let latest = newest_product_ns
        .map(|t| (chrono::Utc::now().timestamp() as f64 - t as f64 / 1e9).max(0.))
        .unwrap_or(1801.);
    health["output"] = serde_json::json!({"exists":output.is_dir(),"newest_age_s":latest});
    let mut disks = vec![];
    for (label, path) in [("Data disk", output), ("Voltage RAM ring", ring)] {
        if let Ok(result) = std::process::Command::new("df")
            .args(["--block-size=1", "--output=size,avail,pcent"])
            .arg(path)
            .output()
        {
            if result.status.success() {
                let text = String::from_utf8_lossy(&result.stdout);
                let fields: Vec<_> = text
                    .lines()
                    .nth(1)
                    .unwrap_or("")
                    .split_whitespace()
                    .collect();
                if fields.len() == 3 {
                    if let (Ok(free), Ok(used)) = (
                        fields[1].parse::<u64>(),
                        fields[2].trim_end_matches('%').parse::<f64>(),
                    ) {
                        disks.push(serde_json::json!({"label":label,"free_bytes":free,"used_percent":used}));
                    }
                }
            }
        }
    }
    health["disks"] = serde_json::json!(disks);
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn interference_spectrum_preserves_persistent_power_and_averages_linearly() {
        let rows = [Profile {
            t: 86430.,
            tx: String::new(),
            y0: 0.01,
            dy: 0.02,
            // First two FFT bins share one 50 kHz display cell: mean(1,99)=50.
            snr: vec![1., 99., 100., 100.],
            freq: vec![],
        }];
        let products = spectrum(&rows, "KREA", 86400.);
        assert_eq!(products.len(), 2);
        let raw = &products[1];
        assert_eq!(raw.quantity, "background-spectrum");
        assert!((raw.grid.values[0] - 10. * 50f32.log10()).abs() < 1e-4);
        assert!((raw.grid.values[NT] - 20.).abs() < 1e-4);
        assert!(raw.grid.values[1].is_nan());
        // A carrier stable throughout the measured day stays bright in raw
        // power; the separate variability view still has a zero-dB baseline.
        assert_eq!(products[0].grid.values[0], 0.);
        assert_eq!(products[0].grid.values[NT], 0.);
    }
    #[test]
    fn broad_e_region_trace_survives_independent_frequency_reduction() {
        let mut g = Grid {
            id: String::new(),
            station: "DOB".into(),
            transmitter: "SGO".into(),
            kind: "lfm".into(),
            mode: "O".into(),
            timestamp: 86430.,
            nx: 8,
            ny: 3,
            x: (2..10).map(|f| f as f64).collect(),
            y: vec![900., 1050., 1200.],
            xlabel: String::new(),
            ylabel: String::new(),
            color_label: String::new(),
            low: 0.,
            high: 20.,
            values: vec![f32::NAN; 24],
        };
        // Seven strong frequencies at the same E-region range, one unmeasured
        // bin. The old range-first reduction retains only the 2 MHz winner.
        g.values[8..15].copy_from_slice(&[20., 19., 18., 17., 16., 15., 14.]);
        let range = crate::catalog::reduce(&g);
        assert_eq!(range.freq.iter().filter(|v| v.is_finite()).count(), 1);
        let frequency = crate::catalog::reduce_frequency(&g);
        let products = rti(&[range], &[frequency], "DOB", "SGO", 86400.);
        let f = &products[2];
        for mhz in 2..9 {
            let y = bin(mhz as f64, [0., 25.], 256).unwrap();
            assert_eq!(f.grid.values[y * NT], 1050.);
            assert!(f.grid.values[y * NT + 1].is_nan());
        }
        assert!(f.grid.values[bin(9., [0., 25.], 256).unwrap() * NT].is_nan());
        assert_eq!(f.occupied_minutes, 1);
        g.kind = "prn".into();
        g.x[7] = 12.;
        g.values[15] = 12.;
        regularize_prn_frequency(&mut g);
        assert_eq!(g.nx, 11);
        assert_eq!(g.x[10], 12.);
        assert!(g.values[11 + 7..11 + 10].iter().all(|v| v.is_nan()));
        assert_eq!(g.values[11 + 10], 12.);
    }
    #[test]
    fn rti_keeps_ranges_and_real_gaps() {
        let p = Profile {
            t: 86430.,
            tx: "Test".into(),
            y0: 10.,
            dy: 100.,
            snr: vec![1., 20., f32::NAN],
            freq: vec![2., 7., f32::NAN],
        };
        let a = rti(&[p], &[], "TGO", "Test", 86400.);
        assert_eq!(a[0].occupied_minutes, 1);
        assert!(a[0]
            .grid
            .values
            .iter()
            .skip(1)
            .step_by(NT)
            .all(|v| v.is_nan()));
        assert!(a[1].grid.values.iter().any(|&v| v == 7.));
    }
}

//! Receiver-local materialization and Rustplotlib compatibility products.
mod assumed;
mod browser;
mod catalog;
mod link_history;
mod render;
use anyhow::{ensure, Context, Result};
use catalog::{Catalog, Profile};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};
#[derive(Deserialize)]
struct Config {
    station: String,
    output_dir: String,
    #[serde(default)]
    data_dir: Option<String>,
    #[serde(default = "browser_interval_default")]
    browser_interval_s: u64,
    #[serde(default = "browser_timeout_default")]
    browser_upload_timeout_s: u64,
    #[serde(default = "browser_rate_default")]
    browser_upload_kib_s: u64,
    #[serde(default = "browser_previous_default")]
    browser_previous_day: bool,
}
fn browser_rate_default() -> u64 {
    256
}
fn browser_previous_default() -> bool {
    true
}
fn browser_interval_default() -> u64 {
    300
}
fn browser_timeout_default() -> u64 {
    600
}
fn upload_command(path: &Path, url: &str) -> Command {
    upload_command_with_timeout(path, url, browser_timeout_default())
}
fn upload_command_with_timeout(path: &Path, url: &str, timeout_s: u64) -> Command {
    browser_upload_command(path, url, timeout_s, browser_rate_default())
}
fn browser_upload_command(path: &Path, url: &str, timeout_s: u64, rate_kib_s: u64) -> Command {
    let browser = path
        .file_name()
        .is_some_and(|p| p.to_string_lossy().starts_with("browser_day-"));
    let mut command = Command::new("curl");
    let timeout = timeout_s.clamp(60, 3600).to_string();
    let rate = format!("{}k", rate_kib_s.clamp(1, 256));
    command.args([
        "--fail",
        "--silent",
        "--show-error",
        "--max-time",
        if browser { &timeout } else { "90" },
        "--retry",
        if browser { "0" } else { "2" },
    ]);
    if browser {
        // A full-day bundle is larger than an individual observation. Allow a
        // slow uplink to finish instead of blindly resending its body. Retry
        // the latest revision on the next worker cycle after a failed attempt.
        command.args([
            "--connect-timeout",
            "15",
            "--limit-rate",
            &rate,
            "--write-out",
            "browser upload: %{size_upload} bytes in %{time_total}s; HTTP %{http_code}\n",
        ]);
    }
    command
        .arg("-F")
        .arg(format!("file=@{}", path.display()))
        .arg(url);
    command
}
fn upload(path: &Path, url: &str) -> Result<()> {
    let status = upload_command(path, url).status()?;
    ensure!(status.success(), "upload failed: {}", path.display());
    Ok(())
}
fn export(profiles: &[Profile], start: f64, end: f64, path: &Path) -> Result<()> {
    let tmp = path.with_extension("partial.h5");
    {
        let h = hdf5::File::create(&tmp)?;
        let mut offsets = vec![0u64];
        let mut snr = Vec::new();
        let mut freq = Vec::new();
        for p in profiles {
            snr.extend_from_slice(&p.snr);
            freq.extend_from_slice(&p.freq);
            offsets.push(snr.len() as u64);
        }
        for (name, values) in [
            (
                "profile_time_unix",
                profiles.iter().map(|p| p.t).collect::<Vec<_>>(),
            ),
            ("range_start_km", profiles.iter().map(|p| p.y0).collect()),
            ("range_step_km", profiles.iter().map(|p| p.dy).collect()),
        ] {
            h.new_dataset::<f64>()
                .shape(values.len())
                .create(name)?
                .write_raw(&values)?;
        }
        h.new_dataset::<u64>()
            .shape(offsets.len())
            .create("profile_offsets")?
            .write_raw(&offsets)?;
        for (name, values) in [("snr_peak_db", snr), ("peak_frequency_mhz", freq)] {
            h.new_dataset::<f32>()
                .shape(values.len())
                .chunk(values.len().min(65536).max(1))
                .deflate(3)
                .create(name)?
                .write_raw(&values)?;
        }
        for (name, value) in [("window_start_unix", start), ("window_end_unix", end)] {
            h.new_attr::<f64>().create(name)?.write_scalar(&value)?;
        }
        let schema: hdf5::types::VarLenUnicode = "receiver-rti-native-v1".parse()?;
        h.new_attr::<hdf5::types::VarLenUnicode>()
            .create("schema")?
            .write_scalar(&schema)?;
    }
    fs::rename(tmp, path)?;
    Ok(())
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    let config = args
        .get(1)
        .context("usage: iono-plots CONFIG.json [--once] [--no-upload] [--only-rti]")?;
    let cfg: Config = serde_json::from_slice(&fs::read(config)?)?;
    let once = args.iter().any(|a| a == "--once");
    let post = !args.iter().any(|a| a == "--no-upload");
    let only_rti = args.iter().any(|a| a == "--only-rti");
    let browser_only = args.iter().any(|a| a == "--browser-only");
    let base = PathBuf::from(&cfg.output_dir);
    let store = base.join(".iono-plots");
    let png = store.join("png");
    fs::create_dir_all(&png)?;
    let mut catalog = Catalog::open(&base, &store, &cfg.station)?;
    let mut latest = BTreeMap::new();
    let mut last_render: Option<Instant> = None;
    let mut previous_cpu = None;
    let _ = browser::health(&mut previous_cpu);
    let mut past_browser_days = std::collections::BTreeSet::new();
    loop {
        let mut failures = 0;
        let now = chrono::Utc::now().timestamp() as f64;
        let changed = catalog.refresh(now)?;
        eprintln!("indexed {changed} new or changed receiver products");
        let mut current_health = browser::health(&mut previous_cpu);
        if let Some(ring) = &cfg.data_dir {
            let newest: Option<i64> = catalog
                .db
                .query_row("SELECT MAX(mtime) FROM files", [], |r| r.get(0))
                .unwrap_or(None);
            browser::acquisition_health(&mut current_health, Path::new(ring), &base, newest);
        }
        let full = last_render.is_none_or(|t| {
            t.elapsed() > Duration::from_secs(cfg.browser_interval_s.clamp(60, 86400))
        });
        if full {
            for offset in [0., 1.] {
                if offset > 0. && !cfg.browser_previous_day {
                    continue;
                }
                let day = (now / 86400.).floor() * 86400. - offset * 86400.;
                if offset > 0. && past_browser_days.contains(&(day as i64)) {
                    continue;
                }
                let date = chrono::DateTime::from_timestamp(day as i64, 0)
                    .unwrap()
                    .format("%Y-%m-%d")
                    .to_string();
                let directory = base.join(date);
                if !directory.is_dir() {
                    continue;
                }
                let output =
                    directory.join(format!("browser_day-{}-{}.h5", cfg.station, day as i64));
                match browser::export(&catalog, &cfg.station, day, &output, current_health.clone())
                {
                    Ok(n) => {
                        eprintln!("browser arrays: {n} plots for {}", day as i64);
                        let mut sent = true;
                        if post {
                            if let Err(e) = browser_upload_command(
                                &output,
                                "https://juha.no/upload_h5.php",
                                cfg.browser_upload_timeout_s,
                                cfg.browser_upload_kib_s,
                            )
                            .status()
                            .and_then(|s| {
                                if s.success() {
                                    Ok(())
                                } else {
                                    Err(std::io::Error::other("receiver bundle upload failed"))
                                }
                            }) {
                                failures += 1;
                                sent = false;
                                eprintln!("browser upload: {e:#}");
                            }
                        }
                        if sent && offset > 0. && now - day > 86700. {
                            past_browser_days.insert(day as i64);
                        }
                        if !sent && offset == 0. {
                            // Live data takes priority over the one-time archive
                            // upload while the site's uplink is unavailable.
                            break;
                        }
                    }
                    Err(e) => {
                        failures += 1;
                        eprintln!("browser arrays: {e:#}");
                    }
                }
            }
        }
        if browser_only {
            if full {
                // Failed uploads also observe the site cooldown. An outage
                // must not trigger repeated full-day bodies every 30 seconds.
                last_render = Some(Instant::now());
            }
            if once {
                ensure!(failures == 0, "browser product failed");
                return Ok(());
            }
            std::thread::sleep(Duration::from_secs(30));
            continue;
        }
        let start = now - 48. * 3600.;
        for tx in catalog.transmitters()? {
            if full {
                let t = Instant::now();
                let rows = catalog.profiles(&tx, start)?;
                let output = png.join(format!("latest-rti-{tx}-{}.png", cfg.station));
                let result = (|| -> Result<()> {
                    render::rti(&rows, &cfg.station, start, now, &store, &output)?;
                    let day = (now / 86400.).floor() * 86400.;
                    let product = base
                        .join(chrono::Utc::now().format("%Y-%m-%d").to_string())
                        .join(format!("rti-{tx}-{}-{}.h5", cfg.station, day as i64));
                    export(&rows, start, now, &product)?;
                    if post {
                        upload(&output, "https://juha.no/upload.php")?;
                        upload(&product, "https://juha.no/upload_h5.php")?;
                    }
                    Ok(())
                })();
                match result {
                    Ok(()) => eprintln!(
                        "RTI {tx}: {} soundings in {:.2}s",
                        rows.len(),
                        t.elapsed().as_secs_f64()
                    ),
                    Err(e) => {
                        failures += 1;
                        eprintln!("RTI {tx}: {e:#}");
                    }
                }
            }
            if !only_rti {
                if let Some(source) = catalog.latest("rti", &tx)? {
                    let version = fs::metadata(&source)?.modified()?;
                    if latest.get(&tx) != Some(&(source.clone(), version)) {
                        let result = (|| -> Result<()> {
                            let grids = iono_core::read_products(&source)?;
                            let g = grids.iter().find(|g| g.mode != "X").context("O mode")?;
                            let output =
                                png.join(format!("latest-{}-{tx}-{}.png", g.kind, cfg.station));
                            render::ionogram(g, &cfg.station, &store, &output)?;
                            if post {
                                upload(&output, "https://juha.no/upload.php")?;
                            }
                            Ok(())
                        })();
                        match result {
                            Ok(()) => {
                                latest.insert(tx.clone(), (source, version));
                            }
                            Err(e) => {
                                failures += 1;
                                eprintln!("ionogram {tx}: {e:#}");
                            }
                        }
                    }
                }
            }
        }
        if full && !only_rti {
            let result = (|| -> Result<()> {
                let rows = catalog.detections(start)?;
                let output = png.join(format!("latest-rothr_jorn-{}.png", cfg.station));
                render::detections(&rows, &cfg.station, start, now, &store, &output)?;
                if post {
                    upload(&output, "https://juha.no/upload.php")?;
                }
                Ok(())
            })();
            if let Err(e) = result {
                failures += 1;
                eprintln!("detection overview: {e:#}")
            }
            let result = (|| -> Result<()> {
                let day = (now / 86400.).floor() * 86400.;
                let rows = catalog.spectrum(day)?;
                let output = png.join(format!("latest-rf-spectrum-{}.png", cfg.station));
                render::spectrum(&rows, &cfg.station, day, day + 86400., &store, &output)?;
                if post {
                    upload(&output, "https://juha.no/upload.php")?;
                    let product = base
                        .join(chrono::Utc::now().format("%Y-%m-%d").to_string())
                        .join(format!(
                            "spectrum_summary-{}-{}.h5",
                            cfg.station, day as i64
                        ));
                    fs::copy(output.with_extension("h5"), &product)?;
                    upload(&product, "https://juha.no/upload_h5.php")?;
                }
                Ok(())
            })();
            if let Err(e) = result {
                failures += 1;
                eprintln!("full-day spectrum: {e:#}")
            }
        }
        if full {
            last_render = if failures == 0 {
                Some(Instant::now())
            } else {
                None
            };
        }
        if once {
            ensure!(failures == 0, "{failures} plot products failed");
            return Ok(());
        }
        std::thread::sleep(Duration::from_secs(30));
    }
}

#[cfg(test)]
mod transport_tests {
    use super::*;
    fn option(args: &[String], name: &str) -> String {
        args.windows(2).find(|a| a[0] == name).unwrap()[1].clone()
    }
    #[test]
    fn daily_bundles_have_a_bandwidth_cap_and_no_blind_body_retries() {
        let command = upload_command(
            Path::new("/data0/2026-10-22/browser_day-DOB-1792627200.h5"),
            "https://example.test/upload_h5.php",
        );
        let args: Vec<_> = command
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        assert_eq!(option(&args, "--max-time"), "600");
        assert_eq!(option(&args, "--retry"), "0");
        assert_eq!(option(&args, "--limit-rate"), "256k");
        assert!(!args.contains(&"--speed-limit".into()));
        let slow = upload_command_with_timeout(
            Path::new("browser_day-DOB-1792627200.h5"),
            "https://example.test",
            3600,
        );
        let args: Vec<_> = slow
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        assert_eq!(option(&args, "--max-time"), "3600");
        assert_eq!(option(&args, "--retry"), "0");
    }
    #[test]
    fn slow_site_config_caps_transfers_without_archive_backfill() {
        let cfg: Config = serde_json::from_str(r#"{"station":"DOB","output_dir":"/data0","browser_upload_kib_s":16,"browser_previous_day":false}"#).unwrap();
        assert!(!cfg.browser_previous_day);
        let command = browser_upload_command(
            Path::new("browser_day-DOB-1792627200.h5"),
            "https://example.test",
            cfg.browser_upload_timeout_s,
            cfg.browser_upload_kib_s,
        );
        let args: Vec<_> = command
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        assert_eq!(option(&args, "--limit-rate"), "16k");
        assert_eq!(option(&args, "--retry"), "0");
    }
    #[test]
    fn individual_legacy_products_retain_their_transport_policy() {
        let command = upload_command(
            Path::new("/data0/plots/latest-rti-SGO-TGO.png"),
            "https://example.test/upload.php",
        );
        let args: Vec<_> = command
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        assert_eq!(option(&args, "--max-time"), "90");
        assert_eq!(option(&args, "--retry"), "2");
        assert!(!args.contains(&"--limit-rate".into()));
    }
}

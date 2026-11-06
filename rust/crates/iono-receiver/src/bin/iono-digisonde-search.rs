//! Bounded receiver-local PINS pulse-search commissioning tool. No UHD access.
use anyhow::{ensure, Context, Result};
use hdf5::File;
use iono_core::digisonde_search::{Candidate, PulseSearch};
use num_complex::Complex32;
use rdigital_rf::{CacheOptions, Reader};
use serde::Deserialize;
use std::{
    fs,
    path::Path,
    thread,
    time::{Instant, SystemTime, UNIX_EPOCH},
};
#[derive(Clone, Deserialize)]
struct Config {
    station: String,
    data_dir: String,
    output_dir: String,
    #[serde(default = "channel")]
    channel: String,
    #[serde(default = "search_band")]
    frequency_range_hz: [f64; 2],
    #[serde(default = "candidates")]
    max_candidates: usize,
    #[serde(default)]
    stratified_frequency_budget: bool,
}
fn search_band() -> [f64; 2] {
    [0.7e6, 24.5e6]
}
fn candidates() -> usize {
    6
}
fn channel() -> String {
    "ch0".into()
}
fn reader(c: &Config) -> Result<Reader> {
    let mut r = Reader::open([&c.data_dir])?;
    r.set_cache_options(CacheOptions {
        minimum_bytes: 1024 * 1024,
        max_open_files: 2,
    })?;
    r.set_cache_byte_limit(16 * 1024 * 1024)?;
    Ok(r)
}
fn save(
    path: &Path,
    c: &Config,
    rows: &[(u64, Candidate)],
    processed: u64,
    gaps: u64,
) -> Result<()> {
    let tmp = path.with_extension("partial.h5");
    let h = File::create(&tmp)?;
    macro_rules! vector {
        ($name:expr,$values:expr) => {
            h.new_dataset_builder().with_data(&$values).create($name)?;
        };
    }
    vector!("sample_index", rows.iter().map(|r| r.0).collect::<Vec<_>>());
    vector!(
        "time_unix",
        rows.iter().map(|r| r.0 as f64 / 25e6).collect::<Vec<_>>()
    );
    vector!(
        "frequency_hz",
        rows.iter().map(|r| r.1.frequency_hz).collect::<Vec<_>>()
    );
    vector!(
        "pulse_period_us",
        rows.iter().map(|r| r.1.ipp_us).collect::<Vec<_>>()
    );
    vector!(
        "pulse_phase_s",
        rows.iter().map(|r| r.1.phase_s).collect::<Vec<_>>()
    );
    vector!(
        "fold_contrast",
        rows.iter().map(|r| r.1.contrast).collect::<Vec<_>>()
    );
    vector!(
        "mean_power",
        rows.iter().map(|r| r.1.mean_power).collect::<Vec<_>>()
    );
    vector!(
        "effective_pulses",
        rows.iter()
            .map(|r| r.1.effective_pulses)
            .collect::<Vec<_>>()
    );
    h.new_attr::<u64>()
        .create("processed_blocks")?
        .write_scalar(&processed)?;
    h.new_attr::<u64>()
        .create("missing_blocks")?
        .write_scalar(&gaps)?;
    for (name,value) in [("receiver",c.station.as_str()),("method","PINS spectral soft clipping and pulse-folded STFT; coarse 5/10 ms recurrence candidates; code/mode, carrier and transmitter require verification"),("raw_source",c.data_dir.as_str())] {
        h.new_attr::<hdf5::types::VarLenUnicode>().create(name)?.write_scalar(&value.parse::<hdf5::types::VarLenUnicode>()?)?;
    }
    drop(h);
    fs::rename(tmp, path)?;
    Ok(())
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    let c: Config = serde_json::from_slice(&fs::read(
        args.get(1)
            .context("usage: iono-digisonde-search CONFIG [--lookback SECONDS]")?,
    )?)?;
    ensure!(
        !c.station.is_empty()
            && c.station
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_'),
        "invalid station"
    );
    let [min_hz, max_hz] = c.frequency_range_hz;
    ensure!(
        min_hz.is_finite()
            && max_hz.is_finite()
            && min_hz >= 0.7e6
            && max_hz <= 24.5e6
            && min_hz < max_hz
            && (1..=64).contains(&c.max_candidates),
        "invalid search band/candidate budget"
    );
    let lookback = args
        .iter()
        .position(|s| s == "--lookback")
        .map(|i| {
            args.get(i + 1)
                .context("lookback value")?
                .parse::<u64>()
                .context("lookback seconds")
        })
        .transpose()?
        .unwrap_or(900)
        .min(900);
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let bounds = reader(&c)?
        .bounds(&c.channel)?
        .context("no readable voltage")?;
    let stride = 8_000_000u64; // 0.32 s, observing a 0.16 s window.
    let start = (bounds.0 + 25_000_000)
        .max(now.saturating_sub(lookback) * 25_000_000)
        .div_ceil(stride)
        * stride;
    let end = bounds.1.saturating_sub(25_000_000);
    ensure!(end > start + 4_000_000, "no complete search windows");
    let count = (end - start - 4_000_000) / stride + 1;
    fs::create_dir_all(&c.output_dir)?;
    let path = Path::new(&c.output_dir).join(format!(
        "digisonde_search-{}-{}.h5",
        c.station,
        start / 25_000_000
    ));
    type Job = (u64, Vec<Complex32>);
    let (result_tx, result_rx) = std::sync::mpsc::sync_channel::<(u64, Result<Vec<Candidate>>)>(4);
    let mut senders = vec![];
    let max_candidates = c.max_candidates;
    let stratified = c.stratified_frequency_budget;
    for _ in 0..4 {
        let (tx, rx) = std::sync::mpsc::sync_channel::<Job>(1);
        senders.push(tx);
        let tx = result_tx.clone();
        thread::spawn(move || {
            let mut search = PulseSearch::new(25e6, 12.5e6);
            while let Ok((i, z)) = rx.recv() {
                if tx
                    .send((
                        i,
                        Ok(if stratified {
                            search.seek_stratified(&z, 1.8, max_candidates, min_hz, max_hz)
                        } else {
                            search.seek_band(&z, 1.8, max_candidates, min_hz, max_hz)
                        }),
                    ))
                    .is_err()
                {
                    break;
                }
            }
        });
    }
    let input = c.clone();
    let input_tx = result_tx.clone();
    drop(result_tx);
    let feeder = thread::spawn(move || -> Result<()> {
        let mut r = reader(&input)?;
        for n in 0..count {
            let i = start + n * stride;
            match r.read_complex32(i, 4_000_000, &input.channel, 0) {
                Ok(z) => senders[n as usize % senders.len()]
                    .send((i, z))
                    .map_err(|_| anyhow::anyhow!("search worker stopped"))?,
                Err(e) => {
                    input_tx
                        .send((i, Err(anyhow::anyhow!(e.to_string()))))
                        .map_err(|_| anyhow::anyhow!("output stopped"))?;
                    r = reader(&input)?;
                }
            }
        }
        Ok(())
    });
    let timer = Instant::now();
    let mut rows = vec![];
    let mut processed = 0;
    let mut gaps = 0;
    for (i, result) in result_rx {
        processed += 1;
        match result {
            Ok(found) => rows.extend(found.into_iter().map(|p| (i, p))),
            Err(e) => {
                gaps += 1;
                eprintln!("missing voltage {i}: {e:#}");
            }
        }
        if processed % 100 == 0 {
            rows.sort_by_key(|r| r.0);
            save(&path, &c, &rows, processed, gaps)?;
            eprintln!(
                "{} {processed}/{count} windows, {} candidates, {gaps} missing, {:.1}s elapsed",
                c.station,
                rows.len(),
                timer.elapsed().as_secs_f64()
            );
        }
    }
    feeder
        .join()
        .map_err(|_| anyhow::anyhow!("reader panicked"))??;
    rows.sort_by_key(|r| r.0);
    save(&path, &c, &rows, processed, gaps)?;
    ensure!(processed == count, "search workers stopped early");
    println!("{} candidates; {}", rows.len(), path.display());
    Ok(())
}

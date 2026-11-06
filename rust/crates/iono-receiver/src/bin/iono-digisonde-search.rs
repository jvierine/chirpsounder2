//! Bounded receiver-local PINS pulse-search commissioning tool. No UHD access.
use anyhow::{ensure, Context, Result};
use hdf5::File;
use iono_core::digisonde_search::{Candidate, PulseSearch};
use iono_core::{
    digisonde_match::{CodeMatch, CodeSearch},
    prn::Channelizer,
};
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
    #[serde(default = "workers")]
    worker_count: usize,
    #[serde(default = "stride")]
    stride_s: f64,
    #[serde(default = "grace")]
    writer_grace_s: f64,
    #[serde(default)]
    verify_every_windows: u64,
    #[serde(default = "one")]
    verify_candidates_per_window: usize,
    /// (fine-frequency count, consecutive physical pulses per fine channel).
    #[serde(default = "verification_programs")]
    verification_programs: Vec<[usize; 2]>,
    #[serde(default = "orders")]
    verification_orders: Vec<bool>,
}
fn orders() -> Vec<bool> {
    vec![false]
}
fn verification_programs() -> Vec<[usize; 2]> {
    vec![[1, 1]]
}
fn one() -> usize {
    1
}
struct Checked {
    sample: u64,
    coarse_hz: f64,
    result: Option<CodeMatch>,
}
struct Found {
    candidates: Vec<Candidate>,
    checked: Vec<Checked>,
}
fn verify_frame(
    decoder: &mut CodeSearch,
    raw: &[Complex32],
    carrier: f64,
) -> Result<Option<CodeMatch>> {
    ensure!(
        raw.len() == 4_002_000,
        "verification requires real FIR overlap"
    );
    let channelizer = Channelizer::new(carrier - 12.5e6, 100e3)?;
    let base = channelizer.process(raw, -1000);
    Ok(decoder.seek_with_doppler(&base[4..16004], 30000., 5000., 5))
}
fn verification_choices(candidates: &[Candidate], turn: u64, budget: usize) -> Vec<usize> {
    let mut order: Vec<_> = (0..candidates.len()).collect();
    // Cycle MHz bands; preserve the finder's strongest-first order within a band.
    order.sort_by_key(|&i| {
        ((candidates[i].frequency_hz / 1e6).floor() as u64 + 25 - turn % 25) % 25
    });
    let mut selected: Vec<usize> = vec![];
    for i in order {
        if selected.iter().all(|&j| {
            (candidates[j].frequency_hz / 1e6).floor() != (candidates[i].frequency_hz / 1e6).floor()
        }) {
            selected.push(i);
            if selected.len() == budget {
                break;
            }
        }
    }
    selected
}
fn workers() -> usize {
    4
}
fn stride() -> f64 {
    0.32
}
fn grace() -> f64 {
    0.15
}
fn stride_samples(c: &Config) -> Result<u64> {
    ensure!(
        !c.verification_orders.is_empty() && c.verification_orders.len() <= 2,
        "invalid verification polarization order bank"
    );
    ensure!(
        !c.verification_programs.is_empty()
            && c.verification_programs.len() <= 5
            && c.verification_programs
                .iter()
                .all(|p| (1..=4).contains(&p[0]) && (1..=16).contains(&p[1])),
        "invalid bounded verification program bank"
    );
    ensure!(
        (1..=2).contains(&c.verify_candidates_per_window),
        "verification budget must be 1..2"
    );
    ensure!(
        (1..=4).contains(&c.worker_count),
        "worker_count must be 1..4"
    );
    ensure!(
        c.stride_s.is_finite() && (0.16..=5.12).contains(&c.stride_s),
        "invalid search stride"
    );
    ensure!(
        c.writer_grace_s.is_finite() && (0.15..=10.).contains(&c.writer_grace_s),
        "invalid writer grace"
    );
    let n = (c.stride_s * 25e6).round() as u64;
    ensure!(
        n % 4_000_000 == 0,
        "stride must be a multiple of the 0.16 s analysis window"
    );
    Ok(n)
}
fn plan(
    bounds: (u64, u64),
    now: u64,
    lookback: u64,
    live_s: u64,
    stride: u64,
) -> Result<(u64, u64)> {
    let start = bounds
        .0
        .saturating_add(25_000_000)
        .max(now.saturating_sub(lookback) * 25_000_000)
        .div_ceil(stride)
        * stride;
    let end = if live_s > 0 {
        (now + live_s) * 25_000_000
    } else {
        bounds.1.saturating_sub(25_000_000)
    };
    ensure!(end > start + 4_000_000, "no complete search windows");
    Ok((start, (end - start - 4_000_000) / stride + 1))
}
fn wait_for_window(i: u64, grace: f64) -> Result<()> {
    let target = (i + 4_000_000) as f64 / 25e6 + grace;
    loop {
        let remaining = target - SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs_f64();
        if remaining <= 0. {
            return Ok(());
        }
        thread::sleep(std::time::Duration::from_secs_f64(remaining.min(1.)));
    }
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
    coverage: [u64; 3],
    checks: &[Checked],
) -> Result<()> {
    let tmp = path.with_extension("partial.h5");
    let h = File::create(&tmp)?;
    iono_core::receiver_timing::write_metadata(&h)?;
    macro_rules! vector {
        ($name:expr,$values:expr) => {
            let values = $values;
            if values.is_empty() {
                h.new_dataset_builder().with_data(&values).create($name)?;
            } else {
                h.new_dataset_builder()
                    .with_data(&values)
                    .chunk(values.len().min(4096))
                    .shuffle()
                    .deflate(3)
                    .create($name)?;
            }
        };
    }
    vector!("sample_index", rows.iter().map(|r| r.0).collect::<Vec<_>>());
    vector!(
        "time_unix",
        rows.iter()
            .map(|r| iono_core::receiver_timing::utc_from_recorded(r.0 as f64 / 25e6))
            .collect::<Vec<_>>()
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
    h.create_group("code_checks")?;
    vector!(
        "code_checks/sample_index",
        checks.iter().map(|r| r.sample).collect::<Vec<_>>()
    );
    vector!(
        "code_checks/coarse_frequency_hz",
        checks.iter().map(|r| r.coarse_hz).collect::<Vec<_>>()
    );
    vector!(
        "code_checks/matched",
        checks
            .iter()
            .map(|r| u8::from(r.result.is_some()))
            .collect::<Vec<_>>()
    );
    vector!(
        "code_checks/frequency_hz",
        checks
            .iter()
            .map(|r| r.coarse_hz + r.result.as_ref().map_or(0., |m| m.carrier_offset_hz))
            .collect::<Vec<_>>()
    );
    vector!(
        "code_checks/ipp_us",
        checks
            .iter()
            .map(|r| r.result.as_ref().map_or(0, |m| m.ipp_us))
            .collect::<Vec<_>>()
    );
    vector!(
        "code_checks/mode",
        checks
            .iter()
            .map(|r| r.result.as_ref().map_or(255, |m| m.mode))
            .collect::<Vec<_>>()
    );
    vector!(
        "code_checks/all_polarizations",
        checks
            .iter()
            .map(|r| r
                .result
                .as_ref()
                .map_or(0, |m| u8::from(m.all_polarizations)))
            .collect::<Vec<_>>()
    );
    vector!(
        "code_checks/lag_s",
        checks
            .iter()
            .map(|r| r.result.as_ref().map_or(f64::NAN, |m| m.lag_s))
            .collect::<Vec<_>>()
    );
    vector!(
        "code_checks/coherence",
        checks
            .iter()
            .map(|r| r.result.as_ref().map_or(f32::NAN, |m| m.coherence))
            .collect::<Vec<_>>()
    );
    vector!(
        "code_checks/score_db",
        checks
            .iter()
            .map(|r| r.result.as_ref().map_or(f32::NAN, |m| m.score_db))
            .collect::<Vec<_>>()
    );
    vector!(
        "code_checks/fine_frequency_count",
        checks
            .iter()
            .map(|r| r
                .result
                .as_ref()
                .map_or(0, |m| m.fine_frequency_count as u32))
            .collect::<Vec<_>>()
    );
    vector!(
        "code_checks/multiplex_block_pulses",
        checks
            .iter()
            .map(|r| r
                .result
                .as_ref()
                .map_or(0, |m| m.multiplex_block_pulses as u32))
            .collect::<Vec<_>>()
    );
    vector!(
        "code_checks/polarization_interleaved",
        checks
            .iter()
            .map(|r| r
                .result
                .as_ref()
                .map_or(0, |m| u8::from(m.polarization_interleaved)))
            .collect::<Vec<_>>()
    );
    vector!(
        "verification_orders",
        c.verification_orders
            .iter()
            .map(|v| u8::from(*v))
            .collect::<Vec<_>>()
    );
    let programs: Vec<_> = c
        .verification_programs
        .iter()
        .flatten()
        .map(|v| *v as u32)
        .collect();
    h.new_dataset::<u32>()
        .shape([c.verification_programs.len(), 2])
        .create("verification_programs")?
        .write_raw(&programs)?;
    h.new_attr::<u64>()
        .create("verify_every_windows")?
        .write_scalar(&c.verify_every_windows)?;
    h.new_attr::<u64>()
        .create("processed_blocks")?
        .write_scalar(&processed)?;
    h.new_attr::<u64>()
        .create("missing_blocks")?
        .write_scalar(&gaps)?;
    for (name, value) in [
        ("first_planned_sample_index", coverage[0]),
        ("last_scanned_sample_index", coverage[1]),
        ("planned_blocks", coverage[2]),
        ("worker_count", c.worker_count as u64),
    ] {
        h.new_attr::<u64>().create(name)?.write_scalar(&value)?;
    }
    for (name, value) in [
        ("stride_s", c.stride_s),
        ("analysis_window_s", 0.16),
        ("writer_grace_s", c.writer_grace_s),
    ] {
        h.new_attr::<f64>().create(name)?.write_scalar(&value)?;
    }
    h.new_dataset_builder()
        .with_data(&c.frequency_range_hz)
        .create("searched_frequency_range_hz")?;
    for (name,value) in [("receiver",c.station.as_str()),("method","PINS spectral soft clipping and pulse-folded STFT; coarse 5/10 ms recurrence candidates; code/mode, carrier and transmitter require verification"),("raw_source",c.data_dir.as_str())] {
        h.new_attr::<hdf5::types::VarLenUnicode>().create(name)?.write_scalar(&value.parse::<hdf5::types::VarLenUnicode>()?)?;
    }
    drop(h);
    fs::rename(tmp, path)?;
    Ok(())
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    let c: Config = serde_json::from_slice(&fs::read(args.get(1).context(
        "usage: iono-digisonde-search CONFIG [--lookback SECONDS] [--duration SECONDS]",
    )?)?)?;
    let stride = stride_samples(&c)?;
    let live_s = args
        .iter()
        .position(|s| s == "--duration")
        .map(|i| {
            args.get(i + 1)
                .context("duration value")?
                .parse::<u64>()
                .context("duration seconds")
        })
        .transpose()?
        .unwrap_or(0);
    ensure!(
        live_s <= 3600,
        "live survey is limited to one hour per invocation"
    );
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
        .unwrap_or(if live_s > 0 { 0 } else { 900 })
        .min(900);
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    iono_core::receiver_timing::initialize(&c.station)?;
    let bounds = reader(&c)?
        .bounds(&c.channel)?
        .context("no readable voltage")?;
    let (start, count) = plan(bounds, now, lookback, live_s, stride)?;
    fs::create_dir_all(&c.output_dir)?;
    let path = Path::new(&c.output_dir).join(format!(
        "digisonde_search-{}-{}.h5",
        c.station,
        start / 25_000_000
    ));
    type Job = (u64, u64, Vec<Complex32>);
    let (result_tx, result_rx) = std::sync::mpsc::sync_channel::<(u64, Result<Found>)>(4);
    let mut senders = vec![];
    let max_candidates = c.max_candidates;
    let stratified = c.stratified_frequency_budget;
    let verify_every = c.verify_every_windows;
    let verify_budget = c.verify_candidates_per_window;
    for _ in 0..c.worker_count {
        let (tx, rx) = std::sync::mpsc::sync_channel::<Job>(1);
        senders.push(tx);
        let tx = result_tx.clone();
        let programs: Vec<_> = c
            .verification_programs
            .iter()
            .map(|p| (p[0], p[1]))
            .collect();
        let orders = c.verification_orders.clone();
        thread::spawn(move || {
            let mut search = PulseSearch::new(25e6, 12.5e6);
            let mut decoder = (verify_every > 0).then(|| {
                CodeSearch::with_orders(
                    16000,
                    100e3,
                    &[5000, 10000, 20000, 40000],
                    &programs,
                    &orders,
                )
            });
            while let Ok((n, i, z)) = rx.recv() {
                let candidates = if stratified {
                    search.seek_stratified(&z[1000..4_001_000], 1.8, max_candidates, min_hz, max_hz)
                } else {
                    search.seek_band(&z[1000..4_001_000], 1.8, max_candidates, min_hz, max_hz)
                };
                let mut checked = vec![];
                if verify_every > 0 && n % verify_every == 0 {
                    for j in verification_choices(&candidates, n / verify_every, verify_budget) {
                        let carrier = (candidates[j].frequency_hz / 10000.).round() * 10000.;
                        let result = verify_frame(decoder.as_mut().unwrap(), &z, carrier)
                            .expect("validated in-band carrier and overlap");
                        checked.push(Checked {
                            sample: i,
                            coarse_hz: carrier,
                            result,
                        });
                    }
                }
                if tx
                    .send((
                        i,
                        Ok(Found {
                            candidates,
                            checked,
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
            if live_s > 0 {
                wait_for_window(i, input.writer_grace_s)?;
            }
            match r.read_complex32(i - 1000, 4_002_000, &input.channel, 0) {
                Ok(z) => senders[n as usize % senders.len()]
                    .send((n, i, z))
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
    let mut checks = vec![];
    let mut processed = 0;
    let mut gaps = 0;
    let mut last_scanned = start;
    let mut last_save = Instant::now();
    for (i, result) in result_rx {
        processed += 1;
        last_scanned = last_scanned.max(i);
        match result {
            Ok(found) => {
                rows.extend(found.candidates.into_iter().map(|p| (i, p)));
                checks.extend(found.checked);
            }
            Err(e) => {
                gaps += 1;
                eprintln!("missing voltage {i}: {e:#}");
            }
        }
        if processed == 1 || processed % 100 == 0 || last_save.elapsed().as_secs() >= 30 {
            rows.sort_by_key(|r| r.0);
            save(
                &path,
                &c,
                &rows,
                processed,
                gaps,
                [start, last_scanned, count],
                &checks,
            )?;
            last_save = Instant::now();
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
    save(
        &path,
        &c,
        &rows,
        processed,
        gaps,
        [start, last_scanned, count],
        &checks,
    )?;
    ensure!(processed == count, "search workers stopped early");
    println!("{} candidates; {}", rows.len(), path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn config() -> Config {
        serde_json::from_str(r#"{"station":"DOB","data_dir":"/dev/shm/hf25","output_dir":"/tmp/search-test","worker_count":1,"stride_s":0.64}"#).unwrap()
    }
    #[test]
    fn verification_bank_is_bounded_and_default_preserves_original_search() {
        let mut c = config();
        assert_eq!(c.verification_programs, vec![[1, 1]]);
        for invalid in [
            vec![],
            vec![[0, 1]],
            vec![[2, 0]],
            vec![[2, 17]],
            vec![[1, 1]; 6],
        ] {
            c.verification_programs = invalid;
            assert!(stride_samples(&c).is_err());
        }
        c.verification_programs = vec![[1, 1], [2, 1], [2, 2], [2, 4], [2, 8]];
        stride_samples(&c).unwrap();
    }
    #[test]
    fn live_plan_extends_beyond_existing_voltage_without_rounding_sample_indices() {
        let now = 1_793_970_450u64;
        let bounds = ((now - 60) * 25_000_000, (now - 1) * 25_000_000);
        let (start, n) = plan(bounds, now, 0, 60, 16_000_000).unwrap();
        assert!(start >= now * 25_000_000);
        assert!(start + (n - 1) * 16_000_000 > bounds.1);
        assert!(start + (n - 1) * 16_000_000 + 4_000_000 <= (now + 60) * 25_000_000);
        let (old, k) = plan(bounds, now, 20, 0, 16_000_000).unwrap();
        assert!(old >= (now - 20) * 25_000_000);
        assert!(old + (k - 1) * 16_000_000 + 4_000_000 <= bounds.1 - 25_000_000);
    }
    #[test]
    fn worker_and_sampling_budgets_are_validated() {
        let mut c = config();
        assert_eq!(stride_samples(&c).unwrap(), 16_000_000);
        c.worker_count = 0;
        assert!(stride_samples(&c).is_err());
        c.worker_count = 1;
        c.stride_s = 0.3;
        assert!(stride_samples(&c).is_err());
        c.stride_s = 0.64;
        c.writer_grace_s = f64::NAN;
        assert!(stride_samples(&c).is_err());
    }
    #[test]
    fn atomic_checkpoints_keep_empty_and_nonempty_candidates_with_coverage() {
        let c = config();
        let path = std::env::temp_dir().join(format!(
            "iono-search-{}-{}.h5",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        save(&path, &c, &[], 1, 1, [100, 100, 3], &[]).unwrap();
        {
            let h = File::open(&path).unwrap();
            assert_eq!(h.dataset("frequency_hz").unwrap().size(), 0);
            assert_eq!(
                h.attr("missing_blocks")
                    .unwrap()
                    .read_scalar::<u64>()
                    .unwrap(),
                1
            );
        }
        let row = (
            100,
            Candidate {
                frequency_hz: 4e6,
                ipp_us: 10000,
                phase_s: 0.002,
                contrast: 3.,
                mean_power: 1.,
                effective_pulses: 8.,
            },
        );
        save(
            &path,
            &c,
            &[row],
            2,
            1,
            [100, 200, 3],
            &[Checked {
                sample: 100,
                coarse_hz: 4e6,
                result: None,
            }],
        )
        .unwrap();
        {
            let h = File::open(&path).unwrap();
            assert_eq!(
                h.dataset("frequency_hz")
                    .unwrap()
                    .read_raw::<f64>()
                    .unwrap(),
                vec![4e6]
            );
            assert_eq!(
                h.attr("last_scanned_sample_index")
                    .unwrap()
                    .read_scalar::<u64>()
                    .unwrap(),
                200
            );
        }
        fs::remove_file(path).unwrap();
    }
    #[test]
    fn code_check_reuses_real_overlapped_rf_with_correct_carrier_and_delay() {
        let code = iono_core::digisonde_match::template(4_000_000, 25e6, 10000, 3);
        let raw: Vec<_> = (0..4_002_000)
            .map(|j| {
                let sample = (j as i64 - 1000).rem_euclid(4_000_000) as usize;
                code[(sample + 4_000_000 - 4250) % 4_000_000]
                    * Complex32::from_polar(
                        1.,
                        (std::f64::consts::TAU * -8.5e6 * (j as f64 - 1000.) / 25e6) as f32,
                    )
            })
            .collect();
        let mut decoder = CodeSearch::with_ipps(16000, 100e3, &[5000, 10000, 20000, 40000]);
        let m = verify_frame(&mut decoder, &raw, 4e6).unwrap().unwrap();
        assert_eq!((m.ipp_us, m.mode), (10000, 3), "{m:?}");
        assert!(
            m.carrier_offset_hz.abs() < 10. && m.coherence > 0.5,
            "{m:?}"
        );
        assert!((m.lag_s - 0.00017).abs() < 0.00003, "{m:?}");
        assert!(verify_frame(&mut decoder, &raw[..1000], 4e6).is_err());
    }
}

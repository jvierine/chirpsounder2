//! Independent receiver-local PRN worker. Existing UHD acquisition is untouched.
use anyhow::{ensure, Context, Result};
use hdf5::File;
use iono_core::{
    prn::{self, Channelizer, Decoder},
    simd, C,
};
use rdigital_rf::{CacheOptions, Reader};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Deserialize, Serialize, PartialEq)]
struct Code {
    seed: u32,
    length: usize,
    pulse_length: isize,
    ipp: usize,
    bandwidth_hz: f64,
    #[serde(default)]
    control_seeds: Vec<u32>,
}
#[derive(Deserialize, Serialize)]
struct Config {
    station: String,
    transmitter: String,
    data_dir: String,
    output_dir: String,
    channel: String,
    frequencies_hz: Vec<f64>,
    code_indices: Vec<usize>,
    codes: Vec<Code>,
    dwell_s: f64,
    period_s: f64,
    #[serde(default)]
    start_offset_s: f64,
    #[serde(default)]
    range_shift_samples: i64,
    range_max_km: f64,
    #[serde(default)]
    range_min_km: f64,
    #[serde(default = "matched_estimator")]
    estimator: String,
    #[serde(default = "default_solver")]
    least_squares_solver: String,
    #[serde(default)]
    configuration_reference: String,
    #[serde(default = "threshold")]
    snr_threshold_db: f32,
    #[serde(default)]
    retain_candidate_snr: bool,
    #[serde(default = "writer_grace")]
    writer_grace_s: f64,
}
fn matched_estimator() -> String {
    "matched-filter".into()
}
fn default_solver() -> String {
    "cached-inverse".into()
}
fn first_lag(c: &Config) -> usize {
    if c.range_min_km == 0. {
        0
    } else {
        ((c.range_min_km * 1000. / C * 100000.).ceil() as i64 + c.range_shift_samples).max(0)
            as usize
    }
}
fn writer_grace() -> f64 {
    0.25
}
fn threshold() -> f32 {
    8.
}
fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs_f64()
}
fn wait(t: f64) {
    if t > now() {
        thread::sleep(Duration::from_secs_f64((t - now()).max(0.)));
    }
}
fn epoch_sample(t: f64) -> Result<u64> {
    ensure!(t.is_finite() && t > 0., "invalid epoch");
    (t.floor() as u64)
        .checked_mul(25_000_000)
        .and_then(|n| n.checked_add((t.fract() * 25e6).round() as u64))
        .context("epoch overflow")
        .and_then(|i| iono_core::receiver_timing::corrected_sample_index(i, 25_000_000))
}
fn text(h: &File, n: &str, s: &str) -> Result<()> {
    h.new_dataset::<hdf5::types::VarLenUnicode>()
        .create(n)?
        .write_scalar(&s.parse::<hdf5::types::VarLenUnicode>()?)?;
    Ok(())
}
fn scalar(h: &File, n: &str, v: f64) -> Result<()> {
    h.new_dataset::<f64>().create(n)?.write_scalar(&v)?;
    Ok(())
}
fn validate(c: &Config) -> Result<()> {
    ensure!(
        c.snr_threshold_db.is_finite() && (0. ..=60.).contains(&c.snr_threshold_db),
        "invalid SNR threshold"
    );
    ensure!(
        !c.station.is_empty()
            && c.station.len() <= 64
            && c.station
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_'),
        "invalid station"
    );
    ensure!(
        !c.transmitter.is_empty()
            && c.transmitter
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_'),
        "invalid transmitter name"
    );
    ensure!(
        !c.frequencies_hz.is_empty()
            && c.frequencies_hz.len() <= 1000
            && c.frequencies_hz.len() == c.code_indices.len(),
        "invalid frequency table"
    );
    ensure!(
        c.frequencies_hz.windows(2).all(|v| v[0] < v[1])
            && c.frequencies_hz.iter().all(|f| f.is_finite() && *f > 0.),
        "frequencies must increase"
    );
    ensure!(
        c.dwell_s.is_finite()
            && c.dwell_s > 0.
            && c.dwell_s <= 10.
            && c.period_s.is_finite()
            && c.period_s >= c.dwell_s * c.frequencies_hz.len() as f64,
        "invalid schedule"
    );
    ensure!(
        c.start_offset_s.is_finite() && c.start_offset_s >= 0. && c.start_offset_s < c.period_s,
        "invalid schedule offset"
    );
    ensure!(
        !c.codes.is_empty()
            && c.codes.len() <= 8
            && c.code_indices.iter().all(|i| *i < c.codes.len()),
        "invalid code selection"
    );
    ensure!(
        c.range_max_km.is_finite()
            && c.range_min_km.is_finite()
            && c.range_min_km >= 0.
            && c.range_min_km < c.range_max_km
            && ["matched-filter", "least-squares"].contains(&c.estimator.as_str())
            && ["cached-inverse", "factor"].contains(&c.least_squares_solver.as_str())
            && c.range_max_km > 100.
            && c.range_max_km <= 10000.
            && c.range_shift_samples.abs() < 1000,
        "invalid range geometry"
    );
    for code in &c.codes {
        ensure!(
            code.control_seeds.len() <= 4 && !code.control_seeds.contains(&code.seed),
            "invalid comparison codes"
        );
        let periods = c.dwell_s * 100000. / code.length as f64;
        ensure!(
            periods >= 4. && periods <= 256. && (periods - periods.round()).abs() < 1e-6,
            "dwell must contain 4..256 complete code periods"
        );
        ensure!(
            code.length <= 20000 && code.bandwidth_hz <= 100000.,
            "unsupported waveform"
        );
        prn::code(code.seed, code.length, code.pulse_length, code.ipp)?;
    }
    Ok(())
}
fn reader(c: &Config) -> Result<Reader> {
    let mut r = Reader::open([&c.data_dir])?;
    ensure!(
        r.properties(&c.channel)?.sample_rate_numerator == 25_000_000,
        "ring buffer must be 25 MHz"
    );
    r.set_cache_options(CacheOptions {
        minimum_bytes: 1024 * 1024,
        max_open_files: 2,
    })?;
    r.set_cache_byte_limit(16 * 1024 * 1024)?;
    Ok(r)
}
fn make_decoders(c: &Config, index: usize, first: usize, nr: usize) -> Result<Vec<Decoder>> {
    let code = &c.codes[index];
    std::iter::once(code.seed)
        .chain(code.control_seeds.iter().copied())
        .enumerate()
        .map(|(which, seed)| {
            let waveform = prn::code(seed, code.length, code.pulse_length, code.ipp)?;
            if c.estimator == "least-squares" && which == 0 {
                if c.least_squares_solver == "factor" {
                    Decoder::new_matrix_factor(&waveform, first, nr)
                } else {
                    Decoder::new_matrix(&waveform, first, nr)
                }
            } else {
                Decoder::new_window(&waveform, first, nr)
            }
        })
        .collect()
}
fn column(
    c: &Config,
    r: &mut Reader,
    epoch: f64,
    f: usize,
    bank: &mut BTreeMap<usize, Vec<Decoder>>,
) -> Result<(Vec<f32>, f32)> {
    let code = &c.codes[c.code_indices[f]];
    let frequency = c.frequencies_hz[f];
    // Do not label aliases outside the actual digitized RF band as measurements.
    ensure!(
        frequency - code.bandwidth_hz / 2. > 0. && frequency + code.bandwidth_hz / 2. < 25e6,
        "frequency outside recorded RF band"
    );
    let channel = Channelizer::new(frequency - 12.5e6, code.bandwidth_hz)?;
    let decoders = bank
        .get_mut(&c.code_indices[f])
        .context("missing cached decoder bank")?;
    for d in decoders.iter_mut() {
        d.reset();
    }
    let base = epoch_sample(epoch + f as f64 * c.dwell_s)?;
    let n = code.length * 250;
    let periods = (c.dwell_s * 100000. / code.length as f64).round() as usize;
    for p in 0..periods {
        // The Doppler estimator discards these transition periods. Do not read
        // padding into a future, still-open recorder file just to discard it.
        if p == 0 || p + 1 == periods {
            for decoder in decoders.iter_mut() {
                decoder.push_zero()?;
            }
            continue;
        }
        let start = base
            .checked_add((p * n) as u64)
            .and_then(|n| n.checked_sub(1000))
            .context("read index overflow")?;
        let mut raw = None;
        let mut last = None;
        for _ in 0..3 {
            match r.read_complex32(start, n + 2000, &c.channel, 0) {
                Ok(v) => {
                    raw = Some(v);
                    break;
                }
                Err(e) => last = Some(e),
            }
            thread::sleep(Duration::from_millis(150));
        }
        let raw = raw.ok_or_else(|| anyhow::anyhow!("missing voltage at {start}: {:?}", last))?;
        let z = channel.process(&raw, p as i64 * n as i64 - 1000);
        Decoder::push_shared(&z[4..4 + code.length], decoders)?;
    }
    let (snr, peak) = decoders[0].finish_with_score()?;
    let mut control_peak = 0f32;
    for control in &decoders[1..] {
        control_peak = control_peak.max(control.finish_with_score()?.1);
    }
    let margin = if control_peak > 0. && peak > 0. {
        10. * (peak / control_peak).log10()
    } else {
        f32::NAN
    };
    Ok((snr, margin))
}
fn product_path(c: &Config, epoch: f64) -> Result<PathBuf> {
    let day = chrono::DateTime::from_timestamp(epoch as i64, 0)
        .context("invalid timestamp")?
        .format("%Y-%m-%d")
        .to_string();
    Ok(Path::new(&c.output_dir).join(day).join(format!(
        "prn_ionogram-{}-{}-{}.h5",
        c.transmitter, c.station, epoch as u64
    )))
}
fn resume(
    c: &Config,
    epoch: f64,
    nr: usize,
    snr: &mut [f32],
    complete: &mut [u8],
    margins: &mut [f32],
) -> Result<()> {
    let path = product_path(c, epoch)?;
    if !path.exists() {
        return Ok(());
    }
    let h = File::open(&path)?;
    ensure!(
        iono_core::receiver_timing::metadata_compatible(&h),
        "existing observation has another fixed receiver correction; preserving it"
    );
    let previous: Config = serde_json::from_str(&iono_core::text(&h, "configuration_json", ""))?;
    ensure!(
        previous.station == c.station
            && previous.transmitter == c.transmitter
            && previous.codes == c.codes
            && previous.frequencies_hz == c.frequencies_hz
            && previous.code_indices == c.code_indices
            && previous.dwell_s == c.dwell_s
            && previous.period_s == c.period_s
            && previous.start_offset_s == c.start_offset_s
            && previous.range_shift_samples == c.range_shift_samples
            && previous.range_max_km == c.range_max_km,
        "existing sweep has a different signal/timing configuration; preserving it"
    );
    ensure!(
        previous.range_min_km == c.range_min_km
            && previous.estimator == c.estimator
            && previous.least_squares_solver == c.least_squares_solver,
        "existing sweep has a different estimator/range window; preserving it"
    );
    ensure!(
        h.dataset("ranges")?.size() == nr,
        "existing range geometry differs"
    );
    if h.link_exists("candidate_snr") {
        let values = h.dataset("candidate_snr")?.read_raw::<f32>()?;
        ensure!(
            values.len() == snr.len(),
            "existing diagnostic shape differs"
        );
        snr.copy_from_slice(&values);
    } else {
        let ids = h.dataset("SNR_sparse_indices")?.read_raw::<u32>()?;
        let values = h.dataset("SNR_sparse_values")?.read_raw::<f32>()?;
        ensure!(
            ids.len() == values.len() && ids.windows(2).all(|p| p[0] < p[1]),
            "invalid existing sparse entries"
        );
        for (&i, &value) in ids.iter().zip(&values) {
            ensure!(
                (i as usize) < snr.len(),
                "existing sparse index outside shape"
            );
            snr[i as usize] = value;
        }
    }
    let old_complete = h.dataset("frequency_complete")?.read_raw::<u8>()?;
    let old_margins = h.dataset("code_identity_margin_db")?.read_raw::<f32>()?;
    ensure!(
        old_complete.len() == complete.len() && old_margins.len() == margins.len(),
        "existing frequency geometry differs"
    );
    complete.copy_from_slice(&old_complete);
    margins.copy_from_slice(&old_margins);
    Ok(())
}
fn save(
    c: &Config,
    epoch: f64,
    nr: usize,
    snr: &[f32],
    completed: &[u8],
    margins: &[f32],
) -> Result<PathBuf> {
    let path = product_path(c, epoch)?;
    fs::create_dir_all(path.parent().unwrap())?;
    let tmp = path.with_extension("partial.h5");
    {
        let h = File::create(&tmp)?;
        iono_core::receiver_timing::write_metadata(&h)?;
        let mut indices = vec![];
        let mut values = vec![];
        let cutoff = 10f32.powf(c.snr_threshold_db / 10.);
        for (i, &v) in snr.iter().enumerate() {
            let margin = margins[i / nr];
            if v.is_finite() && v >= cutoff && margin.is_finite() && margin >= 3. {
                indices.push(i as u32);
                values.push(v);
            }
        }
        h.new_dataset_builder()
            .with_data(&indices)
            .create("SNR_sparse_indices")?;
        h.new_dataset_builder()
            .with_data(&values)
            .create("SNR_sparse_values")?;
        scalar(&h, "snr_threshold_db", c.snr_threshold_db as f64)?;
        let peaks: Vec<_> = snr
            .chunks_exact(nr)
            .map(|v| {
                v.iter()
                    .copied()
                    .filter(|v| v.is_finite())
                    .reduce(f32::max)
                    .unwrap_or(f32::NAN)
            })
            .collect();
        h.new_dataset_builder()
            .with_data(&peaks)
            .create("candidate_peak_snr")?;
        if c.retain_candidate_snr {
            // Commissioning evidence only; the public ionogram is sparse.
            h.new_dataset::<f32>()
                .shape((c.frequencies_hz.len(), nr))
                .chunk((1, nr))
                .deflate(3)
                .create("candidate_snr")?
                .write_raw(snr)?;
        }
        h.new_dataset_builder()
            .with_data(&c.frequencies_hz)
            .create("freqs")?;
        let ranges: Vec<_> = (0..nr)
            .map(|r| (r as i64 + first_lag(c) as i64 - c.range_shift_samples) as f64 * C / 100000.)
            .collect();
        h.new_dataset_builder()
            .with_data(&ranges)
            .create("ranges")?;
        h.new_dataset_builder()
            .with_data(completed)
            .create("frequency_complete")?;
        let times: Vec<_> = (0..completed.len())
            .map(|f| epoch + f as f64 * c.dwell_s)
            .collect();
        h.new_dataset_builder()
            .with_data(margins)
            .create("code_identity_margin_db")?;
        scalar(&h, "code_identity_required_db", 3.)?;
        h.new_dataset_builder()
            .with_data(&times)
            .create("frequency_time_unix")?;
        scalar(&h, "t0", epoch)?;
        scalar(&h, "range_offset_applied", 1.)?;
        scalar(&h, "range_shift_samples", c.range_shift_samples as f64)?;
        scalar(&h, "output_sample_rate_hz", 100000.)?;
        scalar(&h, "last_observation", now())?;
        text(&h, "station_name", &c.station)?;
        text(&h, "txname", &c.transmitter)?;
        text(
            &h,
            "decoder",
            if c.estimator == "least-squares" {
                "complex64-fft-inverse-normal-matrix-least-squares-doppler-v1"
            } else {
                "complex64-fft-matched-filter-doppler-v1"
            },
        )?;
        text(&h, "least_squares_solver", &c.least_squares_solver)?;
        text(&h,"snr_reference","per-Doppler exponential-power median across delay; interference and code sidelobes included")?;
        text(&h, "configuration_reference", &c.configuration_reference)?;
        text(
            &h,
            "timing_quality",
            "configured transmit schedule; absolute group delay not calibrated",
        )?;
        text(&h, "configuration_json", &serde_json::to_string(c)?)?;
        h.flush()?;
    }
    fs::rename(&tmp, &path)?;
    Ok(path)
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    let c: Config = serde_json::from_slice(&fs::read(
        args.get(1)
            .context("usage: iono-prn CONFIG [--once] [--start EPOCH]")?,
    )?)?;
    iono_core::receiver_timing::initialize(&c.station)?;
    validate(&c)?;
    ensure!(
        c.writer_grace_s.is_finite() && (0.25..=15.).contains(&c.writer_grace_s),
        "writer grace must be 0.25--15 seconds"
    );
    if args.iter().any(|s| s == "--ring-status") {
        let inspect = |indices: &[u64]| -> Result<Vec<bool>> {
            let mut r = reader(&c)?;
            Ok(indices
                .iter()
                .map(|&i| r.read_complex32(i, 2_502_000, &c.channel, 0).is_ok())
                .collect())
        };
        let bounds = reader(&c)?.bounds(&c.channel)?.context("empty ring")?;
        let clock = now();
        let second = clock.floor() as u64;
        let indices: Vec<_> = (1..=8)
            .map(|lag| (second - lag) * 25_000_000 + 2_500_000)
            .collect();
        let initial = inspect(&indices)?;
        thread::sleep(Duration::from_secs(3));
        let retried = inspect(&indices)?;
        println!(
            "{}",
            serde_json::json!({"station":c.station,"clock_unix":clock,"newest_sample_age_s":clock-bounds.1 as f64/25e6,"sample_lags_s":[1,2,3,4,5,6,7,8],"present_initial":initial,"present_after_3s_fresh_reader":retried})
        );
        return Ok(());
    }
    let once = args.iter().any(|a| a == "--once");
    let explicit = args
        .iter()
        .position(|a| a == "--start")
        .map(|i| {
            args.get(i + 1)
                .context("start epoch")?
                .parse::<f64>()
                .context("start epoch")
        })
        .transpose()?;
    let first = first_lag(&c);
    let end = ((c.range_max_km * 1000. / C * 100000.).ceil() as i64 + c.range_shift_samples + 1)
        .max(2) as usize;
    ensure!(end > first + 1, "empty range window");
    let nr = end - first;
    ensure!(
        c.codes.iter().all(|s| end <= s.length),
        "range exceeds unambiguous code period"
    );
    let bytes: usize = c.codes.iter().map(|_| nr * nr * 8).sum();
    ensure!(
        c.estimator != "least-squares" || (nr <= 2048 && bytes <= 160 * 1024 * 1024),
        "matrix bank exceeds bounded memory budget"
    );
    let mut bank = BTreeMap::new();
    for index in 0..c.codes.len() {
        bank.insert(index, make_decoders(&c, index, first, nr)?);
    }
    eprintln!(
        "{} -> {} PRN: {} complex64, {} frequencies, period {} s",
        c.transmitter,
        c.station,
        simd::backend(),
        c.frequencies_hz.len(),
        c.period_s
    );
    let mut epoch = explicit.unwrap_or_else(|| {
        c.start_offset_s + ((now() - c.start_offset_s) / c.period_s).floor() * c.period_s
    });
    let mut capacity = iono_core::processing_health::Monitor::new(
        &format!("prn-{}", c.transmitter),
        explicit.is_none() && !once,
    );
    loop {
        let mut snr = vec![f32::NAN; nr * c.frequencies_hz.len()];
        let mut completed = vec![0u8; c.frequencies_hz.len()];
        let mut margins = vec![f32::NAN; c.frequencies_hz.len()];
        if let Err(error) = resume(&c, epoch, nr, &mut snr, &mut completed, &mut margins) {
            if once {
                return Err(error);
            }
            eprintln!(
                "{} preserving existing sweep {epoch}: {error:#}",
                c.transmitter
            );
            epoch += c.period_s;
            wait(epoch + 0.25);
            continue;
        }
        let mut r = None;
        for f in 0..c.frequencies_hz.len() {
            if completed[f] != 0 {
                continue;
            }
            let end = epoch + (f + 1) as f64 * c.dwell_s;
            if explicit.is_none() && now() - end > c.writer_grace_s + 8. {
                capacity.missed(0, 1);
                continue;
            }
            wait(end.ceil() + c.writer_grace_s);
            let timer = Instant::now();
            let cpu = iono_core::processing_health::Sample::capture();
            let result = (|| {
                if r.is_none() {
                    r = Some(reader(&c)?);
                }
                column(&c, r.as_mut().unwrap(), epoch, f, &mut bank)
            })();
            capacity.finished(cpu, timer.elapsed().as_secs_f64(), c.dwell_s);
            match result {
                Ok((v, margin)) => {
                    snr[f * nr..(f + 1) * nr].copy_from_slice(&v);
                    completed[f] = 1;
                    margins[f] = margin;
                }
                Err(e) => {
                    eprintln!("{} epoch {epoch} f{f} skipped: {e:#}", c.transmitter);
                    r = None;
                }
            }
            if completed[f] != 0 && (f % 8 == 0 || f + 1 == c.frequencies_hz.len()) {
                eprintln!(
                    "{} {:.2} MHz: {:.3}s compute; code margin {:.1} dB; {}/{} complete; {}",
                    c.transmitter,
                    c.frequencies_hz[f] / 1e6,
                    timer.elapsed().as_secs_f64(),
                    margins[f],
                    completed.iter().filter(|&&v| v != 0).count(),
                    completed.len(),
                    save(&c, epoch, nr, &snr, &completed, &margins)?.display()
                );
            }
        }
        if completed.iter().any(|&v| v != 0) {
            save(&c, epoch, nr, &snr, &completed, &margins)?;
        }
        if once {
            ensure!(
                completed.iter().any(|&v| v != 0),
                "no complete voltage dwells"
            );
            break;
        }
        epoch += c.period_s;
        if now() > epoch + c.period_s {
            let next =
                c.start_offset_s + ((now() - c.start_offset_s) / c.period_s).floor() * c.period_s;
            capacity.missed(((next - epoch) / c.period_s) as u64, 0);
            epoch = next;
        }
        wait(epoch + 0.25);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sparse_output_thresholds_and_discriminates_codes_without_losing_diagnostics() {
        let root = std::env::temp_dir().join(format!("iono-prn-output-{}", std::process::id()));
        let mut c:Config=serde_json::from_value(serde_json::json!({
            "station":"DOB","transmitter":"Kiruna","data_dir":"unused","output_dir":root,
            "channel":"ch0","frequencies_hz":[2e6,3e6],"code_indices":[0,0],
            "codes":[{"seed":4,"length":10000,"pulse_length":10,"ipp":500,"bandwidth_hz":100000.,"control_seeds":[2]}],
            "dwell_s":1.,"period_s":120.,"start_offset_s":60.,"range_max_km":4500.,"snr_threshold_db":13.,"retain_candidate_snr":true
        })).unwrap();
        c.station = "MTB".into();
        validate(&c).unwrap();
        c.station = "../outside".into();
        assert!(validate(&c).is_err());
        c.station = "DOB".into();
        let path = save(
            &c,
            1700000000.,
            2,
            &[10., 100., 100., 100.],
            &[1, 1],
            &[8., -1.],
        )
        .unwrap();
        let h = File::open(&path).unwrap();
        assert!(!h.link_exists("SNR"));
        assert_eq!(
            h.dataset("SNR_sparse_indices")
                .unwrap()
                .read_raw::<u32>()
                .unwrap(),
            [1]
        );
        assert_eq!(
            h.dataset("candidate_snr")
                .unwrap()
                .read_raw::<f32>()
                .unwrap(),
            [10., 100., 100., 100.]
        );
        drop(h);
        let mut resumed = vec![f32::NAN; 4];
        let mut done = vec![0u8; 2];
        let mut margins = vec![f32::NAN; 2];
        resume(&c, 1700000000., 2, &mut resumed, &mut done, &mut margins).unwrap();
        assert_eq!(resumed, [10., 100., 100., 100.]);
        assert_eq!(done, [1, 1]);
        assert_eq!(margins, [8., -1.]);
        let g = iono_core::read_products(&path).unwrap();
        assert_eq!(g[0].values.iter().filter(|v| v.is_finite()).count(), 1);
        c.retain_candidate_snr = false;
        let path = save(&c, 1700000000., 2, &[1.; 4], &[1, 1], &[0., 0.]).unwrap();
        let h = File::open(&path).unwrap();
        assert_eq!(h.dataset("SNR_sparse_indices").unwrap().size(), 0);
        assert!(!h.link_exists("candidate_snr"));
        drop(h);
        resumed.fill(f32::NAN);
        resume(&c, 1700000000., 2, &mut resumed, &mut done, &mut margins).unwrap();
        assert!(resumed.iter().all(|v| v.is_nan()));
        assert!(iono_core::read_products(&path).unwrap()[0]
            .values
            .iter()
            .all(|v| v.is_nan()));
        fs::remove_dir_all(root).unwrap();
    }
}

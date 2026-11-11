//! Receiver-local SGO VERSO commissioning worker. No UHD access or uploads of
//! voltage; the existing catalogue worker publishes sparse HDF5 products.
#[path = "../verso_program.rs"]
mod program;
use anyhow::{ensure, Context, Result};
use hdf5::File;
#[cfg(test)]
use iono_core::prn::Channelizer;
use iono_core::{
    pulsed::{resample_code, PulseDecoder},
    simd, C,
};
use program::{scalar, text, Group, Program, BAUD_SAMPLES, RATE, SITES};
use rdigital_rf::{CacheOptions, Reader};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
// Copy contents without chmod/reflink operations unsupported on some station data disks.
fn copy_product(source: &Path, destination: &Path) -> Result<()> {
    let mut input = fs::File::open(source)?;
    let mut output = fs::File::create(destination)?;
    std::io::copy(&mut input, &mut output)?;
    output.sync_all()?;
    Ok(())
}
const DECODER: &str = "complex64-fft-matched-pulse-doppler-verso-v2";
const INVERSE_DECODER: &str = "complex64-finite-regularized-inverse-verso-v3";
#[derive(Serialize, Deserialize)]
struct Config {
    station: String,
    data_dir: String,
    output_dir: String,
    channel: String,
    program_file: String,
    #[serde(default = "range_max")]
    range_max_km: f64,
    #[serde(default)]
    range_min_km: f64,
    #[serde(default = "threshold")]
    snr_threshold_db: f32,
    #[serde(default)]
    sites: Vec<String>,
    #[serde(default = "writer_grace")]
    writer_grace_s: f64,
    #[serde(default)]
    inverse_regularization: f32,
}
fn decoder(c: &Config) -> &'static str {
    if c.inverse_regularization > 0. {
        INVERSE_DECODER
    } else {
        DECODER
    }
}
fn writer_grace() -> f64 {
    0.25
}
fn signal_config(c: &Config) -> Result<serde_json::Value> {
    let mut v = serde_json::to_value(c)?;
    v.as_object_mut().unwrap().remove("writer_grace_s");
    Ok(v)
}
fn range_max() -> f64 {
    4500.
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
    ensure!(t.is_finite() && t >= 0., "invalid epoch");
    (t.floor() as u64)
        .checked_mul(25_000_000)
        .and_then(|s| s.checked_add((t.fract() * 25e6).round() as u64))
        .context("epoch overflow")
        .and_then(|i| iono_core::receiver_timing::corrected_sample_index(i, 25_000_000))
}
fn reader(c: &Config) -> Result<Reader> {
    let mut r = Reader::open([&c.data_dir])?;
    ensure!(
        r.properties(&c.channel)?.sample_rate_numerator == 25_000_000,
        "requires 25 MHz ring"
    );
    r.set_cache_options(CacheOptions {
        minimum_bytes: 1024 * 1024,
        max_open_files: 2,
    })?;
    r.set_cache_byte_limit(16 * 1024 * 1024)?;
    Ok(r)
}
struct Observation {
    site: u8,
    epoch: f64,
    frequency: Vec<f64>,
    times: Vec<f64>,
    snr: Vec<f32>,
    complete: Vec<u8>,
    margin: Vec<f32>,
    nr: usize,
}
fn product_path(c: &Config, site: u8, epoch: f64) -> Result<PathBuf> {
    let day = chrono::DateTime::from_timestamp(epoch as i64, 0)
        .context("invalid epoch")?
        .format("%Y-%m-%d")
        .to_string();
    Ok(Path::new(&c.output_dir).join(day).join(format!(
        "prn_ionogram-{}-{}-{}.h5",
        SITES[site as usize], c.station, epoch as u64
    )))
}
impl Observation {
    fn path(&self, c: &Config) -> Result<PathBuf> {
        product_path(c, self.site, self.epoch)
    }
    fn new(c: &Config, p: &Program, site: u8, cycle: f64, nr: usize) -> Result<Self> {
        let groups: Vec<_> = p.groups.iter().filter(|g| g.site == site).collect();
        let epoch = cycle
            + groups.first().context("site has no groups")?.start_samples as f64 / RATE as f64;
        let mut pairs: Vec<_> = groups
            .iter()
            .flat_map(|g| {
                g.frequencies.iter().enumerate().map(move |(i, &f)| {
                    (
                        f,
                        cycle + (g.start_samples + i as u64 * BAUD_SAMPLES) as f64 / RATE as f64,
                    )
                })
            })
            .collect();
        pairs.sort_by(|a, b| a.0.total_cmp(&b.0));
        ensure!(
            pairs.windows(2).all(|v| v[0].0 < v[1].0),
            "duplicate RF channels"
        );
        let n = pairs.len();
        let mut o = Self {
            site,
            epoch,
            frequency: pairs.iter().map(|p| p.0).collect(),
            times: pairs.iter().map(|p| p.1).collect(),
            snr: vec![f32::NAN; n * nr],
            complete: vec![0; n],
            margin: vec![f32::NAN; n],
            nr,
        };
        let path = o.path(c)?;
        if path.exists() {
            let h = File::open(&path)?;
            ensure!(
                iono_core::receiver_timing::metadata_compatible(&h),
                "existing observation has another fixed receiver correction; preserving it"
            );
            ensure!(
                signal_config(&serde_json::from_str::<Config>(&iono_core::text(
                    &h,
                    "configuration_json",
                    ""
                ))?)?
                    == signal_config(c)?
                    && iono_core::text(&h, "program_sha256", "") == p.sha256,
                "retained observation uses a different configuration; preserve it"
            );
            // A processing change cannot inherit prior code-validation decisions.
            if iono_core::text(&h, "decoder", "") != decoder(c) {
                ensure!(
                    iono_core::text(&h, "decoder", "")
                        == "complex64-fft-matched-pulse-doppler-verso-v1",
                    "unknown retained processing version; preserve it"
                );
                let backup = Path::new(&c.output_dir)
                    .join("verso_v1_backup")
                    .join(path.file_name().unwrap());
                fs::create_dir_all(backup.parent().unwrap())?;
                if !backup.exists() || backup.metadata()?.len() == 0 {
                    copy_product(&path, &backup)?;
                }
                return Ok(o);
            }
            let old = h.dataset("frequency_complete")?.read_raw::<u8>()?;
            let margin = h.dataset("code_identity_margin_db")?.read_raw::<f32>()?;
            ensure!(
                old.len() == n && margin.len() == n,
                "retained axes disagree"
            );
            let indices = h.dataset("SNR_sparse_indices")?.read_raw::<u32>()?;
            let values = h.dataset("SNR_sparse_values")?.read_raw::<f32>()?;
            ensure!(
                indices.len() == values.len(),
                "retained sparse axes disagree"
            );
            for (&i, &v) in indices.iter().zip(&values) {
                ensure!((i as usize) < o.snr.len(), "invalid retained sparse cell");
                o.snr[i as usize] = v;
            }
            o.complete = old;
            o.margin = margin;
        }
        Ok(o)
    }
    fn save(&self, c: &Config, p: &Program, cycle: f64) -> Result<PathBuf> {
        let path = self.path(c)?;
        fs::create_dir_all(path.parent().unwrap())?;
        let tmp = path.with_extension("h5.partial");
        let h = File::create(&tmp)?;
        iono_core::receiver_timing::write_metadata(&h)?;
        let mut indices = vec![];
        let mut values = vec![];
        let cutoff = 10f32.powf(c.snr_threshold_db / 10.);
        for (i, &v) in self.snr.iter().enumerate() {
            if v.is_finite() && v >= cutoff && self.margin[i / self.nr] >= 3. {
                indices.push(i as u32);
                values.push(v);
            }
        }
        macro_rules! put {
            ($name:expr,$v:expr) => {
                h.new_dataset_builder().with_data(&$v).create($name)?;
            };
        }
        put!("SNR_sparse_indices", indices);
        put!("SNR_sparse_values", values);
        put!("freqs", self.frequency);
        let ranges: Vec<_> = (0..self.nr)
            .map(|r| c.range_min_km * 1000. + r as f64 * C / 100000.)
            .collect();
        put!("ranges", ranges);
        put!("frequency_complete", self.complete);
        put!("frequency_time_unix", self.times);
        put!("code_identity_margin_db", self.margin);
        let peaks: Vec<_> = self
            .snr
            .chunks_exact(self.nr)
            .map(|v| {
                v.iter()
                    .copied()
                    .filter(|x| x.is_finite())
                    .reduce(f32::max)
                    .unwrap_or(f32::NAN)
            })
            .collect();
        put!("candidate_peak_snr", peaks);
        scalar(&h, "code_identity_required_db", 3.)?;
        scalar(&h, "snr_threshold_db", c.snr_threshold_db as f64)?;
        scalar(&h, "t0", self.epoch)?;
        scalar(&h, "program_cycle_epoch", cycle)?;
        scalar(&h, "range_offset_applied", 1.)?;
        scalar(&h, "output_sample_rate_hz", 100000.)?;
        scalar(&h, "last_observation", now())?;
        scalar(
            &h,
            "matched_filter_sidelobes_present",
            if c.inverse_regularization > 0. {
                0.
            } else {
                1.
            },
        )?;
        scalar(&h, "residual_compression_sidelobes_present", 1.)?;
        scalar(
            &h,
            "inverse_regularization_peak_fraction",
            c.inverse_regularization as f64,
        )?;
        text(
            &h,
            "pulse_compression",
            if c.inverse_regularization > 0. {
                "finite two-sided Tikhonov inverse; real guard samples; matched-filter code-identity control"
            } else {
                "matched filter"
            },
        )?;
        text(&h, "station_name", &c.station)?;
        text(&h, "txname", SITES[self.site as usize])?;
        text(&h, "decoder", decoder(c))?;
        text(&h, "configuration_json", &serde_json::to_string(c)?)?;
        text(&h, "program_sha256", &p.sha256)?;
        text(&h,"timing_quality","supplied SGO TDM schedule; on-air assignment and absolute group delay require verification")?;
        text(&h,"snr_reference","pulse-Doppler range median / ln(2); residual compression sidelobes and interference are included")?;
        text(
            &h,
            "code_control",
            "conjugate time-reversed resampled reference; identical energy and Fourier magnitude",
        )?;
        h.flush()?;
        drop(h);
        fs::rename(tmp, &path)?;
        Ok(path)
    }
}
fn column(
    c: &Config,
    r: &mut Reader,
    cycle_sample: u64,
    g: &Group,
    slot: usize,
    code: &[num_complex::Complex32],
    control: &[num_complex::Complex32],
    nr: usize,
) -> Result<(Vec<f32>, f32)> {
    let f = g.frequencies[slot];
    let mut target = if c.inverse_regularization > 0. {
        PulseDecoder::regularized(code, nr, c.inverse_regularization)?
    } else {
        PulseDecoder::new(code, nr)?
    };
    let mut identity = if c.inverse_regularization > 0. {
        Some(PulseDecoder::new(code, nr)?)
    } else {
        None
    };
    let mut check = PulseDecoder::new(control, nr)?;
    let guard = target.input_prefix();
    let count = g.duration_samples / g.ipp_samples;
    let shift = (c.range_min_km * 1000. / C * 25e6).round() as i64;
    for pulse in 0..count {
        let native = g.start_samples + slot as u64 * BAUD_SAMPLES + pulse * g.ipp_samples;
        let relative = (native * 400) as i64 + shift - 1000 - (guard * 250) as i64;
        let start = cycle_sample
            .checked_add_signed(relative)
            .context("pulse epoch overflow")?;
        let n = target.input_len() * 250 + 2000;
        let z = iono_receiver::frontend::read_window(
            r,
            start,
            n,
            &c.channel,
            relative,
            f - 12.5e6,
            62500.,
        )?;
        target.push(&z[4..4 + target.input_len()])?;
        check.push(&z[4 + guard..4 + guard + check.input_len()])?;
        if let Some(d) = identity.as_mut() {
            d.push(&z[4 + guard..4 + guard + d.input_len()])?;
        }
    }
    let (snr, mut peak) = target.finish()?;
    if let Some(d) = identity {
        peak = d.finish()?.1;
    }
    let (_, negative) = check.finish()?;
    let margin = if peak > 0. && negative > 0. {
        10. * (peak / negative).log10()
    } else {
        f32::NAN
    };
    Ok((snr, margin))
}
/// Same-voltage on-air comparison; writes only bounded reduced diagnostics.
fn compare_recent(c: &Config, p: &Program, path: &Path) -> Result<()> {
    let mut r = reader(c)?;
    let current = 235. + ((now() - 235.) / 240.).floor() * 240.;
    let (cycle, g) = [current, current - 240.]
        .into_iter()
        .flat_map(|cycle| p.groups.iter().map(move |g| (cycle, g)))
        .filter(|(cycle, g)| {
            let end = cycle + (g.start_samples + g.duration_samples) as f64 / RATE as f64;
            g.site < 4 && end + c.writer_grace_s < now() && end > now() - 20.
        })
        .max_by(|(a, g), (b, h)| {
            (a + (g.start_samples + g.duration_samples) as f64 / RATE as f64)
                .total_cmp(&(b + (h.start_samples + h.duration_samples) as f64 / RATE as f64))
        })
        .context("no recently complete VERSO group; retry in a few seconds")?;
    let code = resample_code(&p.code, RATE, 100000)?;
    let control: Vec<_> = code.iter().rev().map(|z| z.conj()).collect();
    let nr = ((c.range_max_km - c.range_min_km) * 1000. / C * 100000.).ceil() as usize + 1;
    let lambda = if c.inverse_regularization > 0. {
        c.inverse_regularization
    } else {
        0.01
    };
    let mut inverse = PulseDecoder::regularized(&code, nr, lambda)?;
    let mut matched = PulseDecoder::new(&code, nr)?;
    let mut check = PulseDecoder::new(&control, nr)?;
    let guard = inverse.input_prefix();
    let slot = g.frequencies.len() / 2;
    let frequency = g.frequencies[slot];
    let cycle_sample = epoch_sample(cycle)?;
    let shift = (c.range_min_km * 1000. / C * 25e6).round() as i64;
    let started = Instant::now();
    for pulse in 0..g.duration_samples / g.ipp_samples {
        let native = g.start_samples + slot as u64 * BAUD_SAMPLES + pulse * g.ipp_samples;
        let relative = (native * 400) as i64 + shift - 1000 - (guard * 250) as i64;
        let z = iono_receiver::frontend::read_window(
            &mut r,
            cycle_sample
                .checked_add_signed(relative)
                .context("invalid comparison epoch")?,
            inverse.input_len() * 250 + 2000,
            &c.channel,
            relative,
            frequency - 12.5e6,
            62500.,
        )?;
        inverse.push(&z[4..4 + inverse.input_len()])?;
        matched.push(&z[4 + guard..4 + guard + matched.input_len()])?;
        check.push(&z[4 + guard..4 + guard + check.input_len()])?;
    }
    let (old, peak) = matched.finish()?;
    let (new, _) = inverse.finish()?;
    let (_, negative) = check.finish()?;
    let h = File::create(path)?;
    scalar(&h, "epoch", cycle + g.start_samples as f64 / RATE as f64)?;
    scalar(&h, "frequency_hz", frequency)?;
    scalar(&h, "regularization", lambda as f64)?;
    scalar(&h, "white_noise_penalty_db", inverse.noise_gain_db() as f64)?;
    scalar(
        &h,
        "code_identity_margin_db",
        (10. * (peak / negative).log10()) as f64,
    )?;
    text(&h, "receiver", &c.station)?;
    text(&h, "transmitter", SITES[g.site as usize])?;
    text(
        &h,
        "comparison",
        "same recorded pulse windows; matched-filter code-identity gate unchanged",
    )?;
    h.new_dataset_builder()
        .with_data(&old)
        .create("matched_snr")?;
    h.new_dataset_builder()
        .with_data(&new)
        .create("inverse_snr")?;
    h.new_dataset_builder()
        .with_data(
            &(0..nr)
                .map(|i| c.range_min_km + i as f64 * C / 100000. / 1000.)
                .collect::<Vec<_>>(),
        )
        .create("range_km")?;
    println!(
        "{} {} {:.4} MHz: code margin {:.2} dB, {} pulses, {:.2}s compare compute",
        c.station,
        SITES[g.site as usize],
        frequency / 1e6,
        10. * (peak / negative).log10(),
        g.duration_samples / g.ipp_samples,
        started.elapsed().as_secs_f64()
    );
    Ok(())
}
fn run(
    c: &Config,
    p: &Program,
    cycle: f64,
    ready_only: bool,
    capacity: &mut iono_core::processing_health::Monitor,
) -> Result<()> {
    let mut r = reader(c)?;
    let bounds = r.bounds(&c.channel)?.context("no recorded voltage")?;
    let nr = ((c.range_max_km - c.range_min_km) * 1000. / C * 100000.).ceil() as usize + 1;
    let code = resample_code(&p.code, RATE, 100000)?;
    // Reverse AFTER rational resampling. Reversing native bauds and then
    // resampling changes the sub-sample phase and can give a CW carrier more
    // than 20 dB unequal gain in the two filters near spectral nulls.
    let control: Vec<_> = code.iter().rev().map(|z| z.conj()).collect();
    let cycle_sample = epoch_sample(cycle)?;
    let mut observations: Vec<_> = (0..4u8)
        .filter(|&s| c.sites.is_empty() || c.sites.iter().any(|n| n == SITES[s as usize]))
        .map(|s| Observation::new(c, p, s, cycle, nr))
        .collect::<Result<_>>()?;
    for (gi, g) in p.groups.iter().enumerate() {
        if g.site == 255 {
            continue;
        }
        let Some(o) = observations.iter_mut().find(|o| o.site == g.site) else {
            continue;
        };
        let start = cycle + g.start_samples as f64 / RATE as f64;
        let end = start + g.duration_samples as f64 / RATE as f64 + c.range_max_km * 1000. / C;
        if ready_only && end.ceil() + c.writer_grace_s > now() {
            continue;
        }
        if epoch_sample(start + c.range_min_km * 1000. / C)?.saturating_sub(1000) < bounds.0 {
            if !ready_only {
                capacity.missed(0, g.frequencies.len() as u64);
            }
            eprintln!(
                "{} group {gi}: voltage no longer retained",
                SITES[g.site as usize]
            );
            continue;
        }
        wait(end.ceil() + c.writer_grace_s);
        let timer = Instant::now();
        let cpu = iono_core::processing_health::Sample::capture();
        let mut accepted = 0;
        for (slot, &frequency) in g.frequencies.iter().enumerate() {
            let j = o
                .frequency
                .binary_search_by(|x| x.total_cmp(&frequency))
                .unwrap();
            if o.complete[j] != 0 {
                continue;
            }
            match column(c, &mut r, cycle_sample, g, slot, &code, &control, nr) {
                Ok((v, margin)) => {
                    let peak = v.iter().copied().fold(0., f32::max);
                    eprintln!(
                        "{} {} {:.6} MHz peak {:.1} dB code margin {:.1} dB",
                        c.station,
                        SITES[g.site as usize],
                        frequency / 1e6,
                        10. * peak.log10(),
                        margin
                    );
                    if margin >= 3. {
                        accepted += 1;
                    }
                    o.snr[j * nr..(j + 1) * nr].copy_from_slice(&v);
                    o.margin[j] = margin;
                    o.complete[j] = 1;
                }
                Err(e) => {
                    eprintln!(
                        "{} {} {:.6} MHz missing voltage: {e:#}",
                        c.station,
                        SITES[g.site as usize],
                        frequency / 1e6
                    );
                    r = reader(c)?;
                }
            }
        }
        capacity.finished(
            cpu,
            timer.elapsed().as_secs_f64(),
            g.duration_samples as f64 / RATE as f64,
        );
        if o.complete.iter().any(|x| *x != 0) {
            let path = o.save(c, p, cycle)?;
            eprintln!(
                "{} group {gi}: {accepted}/{} code checks, {:.2}s compute, {}",
                SITES[g.site as usize],
                g.frequencies.len(),
                timer.elapsed().as_secs_f64(),
                path.display()
            );
        }
    }
    Ok(())
}
fn main() -> Result<()> {
    let a: Vec<_> = std::env::args().collect();
    if a.get(1).is_some_and(|s| s == "import") {
        Program::import(
            Path::new(a.get(2).context("YAML file")?),
            Path::new(a.get(3).context("complex128 waveform file")?),
            Path::new(a.get(4).context("output HDF5")?),
        )?;
        return Ok(());
    }
    let c: Config = serde_json::from_slice(&fs::read(
        a.get(1)
            .context("usage: iono-verso CONFIG [--once] [--ready-only] [--start UNIX_SECONDS]")?,
    )?)?;
    iono_core::receiver_timing::initialize(&c.station)?;
    ensure!(
        !c.station.is_empty()
            && c.station
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
            && c.range_min_km.is_finite()
            && c.range_max_km.is_finite()
            && c.range_min_km >= -1000.
            && c.range_max_km > c.range_min_km
            && c.range_max_km <= 10000.
            && (0. ..=60.).contains(&c.snr_threshold_db),
        "invalid station/range/threshold"
    );
    ensure!(
        c.sites.iter().all(|s| SITES.contains(&s.as_str())),
        "unknown site selection"
    );
    ensure!(
        c.writer_grace_s.is_finite() && (0.25..=6.).contains(&c.writer_grace_s),
        "writer grace must be 0.25--6 seconds"
    );
    let p = Program::read(Path::new(&c.program_file))?;
    if let Some(i) = a.iter().position(|s| s == "--compare-filter") {
        return compare_recent(
            &c,
            &p,
            Path::new(a.get(i + 1).context("comparison HDF5 path")?),
        );
    }
    ensure!(
        c.inverse_regularization == 0.
            || (c.inverse_regularization.is_finite()
                && (0.0001..=1.).contains(&c.inverse_regularization)),
        "inverse regularization must be zero or 0.0001..1 times peak reference power"
    );
    if let Some(i) = a.iter().position(|s| s == "--invalidate-v1-since") {
        let since: f64 = a
            .get(i + 1)
            .context("first commissioning cycle epoch")?
            .parse()?;
        ensure!(
            since.is_finite() && since >= now() - 86400. && since <= now(),
            "repair is limited to the last day"
        );
        let first = 235. + ((since - 235.) / 240.).floor() * 240.;
        let last = 235. + ((now() - 235.) / 240.).floor() * 240.;
        let mut changed = 0;
        for n in 0..=((last - first) / 240.) as usize {
            let cycle = first + n as f64 * 240.;
            for site in 0..4u8 {
                let epoch = cycle
                    + p.groups
                        .iter()
                        .find(|g| g.site == site)
                        .unwrap()
                        .start_samples as f64
                        / RATE as f64;
                let path = product_path(&c, site, epoch)?;
                if !path.exists() {
                    continue;
                }
                let legacy = {
                    let h = File::open(&path)?;
                    iono_core::text(&h, "decoder", "")
                        == "complex64-fft-matched-pulse-doppler-verso-v1"
                        && iono_core::text(&h, "program_sha256", "") == p.sha256
                        && !h.link_exists("superseded_validation")
                };
                if !legacy {
                    continue;
                }
                let backup = Path::new(&c.output_dir)
                    .join("verso_v1_backup")
                    .join(path.file_name().unwrap());
                fs::create_dir_all(backup.parent().unwrap())?;
                if !backup.exists() || backup.metadata()?.len() == 0 {
                    copy_product(&path, &backup)?;
                }
                let tmp = path.with_extension("correction.h5.partial");
                copy_product(&path, &tmp)?;
                let h = File::open_rw(&tmp)?;
                h.unlink("SNR_sparse_indices")?;
                h.unlink("SNR_sparse_values")?;
                h.new_dataset_builder()
                    .with_data(&Vec::<u32>::new())
                    .create("SNR_sparse_indices")?;
                h.new_dataset_builder()
                    .with_data(&Vec::<f32>::new())
                    .create("SNR_sparse_values")?;
                let count = h.dataset("code_identity_margin_db")?.size();
                h.dataset("code_identity_margin_db")?
                    .write_raw(&vec![f32::NAN; count])?;
                text(&h,"superseded_validation","Initial rational-resampling control was superseded by an exactly equal-spectrum reversed reference; original HDF5 retained locally")?;
                h.flush()?;
                drop(h);
                fs::rename(tmp, path)?;
                changed += 1;
            }
        }
        eprintln!(
            "Retained original files and invalidated {changed} preliminary code-validation results"
        );
        return Ok(());
    }
    let explicit = a
        .iter()
        .position(|s| s == "--start")
        .map(|i| {
            a.get(i + 1)
                .context("start epoch")?
                .parse::<f64>()
                .context("invalid start epoch")
        })
        .transpose()?;
    let mut cycle = explicit.unwrap_or_else(|| 235. + ((now() - 235.) / 240.).floor() * 240.);
    let once = a.iter().any(|s| s == "--once");
    let ready = a.iter().any(|s| s == "--ready-only");
    eprintln!(
        "{} VERSO: complex64 {}, 4-minute TDM schedule, 10 ms frequency offsets, program {}",
        c.station,
        simd::backend(),
        p.sha256
    );
    let mut capacity =
        iono_core::processing_health::Monitor::new("verso", explicit.is_none() && !once && !ready);
    loop {
        if let Err(e) = run(&c, &p, cycle, ready, &mut capacity) {
            if once {
                return Err(e);
            }
            eprintln!("VERSO cycle {cycle} failed: {e:#}");
        }
        if once {
            return Ok(());
        }
        cycle += 240.;
        if cycle + 240. < now() {
            let next = 235. + ((now() - 235.) / 240.).floor() * 240.;
            let sites = if c.sites.is_empty() { 4 } else { c.sites.len() };
            capacity.missed(((next - cycle) / 240.) as u64 * sites as u64, 0);
            cycle = next;
        }
        wait(cycle + 0.1);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use num_complex::Complex32 as Z;
    #[test]
    fn supplied_waveform_survives_real_channelization_and_discriminates_control() {
        let b = include_bytes!("../../../../deploy/verso/SGO_bauds_complex128.bin");
        let native: Vec<_> = b
            .chunks_exact(16)
            .map(|v| {
                Z::new(
                    f64::from_le_bytes(v[..8].try_into().unwrap()) as f32,
                    f64::from_le_bytes(v[8..].try_into().unwrap()) as f32,
                )
            })
            .collect();
        let code = resample_code(&native, 62500, 100000).unwrap();
        let control: Vec<_> = code.iter().rev().map(|z| z.conj()).collect();
        let mut d = PulseDecoder::new(&code, 1000).unwrap();
        let mut ctl = PulseDecoder::new(&control, 1000).unwrap();
        let delay = 217usize;
        let offset = 3208500. - 12.5e6;
        let raw: Vec<_> = (0..d.input_len() * 250 + 2000)
            .map(|i| {
                let t = i as i64 - 1000;
                let j = t - delay as i64 * 250;
                let noise = Z::new(
                    ((i * 17) % 37) as f32 * 0.0001,
                    ((i * 31) % 43) as f32 * 0.0001,
                );
                if j >= 0 && (j as usize) < native.len() * 400 {
                    noise
                        + native[j as usize / 400]
                            * Z::from_polar(
                                1.,
                                (std::f64::consts::TAU * offset * t as f64 / 25e6) as f32,
                            )
                } else {
                    noise
                }
            })
            .collect();
        let z = Channelizer::new(offset, 62500.)
            .unwrap()
            .process(&raw, -1000);
        for pulse in 0..16 {
            let phase = Z::from_polar(1., pulse as f32 * 0.2);
            let x: Vec<_> = z[4..4 + d.input_len()].iter().map(|v| *v * phase).collect();
            d.push(&x).unwrap();
            ctl.push(&x).unwrap();
        }
        let (s, peak) = d.finish().unwrap();
        let (_, negative) = ctl.finish().unwrap();
        let r = s
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .unwrap()
            .0;
        assert!(r.abs_diff(delay) <= 1, "range moved: {r}");
        assert!(
            10. * (peak / negative).log10() > 6.,
            "code control did not discriminate"
        );
    }
    #[test]
    fn narrow_carrier_cannot_gain_a_false_code_margin_at_resampling_spectral_nulls() {
        let b = include_bytes!("../../../../deploy/verso/SGO_bauds_complex128.bin");
        let native: Vec<_> = b
            .chunks_exact(16)
            .map(|v| Z::new(f64::from_le_bytes(v[..8].try_into().unwrap()) as f32, 0.))
            .collect();
        let code = resample_code(&native, 62500, 100000).unwrap();
        let control: Vec<_> = code.iter().rev().map(|z| z.conj()).collect();
        for f in [3500., 17300., 18068., 27500.] {
            let mut d = PulseDecoder::new(&code, 1502).unwrap();
            let mut ctl = PulseDecoder::new(&control, 1502).unwrap();
            let input: Vec<_> = (0..d.input_len())
                .map(|i| {
                    Z::from_polar(
                        100.,
                        (std::f64::consts::TAU * f * i as f64 / 100000.) as f32,
                    )
                })
                .collect();
            for _ in 0..16 {
                d.push(&input).unwrap();
                ctl.push(&input).unwrap();
            }
            let a = d.finish().unwrap().1;
            let b = ctl.finish().unwrap().1;
            assert!(
                (10. * (a / b).log10()).abs() < 0.2,
                "CW at {f} gained a false margin"
            );
        }
    }
}

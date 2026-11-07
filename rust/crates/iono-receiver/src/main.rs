//! Station signal processor; UHD acquisition remains in the existing recorder.
mod detection;
mod lfm;
mod radar;
use anyhow::{ensure, Context, Result};
use hdf5::File;
use iono_core::{dsp::Digisonde, median, prn::Channelizer, C};
use num_complex::Complex32;
use rdigital_rf::{CacheOptions, Reader};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
#[derive(Clone, Deserialize, Serialize)]
struct Config {
    station: String,
    data_dir: String,
    output_dir: String,
    #[serde(default = "channel")]
    channel: String,
    #[serde(default = "rate")]
    sample_rate: f64,
    #[serde(default = "center")]
    center_freq: f64,
    #[serde(default)]
    transmit_delay_us: Option<f64>,
    #[serde(default)]
    range_reference: Option<String>,
    #[serde(default)]
    snr_threshold_db: Option<f64>,
    #[serde(default)]
    rfi_clip_ratio: Option<f32>,
    #[serde(default = "transmitter")]
    transmitter: String,
    #[serde(default = "freq_start")]
    freq_start_hz: f64,
    #[serde(default = "freq_stop")]
    freq_stop_hz: f64,
    #[serde(default = "freq_step")]
    freq_step_hz: f64,
    #[serde(default = "nipp")]
    n_ipp: usize,
    #[serde(default = "ipp")]
    ipp_us: usize,
    /// Time spent searching/settling between transmitted frequency dwells.
    #[serde(default)]
    inter_frequency_gap_s: f64,
    /// Optional bounded search around the programmed RF carrier.
    #[serde(default)]
    carrier_search_hz: f64,
    #[serde(default)]
    polarization_interleaved: bool,
    /// Physical transmitter PRI; ipp_us may be the effective per-carrier PRI.
    #[serde(default)]
    physical_ipp_us: Option<usize>,
    /// Commissioning: pulses on a fine frequency before multiplexing away.
    /// None preserves the established constant effective-PRI decoder.
    #[serde(default)]
    multiplex_block_pulses: Option<usize>,
    /// Transmitted sequence phase at the read origin; does not shift its UTC.
    #[serde(default)]
    multiplex_phase_pulses: Option<usize>,
    #[serde(default)]
    start_offset_s: f64,
    #[serde(default = "hours")]
    sounding_hours: [f64; 2],
    #[serde(default = "offset")]
    offset_us: f64,
    #[serde(default = "interval")]
    interval_s: f64,
    #[serde(default)]
    upload_url: Option<String>,
    #[serde(default)]
    radar_sources: Vec<radar::Source>,
    #[serde(default = "writer_grace")]
    writer_grace_s: f64,
    /// Zero preserves full integration; otherwise stratified FFT averages/bin.
    #[serde(default)]
    spectrum_windows_per_bin: usize,
    #[serde(default = "lfm_min")]
    lfm_range_min_km: f64,
    #[serde(default = "lfm_max")]
    lfm_range_max_km: f64,
    #[serde(default = "detector_samples")]
    detector_samples_per_block: usize,
    #[serde(default = "detector_workers")]
    detector_worker_count: usize,
}
fn detector_workers() -> usize {
    2
}
fn detector_samples() -> usize {
    5_000_000
}
fn lfm_min() -> f64 {
    200.
}
fn lfm_max() -> f64 {
    1500.
}
fn writer_grace() -> f64 {
    0.15
}
fn channel() -> String {
    "ch0".into()
}
fn rate() -> f64 {
    25e6
}
fn center() -> f64 {
    12.5e6
}
fn transmitter() -> String {
    "Ramfjordmoen".into()
}
fn freq_start() -> f64 {
    1e6
}
fn freq_stop() -> f64 {
    18e6
}
fn freq_step() -> f64 {
    50e3
}
fn nipp() -> usize {
    64
}
fn ipp() -> usize {
    10000
}
fn hours() -> [f64; 2] {
    [0., 24.]
}
impl Config {
    fn delay(&self) -> f64 {
        self.transmit_delay_us
            .unwrap_or(if self.transmitter == "Ramfjordmoen" {
                4688.
            } else {
                0.
            })
    }
    fn sounding_active(&self, t: f64) -> bool {
        let h = t.rem_euclid(86400.) / 3600.;
        let [a, b] = self.sounding_hours;
        if a < b {
            h >= a && h < b
        } else {
            h >= a || h < b
        }
    }
}

fn offset() -> f64 {
    -330.
}
fn interval() -> f64 {
    450.
}
fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs_f64()
}
fn epoch_sample(t: f64, sr: u64) -> Result<u64> {
    ensure!(t.is_finite() && t >= 0., "invalid epoch");
    let seconds = t.floor() as u64;
    seconds
        .checked_mul(sr)
        .and_then(|n| n.checked_add((t.fract() * sr as f64).round() as u64))
        .context("epoch sample overflow")
        .and_then(|i| iono_core::receiver_timing::corrected_sample_index(i, sr))
}
fn wait(until: f64) {
    let d = until - now();
    if d > 0. {
        thread::sleep(Duration::from_secs_f64(d))
    }
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
#[derive(Debug)]
struct InputExpired;
impl std::fmt::Display for InputExpired {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "processing fell behind the retained voltage window")
    }
}
impl std::error::Error for InputExpired {}
fn read_live(r: &mut Reader, c: &Config, start: u64, n: usize) -> Result<Vec<Complex32>> {
    wait(((start + n as u64) as f64 / c.sample_rate).ceil() + c.writer_grace_s);
    let mut error = None;
    for _ in 0..8 {
        match r.read_complex32(start, n, &c.channel, 0) {
            Ok(z) => return Ok(z),
            Err(e) => error = Some(e),
        }
        thread::sleep(Duration::from_millis(250));
    }
    // Inspect fresh RAM-ring metadata only after a failed read. A gap inside
    // retained voltage remains an acquisition/input error, not a CPU loss.
    if reader(c)
        .ok()
        .and_then(|mut r| r.bounds(&c.channel).ok().flatten())
        .is_some_and(|bounds| start < bounds.0)
    {
        return Err(InputExpired.into());
    }
    Err(error.unwrap())
}
fn dataset(h: &File, name: &str, value: f64) -> Result<()> {
    h.new_dataset::<f64>().create(name)?.write_scalar(&value)?;
    Ok(())
}
fn text(h: &File, name: &str, value: &str) -> Result<()> {
    let v: hdf5::types::VarLenUnicode = value.parse()?;
    h.new_dataset::<hdf5::types::VarLenUnicode>()
        .create(name)?
        .write_scalar(&v)?;
    Ok(())
}
fn output(c: &Config, stem: &str, t: f64) -> Result<PathBuf> {
    let day = chrono::DateTime::from_timestamp(t as i64, 0)
        .context("invalid time")?
        .format("%Y-%m-%d")
        .to_string();
    let dir = PathBuf::from(&c.output_dir).join(day);
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join(format!("{stem}-{}.h5", t as u64)))
}
fn publish(c: &Config, tmp: &Path, path: &Path) -> Result<()> {
    std::fs::rename(tmp, path)?;
    if let Some(url) = &c.upload_url {
        let status = Command::new("curl")
            .args([
                "--fail",
                "--silent",
                "--show-error",
                "--max-time",
                "90",
                "--retry",
                "2",
                "-F",
            ])
            .arg(format!("file=@{}", path.display()))
            .arg(url)
            .status()?;
        ensure!(
            status.success(),
            "upload failed; product retained at {}",
            path.display()
        )
    }
    println!("published {}", path.display());
    Ok(())
}
fn multiplex_pulse_index(pulse: usize, block: usize, fine_frequencies: usize) -> usize {
    (pulse / block) * (block * fine_frequencies) + pulse % block
}
fn digisonde_frequency_count(c: &Config) -> usize {
    // Preserve legacy stop-exclusive grids, including a partial final step.
    // Tolerance prevents decimal MHz arithmetic dropping an exact full bin.
    (((c.freq_stop_hz - c.freq_start_hz) / c.freq_step_hz) + 1e-9).floor() as usize
}
fn digisonde_frequency_sample(c: &Config, t: f64, frequency: usize) -> Result<u64> {
    let dwell = (c.sample_rate
        * (c.n_ipp as f64 * c.ipp_us as f64 * 1e-6 + c.inter_frequency_gap_s))
        .round() as usize;
    let shift = ((c.offset_us + c.delay()) * c.sample_rate / 1e6).round() as i64;
    epoch_sample(t, c.sample_rate as u64)?
        .checked_add((frequency * dwell) as u64)
        .and_then(|v| v.checked_add_signed(shift))
        .context("sounding sample overflow")
}
fn validate_digisonde(c: &Config) -> Result<()> {
    ensure!(
        c.inter_frequency_gap_s.is_finite() && (0. ..=1.).contains(&c.inter_frequency_gap_s),
        "invalid inter-frequency gap"
    );
    ensure!(
        c.carrier_search_hz.is_finite() && (0. ..=30000.).contains(&c.carrier_search_hz),
        "invalid RF carrier search extent"
    );
    ensure!(
        c.carrier_search_hz == 0. || c.n_ipp * c.ipp_us >= 160000,
        "carrier search requires at least 160 ms of RF"
    );
    ensure!(
        [5000, 10000, 20000, 40000].contains(&c.ipp_us),
        "unsupported effective Digisonde PRI"
    );
    let physical = c.physical_ipp_us.unwrap_or(c.ipp_us);
    ensure!(
        [5000, 10000, 20000, 40000].contains(&physical)
            && c.ipp_us % physical == 0
            && c.ipp_us / physical <= 4,
        "invalid physical/effective PRI relationship"
    );
    ensure!(
        (4..=128).contains(&c.n_ipp) && c.n_ipp % 4 == 0,
        "Digisonde pulse count must be a multiple of four, up to 128"
    );
    if let Some(block) = c.multiplex_block_pulses {
        ensure!(
            block > 0 && block <= c.n_ipp && c.n_ipp % block == 0 && c.physical_ipp_us.is_some(),
            "invalid fine-frequency multiplex block"
        );
    }
    if let Some(phase) = c.multiplex_phase_pulses {
        ensure!(phase <= 256, "invalid physical code phase");
        let block = c
            .multiplex_block_pulses
            .context("code phase requires fine-frequency multiplexing")?;
        ensure!(
            iono_core::digisonde_match::multiplex_quartets(
                c.n_ipp * c.ipp_us / physical,
                block,
                c.ipp_us / physical,
                phase,
                c.polarization_interleaved
            )
            .len()
                >= 2,
            "read window contains fewer than two complete code quartets"
        );
    }
    ensure!(
        c.freq_start_hz.is_finite()
            && c.freq_stop_hz.is_finite()
            && c.freq_step_hz.is_finite()
            && c.freq_start_hz >= 0.5e6
            && c.freq_stop_hz <= 24.5e6
            && c.freq_stop_hz > c.freq_start_hz
            && c.freq_step_hz >= 1000.,
        "invalid Digisonde RF grid"
    );
    let nf = digisonde_frequency_count(c);
    ensure!(
        (1..2000).contains(&nf),
        "Digisonde frequency extent is empty or too large"
    );
    let n = (c.sample_rate * c.ipp_us as f64 * 1e-6).round() as usize;
    ensure!(
        n.checked_mul(c.n_ipp).is_some_and(|v| v <= 16_000_000),
        "Digisonde dwell exceeds the 128 MB complex64 voltage budget"
    );
    Ok(())
}
fn digisonde(
    c: &Config,
    t: f64,
    capacity: &mut iono_core::processing_health::Monitor,
) -> Result<()> {
    validate_digisonde(c)?;
    let mut r = reader(c)?;
    let physical_ipp = c.physical_ipp_us.unwrap_or(c.ipp_us);
    let decoding_ipp = if c.multiplex_block_pulses.is_some() {
        physical_ipp
    } else {
        c.ipp_us
    };
    let dsp = Digisonde::new(decoding_ipp);
    let (nf, nr, nipp) = (
        digisonde_frequency_count(c),
        c.physical_ipp_us.unwrap_or(c.ipp_us) / 10,
        c.n_ipp,
    );
    let decoding_nr = decoding_ipp / 10;
    let ipp_samples = (c.sample_rate * decoding_ipp as f64 * 1e-6).round() as usize;
    let raw_samples = nipp * (c.sample_rate * c.ipp_us as f64 * 1e-6).round() as usize;
    let frequency_duration = nipp as f64 * c.ipp_us as f64 * 1e-6 + c.inter_frequency_gap_s;
    let threshold = c
        .snr_threshold_db
        .map(|db| 10f32.powf(db as f32 / 10.))
        .unwrap_or(2.);
    ensure!(
        threshold.is_finite() && threshold > 0.,
        "invalid SNR threshold"
    );
    ensure!(
        c.rfi_clip_ratio.is_none_or(|v| v.is_finite() && v >= 2.),
        "invalid RFI clip ratio"
    );
    let mut power = vec![f32::NAN; 2 * nf * nr];
    let mut noise = vec![f32::NAN; 2 * nf];
    let mut actual_carriers = vec![0f64; nf];
    let mut search_coherence = vec![f32::NAN; nf];
    let mut carrier_decoder = if c.carrier_search_hz > 0. {
        Some(iono_core::digisonde_match::CodeSearch::with_orders(
            16000,
            100e3,
            &[physical_ipp as u32],
            &[(1, 1)],
            &[c.polarization_interleaved],
        ))
    } else {
        None
    };
    let pulse_plan: Vec<(usize, usize)> = if let Some(phase) = c.multiplex_phase_pulses {
        iono_core::digisonde_match::multiplex_quartets(
            raw_samples / ipp_samples,
            c.multiplex_block_pulses
                .context("code phase requires multiplex block")?,
            c.ipp_us / physical_ipp,
            phase,
            c.polarization_interleaved,
        )
        .into_iter()
        .flatten()
        .collect()
    } else {
        (0..nipp)
            .map(|pi| {
                (
                    c.multiplex_block_pulses.map_or(pi, |block| {
                        multiplex_pulse_index(pi, block, c.ipp_us / physical_ipp)
                    }),
                    iono_core::digisonde_match::logical_pulse_index(pi, c.polarization_interleaved),
                )
            })
            .collect()
    };
    ensure!(
        pulse_plan.len() >= 4,
        "read window contains no complete code quartet"
    );
    let start_time = now();
    for f in 0..nf {
        let freq = c.freq_start_hz + f as f64 * c.freq_step_hz;
        let base = digisonde_frequency_sample(c, t, f)?;
        let mut decoded = Vec::with_capacity(nipp);
        // One contiguous frequency read avoids repeated HDF5 hyperslab setup.
        // The voltage budget is 128 MB, including 128 physical 5 ms IPPs.
        let padding = if c.rfi_clip_ratio.is_some() || c.carrier_search_hz > 0. {
            1000
        } else {
            0
        };
        let raw = read_live(
            &mut r,
            c,
            base.checked_sub(padding as u64)
                .context("FIR overlap before sample zero")?,
            raw_samples + 2 * padding,
        )?;
        let cpu = iono_core::processing_health::Sample::capture();
        let processing = std::time::Instant::now();
        // Reuse oscillator and FIR coefficients for the whole frequency dwell.
        // The 30 kHz total FIR bandwidth matches the historical 0.006 cutoff.
        let mut carrier = freq;
        if let Some(decoder) = carrier_decoder.as_mut() {
            let baseband =
                Channelizer::new(freq - c.center_freq, 100e3)?.process(&raw[..4_002_000], -1000);
            if let Some(m) =
                decoder.seek_with_doppler(&baseband[4..16004], c.carrier_search_hz, 5000., 4)
            {
                search_coherence[f] = m.coherence;
                if m.coherence >= 0.01 {
                    carrier += 5000. * (m.carrier_offset_hz / 5000.).round();
                }
            }
        }
        actual_carriers[f] = carrier;
        let channelizer = Channelizer::new(carrier - c.center_freq, 30000.)?;
        for (ppi, logical) in pulse_plan.iter().copied() {
            let ipp_raw = &raw[ppi * ipp_samples..(ppi + 1) * ipp_samples + 2 * padding];
            let mut baseband =
                channelizer.process(ipp_raw, (ppi * ipp_samples) as i64 - padding as i64);
            if padding > 0 {
                baseband = baseband[4..4 + decoding_nr].to_vec();
            }
            decoded.push(dsp.decode_baseband_ipp(baseband, logical, c.rfi_clip_ratio));
        }
        if c.polarization_interleaved && c.multiplex_phase_pulses.is_none() {
            for group in decoded.chunks_exact_mut(4) {
                group.swap(1, 2);
            }
        }
        let p = dsp.power(&decoded);
        for mode in 0..2 {
            // Other interleaved carriers can produce peaks in later physical
            // PRI slots. Keep the selected physical range interval explicit.
            let floor = median(&p[mode][..nr]);
            noise[mode * nf + f] = floor;
            for k in 0..nr {
                let snr = (p[mode][k] - floor) / floor;
                power[(mode * nf + f) * nr + k] = if snr >= threshold { snr } else { f32::NAN }
            }
        }
        capacity.finished(cpu, processing.elapsed().as_secs_f64(), frequency_duration);
        if f % 20 == 0 {
            println!(
                "{} {} frequency {}/{} elapsed {:.2}s observation {:.2}s",
                c.station,
                c.transmitter,
                f,
                nf,
                now() - start_time,
                now() - (t + f as f64 * frequency_duration)
            );
        }
    }
    let path = output(
        c,
        &format!("digisonde_ionogram-{}-{}", c.transmitter, c.station),
        t,
    )?;
    let tmp = path.with_extension("h5.partial");
    {
        let h = File::create(&tmp)?;
        iono_core::receiver_timing::write_metadata(&h)?;
        iono_core::write_sparse_snr(&h, &[2, nf, nr], &power)?;
        h.new_dataset::<f32>()
            .shape([2, nf])
            .create("noise_floor")?
            .write_raw(&noise)?;
        let freq: Vec<_> = (0..nf)
            .map(|i| (c.freq_start_hz + i as f64 * c.freq_step_hz) as f32)
            .collect();
        let ranges: Vec<_> = (0..nr).map(|i| (i as f64 * C / 100000.) as f32).collect();
        h.new_dataset_builder().with_data(&freq).create("freqs")?;
        h.new_dataset_builder()
            .with_data(&ranges)
            .create("ranges")?;
        dataset(&h, "t0", t)?;
        dataset(&h, "offset_us", c.offset_us)?;
        dataset(&h, "transmit_delay_us", c.delay())?;
        dataset(&h, "sample_rate_hz", c.sample_rate)?;
        dataset(&h, "n_ipp", nipp as f64)?;
        dataset(&h, "decoded_pulses_per_frequency", pulse_plan.len() as f64)?;
        dataset(&h, "ipp_us", c.ipp_us as f64)?;
        dataset(&h, "effective_ipp_us", c.ipp_us as f64)?;
        if let Some(phase) = c.multiplex_phase_pulses {
            dataset(&h, "multiplex_phase_pulses", phase as f64)?;
        }
        if let Some(block) = c.multiplex_block_pulses {
            dataset(&h, "multiplex_block_pulses", block as f64)?;
            dataset(&h, "fine_frequency_count", (c.ipp_us / physical_ipp) as f64)?;
            text(&h,"multiplex_channel","first fine frequency only; other fine channels excluded before coherent integration")?;
        }
        dataset(
            &h,
            "physical_ipp_us",
            c.physical_ipp_us.unwrap_or(c.ipp_us) as f64,
        )?;
        dataset(&h, "frequency_duration_s", frequency_duration)?;
        dataset(&h, "inter_frequency_gap_s", c.inter_frequency_gap_s)?;
        dataset(
            &h,
            "polarization_interleaved",
            u8::from(c.polarization_interleaved) as f64,
        )?;
        if c.carrier_search_hz > 0. {
            dataset(&h, "carrier_search_hz", c.carrier_search_hz)?;
            h.new_dataset_builder()
                .with_data(&actual_carriers)
                .create("actual_carrier_hz")?;
            h.new_dataset_builder()
                .with_data(&search_coherence)
                .create("carrier_search_coherence")?;
            text(&h,"frequency_axis_reference","nominal search-channel centres; actual_carrier_hz records the received RF carriers")?;
        }
        if let Some(ratio) = c.rfi_clip_ratio {
            dataset(&h, "rfi_clip_amplitude_ratio", ratio as f64)?;
        }
        dataset(&h, "snr_threshold_db", 10. * (threshold as f64).log10())?;
        text(&h, "type", "digisonde")?;
        text(&h, "receiver", &c.station)?;
        text(&h, "transmitter", &c.transmitter)?;
        text(&h, "processor", "iono-receiver/0.1.0")?;
        text(
            &h,
            "range_reference",
            c.range_reference
                .as_deref()
                .unwrap_or("calibrated transmit epoch; one-way vacuum light time"),
        )?;
        h.flush()?;
    }
    publish(c, &tmp, &path)
}
fn spectrum(c: &Config, t: f64) -> Result<()> {
    let mut r = reader(c)?;
    let nfft = 8192;
    let nt = 120;
    let mut p = vec![0f32; nt * nfft];
    let fft = rustfft::FftPlanner::new().plan_fft_forward(nfft);
    let window: Vec<f32> = (0..nfft)
        .map(|i| (0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / nfft as f64).cos()) as f32)
        .collect();
    let per_bin = (c.sample_rate * 0.5) as usize;
    let windows = per_bin / nfft;
    let mut buf = vec![Complex32::new(0., 0.); nfft];
    for b in 0..nt {
        let start = epoch_sample(t, c.sample_rate as u64)? + b as u64 * per_bin as u64;
        let batches = spectrum_batches(windows, c.spectrum_windows_per_bin, b);
        let averaged = batches.iter().map(|(_, n)| *n).sum::<usize>();
        for (first, count) in batches {
            let raw = read_live(&mut r, c, start + (first * nfft) as u64, count * nfft)?;
            for z in raw.chunks_exact(nfft) {
                for k in 0..nfft {
                    buf[k] = z[k] * window[k]
                }
                fft.process(&mut buf);
                for k in 0..nfft {
                    p[b * nfft + (k + nfft / 2) % nfft] += buf[k].norm_sqr() / averaged as f32;
                }
            }
        }
    }
    let path = output(c, &format!("rf_spectrum-{}-ch0", c.station), t)?;
    let tmp = path.with_extension("h5.partial");
    {
        let h = File::create(&tmp)?;
        iono_core::receiver_timing::write_metadata(&h)?;
        h.new_dataset::<f32>()
            .shape([nt, nfft])
            .deflate(3)
            .create("power")?
            .write_raw(&p)?;
        let freq: Vec<_> = (0..nfft)
            .map(|i| c.center_freq + (i as f64 - nfft as f64 / 2.) * c.sample_rate / nfft as f64)
            .collect();
        let times: Vec<_> = (0..nt).map(|b| t + (b as f64 + 0.5) * 0.5).collect();
        h.new_dataset_builder()
            .with_data(&freq)
            .create("frequency_hz")?;
        h.new_dataset_builder()
            .with_data(&times)
            .create("time_unix")?;
        dataset(&h, "minute_start_unix", t)?;
        dataset(&h, "minute_end_unix", t + 60.)?;
        dataset(&h, "available_windows_per_half_second", windows as f64)?;
        dataset(
            &h,
            "averaged_windows_per_half_second",
            spectrum_batches(windows, c.spectrum_windows_per_bin, 0)
                .iter()
                .map(|(_, n)| *n)
                .sum::<usize>() as f64,
        )?;
        text(
            &h,
            "sampling_strategy",
            if c.spectrum_windows_per_bin > 0 && c.spectrum_windows_per_bin < windows {
                "stratified time blocks with deterministic within-stratum jitter; positive linear power averaged without background removal"
            } else {
                "full non-overlapping Hann-windowed FFT integration; no background removal"
            },
        )?;
        let station: hdf5::types::VarLenUnicode = c.station.parse()?;
        h.new_attr::<hdf5::types::VarLenUnicode>()
            .create("station")?
            .write_scalar(&station)?;
        text(&h, "processor", "iono-receiver/0.1.0")?;
        h.flush()?;
    }
    publish(c, &tmp, &path)
}
fn spectrum_batches(windows: usize, limit: usize, bin: usize) -> Vec<(usize, usize)> {
    if limit == 0 || limit >= windows {
        return (0..windows)
            .step_by(32)
            .map(|i| (i, 32.min(windows - i)))
            .collect();
    }
    let blocks = limit.div_ceil(32);
    let mut remaining = limit;
    (0..blocks)
        .map(|i| {
            let first = i * windows / blocks;
            let end = (i + 1) * windows / blocks;
            let count = 32.min(remaining).min(end - first);
            remaining -= count;
            let mix = (bin as u64)
                .wrapping_mul(0x9e3779b97f4a7c15)
                .wrapping_add(i as u64 * 131);
            let jitter = ((mix ^ (mix >> 31)) % ((end - first - count + 1) as u64)) as usize;
            (first + jitter, count)
        })
        .collect()
}
fn verify(path: &Path) -> Result<()> {
    let h = File::open(path)?;
    let re = h.dataset("raw_re")?.read_raw::<f32>()?;
    let im = h.dataset("raw_im")?.read_raw::<f32>()?;
    ensure!(
        re.len() == im.len() && re.len() % 250000 == 0,
        "invalid fixture"
    );
    let dsp = Digisonde::new(10000);
    let freq = iono_core::scalar(&h, "frequency_offset_hz", 0.);
    let mut decoded = Vec::new();
    for (pi, (a, b)) in re
        .chunks_exact(250000)
        .zip(im.chunks_exact(250000))
        .enumerate()
    {
        let raw: Vec<_> = a
            .iter()
            .zip(b)
            .map(|(&re, &im)| Complex32::new(re, im))
            .collect();
        decoded.push(dsp.decode_ipp(&raw, freq, 25e6, pi))
    }
    let p = dsp.power(&decoded);
    let out = File::create(path.with_extension("rust.h5"))?;
    out.new_dataset_builder()
        .with_data(&p[0])
        .create("power_o")?;
    out.new_dataset_builder()
        .with_data(&p[1])
        .create("power_x")?;
    let z: Vec<_> = decoded.iter().flatten().collect();
    out.new_dataset_builder()
        .with_data(&z.iter().map(|v| v.re).collect::<Vec<_>>())
        .create("decoded_re")?;
    out.new_dataset_builder()
        .with_data(&z.iter().map(|v| v.im).collect::<Vec<_>>())
        .create("decoded_im")?;
    Ok(())
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("verify-lfm") {
        return lfm::verify(Path::new(args.get(2).context("fixture path")?));
    }
    if args.get(1).map(String::as_str) == Some("verify") {
        return verify(Path::new(args.get(2).context("fixture path")?));
    }
    let task = args.get(1).map(String::as_str).unwrap_or("digisonde");
    let c: Config =
        serde_json::from_slice(&std::fs::read(args.get(2).context("configuration path")?)?)?;
    iono_core::receiver_timing::initialize(&c.station)?;
    let once = args.iter().any(|s| s == "--once");
    ensure!(
        c.writer_grace_s.is_finite() && (0.15..=10.).contains(&c.writer_grace_s),
        "invalid recorder writer grace"
    );
    let start = args
        .iter()
        .position(|s| s == "--start")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.parse::<f64>())
        .transpose()?;
    ensure!(
        (c.sample_rate - 25e6).abs() < 1.,
        "only the validated 25 MHz input configuration is supported"
    );
    if task == "check-config" {
        validate_digisonde(&c)?;
        ensure!(
            c.interval_s.is_finite()
                && c.interval_s > 0.
                && c.start_offset_s.is_finite()
                && c.offset_us.is_finite()
                && c.delay().is_finite()
                && c.sounding_hours
                    .iter()
                    .all(|h| h.is_finite() && (0. ..=24.).contains(h)),
            "invalid receiver schedule or timing"
        );
        ensure!(
            (1..=4).contains(&c.detector_worker_count)
                && (1_000_000..=8_388_608).contains(&c.detector_samples_per_block),
            "invalid detector capacity configuration"
        );
        println!("valid native receiver configuration for {}", c.station);
        return Ok(());
    }
    if task == "detect" {
        return detection::run(&c, once);
    }
    if task == "detect-bench" {
        return detection::benchmark(&c);
    }
    let interval = if task == "spectrum" || task == "lfm" {
        60.
    } else {
        c.interval_s
    };
    let schedule_offset = if task == "lfm" { 54. } else { c.start_offset_s };
    let mut t = start.unwrap_or_else(|| {
        let current = schedule_offset + ((now() - schedule_offset) / interval).floor() * interval;
        if task == "spectrum" {
            current - 60.
        } else if now() - current < 120. {
            current
        } else {
            current + interval
        }
    });
    let key = format!(
        "{}-{}",
        task,
        Path::new(&args[2])
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
    );
    let mut capacity = iono_core::processing_health::Monitor::new(&key, start.is_none() && !once);
    loop {
        let cpu = iono_core::processing_health::Sample::capture();
        let processing = std::time::Instant::now();
        let result = match task {
            "digisonde" if c.sounding_active(t) => digisonde(&c, t, &mut capacity),
            "digisonde" => Ok(()),
            "spectrum" => spectrum(&c, t),
            "lfm" => lfm::run(&c, t, &mut capacity),
            _ => anyhow::bail!("unknown task {task}"),
        };
        if task != "spectrum"
            && result
                .as_ref()
                .err()
                .is_some_and(|e| e.is::<InputExpired>())
        {
            capacity.missed(1, 0);
        }
        capacity.finished(cpu, processing.elapsed().as_secs_f64(), interval);
        if once {
            return result;
        }
        if let Err(e) = result {
            eprintln!("{task} skipped epoch {t}: {e:#}")
        };
        t += interval;
        if t + interval < now() {
            let next = schedule_offset + ((now() - schedule_offset) / interval).floor() * interval;
            if task != "spectrum" {
                let missed = (0..((next - t) / interval) as usize)
                    .filter(|n| task != "digisonde" || c.sounding_active(t + *n as f64 * interval))
                    .count();
                capacity.missed(missed as u64, 0);
            }
            t = next;
        }
        // Missing recorder metadata can fail before spectrum() reaches its
        // first read. Every task must still wait for the next scheduled epoch;
        // otherwise an outage spins through future minutes and never recovers.
        wait(t + 1.);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn frequency_search_pause_changes_step_time_but_not_pulse_integration() {
        let mut c:Config=serde_json::from_str(r#"{"station":"DOB","data_dir":"x","output_dir":"y","ipp_us":5000,"n_ipp":128,"inter_frequency_gap_s":0.26,"carrier_search_hz":25000}"#).unwrap();
        validate_digisonde(&c).unwrap();
        assert_eq!(
            digisonde_frequency_sample(&c, 1793995200., 80).unwrap()
                - digisonde_frequency_sample(&c, 1793995200., 0).unwrap(),
            1_800_000_000
        );
        assert_eq!(c.n_ipp * c.ipp_us, 640000);
        c.inter_frequency_gap_s = f64::NAN;
        assert!(validate_digisonde(&c).is_err());
    }
    #[test]
    fn legacy_juliusruh_partial_frequency_step_limits_remain_supported() {
        let mut c:Config=serde_json::from_str(r#"{"station":"DOB","data_dir":"x","output_dir":"y","transmitter":"Juliusruh","freq_start_hz":1010000,"freq_stop_hz":14000000,"freq_step_hz":50000,"n_ipp":16}"#).unwrap();
        validate_digisonde(&c).unwrap();
        assert_eq!(digisonde_frequency_count(&c), 259);
        c.freq_stop_hz = 10000000.;
        c.freq_step_hz = 25000.;
        validate_digisonde(&c).unwrap();
        assert_eq!(digisonde_frequency_count(&c), 359);
    }
    #[test]
    fn fine_frequency_selection_does_not_advance_the_sweep_twice_as_fast() {
        let mut c:Config=serde_json::from_str(r#"{"station":"DOB","data_dir":"x","output_dir":"y","transmitter":"DB049","ipp_us":20000,"physical_ipp_us":10000,"n_ipp":16,"offset_us":3535}"#).unwrap();
        for block in [1, 2, 4, 16] {
            c.multiplex_block_pulses = Some(block);
            validate_digisonde(&c).unwrap();
            // 100 coarse steps at 0.32 s/step = 32 s of physical RF.
            assert_eq!(
                digisonde_frequency_sample(&c, 1793980801., 100).unwrap(),
                44_849_520_825_088_375
            );
        }
    }
    #[test]
    fn explicit_code_phase_leaves_receiver_delay_and_frequency_times_fixed() {
        let mut c:Config=serde_json::from_str(r#"{"station":"DOB","data_dir":"x","output_dir":"y","ipp_us":20000,"physical_ipp_us":10000,"n_ipp":16,"offset_us":10006.923,"multiplex_block_pulses":4}"#).unwrap();
        let before = digisonde_frequency_sample(&c, 1794064050., 280).unwrap();
        c.multiplex_phase_pulses = Some(1);
        validate_digisonde(&c).unwrap();
        assert_eq!(
            before,
            digisonde_frequency_sample(&c, 1794064050., 280).unwrap()
        );
        let groups = iono_core::digisonde_match::multiplex_quartets(32, 4, 2, 1, false);
        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0], [(7, 4), (8, 5), (9, 6), (10, 7)]);
        c.multiplex_phase_pulses = Some(257);
        assert!(validate_digisonde(&c).is_err());
        c.multiplex_phase_pulses = Some(1);
        c.multiplex_block_pulses = None;
        assert!(validate_digisonde(&c).is_err());
    }
    #[test]
    fn physical_fine_frequency_demultiplexing_recovers_a_known_echo_delay() {
        let (nr, nipp, lag) = (1000, 16, 80);
        let waveform = iono_core::digisonde_match::template(nipp * nr, 100000., 10000, 3);
        for block in [1, 2, 4, 16] {
            let mut physical = vec![vec![Complex32::default(); nr]; nipp * 2];
            let mut selected = 0;
            // An independent physical time loop alternates blocks between two
            // RF carriers; the second carrier is outside this synthetic channel.
            for (p, row) in physical.iter_mut().enumerate() {
                if (p / block) % 2 == 0 {
                    for r in lag..nr {
                        row[r] = waveform[selected * nr + r - lag];
                    }
                    selected += 1;
                }
            }
            let d = Digisonde::new(10000);
            let decoded: Vec<_> = (0..nipp)
                .map(|p| {
                    d.decode_baseband_ipp(
                        physical[multiplex_pulse_index(p, block, 2)].clone(),
                        p,
                        None,
                    )
                })
                .collect();
            let power = d.power(&decoded);
            let peak = power[0]
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.total_cmp(b.1))
                .unwrap()
                .0;
            assert!(peak.abs_diff(lag) <= 1, "block={block}, peak={peak}");
        }
    }
    #[test]
    fn spectrum_sampling_is_bounded_spread_and_normalized_by_actual_count() {
        for bin in 0..120 {
            let batches = spectrum_batches(1525, 128, bin);
            assert_eq!(batches.iter().map(|(_, n)| n).sum::<usize>(), 128);
            assert!(batches.windows(2).all(|x| x[0].0 + x[0].1 <= x[1].0));
            assert!(batches.iter().all(|(i, n)| i + n <= 1525 && *n > 0));
            assert!(batches.first().unwrap().0 < 381 && batches.last().unwrap().0 >= 1143);
        }
        assert_ne!(
            spectrum_batches(1525, 128, 0),
            spectrum_batches(1525, 128, 1)
        );
        assert_eq!(
            spectrum_batches(1525, 0, 0)
                .iter()
                .map(|(_, n)| n)
                .sum::<usize>(),
            1525
        );
        assert_eq!(spectrum_batches(19, 128, 0), vec![(0, 19)]);
    }
    #[test]
    fn absolute_indices_are_not_rounded_through_f64() {
        let a = epoch_sample(1792615500., 25_000_000).unwrap();
        assert_eq!(a, 44_815_387_500_000_000);
        assert_eq!(a + 108950, 44_815_387_500_108_950);
    }
    #[test]
    fn transmitter_default_is_specific() {
        let a: Config = serde_json::from_str(
            r#"{"station":"TGO","data_dir":"x","output_dir":"y","transmitter":"Juliusruh"}"#,
        )
        .unwrap();
        assert_eq!(a.delay(), 0.);
        let b: Config =
            serde_json::from_str(r#"{"station":"TGO","data_dir":"x","output_dir":"y"}"#).unwrap();
        assert_eq!(b.delay(), 4688.);
    }
    #[test]
    fn long_5ms_dwells_fit_the_existing_voltage_budget_and_multiplexing_is_explicit() {
        let mut c:Config=serde_json::from_str(r#"{"station":"DOB","data_dir":"x","output_dir":"y","transmitter":"RO041","n_ipp":128,"ipp_us":5000}"#).unwrap();
        validate_digisonde(&c).unwrap();
        c.ipp_us = 10000;
        assert!(validate_digisonde(&c).is_err());
        c.ipp_us = 20000;
        c.n_ipp = 16;
        c.physical_ipp_us = Some(10000);
        validate_digisonde(&c).unwrap();
        assert_eq!(c.physical_ipp_us.unwrap() / 10, 1000);
        c.physical_ipp_us = Some(40000);
        assert!(validate_digisonde(&c).is_err());
    }
}

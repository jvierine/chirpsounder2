//! Station signal processor; UHD acquisition remains in the existing recorder.
mod detection;
mod lfm;
mod radar;
use anyhow::{ensure, Context, Result};
use hdf5::File;
use iono_core::{dsp::Digisonde, median, C};
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
fn read_live(r: &mut Reader, c: &Config, start: u64, n: usize) -> Result<Vec<Complex32>> {
    wait(((start + n as u64) as f64 / c.sample_rate).ceil() + 0.15);
    let mut error = None;
    for _ in 0..8 {
        match r.read_complex32(start, n, &c.channel, 0) {
            Ok(z) => return Ok(z),
            Err(e) => error = Some(e),
        }
        thread::sleep(Duration::from_millis(250));
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
fn digisonde(c: &Config, t: f64) -> Result<()> {
    let mut r = reader(c)?;
    let dsp = Digisonde::new(c.ipp_us);
    let (nf, nr, nipp) = (
        ((c.freq_stop_hz - c.freq_start_hz) / c.freq_step_hz) as usize,
        c.ipp_us / 10,
        c.n_ipp,
    );
    ensure!(
        nf > 0 && nf < 2000 && nr > 0 && nipp >= 4 && nipp <= 64 && nipp % 4 == 0,
        "invalid sounding configuration"
    );
    let ipp_samples = (c.sample_rate * c.ipp_us as f64 * 1e-6).round() as usize;
    let frequency_duration = nipp as f64 * c.ipp_us as f64 * 1e-6;
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
    let start_time = now();
    for f in 0..nf {
        let freq = c.freq_start_hz + f as f64 * c.freq_step_hz;
        let nominal = epoch_sample(t, c.sample_rate as u64)?;
        let shift = ((c.offset_us + c.delay()) * c.sample_rate / 1e6).round() as i64;
        let base = nominal
            .checked_add((f * nipp * ipp_samples) as u64)
            .and_then(|v| v.checked_add_signed(shift))
            .context("sounding sample overflow")?;
        let mut decoded = Vec::with_capacity(nipp);
        // One contiguous frequency read avoids repeated HDF5 hyperslab setup.
        // The validated maximum is 64 IPPs (128 MB of complex-f32 voltage).
        let padding = if c.rfi_clip_ratio.is_some() { 1000 } else { 0 };
        let raw = read_live(
            &mut r,
            c,
            base.checked_sub(padding as u64)
                .context("FIR overlap before sample zero")?,
            nipp * ipp_samples + 2 * padding,
        )?;
        for pi in 0..nipp {
            let ipp_raw = &raw[pi * ipp_samples..(pi + 1) * ipp_samples + 2 * padding];
            decoded.push(dsp.decode_ipp_with_clip(
                ipp_raw,
                freq - c.center_freq,
                c.sample_rate,
                pi,
                c.rfi_clip_ratio,
            ));
        }
        let p = dsp.power(&decoded);
        for mode in 0..2 {
            let floor = median(&p[mode]);
            noise[mode * nf + f] = floor;
            for k in 0..nr {
                let snr = (p[mode][k] - floor) / floor;
                power[(mode * nf + f) * nr + k] = if snr >= threshold { snr } else { f32::NAN }
            }
        }
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
        h.new_dataset::<f32>()
            .shape([2, nf, nr])
            .deflate(3)
            .create("SNR")?
            .write_raw(&power)?;
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
        dataset(&h, "ipp_us", c.ipp_us as f64)?;
        dataset(&h, "frequency_duration_s", frequency_duration)?;
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
        for first in (0..windows).step_by(32) {
            let count = 32.min(windows - first);
            let raw = read_live(&mut r, c, start + (first * nfft) as u64, count * nfft)?;
            for z in raw.chunks_exact(nfft) {
                for k in 0..nfft {
                    buf[k] = z[k] * window[k]
                }
                fft.process(&mut buf);
                for k in 0..nfft {
                    p[b * nfft + (k + nfft / 2) % nfft] += buf[k].norm_sqr() / windows as f32;
                }
            }
        }
    }
    let path = output(c, &format!("rf_spectrum-{}-ch0", c.station), t)?;
    let tmp = path.with_extension("h5.partial");
    {
        let h = File::create(&tmp)?;
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
        let station: hdf5::types::VarLenUnicode = c.station.parse()?;
        h.new_attr::<hdf5::types::VarLenUnicode>()
            .create("station")?
            .write_scalar(&station)?;
        text(&h, "processor", "iono-receiver/0.1.0")?;
        h.flush()?;
    }
    publish(c, &tmp, &path)
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
    let once = args.iter().any(|s| s == "--once");
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
    if task == "detect" {
        return detection::run(&c, once);
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
    loop {
        let result = match task {
            "digisonde" if c.sounding_active(t) => digisonde(&c, t),
            "digisonde" => Ok(()),
            "spectrum" => spectrum(&c, t),
            "lfm" => lfm::run(&c, t),
            _ => anyhow::bail!("unknown task {task}"),
        };
        if once {
            return result;
        }
        if let Err(e) = result {
            eprintln!("{task} skipped epoch {t}: {e:#}")
        };
        t += interval;
        if t + interval < now() {
            t = schedule_offset + ((now() - schedule_offset) / interval).floor() * interval;
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
}

//! Receiver-local complementary-code verification of PINS pulse candidates.
//! Discovery results are deliberately not assigned transmitter identities.
//! Scores and circular lags are commissioning diagnostics, not calibrated
//! false-alarm probabilities or propagation ranges.
use anyhow::{ensure, Context, Result};
use hdf5::File;
use iono_core::{digisonde_match::CodeSearch, prn::Channelizer};
use rdigital_rf::{CacheOptions, Reader};
use serde::Deserialize;
use std::{fs, path::Path};
#[derive(Deserialize)]
struct Config {
    data_dir: String,
    output_dir: String,
    channel: String,
    #[serde(default = "doppler")]
    doppler_search_hz: f64,
    #[serde(default = "default_ipps")]
    ipp_candidates_us: Vec<u32>,
    #[serde(default = "one")]
    fine_frequency_count: usize,
    #[serde(default)]
    multiplex_blocks: Vec<usize>,
}
fn one() -> usize {
    1
}
fn default_ipps() -> Vec<u32> {
    vec![5000, 10000]
}
fn doppler() -> f64 {
    100.
}
fn main() -> Result<()> {
    let a: Vec<_> = std::env::args().collect();
    let c: Config = serde_json::from_slice(&fs::read(
        a.get(1)
            .context("usage: iono-digisonde-verify CONFIG SEARCH_HDF5 [MAX_ROWS]")?,
    )?)?;
    ensure!(
        c.doppler_search_hz.is_finite() && (0.0..=500.).contains(&c.doppler_search_hz),
        "invalid Doppler search extent"
    );
    ensure!(
        (1..=4).contains(&c.fine_frequency_count)
            && c.multiplex_blocks.len() <= 16
            && c.multiplex_blocks.iter().all(|b| (1..=16).contains(b)),
        "invalid fine-frequency program bank"
    );
    let grid = a.get(2).is_some_and(|s| s == "--grid");
    ensure!(
        !c.ipp_candidates_us.is_empty()
            && c.ipp_candidates_us.len() <= 8
            && c.ipp_candidates_us
                .iter()
                .all(|p| [5000, 10000, 20000, 40000].contains(p)),
        "unsupported commissioning pulse-spacing bank"
    );
    let (samples, freq, contrast) = if grid {
        let epoch: f64 = a
            .get(3)
            .context("--grid requires UTC Unix epoch")?
            .parse()?;
        let mut samples = vec![];
        let mut freq = vec![];
        let number = |i: usize, default: f64| -> Result<f64> {
            Ok(a.get(i).map(|s| s.parse()).transpose()?.unwrap_or(default))
        };
        let (tmin, tmax, dt) = (number(4, 2.)?, number(5, 122.)?, number(6, 8.)?);
        let (fmin, fmax, df) = (number(7, 1e6)?, number(8, 12e6)?, number(9, 50e3)?);
        ensure!(
            epoch.is_finite()
                && [tmin, tmax, dt, fmin, fmax, df]
                    .iter()
                    .all(|v| v.is_finite())
                && epoch >= 0.
                && tmin >= 0.
                && tmax >= tmin
                && dt >= 0.04
                && fmin >= 0.5e6
                && fmax <= 24e6
                && fmax >= fmin
                && df >= 1000.,
            "invalid commissioning grid"
        );
        let nt = ((tmax - tmin) / dt).round() as usize + 1;
        let nf = ((fmax - fmin) / df).round() as usize + 1;
        ensure!(
            nt.saturating_mul(nf) <= 100000,
            "commissioning grid too large"
        );
        for ti in 0..nt {
            for k in 0..nf {
                samples.push(((epoch + tmin + ti as f64 * dt) * 25e6).round() as u64);
                freq.push(fmin + k as f64 * df);
            }
        }
        let n = samples.len();
        (samples, freq, vec![1f32; n])
    } else {
        let h = File::open(a.get(2).context("missing search HDF5")?)?;
        ensure!(
            h.dataset("sample_index")?.size() <= 1_000_000,
            "candidate file exceeds budget"
        );
        (
            h.dataset("sample_index")?.read_raw::<u64>()?,
            h.dataset("frequency_hz")?.read_raw::<f64>()?,
            h.dataset("fold_contrast")?.read_raw::<f32>()?,
        )
    };
    ensure!(
        samples.len() == freq.len() && samples.len() == contrast.len(),
        "candidate axes disagree"
    );
    ensure!(
        freq.iter()
            .all(|v| v.is_finite() && (0.7e6..=24.5e6).contains(v))
            && contrast.iter().all(|v| v.is_finite()),
        "invalid candidate values"
    );
    let mut r = Reader::open([&c.data_dir])?;
    r.set_cache_options(CacheOptions {
        minimum_bytes: 1024 * 1024,
        max_open_files: 2,
    })?;
    r.set_cache_byte_limit(16 * 1024 * 1024)?;
    let bounds = r.bounds(&c.channel)?.context("no voltage")?;
    let mut order: Vec<_> = (0..samples.len())
        .filter(|&i| {
            samples[i] > bounds.0.saturating_add(25_000_000)
                && samples[i]
                    .checked_add(4_002_000)
                    .is_some_and(|end| end < bounds.1)
        })
        .collect();
    // Avoid spending the commissioning budget on a single stationary interferer.
    order.sort_by(|&i, &j| contrast[j].total_cmp(&contrast[i]));
    if !grid {
        // Round-robin MHz bands, strongest first within each. Coarse radar
        // candidates at 17--19 MHz must not starve weak 2--6 MHz sounders.
        let mut bands = vec![Vec::new(); 25];
        for i in order {
            bands[(freq[i] / 1e6).floor() as usize].push(i);
        }
        order = Vec::new();
        let depth = bands.iter().map(Vec::len).max().unwrap_or(0);
        for n in 0..depth {
            for b in &bands {
                if let Some(&i) = b.get(n) {
                    order.push(i);
                }
            }
        }
    }
    let max = if grid {
        samples.len()
    } else {
        a.get(3)
            .and_then(|s| s.parse().ok())
            .unwrap_or(128usize)
            .min(10000)
    };
    let mut chosen: Vec<usize> = vec![];
    for i in order {
        if !grid
            && chosen.iter().any(|&j| {
                (freq[i] - freq[j]).abs() < 100e3 && samples[i].abs_diff(samples[j]) < 250_000_000
            })
        {
            continue;
        }
        chosen.push(i);
        if chosen.len() == max {
            break;
        }
    }
    chosen.sort_by_key(|&i| samples[i]);
    let programs: Vec<_> = if c.multiplex_blocks.is_empty() {
        vec![(1, 1)]
    } else {
        c.multiplex_blocks
            .iter()
            .map(|&b| (c.fine_frequency_count, b))
            .collect()
    };
    let mut decoder = CodeSearch::with_programs(16000, 100e3, &c.ipp_candidates_us, &programs);
    let mut time = vec![];
    let mut frequency = vec![];
    let mut ipp = vec![];
    let mut mode = vec![];
    let mut lag = vec![];
    let mut score = vec![];
    let mut coherence = vec![];
    let mut all_polarizations = vec![];
    let mut fine_frequency_count = vec![];
    let mut multiplex_block_pulses = vec![];
    for (n, &i) in chosen.iter().enumerate() {
        let carrier = if grid {
            freq[i]
        } else {
            (freq[i] / 10000.).round() * 10000.
        };
        match r.read_complex32(samples[i] - 1000, 4_002_000, &c.channel, 0) {
            Ok(raw) => {
                let z = Channelizer::new(carrier - 12.5e6, 100e3)?.process(&raw, -1000);
                if let Some(m) = decoder.seek_with_doppler(
                    &z[4..16004],
                    if grid { 0. } else { 30000. },
                    5000.,
                    (c.doppler_search_hz / 100e3 * 16384.).ceil() as isize,
                ) {
                    time.push(samples[i] as f64 / 25e6);
                    frequency.push(carrier + m.carrier_offset_hz);
                    ipp.push(m.ipp_us);
                    mode.push(m.mode);
                    lag.push(m.lag_s);
                    score.push(m.score_db);
                    coherence.push(m.coherence);
                    all_polarizations.push(u8::from(m.all_polarizations));
                    fine_frequency_count.push(m.fine_frequency_count as u32);
                    multiplex_block_pulses.push(m.multiplex_block_pulses as u32);
                    println!(
                        "{} {:.6} MHz IPP={} mode={} coherence={:.4} score={:.1} lag={:.6}",
                        samples[i] as f64 / 25e6,
                        frequency.last().unwrap() / 1e6,
                        m.ipp_us,
                        m.mode,
                        m.coherence,
                        m.score_db,
                        m.lag_s
                    );
                }
            }
            Err(e) => eprintln!("missing voltage: {e}"),
        }
        if n % 16 == 0 {
            eprintln!("verified {}/{} candidate windows", n + 1, chosen.len());
        }
    }
    fs::create_dir_all(&c.output_dir)?;
    let path = Path::new(&c.output_dir).join(format!(
        "digisonde_code_matches-{}.h5",
        samples.last().copied().unwrap_or(0) / 25_000_000
    ));
    let tmp = path.with_extension("partial.h5");
    let out = File::create(&tmp)?;
    macro_rules! save {
        ($name:expr,$v:expr) => {
            out.new_dataset_builder().with_data(&$v).create($name)?;
        };
    }
    save!("searched_ipp_candidates_us", c.ipp_candidates_us);
    save!("time_unix", time);
    save!("frequency_hz", frequency);
    save!("ipp_us", ipp);
    save!("mode", mode);
    save!("lag_s", lag);
    save!("score_db", score);
    save!("coherence", coherence);
    save!("all_polarizations", all_polarizations);
    save!("fine_frequency_count", fine_frequency_count);
    save!("multiplex_block_pulses", multiplex_block_pulses);
    out.new_attr::<u64>()
        .create("attempted_windows")?
        .write_scalar(&(chosen.len() as u64))?;
    drop(out);
    fs::rename(tmp, &path)?;
    println!("{}", path.display());
    Ok(())
}

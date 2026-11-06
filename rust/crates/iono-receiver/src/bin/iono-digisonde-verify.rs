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
    #[serde(default)]
    save_iq: bool,
    #[serde(default = "orders")]
    polarization_orders: Vec<bool>,
    #[serde(default)]
    replay_carrier_search_hz: Option<f64>,
}
fn orders() -> Vec<bool> {
    vec![false]
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
/// Export only small numeric diagnostics; cached voltage stays at the receiver.
fn export_summary(source: &Path, destination: &Path) -> Result<()> {
    ensure!(
        source != destination,
        "summary must use a separate output path"
    );
    let input = File::open(source)?;
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = destination.with_extension("partial.h5");
    let out = File::create(&temporary)?;
    fn copy_dataset(input: &File, out: &File, name: &str) -> Result<()> {
        let d = input.dataset(name)?;
        ensure!(d.size() <= 10000, "summary dataset exceeds budget");
        macro_rules! copy {
            ($t:ty) => {
                out.new_dataset::<$t>()
                    .shape(d.shape())
                    .create(name)?
                    .write_raw(&d.read_raw::<$t>()?)?;
            };
        }
        let t = d.dtype()?;
        if t.is::<f32>() {
            copy!(f32);
        } else if t.is::<f64>() {
            copy!(f64);
        } else if t.is::<u64>() {
            copy!(u64);
        } else if t.is::<u32>() {
            copy!(u32);
        } else if t.is::<u8>() {
            copy!(u8);
        } else {
            anyhow::bail!("unsupported diagnostic datatype");
        }
        Ok(())
    }
    for name in input.member_names()? {
        if name != "iq" {
            copy_dataset(&input, &out, &name)?;
        }
    }
    if input.link_exists("iq/sample_index") {
        out.create_group("iq")?;
        for name in ["iq/sample_index", "iq/carrier_hz"] {
            copy_dataset(&input, &out, name)?;
        }
    }
    for name in input.attr_names()? {
        let a = input.attr(&name)?;
        ensure!(a.size() == 1, "non-scalar diagnostic attribute");
        macro_rules! copy {
            ($t:ty) => {
                out.new_attr::<$t>()
                    .create(name.as_str())?
                    .write_scalar(&a.read_scalar::<$t>()?)?;
            };
        }
        let t = a.dtype()?;
        if t.is::<u64>() {
            copy!(u64);
        } else if t.is::<u8>() {
            copy!(u8);
        } else if t.is::<f64>() {
            copy!(f64);
        } else {
            anyhow::bail!("unsupported diagnostic attribute");
        }
    }
    out.flush()?;
    drop(out);
    fs::rename(temporary, destination)?;
    Ok(())
}
fn main() -> Result<()> {
    let a: Vec<_> = std::env::args().collect();
    if a.get(1).is_some_and(|s| s == "--summary") {
        ensure!(
            a.len() == 4,
            "usage: iono-digisonde-verify --summary INPUT.h5 OUTPUT.h5"
        );
        return export_summary(Path::new(&a[2]), Path::new(&a[3]));
    }
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
    let replay = a.get(2).is_some_and(|s| s == "--iq");
    let mut replay_iq: Vec<[f32; 2]> = vec![];
    let mut search_span_hz = if grid { 0. } else { 30000. };
    // Explicit bounded commissioning grids may revisit adjacent pulse epochs.
    let dense = a.iter().any(|s| s == "--dense");
    ensure!(
        !c.ipp_candidates_us.is_empty()
            && c.ipp_candidates_us.len() <= 8
            && c.ipp_candidates_us
                .iter()
                .all(|p| [5000, 10000, 20000, 40000].contains(p)),
        "unsupported commissioning pulse-spacing bank"
    );
    let (samples, freq, contrast) = if replay {
        let h = File::open(a.get(3).context("--iq requires a captured HDF5 product")?)?;
        let d = h.dataset("iq/baseband_complex32")?;
        ensure!(
            d.shape().len() == 2 && d.shape()[1] == 16000 && d.shape()[0] <= 512,
            "invalid bounded IQ cache"
        );
        replay_iq = d.read_raw::<[f32; 2]>()?;
        search_span_hz = h.attr("iq_carrier_search_hz")?.read_scalar::<f64>()?;
        if let Some(span) = c.replay_carrier_search_hz {
            search_span_hz = span;
        }
        ensure!(
            search_span_hz.is_finite() && (0. ..=30000.).contains(&search_span_hz),
            "invalid cached carrier search extent"
        );
        let samples = h.dataset("iq/sample_index")?.read_raw::<u64>()?;
        let freq = h.dataset("iq/carrier_hz")?.read_raw::<f64>()?;
        ensure!(
            samples.len() * 16000 == replay_iq.len(),
            "IQ cache axes disagree"
        );
        let n = samples.len();
        (samples, freq, vec![1f32; n])
    } else if grid {
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
    let mut r = if replay {
        None
    } else {
        let mut r = Reader::open([&c.data_dir])?;
        r.set_cache_options(CacheOptions {
            minimum_bytes: 1024 * 1024,
            max_open_files: 2,
        })?;
        r.set_cache_byte_limit(16 * 1024 * 1024)?;
        Some(r)
    };
    let bounds = if let Some(r) = r.as_mut() {
        r.bounds(&c.channel)?.context("no voltage")?
    } else {
        (0, u64::MAX)
    };
    let mut order: Vec<_> = (0..samples.len())
        .filter(|&i| {
            replay
                || (samples[i] > bounds.0.saturating_add(25_000_000)
                    && samples[i]
                        .checked_add(4_002_000)
                        .is_some_and(|end| end < bounds.1))
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
    let max = if grid || replay {
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
            && !replay
            && !dense
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
    ensure!(
        !c.save_iq || chosen.len() <= 512,
        "IQ capture exceeds 512-window budget"
    );
    let mut captured_iq: Vec<[f32; 2]> = vec![];
    let mut captured_samples = vec![];
    let mut captured_carriers = vec![];
    let mut successful = 0u64;
    ensure!(
        !c.polarization_orders.is_empty() && c.polarization_orders.len() <= 2,
        "invalid polarization order bank"
    );
    let mut decoder = CodeSearch::with_orders(
        16000,
        100e3,
        &c.ipp_candidates_us,
        &programs,
        &c.polarization_orders,
    );
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
    let mut polarization_interleaved = vec![];
    for (n, &i) in chosen.iter().enumerate() {
        let carrier = if grid || replay {
            freq[i]
        } else {
            (freq[i] / 10000.).round() * 10000.
        };
        let baseband = if replay {
            Ok(replay_iq[i * 16000..(i + 1) * 16000]
                .iter()
                .map(|v| num_complex::Complex32::new(v[0], v[1]))
                .collect::<Vec<_>>())
        } else {
            r.as_mut()
                .unwrap()
                .read_complex32(samples[i] - 1000, 4_002_000, &c.channel, 0)
                .and_then(|raw| {
                    let z = Channelizer::new(carrier - 12.5e6, 100e3)?.process(&raw, -1000);
                    Ok(z[4..16004].to_vec())
                })
        };
        match baseband {
            Ok(z) => {
                successful += 1;
                if c.save_iq {
                    captured_iq.extend(z.iter().map(|v| [v.re, v.im]));
                    captured_samples.push(samples[i]);
                    captured_carriers.push(carrier);
                }
                if let Some(m) = decoder.seek_with_doppler(
                    &z,
                    search_span_hz,
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
                    polarization_interleaved.push(u8::from(m.polarization_interleaved));
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
    out.new_attr::<u64>()
        .create("successful_windows")?
        .write_scalar(&successful)?;
    out.new_attr::<u64>()
        .create("requested_windows")?
        .write_scalar(&(samples.len() as u64))?;
    if c.save_iq {
        out.create_group("iq")?;
        out.new_attr::<f64>()
            .create("iq_carrier_search_hz")?
            .write_scalar(&search_span_hz)?;
        out.new_attr::<f64>()
            .create("iq_sample_rate_hz")?
            .write_scalar(&100000.)?;
        out.new_dataset_builder()
            .with_data(&captured_samples)
            .create("iq/sample_index")?;
        out.new_dataset_builder()
            .with_data(&captured_carriers)
            .create("iq/carrier_hz")?;
        if !captured_samples.is_empty() {
            out.new_dataset::<[f32; 2]>()
                .shape([captured_samples.len(), 16000])
                .chunk([1, 16000])
                .deflate(4)
                .create("iq/baseband_complex32")?
                .write_raw(&captured_iq)?;
        }
    }
    macro_rules! save {
        ($name:expr,$v:expr) => {
            out.new_dataset_builder().with_data(&$v).create($name)?;
        };
    }
    out.new_attr::<u8>()
        .create("dense_candidate_grid")?
        .write_scalar(&u8::from(dense))?;
    save!("searched_ipp_candidates_us", c.ipp_candidates_us);
    save!("time_unix", time);
    save!("frequency_hz", frequency);
    save!("ipp_us", ipp);
    save!("mode", mode);
    save!("lag_s", lag);
    save!("score_db", score);
    save!("coherence", coherence);
    save!("polarization_interleaved", polarization_interleaved);
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

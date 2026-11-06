//! Candidate stepped-frequency tracks from a receiver-local pulse survey.
//! Hough seeds are refined by least squares; identities are never assigned here.
use anyhow::{ensure, Result};
use hdf5::File;
use std::collections::HashMap;
const RATES: [f64; 14] = [
    0.01953125, 0.025, 0.0390625, 0.05, 0.075, 0.078125, 0.1, 0.125, 0.15, 0.15625, 0.2, 0.25,
    0.3125, 0.5,
];
#[derive(Clone)]
struct Run {
    n: u64,
    first: f64,
    last: f64,
    frame: u64,
    minf: f64,
    maxf: f64,
    sx: f64,
    sy: f64,
    sxx: f64,
    sxy: f64,
    syy: f64,
}
impl Run {
    fn new(t: f64, f: f64, frame: u64) -> Self {
        Self {
            n: 1,
            first: t,
            last: t,
            frame,
            minf: f,
            maxf: f,
            sx: t,
            sy: f,
            sxx: t * t,
            sxy: t * f,
            syy: f * f,
        }
    }
    fn add(&mut self, t: f64, f: f64, frame: u64) {
        self.n += 1;
        self.last = t;
        self.frame = frame;
        self.minf = self.minf.min(f);
        self.maxf = self.maxf.max(f);
        self.sx += t;
        self.sy += f;
        self.sxx += t * t;
        self.sxy += t * f;
        self.syy += f * f;
    }
}
#[derive(Clone, Debug)]
struct Track {
    n: u64,
    first: f64,
    last: f64,
    rate: f64,
    intercept: f64,
    rms: f64,
    minf: f64,
    maxf: f64,
}
fn finish(r: &Run, seed: f64) -> Option<Track> {
    if r.n < 12 || r.last - r.first < 3. || r.maxf - r.minf < 0.25 {
        return None;
    }
    let n = r.n as f64;
    let den = n * r.sxx - r.sx * r.sx;
    if den <= 0. {
        return None;
    }
    let rate = (n * r.sxy - r.sx * r.sy) / den;
    let intercept = (r.sy - rate * r.sx) / n;
    let residual = (r.syy + rate * rate * r.sxx + n * intercept * intercept
        - 2. * rate * r.sxy
        - 2. * intercept * r.sy
        + 2. * rate * intercept * r.sx)
        .max(0.);
    let rms = (residual / n).sqrt();
    if rate <= 0. || (rate / seed - 1.).abs() > 0.2 || rms > 0.0375 {
        return None;
    }
    Some(Track {
        n: r.n,
        first: r.first,
        last: r.last,
        rate,
        intercept,
        rms,
        minf: r.minf,
        maxf: r.maxf,
    })
}
fn tracks(points: &[(u64, f64, f64)], stride: f64) -> Vec<Track> {
    let mut bank: HashMap<(usize, i64), Run> = HashMap::new();
    let mut found = vec![];
    for &(frame, t, f) in points {
        for (k, &rate) in RATES.iter().enumerate() {
            let bin = ((f - rate * t) / 0.025).floor() as i64;
            for j in bin - 1..=bin + 1 {
                match bank.get_mut(&(k, j)) {
                    Some(r) if r.frame == frame => {}
                    Some(r) if t - r.last <= 3. * stride + 1e-5 => r.add(t, f, frame),
                    Some(r) => {
                        if let Some(x) = finish(r, rate) {
                            found.push(x);
                        }
                        *r = Run::new(t, f, frame);
                    }
                    None => {
                        bank.insert((k, j), Run::new(t, f, frame));
                    }
                }
            }
        }
    }
    for ((k, _), r) in bank {
        if let Some(x) = finish(&r, RATES[k]) {
            found.push(x);
        }
    }
    found.sort_by(|a, b| b.n.cmp(&a.n).then_with(|| a.rms.total_cmp(&b.rms)));
    let mut unique: Vec<Track> = vec![];
    for x in found {
        if unique.iter().any(|y| {
            let overlap = x.last.min(y.last) - x.first.max(y.first);
            let mid = (x.first.max(y.first) + x.last.min(y.last)) / 2.;
            overlap > 0.5 * (x.last - x.first).min(y.last - y.first)
                && (x.rate / y.rate - 1.).abs() < 0.2
                && ((x.rate - y.rate) * mid + x.intercept - y.intercept).abs() < 0.1
        }) {
            continue;
        }
        unique.push(x);
        if unique.len() == 100 {
            break;
        }
    }
    unique
}
fn main() -> Result<()> {
    let a: Vec<_> = std::env::args().collect();
    ensure!(
        a.len() == 3,
        "usage: iono-digisonde-tracks survey.h5 tracks.h5"
    );
    let h = File::open(&a[1])?;
    let ix = h.dataset("sample_index")?.read_raw::<u64>()?;
    let f = h.dataset("frequency_hz")?.read_raw::<f64>()?;
    ensure!(
        ix.len() == f.len() && ix.len() <= 10_000_000,
        "invalid candidate rows"
    );
    let reference = h.attr("first_planned_sample_index")?.read_scalar::<u64>()?;
    let stride = h.attr("stride_s")?.read_scalar::<f64>()?;
    ensure!(stride.is_finite() && stride > 0., "invalid stride");
    let step = (stride * 25e6).round() as u64;
    let mut points: Vec<_> = ix
        .iter()
        .zip(&f)
        .filter(|(i, f)| **i >= reference && f.is_finite() && (0.7e6..=24.5e6).contains(*f))
        .map(|(&i, &f)| (i / step, (i - reference) as f64 / 25e6, f / 1e6))
        .collect();
    points.sort_by(|a, b| a.0.cmp(&b.0));
    let result = tracks(&points, stride);
    let ci = h.dataset("code_checks/sample_index")?.read_raw::<u64>()?;
    let cf = h.dataset("code_checks/frequency_hz")?.read_raw::<f64>()?;
    let cm = h.dataset("code_checks/matched")?.read_raw::<u8>()?;
    let cc = h.dataset("code_checks/coherence")?.read_raw::<f32>()?;
    ensure!(
        ci.len() == cf.len() && ci.len() == cm.len() && ci.len() == cc.len(),
        "code-check axes disagree"
    );
    let mut attempted = vec![];
    let mut matched = vec![];
    let mut best = vec![];
    for (k, r) in result.iter().enumerate() {
        let mut n = 0u64;
        let mut m = 0u64;
        let mut maximum = 0f32;
        for j in 0..ci.len() {
            if ci[j] < reference {
                continue;
            }
            let t = (ci[j] - reference) as f64 / 25e6;
            if t >= r.first
                && t <= r.last
                && (cf[j] / 1e6 - r.rate * t - r.intercept).abs() <= 0.075
            {
                n += 1;
                if cm[j] > 0 {
                    m += 1;
                    maximum = maximum.max(cc[j]);
                }
            }
        }
        attempted.push(n);
        matched.push(m);
        best.push(maximum);
        println!("candidate {}: {:.3} kHz/s, {:.2}--{:.2} MHz, {:.2}--{:.2}s, {} windows, {:.1} kHz RMS; {m}/{n} code matches, max coherence {maximum:.3}",k+1,r.rate*1000.,r.minf,r.maxf,r.first,r.last,r.n,r.rms*1000.);
    }
    let out = File::create(&a[2])?;
    for name in [
        "processed_blocks",
        "planned_blocks",
        "missing_blocks",
        "first_planned_sample_index",
        "last_scanned_sample_index",
    ] {
        out.new_attr::<u64>()
            .create(name)?
            .write_scalar(&h.attr(name)?.read_scalar::<u64>()?)?;
    }

    macro_rules! save {
        ($name:expr,$v:expr) => {
            out.new_dataset_builder().with_data(&$v).create($name)?;
        };
    }
    save!(
        "searched_seed_rates_hz_s",
        RATES.iter().map(|r| r * 1e6).collect::<Vec<_>>()
    );
    save!("checked_sample_index", ci);
    save!("checked_frequency_hz", cf);
    save!("checked_matched", cm);
    save!("checked_coherence", cc);
    save!(
        "checked_ipp_us",
        h.dataset("code_checks/ipp_us")?.read_raw::<u32>()?
    );
    save!(
        "checked_mode",
        h.dataset("code_checks/mode")?.read_raw::<u8>()?
    );
    save!(
        "checked_lag_s",
        h.dataset("code_checks/lag_s")?.read_raw::<f64>()?
    );
    save!(
        "checked_score_db",
        h.dataset("code_checks/score_db")?.read_raw::<f32>()?
    );
    save!(
        "supporting_windows",
        result.iter().map(|x| x.n).collect::<Vec<_>>()
    );
    save!(
        "first_unix",
        result
            .iter()
            .map(|x| reference as f64 / 25e6 + x.first)
            .collect::<Vec<_>>()
    );
    save!(
        "last_unix",
        result
            .iter()
            .map(|x| reference as f64 / 25e6 + x.last)
            .collect::<Vec<_>>()
    );
    save!(
        "rate_hz_s",
        result.iter().map(|x| x.rate * 1e6).collect::<Vec<_>>()
    );
    save!(
        "frequency_at_reference_hz",
        result.iter().map(|x| x.intercept * 1e6).collect::<Vec<_>>()
    );
    save!(
        "frequency_rms_hz",
        result.iter().map(|x| x.rms * 1e6).collect::<Vec<_>>()
    );
    save!(
        "min_frequency_hz",
        result.iter().map(|x| x.minf * 1e6).collect::<Vec<_>>()
    );
    save!(
        "max_frequency_hz",
        result.iter().map(|x| x.maxf * 1e6).collect::<Vec<_>>()
    );
    save!("code_checks", attempted);
    save!("code_matches", matched);
    save!("maximum_code_coherence", best);
    out.new_attr::<u64>()
        .create("reference_sample_index")?
        .write_scalar(&reference)?;
    out.new_attr::<hdf5::types::VarLenUnicode>().create("interpretation")?.write_scalar(&"Candidate upward frequency tracks, including possible LFM radars and interference; no transmitter identities or calibrated false-alarm probabilities.".parse::<hdf5::types::VarLenUnicode>()?)?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recovers_a_stepped_sweep_amid_scattered_candidates() {
        let mut p = vec![];
        let mut seed = 7u64;
        for i in 0..100u64 {
            let t = i as f64 * 0.32;
            p.push((i, t, 2. + i as f64 * 0.025));
            for _ in 0..8 {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                p.push((i, t, 0.7 + 23.8 * (seed >> 32) as f64 / u32::MAX as f64));
            }
        }
        let r = tracks(&p, 0.32);
        assert!(
            r.iter()
                .any(|r| (r.rate - 0.078125).abs() < 0.001 && r.n >= 90),
            "{r:?}"
        );
    }
    #[test]
    fn does_not_promote_a_stationary_carrier() {
        let p: Vec<_> = (0..200u64).map(|i| (i, i as f64 * 0.32, 5.)).collect();
        assert!(tracks(&p, 0.32).is_empty());
    }
    #[test]
    fn breaks_a_track_across_a_real_observation_gap() {
        let p: Vec<_> = (0..80u64)
            .filter(|i| *i < 30 || *i > 50)
            .map(|i| (i, i as f64 * 0.32, 2. + i as f64 * 0.025))
            .collect();
        let r = tracks(&p, 0.32);
        assert!(r.len() >= 2, "{r:?}");
        assert!(r.iter().all(|r| !(r.first < 10. && r.last > 16.)), "{r:?}");
    }
    #[test]
    fn common_digisonde_dwell_rates_survive_step_quantization() {
        for rate in [0.0390625, 0.078125, 0.15625, 0.3125] {
            let p: Vec<_> = (0..100u64)
                .map(|i| {
                    let t = i as f64 * 0.32;
                    (i, t, 2. + (rate * t / 0.025 + 1e-8).floor() * 0.025)
                })
                .collect();
            let r = tracks(&p, 0.32);
            assert!(
                r.iter()
                    .any(|r| (r.rate / rate - 1.).abs() < 0.01 && r.n >= 80),
                "rate={rate}: {r:?}"
            );
        }
    }
}

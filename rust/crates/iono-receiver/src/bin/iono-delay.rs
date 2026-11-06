//! Delay diagnostics never modify station timing or align individual traces.
use anyhow::{ensure, Context, Result};
use hdf5::{types::VarLenUnicode, File};
use iono_core::{read_products, scalar, C};
use std::path::Path;
fn scalar_out(h: &File, key: &str, v: f64) -> Result<()> {
    h.new_dataset::<f64>().create(key)?.write_scalar(&v)?;
    Ok(())
}
fn text(h: &File, key: &str, v: &str) -> Result<()> {
    h.new_dataset::<VarLenUnicode>()
        .create(key)?
        .write_scalar(&v.parse::<VarLenUnicode>()?)?;
    Ok(())
}
fn median(v: &[f64]) -> f64 {
    let mut a = v.to_vec();
    a.sort_by(f64::total_cmp);
    (a[(a.len() - 1) / 2] + a[a.len() / 2]) / 2.
}
// A conservative scatter diagnostic, not a claim of independent samples.
fn estimate(observed: &[f64], reference: &[f64], uncertainty: &[f64]) -> Result<(f64, f64)> {
    ensure!(
        observed.len() >= 3
            && observed.len() <= 100000
            && observed.len() == reference.len()
            && observed.len() == uncertainty.len(),
        "need 3..100000 paired independent-reference measurements"
    );
    ensure!(
        observed.iter().chain(reference).all(|v| v.is_finite())
            && uncertainty.iter().all(|v| v.is_finite() && *v >= 0.),
        "invalid reference measurements"
    );
    let residual: Vec<_> = observed.iter().zip(reference).map(|(a, b)| a - b).collect();
    let bias = median(&residual);
    let scatter = 1.4826
        * median(
            &residual
                .iter()
                .map(|r| (r - bias).abs())
                .collect::<Vec<_>>(),
        );
    // Reference systematic uncertainty must not shrink with the number of rows.
    Ok((
        bias,
        scatter.hypot(uncertainty.iter().copied().fold(0., f64::max)),
    ))
}
fn calibrate(input: &str, output: &str) -> Result<()> {
    let h = File::open(input)?;
    let station = h.dataset("receiver")?.read_scalar::<VarLenUnicode>()?;
    let reference = h
        .dataset("independent_reference")?
        .read_scalar::<VarLenUnicode>()?;
    ensure!(
        !station.as_str().is_empty() && !reference.as_str().is_empty(),
        "receiver and independent timing reference required"
    );
    for key in [
        "observed_delay_us",
        "reference_delay_us",
        "reference_uncertainty_us",
    ] {
        ensure!(
            h.dataset(key)?.size() <= 100000,
            "reference input too large"
        );
    }
    let observed = h.dataset("observed_delay_us")?.read_raw::<f64>()?;
    let expected = h.dataset("reference_delay_us")?.read_raw::<f64>()?;
    let uncertainty = h.dataset("reference_uncertainty_us")?.read_raw::<f64>()?;
    // Mandatory: never guess whether an input measurement was already corrected.
    let applied = h
        .dataset("applied_receiver_delay_us")?
        .read_scalar::<f64>()?;
    ensure!(applied.is_finite(), "invalid existing correction");
    let (bias, uncertainty) = estimate(&observed, &expected, &uncertainty)?;
    let out = File::create_excl(output)?;
    scalar_out(&out, "residual_bias_us", bias)?;
    scalar_out(&out, "proposed_receiver_delay_us", applied + bias)?;
    scalar_out(&out, "scatter_and_reference_uncertainty_us", uncertainty)?;
    scalar_out(&out, "applied_receiver_delay_us", applied)?;
    text(&out, "receiver", station.as_str())?;
    text(&out, "independent_reference", reference.as_str())?;
    text(&out,"status","proposal only; reference timing, ambiguity and hardware changes require review; no receiver configuration changed")?;
    println!(
        "{}: proposed fixed delay {:.3} us; scatter/reference diagnostic {:.3} us; not applied",
        station,
        applied + bias,
        uncertainty
    );
    Ok(())
}
fn peaks(args: &[String]) -> Result<()> {
    ensure!(
        (6..=7).contains(&args.len()),
        "usage: iono-delay peaks INPUT.h5 OUTPUT.h5 MIN_KM MAX_KM [MIN_DB]"
    );
    let lo: f64 = args[4].parse()?;
    let hi: f64 = args[5].parse()?;
    let threshold: f32 = args.get(6).map(|s| s.parse()).transpose()?.unwrap_or(3.);
    ensure!(
        lo.is_finite() && hi.is_finite() && lo < hi && threshold.is_finite(),
        "invalid range gate/threshold"
    );
    let h = File::open(&args[2])?;
    let grids = read_products(Path::new(&args[2]))?;
    ensure!(
        !grids.is_empty()
            && grids
                .iter()
                .all(|g| ["digisonde", "prn", "lfm"].contains(&g.kind.as_str())),
        "need a frequency/range ionogram"
    );
    let out = File::create_excl(&args[3])?;
    for (mode, g) in grids.iter().enumerate() {
        let group = out.create_group(&format!("mode_{mode}"))?;
        let mut frequency = vec![];
        let mut delay = vec![];
        let mut power = vec![];
        for f in 0..g.nx {
            let best = (0..g.ny)
                .filter(|&r| {
                    g.y[r] >= lo
                        && g.y[r] <= hi
                        && g.values[r * g.nx + f].is_finite()
                        && g.values[r * g.nx + f] >= threshold
                })
                .max_by(|&a, &b| g.values[a * g.nx + f].total_cmp(&g.values[b * g.nx + f]));
            if let Some(r) = best {
                frequency.push(g.x[f] * 1e6);
                delay.push(g.y[r] * 1000. / C * 1e6);
                power.push(g.values[r * g.nx + f]);
            }
        }
        group
            .new_dataset_builder()
            .with_data(&frequency)
            .create("frequency_hz")?;
        group
            .new_dataset_builder()
            .with_data(&delay)
            .create("strongest_echo_delay_us")?;
        group
            .new_dataset_builder()
            .with_data(&power)
            .create("snr_db")?;
        let resolution = (g.y[1] - g.y[0]).abs() * 1000. / C * 1e6;
        group
            .new_dataset::<f64>()
            .create("delay_bin_us")?
            .write_scalar(&resolution)?;
        text(&out, &format!("mode_{mode}/polarization"), &g.mode)?;
        text(&out, &format!("mode_{mode}/receiver"), &g.station)?;
        text(&out, &format!("mode_{mode}/transmitter"), &g.transmitter)?;
        scalar_out(&out, &format!("mode_{mode}/time_unix"), g.timestamp)?;
    }
    scalar_out(
        &out,
        "receiver_delay_us",
        scalar(&h, "receiver_delay_us", 0.),
    )?;
    scalar_out(
        &out,
        "physical_ipp_us",
        scalar(&h, "physical_ipp_us", scalar(&h, "ipp_us", f64::NAN)),
    )?;
    scalar_out(&out, "range_min_km", lo)?;
    scalar_out(&out, "range_max_km", hi)?;
    scalar_out(&out, "threshold_db", threshold as f64)?;
    text(&out,"interpretation","Strongest retained echo per frequency within requested gate; not necessarily ground wave or first arrival. Missing sparse bins are unobserved. Periodic code/pulse delays may be ambiguous by the physical PRI; no automatic unwrapping or clock adjustment.")?;
    println!("Delay peaks saved; receiver timing unchanged.");
    Ok(())
}
fn main() -> Result<()> {
    let a: Vec<_> = std::env::args().collect();
    match a
        .get(1)
        .context("usage: iono-delay peaks|calibrate ...")?
        .as_str()
    {
        "peaks" => peaks(&a),
        "calibrate" => {
            ensure!(
                a.len() == 4,
                "usage: iono-delay calibrate REFERENCE.h5 PROPOSAL.h5"
            );
            calibrate(&a[2], &a[3])
        }
        _ => anyhow::bail!("unknown delay diagnostic"),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn residual_sign_and_systematic_uncertainty_are_preserved() {
        let (b, u) = estimate(&[113., 113., 113.], &[100., 100., 100.], &[2., 2., 2.]).unwrap();
        assert_eq!(b, 13.);
        assert_eq!(u, 2.);
    }
    #[test]
    fn one_outlier_does_not_move_the_fixed_bias_estimate() {
        assert_eq!(
            estimate(&[13., 13., 13., 13., 9000.], &[0.; 5], &[1.; 5]).unwrap(),
            (13., 1.)
        );
        assert!(estimate(&[f64::NAN; 3], &[0.; 3], &[1.; 3]).is_err());
        assert!(estimate(&[0.; 2], &[0.; 2], &[1.; 2]).is_err());
    }
}

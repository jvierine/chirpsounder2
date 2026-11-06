//! Fixed receiver timing, independent of transmitter timing and range gates.
//! Positive delay means recorded timestamps are late relative to antenna input.
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::OnceLock};
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct ReceiverTiming {
    pub station: String,
    pub revision: String,
    pub delay_us: f64,
    pub uncertainty_us: Option<f64>,
    pub status: String,
    pub reference: String,
    #[serde(default)]
    pub valid_from_unix: f64,
    #[serde(default)]
    pub valid_until_unix: Option<f64>,
}
static FIXED: OnceLock<ReceiverTiming> = OnceLock::new();
impl ReceiverTiming {
    pub fn nominal(station: &str) -> Self {
        Self {station:station.into(),revision:"gps-sample-time-zero-additional-correction-v1".into(),delay_us:0.,uncertainty_us:None,status:"nominal".into(),reference:"Existing UHD/GPS sample-time convention, preserved without an additional correction".into(),valid_from_unix:0.,valid_until_unix:None}
    }
    pub fn validate(&self, station: &str) -> Result<()> {
        ensure!(
            self.station == station,
            "receiver timing belongs to another station"
        );
        ensure!(
            self.delay_us.is_finite() && self.delay_us.abs() <= 10000.,
            "receiver correction exceeds timing-calibration bounds"
        );
        ensure!(
            !self.revision.is_empty() && !self.reference.is_empty(),
            "timing revision and independent reference required"
        );
        ensure!(
            self.uncertainty_us.is_none_or(|v| v.is_finite() && v >= 0.),
            "invalid timing uncertainty"
        );
        ensure!(
            self.valid_from_unix.is_finite()
                && self.valid_from_unix >= 0.
                && self
                    .valid_until_unix
                    .is_none_or(|v| v.is_finite() && v > self.valid_from_unix),
            "invalid timing validity interval"
        );
        ensure!(
            ["nominal", "measured"].contains(&self.status.as_str()),
            "unknown receiver calibration status"
        );
        ensure!(
            self.status != "measured" || self.uncertainty_us.is_some(),
            "measured timing needs an uncertainty"
        );
        ensure!(
            self.status != "nominal" || self.delay_us == 0.,
            "a nonzero receiver correction requires a measured calibration"
        );
        Ok(())
    }
    pub fn sample_index(&self, nominal: u64, sr: u64) -> Result<u64> {
        ensure!(sr > 0, "invalid sample rate");
        let t = nominal as f64 / sr as f64;
        ensure!(
            t >= self.valid_from_unix && self.valid_until_unix.is_none_or(|v| t < v),
            "receiver timing revision is not valid for this observation"
        );
        let shift = (self.delay_us * sr as f64 / 1e6).round() as i64;
        nominal
            .checked_add_signed(shift)
            .context("receiver correction exceeds sample index bounds")
    }
}
pub fn initialize(station: &str) -> Result<()> {
    let explicit = std::env::var_os("IONO_RECEIVER_TIMING_FILE");
    let path = explicit.clone().map(PathBuf::from).or_else(|| {
        std::env::var_os("HOME")
            .map(|h| PathBuf::from(h).join(".config/chirpsounder2/receiver-timing.json"))
    });
    let timing = if let Some(p) = path {
        if p.exists() {
            serde_json::from_slice::<ReceiverTiming>(&std::fs::read(p)?)?
        } else {
            ensure!(
                explicit.is_none(),
                "explicit receiver timing file is absent"
            );
            ReceiverTiming::nominal(station)
        }
    } else {
        ReceiverTiming::nominal(station)
    };
    timing.validate(station)?;
    if let Some(old) = FIXED.get() {
        ensure!(
            old == &timing,
            "receiver timing cannot change within a worker"
        );
        return Ok(());
    }
    FIXED
        .set(timing)
        .map_err(|_| anyhow::anyhow!("receiver timing was concurrently initialized"))?;
    Ok(())
}
pub fn corrected_sample_index(n: u64, sr: u64) -> Result<u64> {
    if let Some(t) = FIXED.get() {
        t.sample_index(n, sr)
    } else {
        Ok(n)
    }
}
pub fn utc_from_recorded(t: f64) -> f64 {
    t - FIXED.get().map_or(0., |p| p.delay_us) * 1e-6
}
pub fn write_metadata(h: &hdf5::File) -> Result<()> {
    let fallback = ReceiverTiming::nominal("unspecified");
    let p = FIXED.get().unwrap_or(&fallback);
    for (name, value) in [
        ("receiver_delay_us", p.delay_us),
        ("receiver_timing_valid_from_unix", p.valid_from_unix),
        (
            "receiver_timing_valid_until_unix",
            p.valid_until_unix.unwrap_or(f64::NAN),
        ),
        (
            "receiver_delay_uncertainty_us",
            p.uncertainty_us.unwrap_or(f64::NAN),
        ),
    ] {
        h.new_dataset::<f64>().create(name)?.write_scalar(&value)?;
    }
    for (name, value) in [
        ("receiver_timing_station", p.station.as_str()),
        ("receiver_timing_revision", p.revision.as_str()),
        ("receiver_timing_status", p.status.as_str()),
        ("receiver_timing_reference", p.reference.as_str()),
    ] {
        h.new_dataset::<hdf5::types::VarLenUnicode>()
            .create(name)?
            .write_scalar(&value.parse::<hdf5::types::VarLenUnicode>()?)?;
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_single_fixed_delay_is_applied_without_rounding_the_absolute_index() {
        let mut p = ReceiverTiming::nominal("DOB");
        p.delay_us = 1.24;
        p.status = "measured".into();
        p.uncertainty_us = Some(0.04);
        p.validate("DOB").unwrap();
        let i = 44_849_298_128_000_000;
        assert_eq!(p.sample_index(i, 25_000_000).unwrap(), i + 31);
        assert_eq!(
            p.sample_index(i + 900 * 25_000_000, 25_000_000).unwrap(),
            i + 900 * 25_000_000 + 31
        );
        p.delay_us = -1.24;
        assert_eq!(p.sample_index(i, 25_000_000).unwrap(), i - 31);
    }
    #[test]
    fn nominal_preserves_existing_samples_and_does_not_claim_calibration() {
        let p = ReceiverTiming::nominal("TGO");
        p.validate("TGO").unwrap();
        assert!(p.uncertainty_us.is_none());
        assert_eq!(p.sample_index(100000000, 25_000_000).unwrap(), 100000000);
        let mut invalid = p.clone();
        invalid.delay_us = 1.;
        assert!(invalid.validate("TGO").is_err());
    }
    #[test]
    fn station_mismatch_and_out_of_validity_are_rejected() {
        let mut p = ReceiverTiming::nominal("DOB");
        assert!(p.validate("TGO").is_err());
        p.valid_from_unix = 100.;
        p.valid_until_unix = Some(200.);
        assert!(p.sample_index(99 * 25_000_000, 25_000_000).is_err());
        assert!(p.sample_index(200 * 25_000_000, 25_000_000).is_err());
    }
}

pub fn metadata_compatible(h: &hdf5::File) -> bool {
    let value = crate::scalar(h, "receiver_delay_us", 0.);
    let expected = FIXED.get().map_or(0., |p| p.delay_us);
    let expected_revision = FIXED
        .get()
        .map_or("gps-sample-time-zero-additional-correction-v1", |p| {
            p.revision.as_str()
        });
    let revision_ok = if h.link_exists("receiver_timing_revision") {
        h.dataset("receiver_timing_revision")
            .ok()
            .and_then(|d| d.read_scalar::<hdf5::types::VarLenUnicode>().ok())
            .is_some_and(|s| s.as_str() == expected_revision)
    } else {
        expected == 0. && expected_revision == "gps-sample-time-zero-additional-correction-v1"
    };
    value.is_finite() && (value - expected).abs() < 1e-9 && revision_ok
}

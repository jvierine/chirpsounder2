//! Assess the latest sampled band on each radio path without carrying older
//! positive detections across a newer non-detection. This is evidence status,
//! not an operational LUF/MUF confidence estimate.
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// Quality policy applies to archived and new rows alike. Diagnostic samples
/// stay on disk; excluded estimates cannot masquerade as quantitative limits.
pub fn measurement_quality(mut row: Value) -> Value {
    let receiver = row["receiver"].as_str().unwrap_or("");
    let tx = row["transmitter"].as_str().unwrap_or("");
    let excluded = receiver == "W2NAF" && ["Australia", "Cyprus", "JORN", "NIC"].contains(&tx);
    let limited = receiver == "W2NAF" && (tx == "US" || tx.starts_with("ROTHR"));
    let commissioning = !excluded && row["quantitative_eligible"] == false;
    row["quantitative_eligible"] = json!(!excluded && !commissioning);
    row["measurement_quality"] = json!(if commissioning {
        "commissioning"
    } else if excluded {
        "cross-modulation-excluded"
    } else if limited {
        "limited-reliability"
    } else {
        "experimental"
    });
    if excluded || commissioning {
        row["diagnostic_band_mhz"] = json!([row["lower_mhz"], row["upper_mhz"]]);
        row["lower_mhz"] = Value::Null;
        row["upper_mhz"] = Value::Null;
        row["bands"] = json!([]);
    }
    row
}

pub fn assess(records: &Value, selected_day: &str, now: f64) -> Vec<Value> {
    let today = chrono::DateTime::from_timestamp(now as i64, 0)
        .unwrap()
        .format("%Y-%m-%d")
        .to_string();
    let mut paths = BTreeMap::<(String, String), Vec<&Value>>::new();
    for row in records.as_array().into_iter().flatten() {
        if row["time"].as_f64().is_some_and(f64::is_finite) {
            paths
                .entry((
                    row["transmitter"].as_str().unwrap_or("").into(),
                    row["receiver"].as_str().unwrap_or("").into(),
                ))
                .or_default()
                .push(row);
        }
    }
    paths
        .into_values()
        .map(|mut rows| {
            rows.sort_by(|a, b| {
                a["time"]
                    .as_f64()
                    .unwrap()
                    .total_cmp(&b["time"].as_f64().unwrap())
            });
            let mut intervals: Vec<_> = rows
                .windows(2)
                .map(|p| p[1]["time"].as_f64().unwrap() - p[0]["time"].as_f64().unwrap())
                .filter(|d| *d > 0.)
                .collect();
            intervals.sort_by(f64::total_cmp);
            // Report the measured cadence and grace period explicitly. Clamp only
            // the fallback freshness policy, never the scientific measurement.
            let cadence = intervals.get(intervals.len() / 2).copied().unwrap_or(300.);
            let stale_after = (3. * cadence.clamp(15., 3600.)).max(900.);
            let mut latest = (*rows.last().unwrap()).clone();
            let time = latest["time"].as_f64().unwrap();
            let age = (now - time).max(0.);
            let freshness = if time > now + 120. {
                "clock-ahead"
            } else if selected_day != today {
                "archived"
            } else if age > stale_after {
                "stale"
            } else {
                "current"
            };
            let mut flags = vec![];
            if latest["measurement_quality"] == "commissioning" {
                flags.push("commissioning");
            } else if latest["lower_mhz"].is_null() {
                flags.push("no-supported-band");
            } else {
                if latest["lower_censored"] == true || latest["upper_censored"] == true {
                    flags.push("sampling-edge-limited");
                }
                if latest["bands"].as_array().is_some_and(|b| b.len() > 1) {
                    flags.push("multiple-bands");
                }
                if flags.is_empty() {
                    flags.push("supported");
                }
            }
            latest["freshness"] = json!(freshness);
            latest["age_s"] = json!(age);
            latest["expected_interval_s"] = json!(cadence);
            latest["stale_after_s"] = json!(stale_after);
            latest["quality_flags"] = json!(flags);
            if latest["measurement_quality"] == "cross-modulation-excluded" {
                latest["quality_flags"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!("cross-modulation-excluded"));
            } else if latest["measurement_quality"] == "limited-reliability" {
                latest["quality_flags"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!("limited-reliability"));
            }
            latest
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    const MIDNIGHT: f64 = 1792627200.;
    #[test]
    fn commissioning_preserves_diagnostics_without_publishing_usable_limits() {
        let row = measurement_quality(
            json!({"receiver":"W2NAF","transmitter":"new-site","quantitative_eligible":false,"lower_mhz":2.,"upper_mhz":16.,"bands":[[2.,16.,50]],"time":MIDNIGHT+60.}),
        );
        assert_eq!(row["measurement_quality"], "commissioning");
        assert_eq!(row["diagnostic_band_mhz"], json!([2., 16.]));
        assert!(row["upper_mhz"].is_null());
        assert_eq!(row["quantitative_eligible"], false);
        assert_eq!(
            assess(&json!([row]), "2026-10-22", MIDNIGHT + 90.)[0]["quality_flags"],
            json!(["commissioning"])
        );
    }
    #[test]
    fn w2naf_quality_excludes_cross_modulation_and_retains_rothr_with_flags() {
        for tx in ["Australia", "Cyprus", "JORN", "NIC"] {
            let r = measurement_quality(
                json!({"receiver":"W2NAF","transmitter":tx,"lower_mhz":3.,"upper_mhz":12.,"bands":[[3.,12.,20]]}),
            );
            assert_eq!(r["quantitative_eligible"], false);
            assert!(r["lower_mhz"].is_null());
            assert_eq!(r["bands"], json!([]));
        }
        let r = measurement_quality(
            json!({"receiver":"W2NAF","transmitter":"ROTHR1","lower_mhz":3.,"upper_mhz":12.}),
        );
        assert_eq!(r["quantitative_eligible"], true);
        assert_eq!(r["measurement_quality"], "limited-reliability");
        assert_eq!(r["upper_mhz"], 12.);
        let r = measurement_quality(
            json!({"receiver":"TGO","transmitter":"Australia","upper_mhz":12.}),
        );
        assert_eq!(r["upper_mhz"], 12.);
    }
    #[test]
    fn latest_non_detection_is_not_replaced_by_an_older_positive_band() {
        let records = json!([
            {"transmitter":"SGO","receiver":"TGO","time":MIDNIGHT+60.,"lower_mhz":2.,"upper_mhz":7.,"bands":[[2.,7.,101]]},
            {"transmitter":"SGO","receiver":"TGO","time":MIDNIGHT+120.,"lower_mhz":null,"upper_mhz":null,"bands":[]}
        ]);
        let row = &assess(&records, "2026-10-22", MIDNIGHT + 150.)[0];
        assert!(row["lower_mhz"].is_null());
        assert_eq!(row["quality_flags"], json!(["no-supported-band"]));
        assert_eq!(row["freshness"], "current");
        let stale = assess(&records, "2026-10-22", MIDNIGHT + 1021.);
        assert_eq!(stale[0]["freshness"], "stale");
        assert_eq!(stale[0]["stale_after_s"], 900.);
    }
    #[test]
    fn cadence_archive_and_clock_status_are_distinct() {
        let records=json!([0.,450.,900.,1350.].map(|t|json!({"transmitter":"Ramfjordmoen","receiver":"TGO","time":MIDNIGHT+t,"lower_mhz":null})));
        let row = &assess(&records, "2026-10-22", MIDNIGHT + 2000.)[0];
        assert_eq!(row["expected_interval_s"], 450.);
        assert_eq!(row["stale_after_s"], 1350.);
        assert_eq!(row["freshness"], "current");
        assert_eq!(
            assess(&records, "2026-10-22", MIDNIGHT + 86400.)[0]["freshness"],
            "archived"
        );
        assert_eq!(
            assess(&records, "2026-10-22", MIDNIGHT)[0]["freshness"],
            "clock-ahead"
        );
    }
    #[test]
    fn sampling_edges_and_disjoint_bands_remain_explicit() {
        let records = json!([{"transmitter":"SGO","receiver":"TGO","time":MIDNIGHT+60.,"lower_mhz":2.,"upper_mhz":7.,"lower_censored":true,"bands":[[2.,3.,21],[6.,7.,21]]}]);
        assert_eq!(
            assess(&records, "2026-10-22", MIDNIGHT + 90.)[0]["quality_flags"],
            json!(["sampling-edge-limited", "multiple-bands"])
        );
    }
}

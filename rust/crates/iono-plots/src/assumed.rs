//! Provisional transmitter associations using the existing virtual-range gates.
//! Compute beside the receiver, from clustered detections, never on juha.no.
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub(crate) struct Source {
    pub(crate) id: String,
    label: String,
    pub(crate) gates: Vec<[f64; 2]>,
}
pub(crate) fn sources(station: &str) -> Vec<Source> {
    let model: Value =
        serde_json::from_str(include_str!("../../../web/transmitters.json")).unwrap();
    let mut result = BTreeMap::<String, Source>::new();
    for band in model[station].as_array().into_iter().flatten() {
        let name = band["name"].as_str().unwrap();
        let (id, label) = match name {
            "NIC" => ("Cyprus", "Cyprus"),
            "JORN" => ("Australia", "Australia"),
            _ if name.starts_with("ROTHR") && station != "W2NAF" => ("US", "US"),
            _ if name.starts_with("ROTHR") => (name, band["label"].as_str().unwrap()),
            _ => continue,
        };
        result
            .entry(id.into())
            .or_insert_with(|| Source {
                id: id.into(),
                label: label.into(),
                gates: vec![],
            })
            .gates
            .push([
                band["min_km"].as_f64().unwrap(),
                band["max_km"].as_f64().unwrap(),
            ]);
    }
    // A detection in overlapping W2NAF US gates remains a US association.
    if station == "W2NAF" {
        let gates = result
            .values()
            .filter(|s| s.id.starts_with("ROTHR"))
            .flat_map(|s| s.gates.iter().copied())
            .collect();
        result.insert(
            "US".into(),
            Source {
                id: "US".into(),
                label: "US".into(),
                gates,
            },
        );
    }
    result.into_values().collect()
}

pub fn envelopes(rows: &[[f64; 5]], station: &str, start: f64) -> Vec<Value> {
    let sources = sources(station);
    // Keep an entire sweep together using its extrapolated zero-frequency
    // epoch and rate. Minute bins would truncate a 2–4 minute sweep's band.
    // Timestamp each envelope at its last measured point, avoiding look-ahead.
    let mut windows = BTreeMap::<(i64, i64), (f64, Vec<Vec<f64>>)>::new();
    for rate in [100000., 125000.] {
        for p in crate::render::clustered(rows, rate) {
            if !(start..start + 86400.).contains(&p[1]) || !(0.0..=25.).contains(&p[2]) {
                continue;
            }
            let delay = p[0].rem_euclid(1.);
            let range = (if delay > 0.5 { delay - 1. } else { delay }) * iono_core::C / 1000.;
            let index = (p[0].floor() as i64, rate as i64);
            let window = windows
                .entry(index)
                .or_insert_with(|| (p[1], vec![vec![]; sources.len()]));
            window.0 = window.0.max(p[1]);
            let matches: Vec<_> = sources
                .iter()
                .enumerate()
                .filter(|(_, s)| s.id != "US" || station != "W2NAF")
                .filter(|(_, s)| s.gates.iter().any(|g| range >= g[0] && range <= g[1]))
                .map(|(i, _)| i)
                .collect();
            let us: Vec<_> = matches
                .iter()
                .copied()
                .filter(|&i| sources[i].id.starts_with("ROTHR"))
                .collect();
            for i in matches {
                if station == "W2NAF" && sources[i].id.starts_with("ROTHR") && us.len() > 1 {
                    continue;
                }
                window.1[i].push(p[2]);
            }
            if us.len() > 1 {
                if let Some(i) = sources.iter().position(|s| s.id == "US") {
                    window.1[i].push(p[2]);
                }
            }
        }
    }
    let mut result = vec![];
    for (_, (time, frequencies)) in windows {
        for (source, mut f) in sources.iter().zip(frequencies) {
            // Other radars' activity does not establish a non-detection for
            // this unknown schedule. An absent source ages into stale instead.
            if f.is_empty() {
                continue;
            }
            // Count occupied 100 kHz cells, not repeated FFT detections. Values
            // remain the actual detected frequencies; these are envelopes, not
            // contiguous-band assertions or validated usable frequencies.
            f.sort_by(f64::total_cmp);
            let count = f
                .iter()
                .map(|v| (v / 0.1).floor() as i64)
                .collect::<std::collections::BTreeSet<_>>()
                .len();
            let detected = count >= 3;
            let low = detected.then(|| f[0]);
            let high = detected.then(|| *f.last().unwrap());
            let gate = [
                source
                    .gates
                    .iter()
                    .map(|g| g[0])
                    .fold(f64::INFINITY, f64::min),
                source
                    .gates
                    .iter()
                    .map(|g| g[1])
                    .fold(f64::NEG_INFINITY, f64::max),
            ];
            result.push(json!({"transmitter":source.id,"source_label":format!("{} (assumed)",source.label),"receiver":station,"time":time,"lower_mhz":low,"upper_mhz":high,"bands":if detected {json!([[low,high,count]])}else{json!([])},"support_bins":if detected {count}else{0},"range_gate_km":gate,"range_gates_km":source.gates,"method":"range-gated-sweep-envelope-v1","assumed":true,"experimental":true,"lower_censored":false,"upper_censored":false}));
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn rows(start: f64, range: f64) -> Vec<[f64; 5]> {
        (0..10)
            .map(|i| {
                [
                    start + range * 1000. / iono_core::C,
                    start + 100. + i as f64,
                    5. + i as f64 * 0.1,
                    100000.,
                    20.,
                ]
            })
            .collect()
    }
    #[test]
    fn long_range_sources_are_countries_and_time_is_measurement_time() {
        let start = 1793750400.;
        let result = envelopes(&rows(start, 4600.), "TGO", start);
        let cyprus = result
            .iter()
            .find(|r| r["transmitter"] == "Cyprus")
            .unwrap();
        assert_eq!(cyprus["lower_mhz"], 5.);
        assert_eq!(cyprus["upper_mhz"], 5.9);
        assert_eq!(cyprus["time"], start + 109.);
        assert_eq!(cyprus["assumed"], true);
        assert!(result
            .iter()
            .all(|r| !r["transmitter"].as_str().unwrap().starts_with("ROTHR")));
        assert!(sources("TGO").iter().any(|s| s.id == "Australia"));
        assert!(result.iter().all(|r| r["transmitter"] != "US"));
    }
    #[test]
    fn a_multi_minute_sweep_keeps_its_full_frequency_envelope() {
        let start = 1793750400.;
        let mut detections = rows(start, 4600.);
        for (i, p) in detections.iter_mut().enumerate() {
            p[1] = start + 100. + i as f64 * 20.;
            p[2] = 5. + i as f64 * 0.5;
        }
        let result = envelopes(&detections, "TGO", start);
        let band = result
            .iter()
            .find(|r| r["transmitter"] == "Cyprus")
            .unwrap();
        assert_eq!(band["lower_mhz"], 5.);
        assert_eq!(band["upper_mhz"], 9.5);
        assert_eq!(band["time"], start + 280.);
        assert_eq!(
            result
                .iter()
                .filter(|r| r["transmitter"] == "Cyprus")
                .count(),
            1
        );
    }
    #[test]
    fn w2naf_distinguishes_unique_paths_and_does_not_duplicate_ambiguous_detections() {
        let start = 1793750400.;
        let near = envelopes(&rows(start, 500.), "W2NAF", start);
        assert!(near
            .iter()
            .any(|r| r["transmitter"] == "ROTHR1" && !r["lower_mhz"].is_null()));
        let overlap = envelopes(&rows(start, 3000.), "W2NAF", start);
        assert!(overlap
            .iter()
            .any(|r| r["transmitter"] == "US" && !r["lower_mhz"].is_null()));
        assert!(overlap
            .iter()
            .filter(|r| r["transmitter"].as_str().unwrap().starts_with("ROTHR"))
            .all(|r| r["lower_mhz"].is_null()));
    }
}

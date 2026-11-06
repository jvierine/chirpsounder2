//! Receiver-network LFM coincidence analysis. Coordinates remain candidates:
//! geometric and spherical virtual-height models are not calibrated ray tracing.
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf};
const EARTH_KM: f64 = 6371.0088;
const C_KM_S: f64 = 299792.458;
#[derive(Clone, Deserialize, Serialize)]
pub struct Receiver {
    pub station: String,
    pub latitude: f64,
    pub longitude: f64,
    /// False for cross-modulation-prone or uncalibrated timing inputs.
    pub quantitative: bool,
    pub files: Vec<PathBuf>,
}
#[derive(Clone, Deserialize, Serialize)]
pub struct Settings {
    pub receivers: Vec<Receiver>,
    pub minimum_snr: f64,
    pub minimum_common_bands: usize,
    pub minimum_events: usize,
    pub grid_step_deg: f64,
}
#[derive(Clone, Copy)]
pub struct Detection {
    pub receiver: usize,
    pub epoch: f64,
    pub frequency: f64,
    pub rate: u32,
    pub snr: f64,
}
#[derive(Clone, Serialize)]
pub struct Event {
    pub epoch: f64,
    pub rate: u32,
    pub receivers: Vec<usize>,
    pub delay_s: Vec<f64>,
    pub common_bands: usize,
    pub band_scatter_s: f64,
}
#[derive(Serialize)]
pub struct Family {
    pub rate: u32,
    pub receivers: Vec<usize>,
    pub events: Vec<Event>,
    pub delay_s: Vec<f64>,
    pub scatter_s: Vec<f64>,
}
pub fn quantile(v: &mut [f64], p: f64) -> f64 {
    v.sort_by(f64::total_cmp);
    let t = (v.len() - 1) as f64 * p;
    v[t.floor() as usize] * (1. - t.fract()) + v[t.ceil() as usize] * t.fract()
}
pub fn load(c: &Settings) -> Result<Vec<Detection>> {
    ensure!(
        c.receivers.len() >= 3 && c.receivers.len() <= 16,
        "need 3..16 receivers"
    );
    ensure!(
        c.minimum_common_bands > 0 && c.minimum_events >= 3,
        "insufficient support settings"
    );
    ensure!(
        (0.5..=5.).contains(&c.grid_step_deg),
        "grid step outside 0.5..5 degrees"
    );
    let mut result = vec![];
    for (i, r) in c.receivers.iter().enumerate() {
        ensure!(
            r.latitude.abs() <= 90. && r.longitude.abs() <= 180.,
            "invalid station coordinates"
        );
        ensure!(
            r.files.len() <= 2000,
            "use bounded explicit input manifests"
        );
        for path in &r.files {
            let h = hdf5::File::open(path)?;
            let d = h.dataset("data")?;
            ensure!(
                d.shape().len() == 2 && d.shape()[1] == 5 && d.size() <= 500000,
                "invalid detection table"
            );
            for row in d.read_raw::<f64>()?.chunks_exact(5) {
                if row.iter().all(|x| x.is_finite())
                    && row[4] >= c.minimum_snr
                    && (1e6..24.5e6).contains(&row[2])
                    && [100000., 125000.].contains(&row[3])
                {
                    result.push(Detection {
                        receiver: i,
                        epoch: row[0],
                        frequency: row[2],
                        rate: row[3] as u32,
                        snr: row[4],
                    });
                }
            }
        }
    }
    result.sort_by(|a, b| {
        a.rate
            .cmp(&b.rate)
            .then(a.epoch.total_cmp(&b.epoch))
            .then(a.receiver.cmp(&b.receiver))
            .then(a.frequency.total_cmp(&b.frequency))
    });
    // Overlapping/repeated input files must not inflate support.
    result.dedup_by(|a, b| {
        a.receiver == b.receiver
            && a.rate == b.rate
            && a.epoch == b.epoch
            && a.frequency == b.frequency
    });
    Ok(result)
}
pub fn events(c: &Settings, rows: &[Detection]) -> Vec<Event> {
    let mut out = vec![];
    let mut start = 0;
    while start < rows.len() {
        let first = rows[start];
        let mut end = start + 1;
        while end < rows.len()
            && rows[end].rate == first.rate
            && rows[end].epoch - first.epoch <= 0.08
        {
            end += 1;
        }
        let g = &rows[start..end];
        let mut bins = BTreeMap::<(usize, u32), Vec<f64>>::new();
        for r in g {
            if c.receivers[r.receiver].quantitative {
                bins.entry((r.receiver, (r.frequency / 1e6).floor() as u32))
                    .or_default()
                    .push(r.epoch - first.epoch);
            }
        }
        let mut centers = BTreeMap::new();
        for (key, mut v) in bins {
            if v.len() >= 3 {
                v.sort_by(f64::total_cmp);
                // Reject blended source epochs or strongly split propagation modes.
                if v[v.len() - 1] - v[0] < 0.008 {
                    centers.insert(key, quantile(&mut v, 0.2));
                }
            }
        }
        let ids: Vec<_> = (0..c.receivers.len())
            .filter(|&i| centers.keys().filter(|(j, _)| *j == i).count() >= c.minimum_common_bands)
            .collect();
        if ids.len() >= 3 {
            let mut delays = vec![vec![]; ids.len()];
            for band in 1..25 {
                if ids.iter().all(|i| centers.contains_key(&(*i, band))) {
                    let reference = centers[&(ids[0], band)];
                    for (k, &i) in ids.iter().enumerate() {
                        delays[k].push(centers[&(i, band)] - reference);
                    }
                }
            }
            if delays[0].len() >= c.minimum_common_bands {
                let count = delays[0].len();
                let med: Vec<_> = delays.iter_mut().map(|v| quantile(v, 0.5)).collect();
                let scatter = delays
                    .iter()
                    .zip(&med)
                    .flat_map(|(v, m)| v.iter().map(move |x| (x - m).abs()))
                    .fold(0., f64::max);
                if scatter <= 0.001 {
                    out.push(Event {
                        epoch: first.epoch,
                        rate: first.rate,
                        receivers: ids,
                        delay_s: med,
                        common_bands: count,
                        band_scatter_s: scatter,
                    });
                }
            }
        }
        start = end;
    }
    out
}
pub fn families(events: &[Event], minimum: usize) -> Vec<Family> {
    // Connected components retain distinct receiver geometries. These are
    // timing families, never an assertion that one cluster equals one site.
    let mut labels = vec![usize::MAX; events.len()];
    let mut out = vec![];
    for seed in 0..events.len() {
        if labels[seed] != usize::MAX {
            continue;
        }
        labels[seed] = seed;
        let mut queue = vec![seed];
        let mut pos = 0;
        while pos < queue.len() {
            let a = &events[queue[pos]];
            pos += 1;
            for (j, b) in events.iter().enumerate() {
                if labels[j] == usize::MAX
                    && a.rate == b.rate
                    && a.receivers == b.receivers
                    && a.delay_s
                        .iter()
                        .zip(&b.delay_s)
                        .map(|(x, y)| (x - y).powi(2))
                        .sum::<f64>()
                        .sqrt()
                        <= 0.00065
                {
                    labels[j] = seed;
                    queue.push(j);
                }
            }
        }
        if queue.len() < minimum {
            continue;
        }
        let es: Vec<_> = queue.iter().map(|&i| events[i].clone()).collect();
        let med: Vec<_> = (0..es[0].delay_s.len())
            .map(|i| {
                quantile(
                    &mut es.iter().map(|e| e.delay_s[i]).collect::<Vec<_>>(),
                    0.5,
                )
            })
            .collect();
        let scatter = (0..med.len())
            .map(|i| {
                1.4826
                    * quantile(
                        &mut es
                            .iter()
                            .map(|e| (e.delay_s[i] - med[i]).abs())
                            .collect::<Vec<_>>(),
                        0.5,
                    )
            })
            .collect();
        out.push(Family {
            rate: es[0].rate,
            receivers: es[0].receivers.clone(),
            events: es,
            delay_s: med,
            scatter_s: scatter,
        });
    }
    out
}
pub fn distance_km(lat: f64, lon: f64, r: &Receiver) -> f64 {
    let (a, b, x, y) = (
        lat.to_radians(),
        lon.to_radians(),
        r.latitude.to_radians(),
        r.longitude.to_radians(),
    );
    EARTH_KM
        * (a.sin() * x.sin() + a.cos() * x.cos() * (b - y).cos())
            .clamp(-1., 1.)
            .acos()
}
pub fn path_km(distance: f64, height: f64) -> f64 {
    if height == 0. {
        return distance;
    }
    let max_hop = 2. * EARTH_KM * (EARTH_KM / (EARTH_KM + height)).acos();
    let hops = (distance / max_hop).ceil().max(1.);
    let angle = distance / (2. * hops * EARTH_KM);
    2. * hops * (height * height + 2. * EARTH_KM * (EARTH_KM + height) * (1. - angle.cos())).sqrt()
}
pub struct Surface {
    pub latitude: Vec<f64>,
    pub longitude: Vec<f64>,
    pub heights: Vec<f64>,
    pub rms_km: Vec<f64>,
    pub best: Vec<[f64; 3]>,
}
pub fn surface(c: &Settings, f: &Family) -> Surface {
    let latitude: Vec<_> = (0..=(180. / c.grid_step_deg) as usize)
        .map(|i| -90. + i as f64 * c.grid_step_deg)
        .collect();
    let longitude: Vec<_> = (0..(360. / c.grid_step_deg) as usize)
        .map(|i| -180. + i as f64 * c.grid_step_deg)
        .collect();
    let heights = vec![0., 110., 200., 300., 400.];
    let mut rms_km = vec![];
    let mut best = vec![];
    for &height in &heights {
        let mut optimum = [f64::INFINITY, 0., 0.];
        for &lat in &latitude {
            for &lon in &longitude {
                let ranges: Vec<_> = f
                    .receivers
                    .iter()
                    .map(|&i| path_km(distance_km(lat, lon, &c.receivers[i]), height))
                    .collect();
                let rms = (ranges
                    .iter()
                    .skip(1)
                    .zip(f.delay_s.iter().skip(1))
                    .map(|(r, t)| (r - ranges[0] - t * C_KM_S).powi(2))
                    .sum::<f64>()
                    / (ranges.len() - 1) as f64)
                    .sqrt();
                if rms < optimum[0] {
                    optimum = [rms, lat, lon];
                }
                rms_km.push(rms);
            }
        }
        best.push(optimum);
    }
    Surface {
        latitude,
        longitude,
        heights,
        rms_km,
        best,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn receiver(lat: f64, lon: f64) -> Receiver {
        Receiver {
            station: String::new(),
            latitude: lat,
            longitude: lon,
            quantitative: true,
            files: vec![],
        }
    }
    #[test]
    fn globe_and_skywave_geometry() {
        assert!((distance_km(0., 179., &receiver(0., -179.)) - 222.39).abs() < 0.1);
        for d in [100., 1000., 5000., 15000.] {
            assert!(path_km(d, 300.) >= d);
        }
    }
    #[test]
    fn known_source_has_zero_residual_on_global_grid() {
        let c = Settings {
            receivers: vec![
                receiver(62.073, 9.111),
                receiver(69.662, 18.939),
                receiver(78.148, 16.04),
                receiver(41.335, -75.601),
            ],
            minimum_snr: 16.,
            minimum_common_bands: 2,
            minimum_events: 5,
            grid_step_deg: 1.,
        };
        let ranges: Vec<_> = c
            .receivers
            .iter()
            .map(|r| distance_km(35., 33., r))
            .collect();
        let f = Family {
            rate: 100000,
            receivers: vec![0, 1, 2, 3],
            events: vec![],
            delay_s: ranges.iter().map(|x| (x - ranges[0]) / C_KM_S).collect(),
            scatter_s: vec![0.; 4],
        };
        let s = surface(&c, &f);
        assert!(s.best[0][0] < 1e-7);
        assert_eq!(&s.best[0][1..], &[35., 33.]);
    }
    #[test]
    fn cross_modulated_receiver_is_not_used_for_timing() {
        let mut c = Settings {
            receivers: vec![receiver(60., 0.); 4],
            minimum_snr: 16.,
            minimum_common_bands: 2,
            minimum_events: 5,
            grid_step_deg: 1.,
        };
        c.receivers[3].quantitative = false;
        let mut rows = vec![];
        for i in 0..4 {
            for band in [8., 9.] {
                for k in 0..3 {
                    rows.push(Detection {
                        receiver: i,
                        epoch: 1000. + i as f64 * 0.001,
                        frequency: band * 1e6 + k as f64 * 10000.,
                        rate: 100000,
                        snr: 20.,
                    });
                }
            }
        }
        let ev = events(&c, &rows);
        assert_eq!(ev.len(), 1);
        assert_eq!(ev[0].receivers, vec![0, 1, 2]);
        assert!((ev[0].delay_s[2] - 0.002).abs() < 1e-10);
    }

    #[test]
    fn different_frequency_bands_do_not_create_coincidences() {
        let c = Settings {
            receivers: vec![receiver(60., 0.); 3],
            minimum_snr: 16.,
            minimum_common_bands: 2,
            minimum_events: 5,
            grid_step_deg: 1.,
        };
        let mut rows = vec![];
        for i in 0..3 {
            for band in [5., 6.] {
                for k in 0..3 {
                    rows.push(Detection {
                        receiver: i,
                        epoch: 1000. + i as f64 * 0.001,
                        frequency: (band + i as f64 * 3.) * 1e6 + k as f64 * 10000.,
                        rate: 100000,
                        snr: 20.,
                    });
                }
            }
        }
        assert!(events(&c, &rows).is_empty());
    }
}

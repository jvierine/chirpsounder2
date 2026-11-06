//! Bounded 48-hour link panels, prepared from the receiver's indexed reductions.
use crate::{assumed, browser::grid, catalog::Catalog};
use anyhow::Result;
use iono_browser::Product;
use std::collections::BTreeMap;
// Stable three-day storage grid; the browser clips the requested last 48 h.
// Avoid shifting every cell (and defeating rsync) on every five-minute export.
const NX: usize = 2160;
const WINDOW: f64 = 259200.;
fn window_start(end: f64) -> f64 {
    (end / 86400.).floor() * 86400. - 172800.
}
const NY: usize = 256;
struct Panels {
    gate: [f64; 2],
    frequency: Vec<f32>,
    range: Vec<f32>,
    rsnr: Vec<f32>,
    fsnr: Vec<f32>,
}
impl Panels {
    fn new(gate: [f64; 2]) -> Self {
        Self {
            gate,
            frequency: vec![f32::NAN; NX * NY],
            range: vec![f32::NAN; NX * NY],
            rsnr: vec![f32::NEG_INFINITY; NX * NY],
            fsnr: vec![f32::NEG_INFINITY; NX * NY],
        }
    }
    fn point(&mut self, t: f64, r: f64, f: f64, snr: f32, start: f64) {
        if ![t, r, f].iter().all(|v| v.is_finite())
            || !snr.is_finite()
            || t < start
            || t >= start + WINDOW
            || r < self.gate[0]
            || r > self.gate[1]
            || !(0.0..25.).contains(&f)
        {
            return;
        }
        let x = ((t - start) / 120.).floor() as usize;
        let yr = (((r - self.gate[0]) / (self.gate[1] - self.gate[0]) * NY as f64).floor()
            as usize)
            .min(NY - 1);
        let yf = (f / 25. * NY as f64).floor() as usize;
        let k = yr * NX + x;
        if snr > self.rsnr[k] {
            self.rsnr[k] = snr;
            self.frequency[k] = f as f32;
        }
        let k = yf * NX + x;
        if snr > self.fsnr[k] {
            self.fsnr[k] = snr;
            self.range[k] = r as f32;
        }
    }
    fn products(self, station: &str, tx: &str, start: f64) -> Vec<Product> {
        let mut p = vec![
            grid(
                station,
                tx,
                "link-history",
                "peak-frequency",
                "turbo",
                start,
                NY,
                self.gate,
                [0., 25.],
                "Frequency (MHz)",
                self.frequency,
            ),
            grid(
                station,
                tx,
                "link-history",
                "range-frequency",
                "turbo",
                start,
                NY,
                [0., 25.],
                [self.gate[0] as f32, self.gate[1] as f32],
                "Virtual range (km)",
                self.range,
            ),
        ];
        for p in &mut p {
            p.grid.nx = NX;
            p.occupied_minutes = (0..NX)
                .filter(|&x| (0..NY).any(|y| p.grid.values[y * NX + x].is_finite()))
                .count();
            p.grid.x = (0..NX).map(|i| start + (i as f64 + 0.5) * 120.).collect();
        }
        p
    }
}
pub fn prepare(catalog: &Catalog, station: &str, end: f64) -> Result<Vec<Product>> {
    let start = window_start(end);
    let mut products = vec![];
    for tx in catalog.transmitters()? {
        let rows = catalog.profiles(&tx, start)?;
        let gate:Option<String>=catalog.db.query_row("SELECT payload FROM frequency_evidence WHERE tx=?1 AND time<=?2 ORDER BY time DESC LIMIT 1",rusqlite::params![tx,end],|r|r.get(0)).ok();
        let gate = gate
            .and_then(|s| serde_json::from_str::<iono_browser::Evidence>(&s).ok())
            .map(|e| e.range_gate_km);
        let Some(gate) = gate.filter(|g| g[1] > g[0]) else {
            continue;
        };
        let mut p = Panels::new(gate);
        for row in rows.iter().filter(|r| r.t < end) {
            for (i, &snr) in row.snr.iter().enumerate() {
                if snr >= iono_core::db(20.) {
                    p.point(
                        row.t,
                        row.y0 + i as f64 * row.dy,
                        row.freq[i] as f64,
                        snr,
                        start,
                    );
                }
            }
        }
        // Preserve the independent frequency reduction; range winners alone
        // erase broad E-region traces at nearly constant virtual range.
        p.range.fill(f32::NAN);
        p.fsnr.fill(f32::NEG_INFINITY);
        for row in catalog.frequency_profiles(&tx, start, end)? {
            for (i, &snr) in row.snr.iter().enumerate() {
                if snr >= iono_core::db(20.) {
                    let f = row.frequency[i] as f64;
                    let r = row.range[i] as f64;
                    if r >= gate[0] && r <= gate[1] && (0.0..25.).contains(&f) {
                        let x = ((row.t - start) / 120.).floor() as usize;
                        let y = (f / 25. * NY as f64).floor() as usize;
                        let k = y * NX + x;
                        if snr > p.fsnr[k] {
                            p.fsnr[k] = snr;
                            p.range[k] = r as f32;
                        }
                    }
                }
            }
        }
        products.extend(p.products(station, &tx, start));
    }
    let sources = assumed::sources(station);
    let mut panels: BTreeMap<_, _> = sources
        .iter()
        .map(|s| {
            let lo = s.gates.iter().map(|g| g[0]).fold(f64::INFINITY, f64::min);
            let hi = s
                .gates
                .iter()
                .map(|g| g[1])
                .fold(f64::NEG_INFINITY, f64::max);
            (s.id.clone(), Panels::new([lo, hi]))
        })
        .collect();
    let rows = catalog.detections(start)?;
    for rate in [100000., 125000.] {
        for row in crate::render::clustered(&rows, rate) {
            let delay = row[0].rem_euclid(1.);
            let range = (if delay > 0.5 { delay - 1. } else { delay }) * iono_core::C / 1000.;
            let matched: Vec<_> = sources
                .iter()
                .filter(|s| s.gates.iter().any(|g| range >= g[0] && range <= g[1]))
                .collect();
            for s in matched {
                panels
                    .get_mut(&s.id)
                    .unwrap()
                    .point(row[1], range, row[2], row[4] as f32, start);
            }
        }
    }
    for (tx, p) in panels {
        products.extend(p.products(station, &tx, start));
    }
    Ok(products)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stable_storage_still_covers_the_requested_sliding_48_hours() {
        let morning = 1793923200. + 9. * 3600.;
        let evening = 1793923200. + 21. * 3600.;
        assert_eq!(window_start(morning), window_start(evening));
        for end in [morning, evening] {
            let start = window_start(end);
            assert!(start <= end - 172800. && start + WINDOW >= end);
        }
        assert!(NX * NY <= iono_browser::MAX_CELLS);
    }
    #[test]
    fn bounded_window_filters_ranges_and_uses_strongest_measured_frequency() {
        let mut p = Panels::new([4000., 5000.]);
        let start = 1793750400.;
        p.point(start + 60., 4500., 8., 20., start);
        p.point(start + 61., 4500., 9., 30., start);
        p.point(start + 60., 2000., 19., 50., start);
        p.point(start - 1., 4500., 10., 50., start);
        p.point(start + 172799., 4600., 12., 25., start);
        let products = p.products("TGO", "Cyprus", start);
        assert_eq!(
            products[0]
                .grid
                .values
                .iter()
                .filter(|v| v.is_finite())
                .count(),
            2
        );
        assert!(products[0].grid.values.contains(&9.));
        assert!(!products[0].grid.values.contains(&19.));
        assert_eq!(products[0].grid.x[1439] - products[0].grid.x[0], 172680.);
        assert_eq!(products[1].grid.low, 4000.);
        assert_eq!(products[1].grid.high, 5000.);
    }
}

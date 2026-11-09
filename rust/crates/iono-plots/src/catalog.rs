//! Receiver-local index of retained observations and reduced profiles.
//! Separate from the web server's transport catalogue. Incremental indexing
//! avoids reopening every observation on each RTI/publication cycle.
use anyhow::{ensure, Result};
use chrono::{TimeZone, Utc};
use iono_core::{read_products, Grid};
use rusqlite::{params, Connection, OptionalExtension};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

pub struct Catalog {
    pub db: Connection,
    base: PathBuf,
    station: String,
    scanned: BTreeSet<String>,
}
#[derive(Clone, Debug)]
pub struct Profile {
    pub t: f64,
    pub tx: String,
    pub y0: f64,
    pub dy: f64,
    pub snr: Vec<f32>,
    pub freq: Vec<f32>,
}
#[derive(Clone, Debug)]
pub struct FrequencyProfile {
    pub t: f64,
    pub frequency: Vec<f32>,
    pub snr: Vec<f32>,
    pub range: Vec<f32>,
}
// Independent reduction: taking range-wise frequency winners first destroys
// broad traces at nearly constant range, notably E-region echoes.
pub fn reduce_frequency(g: &Grid) -> FrequencyProfile {
    let mut snr = vec![f32::NAN; g.nx];
    let mut range = snr.clone();
    for r in 0..g.ny {
        for f in 0..g.nx {
            let value = g.values[r * g.nx + f];
            if value.is_finite() && (!snr[f].is_finite() || value > snr[f]) {
                snr[f] = value;
                range[f] = g.y[r] as f32;
            }
        }
    }
    FrequencyProfile {
        t: g.timestamp,
        frequency: g.x.iter().map(|f| *f as f32).collect(),
        snr,
        range,
    }
}
pub fn pack(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}
pub fn unpack(v: &[u8]) -> Vec<f32> {
    v.chunks_exact(4)
        .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
        .collect()
}
fn identity(p: &Path) -> Option<(String, String, String, f64)> {
    let s = p.file_stem()?.to_str()?;
    let a: Vec<_> = s.split('-').collect();
    let t = a.last()?.parse::<f64>().ok()?;
    match *a.first()? {
        "digisonde_ionogram" | "prn_ionogram" if a.len() >= 4 => Some((
            "rti".into(),
            a[1..a.len() - 2].join("-"),
            a[a.len() - 2].into(),
            t,
        )),
        "lfm_ionogram" if a.len() >= 6 => Some((
            "rti".into(),
            a[1..a.len() - 4].join("-"),
            a[a.len() - 4].into(),
            t,
        )),
        "rf_spectrum" if a.len() >= 3 => Some(("spectrum".into(), String::new(), a[1].into(), t)),
        "cdetections" if a.len() >= 3 => Some(("detections".into(), String::new(), a[1].into(), t)),
        _ => None,
    }
}
pub fn reduce(g: &Grid) -> Profile {
    let mut snr = vec![f32::NAN; g.ny];
    let mut freq = snr.clone();
    for r in 0..g.ny {
        for f in 0..g.nx {
            let v = g.values[r * g.nx + f];
            if v.is_finite() && (!snr[r].is_finite() || v > snr[r]) {
                snr[r] = v;
                freq[r] = g.x[f] as f32;
            }
        }
    }
    Profile {
        t: g.timestamp,
        tx: g.transmitter.clone(),
        y0: g.y[0],
        dy: (g.y[g.ny - 1] - g.y[0]) / (g.ny - 1) as f64,
        snr,
        freq,
    }
}
// Use the recorded geometry. A fixed upper gate silently loses long paths.
fn frequency_profile(g: &Grid, station: &str, tx: &str, time: f64) -> iono_browser::Evidence {
    let lo =
        g.y.iter()
            .copied()
            .filter(|v| v.is_finite())
            .reduce(f64::min)
            .unwrap_or(100.)
            .max(100.);
    let hi =
        g.y.iter()
            .copied()
            .filter(|v| v.is_finite())
            .reduce(f64::max)
            .unwrap_or(lo)
            .max(lo);
    let mut samples: Vec<_> =
        g.x.iter()
            .enumerate()
            .map(|(f, &frequency)| {
                let peak = (0..g.ny)
                    .filter(|&r| g.y[r] >= lo && g.y[r] <= hi)
                    .map(|r| g.values[r * g.nx + f])
                    .filter(|v| v.is_finite())
                    .max_by(f32::total_cmp);
                (frequency, peak)
            })
            .collect();
    samples.sort_by(|a, b| a.0.total_cmp(&b.0));
    iono_browser::Evidence {
        transmitter: tx.into(),
        receiver: station.into(),
        time,
        frequency_mhz: samples.iter().map(|p| p.0).collect(),
        snr_db: samples.iter().map(|p| p.1).collect(),
        range_gate_km: [lo, hi],
    }
}
impl Catalog {
    pub fn open(base: &Path, store: &Path, station: &str) -> Result<Self> {
        fs::create_dir_all(store)?;
        let db = Connection::open(store.join("catalog.sqlite"))?;
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA cache_size=-8192;
 CREATE TABLE IF NOT EXISTS files(path TEXT PRIMARY KEY,mtime INTEGER,size INTEGER,kind TEXT,tx TEXT,time REAL);
 CREATE INDEX IF NOT EXISTS source_time ON files(kind,tx,time);
 CREATE TABLE IF NOT EXISTS profiles(path TEXT PRIMARY KEY,time REAL,tx TEXT,y0 REAL,dy REAL,snr BLOB,freq BLOB);
 CREATE INDEX IF NOT EXISTS profile_time ON profiles(tx,time);
 CREATE TABLE IF NOT EXISTS spectra(path TEXT PRIMARY KEY,time REAL,y0 REAL,dy REAL,power BLOB);
 CREATE TABLE IF NOT EXISTS detections(path TEXT PRIMARY KEY,data BLOB);")?;
        db.execute_batch("CREATE TABLE IF NOT EXISTS frequency_evidence(path TEXT PRIMARY KEY,time REAL,tx TEXT,payload TEXT); CREATE INDEX IF NOT EXISTS evidence_time ON frequency_evidence(time);")?;
        db.execute_batch("CREATE TABLE IF NOT EXISTS frequency_profiles(path TEXT PRIMARY KEY,time REAL,tx TEXT,frequency BLOB,snr BLOB,range BLOB); CREATE INDEX IF NOT EXISTS frequency_profiles_time ON frequency_profiles(time);")?;
        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS reduction_metadata(key TEXT PRIMARY KEY,value TEXT);",
        )?;
        let version: Option<String> = db
            .query_row(
                "SELECT value FROM reduction_metadata WHERE key='frequency_gate'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        if version.as_deref() != Some("recorded-positive-v2") {
            // Only derived evidence is invalidated. Source HDF5, range profiles
            // and other caches are preserved; each missing profile is rebuilt once.
            db.execute_batch("BEGIN IMMEDIATE; DELETE FROM frequency_evidence; INSERT OR REPLACE INTO reduction_metadata VALUES('frequency_gate','recorded-positive-v2'); COMMIT;")?;
        }
        Ok(Self {
            db,
            base: base.into(),
            station: station.into(),
            scanned: BTreeSet::new(),
        })
    }
    pub fn refresh(&mut self, now: f64) -> Result<usize> {
        let day = Utc
            .timestamp_opt(now as i64, 0)
            .single()
            .unwrap()
            .date_naive();
        let mut changed = 0;
        for d in 0..=2 {
            let date = (day - chrono::Duration::days(d)).to_string();
            if d > 0 && self.scanned.contains(&date) {
                continue;
            }
            let dir = self.base.join(&date);
            if !dir.is_dir() {
                continue;
            }
            for e in fs::read_dir(dir)? {
                let p = e?.path();
                if p.extension().and_then(|s| s.to_str()) != Some("h5") {
                    continue;
                }
                let Some((kind, tx, rx, t)) = identity(&p) else {
                    continue;
                };
                if rx != self.station || t < now - 49. * 3600. || t > now + 60. {
                    continue;
                }
                let m = fs::metadata(&p)?;
                let stamp = m.modified()?.duration_since(UNIX_EPOCH)?.as_nanos() as i64;
                let path = p.to_string_lossy().into_owned();
                let old: Option<(i64, i64)> = self
                    .db
                    .query_row("SELECT mtime,size FROM files WHERE path=?1", [&path], |r| {
                        Ok((r.get(0)?, r.get(1)?))
                    })
                    .optional()?;
                let has_evidence = kind != "rti"
                    || self.db.query_row(
                        "SELECT EXISTS(SELECT 1 FROM frequency_evidence WHERE path=?1) AND EXISTS(SELECT 1 FROM frequency_profiles WHERE path=?1)",
                        [&path],
                        |r| r.get::<_, bool>(0),
                    )?;
                if old == Some((stamp, m.len() as i64)) && has_evidence {
                    continue;
                }
                match self.import(&p, &kind, &tx, t) {
                    Ok(()) => {
                        self.db.execute(
                            "INSERT OR REPLACE INTO files VALUES(?1,?2,?3,?4,?5,?6)",
                            params![path, stamp, m.len() as i64, kind, tx, t],
                        )?;
                        changed += 1
                    }
                    Err(e) => eprintln!("{}: {e:#}", p.display()),
                }
            }
            if d > 0 && (d > 1 || now.rem_euclid(86400.) >= 120.) {
                self.scanned.insert(date);
            }
        }
        let cutoff = now - 49. * 3600.;
        self.db
            .execute("DELETE FROM frequency_evidence WHERE time<?1", [cutoff])?;
        self.db
            .execute("DELETE FROM frequency_profiles WHERE time<?1", [cutoff])?;
        self.db
            .execute("DELETE FROM profiles WHERE time<?1", [cutoff])?;
        self.db
            .execute("DELETE FROM spectra WHERE time<?1", [cutoff])?;
        self.db.execute(
            "DELETE FROM detections WHERE path IN (SELECT path FROM files WHERE time<?1)",
            [cutoff],
        )?;
        self.db
            .execute("DELETE FROM files WHERE time<?1", [cutoff])?;
        Ok(changed)
    }
    fn import(&self, p: &Path, kind: &str, tx: &str, t: f64) -> Result<()> {
        let path = p.to_string_lossy();
        match kind {
            "rti" => {
                let grids = read_products(p)?;
                let g = grids
                    .iter()
                    .find(|g| g.mode != "X")
                    .ok_or_else(|| anyhow::anyhow!("no ordinary-mode product"))?;
                let mut v = reduce(g);
                v.tx = tx.into();
                self.db.execute(
                    "INSERT OR REPLACE INTO profiles VALUES(?1,?2,?3,?4,?5,?6,?7)",
                    params![path, t, tx, v.y0, v.dy, pack(&v.snr), pack(&v.freq)],
                )?;
                let evidence = frequency_profile(g, &self.station, tx, t);
                let f = reduce_frequency(g);
                self.db.execute(
                    "INSERT OR REPLACE INTO frequency_profiles VALUES(?1,?2,?3,?4,?5,?6)",
                    params![
                        path,
                        t,
                        tx,
                        pack(&f.frequency),
                        pack(&f.snr),
                        pack(&f.range)
                    ],
                )?;
                self.db.execute(
                    "INSERT OR REPLACE INTO frequency_evidence VALUES(?1,?2,?3,?4)",
                    params![path, t, tx, serde_json::to_string(&evidence)?],
                )?;
            }
            "spectrum" => {
                let h = hdf5::File::open(p)?;
                let d = h.dataset("power")?;
                let sh = d.shape();
                ensure!(sh.len() == 2 && d.size() < 8_000_000, "spectrum shape");
                let a = d.read_raw::<f32>()?;
                let f = h.dataset("frequency_hz")?.read_raw::<f64>()?;
                ensure!(f.len() == sh[1] && f.len() > 1, "frequency axis");
                let mut power = vec![f32::NAN; sh[1]];
                for j in 0..sh[1] {
                    let mut sum = 0.;
                    let mut count = 0;
                    for i in 0..sh[0] {
                        let v = a[i * sh[1] + j];
                        if v.is_finite() && v > 0. {
                            sum += v as f64;
                            count += 1
                        }
                    }
                    if count > 0 {
                        power[j] = (sum / count as f64) as f32
                    }
                }
                self.db.execute(
                    "INSERT OR REPLACE INTO spectra VALUES(?1,?2,?3,?4,?5)",
                    params![
                        path,
                        t,
                        f[0] / 1e6,
                        (f[f.len() - 1] - f[0]) / (f.len() - 1) as f64 / 1e6,
                        pack(&power)
                    ],
                )?;
            }
            "detections" => {
                let h = hdf5::File::open(p)?;
                let d = h.dataset("data")?;
                ensure!(
                    d.shape().len() == 2 && d.shape()[1] == 5 && d.size() < 2_000_000,
                    "detection shape"
                );
                let mut a = d.read_raw::<f64>()?;
                for row in a.chunks_exact_mut(5) {
                    if row[2] > 1000. {
                        row[2] /= 1e6;
                    }
                }
                let blob: Vec<u8> = a.iter().flat_map(|x| x.to_le_bytes()).collect();
                self.db.execute(
                    "INSERT OR REPLACE INTO detections VALUES(?1,?2)",
                    params![path, blob],
                )?;
            }
            _ => {}
        }
        Ok(())
    }
    pub fn transmitters(&self) -> Result<Vec<String>> {
        let mut q = self
            .db
            .prepare("SELECT DISTINCT tx FROM profiles ORDER BY tx")?;
        let v = q
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }
    pub fn profiles(&self, tx: &str, start: f64) -> Result<Vec<Profile>> {
        let mut q = self.db.prepare(
            "SELECT time,y0,dy,snr,freq FROM profiles WHERE tx=?1 AND time>=?2 ORDER BY time,path",
        )?;
        let v = q
            .query_map(params![tx, start], |r| {
                let a: Vec<u8> = r.get(3)?;
                let b: Vec<u8> = r.get(4)?;
                Ok(Profile {
                    t: r.get(0)?,
                    tx: tx.into(),
                    y0: r.get(1)?,
                    dy: r.get(2)?,
                    snr: unpack(&a),
                    freq: unpack(&b),
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }
    pub fn frequency_profiles(
        &self,
        tx: &str,
        start: f64,
        end: f64,
    ) -> Result<Vec<FrequencyProfile>> {
        let mut q = self.db.prepare("SELECT time,frequency,snr,range FROM frequency_profiles WHERE tx=?1 AND time>=?2 AND time<?3 ORDER BY time,path")?;
        let rows = q
            .query_map(params![tx, start, end], |r| {
                let f: Vec<u8> = r.get(1)?;
                let s: Vec<u8> = r.get(2)?;
                let d: Vec<u8> = r.get(3)?;
                Ok(FrequencyProfile {
                    t: r.get(0)?,
                    frequency: unpack(&f),
                    snr: unpack(&s),
                    range: unpack(&d),
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }
    pub fn latest(&self, kind: &str, tx: &str) -> Result<Option<PathBuf>> {
        Ok(self
            .db
            .query_row(
                "SELECT path FROM files WHERE kind=?1 AND tx=?2 ORDER BY time DESC LIMIT 1",
                params![kind, tx],
                |r| r.get::<_, String>(0),
            )
            .optional()?
            .map(PathBuf::from))
    }
    pub fn detections(&self, start: f64) -> Result<Vec<[f64; 5]>> {
        let mut q=self.db.prepare("SELECT d.data FROM detections d JOIN files f ON f.path=d.path WHERE f.time>=?1 ORDER BY f.time")?;
        let mut result = Vec::new();
        for blob in q.query_map([start - 900.], |r| r.get::<_, Vec<u8>>(0))? {
            let a: Vec<f64> = blob?
                .chunks_exact(8)
                .map(|b| f64::from_le_bytes(b.try_into().unwrap()))
                .collect();
            for row in a.chunks_exact(5) {
                if row[0] >= start {
                    result.push(row.try_into().unwrap());
                }
            }
        }
        Ok(result)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn binary_cache_keeps_nan_and_values() {
        let a = [f32::NAN, 1., 13.0103, 20.];
        let b = unpack(&pack(&a));
        assert!(b[0].is_nan());
        assert_eq!(&a[1..], &b[1..]);
    }
}
impl Catalog {
    pub fn spectrum(&self, start: f64) -> Result<Vec<Profile>> {
        let mut q = self
            .db
            .prepare("SELECT time,y0,dy,power FROM spectra WHERE time>=?1 ORDER BY time")?;
        let v = q
            .query_map([start], |r| {
                let a: Vec<u8> = r.get(3)?;
                Ok(Profile {
                    t: r.get(0)?,
                    tx: String::new(),
                    y0: r.get(1)?,
                    dy: r.get(2)?,
                    snr: unpack(&a),
                    freq: Vec::new(),
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }
}
#[cfg(test)]
mod reduction_tests {
    use super::*;
    #[test]
    fn frequency_evidence_keeps_long_paths_and_excludes_local_zero_range() {
        let mut g = Grid {
            id: String::new(),
            station: "W2NAF".into(),
            transmitter: "SGO".into(),
            kind: "lfm".into(),
            mode: "O".into(),
            timestamp: 1234.,
            nx: 3,
            ny: 3,
            x: vec![5., 6., 7.],
            y: vec![50., 3000., 8000.],
            xlabel: String::new(),
            ylabel: String::new(),
            color_label: String::new(),
            low: 0.,
            high: 20.,
            values: vec![99., 99., 99., 14., 17., 15., 19., 12., 20.],
        };
        let e = frequency_profile(&g, "W2NAF", "SGO", 1234.);
        assert_eq!(e.range_gate_km, [100., 8000.]);
        assert_eq!(e.snr_db, vec![Some(19.), Some(17.), Some(20.)]);
        g.y = vec![-3., -2., -1.];
        let e = frequency_profile(&g, "W2NAF", "SGO", 1234.);
        assert_eq!(e.range_gate_km, [100., 100.]);
        assert!(e.snr_db.iter().all(Option::is_none));
    }
    #[test]
    fn peak_snr_keeps_its_measured_frequency_and_empty_ranges() {
        let g = Grid {
            id: String::new(),
            station: "TGO".into(),
            transmitter: "SGO".into(),
            kind: "lfm".into(),
            mode: String::new(),
            timestamp: 1234.,
            nx: 3,
            ny: 3,
            x: vec![1., 2., 3.],
            y: vec![200., 202., 204.],
            xlabel: String::new(),
            ylabel: String::new(),
            color_label: String::new(),
            low: 0.,
            high: 20.,
            values: vec![2., 20., 5., f32::NAN, f32::NAN, f32::NAN, 8., 8., 3.],
        };
        let p = reduce(&g);
        assert_eq!(p.snr[0], 20.);
        assert_eq!(p.freq[0], 2.);
        assert!(p.snr[1].is_nan() && p.freq[1].is_nan());
        assert_eq!(p.freq[2], 1.);
        assert_eq!(p.y0, 200.);
        assert_eq!(p.dy, 2.);
    }
}

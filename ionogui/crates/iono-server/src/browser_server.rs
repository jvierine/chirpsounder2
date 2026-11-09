//! Thin catalogue/transport host. HTTP never enumerates the scientific archive.
//!
//! Ingestion consumes completed receiver bundles/inbox tickets. `bundles` holds
//! bulk metadata, `latest_bundles` the priority lane, and `sources` exact archive
//! paths for rehydration. HTTP serves prepared arrays; it neither decodes
//! sounders nor constructs RTIs from individual scientific observations.
mod path_quality;
use anyhow::{ensure, Context, Result};
use axum::{
    body::Body,
    extract::{Path as UrlPath, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use iono_browser::Bundle;
use rusqlite::{params, Connection};
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::Write;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};
struct App {
    db: Mutex<Connection>,
    store: PathBuf,
    archive: PathBuf,
    inbox: PathBuf,
}
type Shared = Arc<App>;
struct Staged(PathBuf);
impl Drop for Staged {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
fn setup(store: PathBuf, archive: PathBuf, inbox: PathBuf) -> Result<Shared> {
    fs::create_dir_all(store.join("arrays"))?;
    fs::create_dir_all(store.join("incoming"))?;
    fs::set_permissions(store.join("incoming"), fs::Permissions::from_mode(0o2775))?;
    fs::create_dir_all(&inbox)?;
    let db = Connection::open(store.join("catalog.sqlite"))?;
    db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA busy_timeout=2000; PRAGMA cache_size=-4096; PRAGMA journal_size_limit=2097152; PRAGMA max_page_count=131072;
    CREATE TABLE IF NOT EXISTS bundles(station TEXT,day TEXT,path TEXT,revision TEXT,metadata TEXT,limits TEXT,generated REAL,PRIMARY KEY(station,day));
    CREATE TABLE IF NOT EXISTS latest_bundles(station TEXT,day TEXT,path TEXT,revision TEXT,metadata TEXT,limits TEXT,generated REAL,PRIMARY KEY(station,day));
    CREATE TABLE IF NOT EXISTS arrays(id TEXT PRIMARY KEY,path TEXT,bytes INTEGER,used REAL);
    CREATE TABLE IF NOT EXISTS sources(station TEXT,day TEXT,path TEXT,PRIMARY KEY(station,day));
    CREATE INDEX IF NOT EXISTS bundle_day ON bundles(day,station);")?;
    prune_metadata(&db)?;
    Ok(Arc::new(App {
        db: Mutex::new(db),
        store,
        archive: archive.canonicalize()?,
        inbox,
    }))
}
fn import(app: &App, path: &Path) -> Result<()> {
    import_source(app, path, None)
}
fn import_source(app: &App, path: &Path, staged: Option<&Path>) -> Result<()> {
    // Evict rehydratable catalogue entries before writing, so a full cache
    // cannot prevent the successful import that used to trigger eviction.
    prune_metadata(&app.db.lock().unwrap())?;
    let canonical = if staged.is_some() {
        ensure!(
            path.is_absolute()
                && !path
                    .components()
                    .any(|c| matches!(c, std::path::Component::ParentDir)),
            "invalid archive path"
        );
        path.to_path_buf()
    } else {
        path.canonicalize()?
    };
    ensure!(
        canonical.starts_with(&app.archive),
        "source outside archive"
    );
    if let Some(p) = staged {
        ensure!(
            p.canonicalize()?
                .starts_with(app.store.join("incoming").canonicalize()?),
            "invalid staging path"
        );
    }
    let stat = staged.unwrap_or(&canonical).metadata()?;
    ensure!(stat.len() < 25 * 1024 * 1024, "bundle exceeds upload limit");
    let revision = format!(
        "{}:{}",
        stat.len(),
        stat.modified()?
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    );
    let unchanged = app.db.lock().unwrap().query_row(
        "SELECT EXISTS(SELECT 1 FROM bundles WHERE path=?1 AND revision=?2 UNION ALL SELECT 1 FROM latest_bundles WHERE path=?1 AND revision=?2)",
        params![canonical.to_string_lossy(), revision],
        |r| r.get::<_, bool>(0),
    )?;
    if unchanged {
        let metadata: Option<String> = app
            .db
            .lock()
            .unwrap()
            .query_row(
                "SELECT metadata FROM bundles WHERE path=?1 UNION ALL SELECT metadata FROM latest_bundles WHERE path=?1",
                [canonical.to_string_lossy()],
                |r| r.get(0),
            )
            .ok();
        if metadata
            .and_then(|s| serde_json::from_str::<Bundle>(&s).ok())
            .is_some_and(|b| {
                b.products.iter().all(|p| {
                    app.store
                        .join("arrays")
                        .join(format!("{}.bin.gz", p.grid.id))
                        .exists()
                })
            })
        {
            return Ok(());
        }
    }
    // Sequential copy once. HDF5 metadata/chunk seeks must stay on local disk,
    // particularly over slow SMB mounts with high per-operation latency.
    let local = if staged.is_none() {
        let copy = Staged(app.store.join(format!("import-{}.h5", std::process::id())));
        fs::copy(&canonical, &copy.0)?;
        Some(copy)
    } else {
        None
    };
    let h = hdf5::File::open(staged.unwrap_or_else(|| &local.as_ref().unwrap().0))?;
    let metadata = h
        .dataset("browser_metadata")?
        .read_scalar::<hdf5::types::VarLenUnicode>()?;
    ensure!(metadata.len() < 16 * 1024 * 1024, "metadata too large");
    let mut bundle: Bundle = serde_json::from_str(metadata.as_str())?;
    iono_browser::validate(&bundle)?;
    chrono::NaiveDate::parse_from_str(&bundle.day, "%Y-%m-%d")?;
    let now = chrono::Utc::now().timestamp() as f64;
    for p in &bundle.products {
        let target = app
            .store
            .join("arrays")
            .join(format!("{}.bin.gz", p.grid.id));
        if !target.exists() {
            let key = format!("{}_gzip", p.grid.id);
            let bytes = if h.link_exists(&key) {
                let d = h.dataset(&key)?;
                ensure!(d.size() <= 8 * 1024 * 1024, "compressed array too large");
                d.read_raw::<u8>()?
            } else {
                // Transport serialization only: no scientific reductions or
                // image rendering. Cache the exact receiver-computed cells.
                let d = h.dataset(&p.grid.id)?;
                ensure!(
                    d.shape() == [p.grid.ny, p.grid.nx] && d.size() <= iono_browser::MAX_CELLS,
                    "invalid numeric array"
                );
                let values = d.read_raw::<f32>()?;
                let mut encoder =
                    flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
                for v in values {
                    encoder.write_all(&v.to_le_bytes())?;
                }
                encoder.finish()?
            };
            let tmp = target.with_extension("partial");
            fs::write(&tmp, &bytes)?;
            fs::rename(tmp, &target)?;
        }
        app.db.lock().unwrap().execute(
            "INSERT OR REPLACE INTO arrays VALUES(?1,?2,?3,?4)",
            params![
                p.grid.id,
                target.to_string_lossy(),
                target.metadata()?.len(),
                now
            ],
        )?;
    }
    // Classification uses small receiver-reduced frequency profiles. No full
    // ionograms, raw voltages, or RTI reconstruction are processed here.
    let evidence = iono_browser::read_evidence(&h)?;
    let mut limits: Vec<_> = evidence
        .iter()
        .map(|e| {
            let mut row = iono_browser::limits(e, 13., 3);
            if e.receiver == "KREA" && ["Cyprus", "Australia"].contains(&e.transmitter.as_str()) {
                row["assumed"] = json!(true);
                row["source_label"] = json!(format!("{} (assumed)", e.transmitter));
                row["method"] = json!("sparse-range-gated-chirp-fft-v1");
            }
            row
        })
        .collect();
    limits.extend(std::mem::take(&mut bundle.assumed_paths));
    limits.sort_by(|a, b| {
        a["time"]
            .as_f64()
            .unwrap_or(0.)
            .total_cmp(&b["time"].as_f64().unwrap_or(0.))
    });
    bundle.evidence.clear();
    app.db.lock().unwrap().execute(
        if bundle.partial {
            "INSERT OR IGNORE INTO sources VALUES(?1,?2,?3)"
        } else {
            "INSERT OR REPLACE INTO sources VALUES(?1,?2,?3)"
        },
        params![bundle.station, bundle.day, canonical.to_string_lossy()],
    )?;
    app.db.lock().unwrap().execute(
        if bundle.partial {
            "INSERT OR REPLACE INTO latest_bundles VALUES(?1,?2,?3,?4,?5,?6,?7)"
        } else {
            "INSERT OR REPLACE INTO bundles VALUES(?1,?2,?3,?4,?5,?6,?7)"
        },
        params![
            bundle.station,
            bundle.day,
            canonical.to_string_lossy(),
            revision,
            serde_json::to_string(&bundle)?,
            serde_json::to_string(&limits)?,
            bundle.generated_unix
        ],
    )?;
    prune(app)?;
    eprintln!(
        "imported {} {}: {} display arrays",
        bundle.station,
        bundle.day,
        bundle.products.len()
    );
    Ok(())
}
/// Busy days vary greatly in size, so evict by bytes rather than day count.
/// Preserve each receiver's newest row, even above the target budget. Prune
/// before insertion too: cleanup only after success cannot recover a full DB.
fn prune_metadata(db: &Connection) -> Result<()> {
    for table in ["bundles", "latest_bundles"] {
        let mut q = db.prepare(&format!("SELECT rowid,station,length(CAST(metadata AS BLOB))+length(CAST(limits AS BLOB)) FROM {table} ORDER BY generated DESC"))?;
        let rows = q
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, u64>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut stations = std::collections::HashSet::new();
        let mut bytes = 0u64;
        for (id, station, size) in rows {
            // Always preserve the newest publication from each receiver.
            let newest = stations.insert(station);
            if newest || bytes.saturating_add(size) <= 32 * 1024 * 1024 {
                bytes = bytes.saturating_add(size);
            } else {
                db.execute(&format!("DELETE FROM {table} WHERE rowid=?1"), [id])?;
            }
        }
    }
    Ok(())
}
fn prune(app: &App) -> Result<()> {
    let db = app.db.lock().unwrap();
    let mut q = db.prepare("SELECT id,path,bytes FROM arrays ORDER BY used DESC")?;
    let entries = q
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, u64>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut kept = 0u64;
    for (id, path, bytes) in entries {
        kept += bytes;
        if kept > 96 * 1024 * 1024 {
            fs::remove_file(path).ok();
            db.execute("DELETE FROM arrays WHERE id=?1", [id])?;
        }
    }
    // The small source inventory survives eviction. Cold days are rehydrated
    // from one exact receiver bundle, never by rescanning source observations.
    prune_metadata(&db)?;
    Ok(())
}
fn poll(app: Shared) {
    std::thread::spawn(move || loop {
        // Superseded live-upload stages are disposable cache copies. Bound
        // their lifetime even when a newer coalesced ticket replaced them.
        if let Ok(entries) = fs::read_dir(app.store.join("incoming")) {
            for e in entries.flatten() {
                if e.metadata()
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.elapsed().ok())
                    .is_some_and(|age| age > Duration::from_secs(3600))
                {
                    fs::remove_file(e.path()).ok();
                }
            }
        }
        if let Ok(entries) = fs::read_dir(&app.inbox) {
            for e in entries.flatten().take(16) {
                let ticket = e.path();
                if ticket.extension().and_then(|s| s.to_str()) != Some("json") {
                    continue;
                }
                let ticket_inode = fs::metadata(&ticket).map(|m| m.ino()).unwrap_or(0);
                let result = (|| -> Result<()> {
                    let v: Value = serde_json::from_slice(&fs::read(&ticket)?)?;
                    let p = Path::new(v["path"].as_str().context("ticket path")?);
                    let staged = v["staged_path"]
                        .as_str()
                        .map(Path::new)
                        .filter(|p| p.exists());
                    let inode = staged.and_then(|p| fs::metadata(p).ok().map(|m| m.ino()));
                    import_source(&app, p, staged)?;
                    if let Some(p) = staged {
                        if fs::metadata(p).ok().map(|m| m.ino()) == inode {
                            fs::remove_file(p).ok();
                        }
                    }
                    Ok(())
                })();
                match result {
                    Ok(()) => {
                        if fs::metadata(&ticket).map(|m| m.ino()).ok() == Some(ticket_inode) {
                            fs::remove_file(ticket).ok();
                        }
                    }
                    Err(e) => {
                        eprintln!("bundle pending: {e:#}");
                    }
                }
            }
        }
        std::thread::sleep(Duration::from_secs(5));
    });
}
#[derive(Deserialize)]
struct Selection {
    day: String,
    station: String,
}
fn selected(app: &App, s: &Selection) -> Result<Bundle> {
    chrono::NaiveDate::parse_from_str(&s.day, "%Y-%m-%d")?;
    let db = app.db.lock().unwrap();
    let full: Option<String> = db
        .query_row(
            "SELECT metadata FROM bundles WHERE station=?1 AND day=?2",
            params![s.station, s.day],
            |r| r.get(0),
        )
        .ok();
    let latest: Option<String> = db
        .query_row(
            "SELECT metadata FROM latest_bundles WHERE station=?1 AND day=?2",
            params![s.station, s.day],
            |r| r.get(0),
        )
        .ok();
    let mut bundle: Bundle = serde_json::from_str(
        full.as_ref()
            .or(latest.as_ref())
            .context("no receiver products")?,
    )?;
    if let Some(metadata) = latest {
        let current: Bundle = serde_json::from_str(&metadata)?;
        for p in current.products {
            let matching = bundle.products.iter_mut().find(|old| {
                old.grid.transmitter == p.grid.transmitter
                    && old.grid.kind == p.grid.kind
                    && old.grid.mode == p.grid.mode
                    && old.quantity == p.quantity
            });
            if let Some(old) = matching {
                if p.grid.timestamp >= old.grid.timestamp {
                    *old = p;
                }
            } else {
                bundle.products.push(p);
            }
        }
        if current.generated_unix > bundle.generated_unix {
            bundle.generated_unix = current.generated_unix;
            bundle.health = current.health;
        }
    }
    Ok(bundle)
}
async fn day(State(app): State<Shared>, Query(s): Query<Selection>) -> Response {
    let result = selected(&app, &s);
    if let Ok(bundle) = result {
        if bundle.products.iter().all(|p| {
            app.store
                .join("arrays")
                .join(format!("{}.bin.gz", p.grid.id))
                .exists()
        }) {
            let bulk_generated: Option<f64> = app
                .db
                .lock()
                .unwrap()
                .query_row(
                    "SELECT generated FROM bundles WHERE station=?1 AND day=?2",
                    params![s.station, s.day],
                    |r| r.get(0),
                )
                .ok();
            return Json(json!({"state":"ready","products":bundle.products,"generated_unix":bundle.generated_unix,"bulk_generated_unix":bulk_generated,"processed":bundle.products.len(),"total":bundle.products.len(),"bands":bands(&s.station),"health":with_station_status(bundle.health,&s.station),"source":"receiver-precomputed-arrays","archive_reads_per_request":0})).into_response();
        }
    }
    if chrono::NaiveDate::parse_from_str(&s.day, "%Y-%m-%d").is_err()
        || !iono_browser::RECEIVERS.contains(&s.station.as_str())
    {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let path: Option<String> = app
        .db
        .lock()
        .unwrap()
        .query_row(
            "SELECT path FROM sources WHERE station=?1 AND day=?2",
            params![s.station, s.day],
            |r| r.get(0),
        )
        .ok();
    let latest_path: Option<String> = app
        .db
        .lock()
        .unwrap()
        .query_row(
            "SELECT path FROM latest_bundles WHERE station=?1 AND day=?2",
            params![s.station, s.day],
            |r| r.get(0),
        )
        .ok();
    let mut any = false;
    for (lane, path) in [("bulk", path), ("latest", latest_path)] {
        let Some(path) = path else {
            continue;
        };
        any = true;
        let ticket = app.inbox.join(if lane == "bulk" {
            format!("request-{}-{}.json", s.station, s.day)
        } else {
            format!("request-{}-{}-{lane}.json", s.station, s.day)
        });
        if !ticket.exists() {
            let temporary = ticket.with_extension("part");
            if fs::write(&temporary, json!({"path":path}).to_string()).is_ok() {
                fs::rename(temporary, ticket).ok();
            }
        }
    }
    if any {
        return Json(json!({"state":"loading","products":[],"processed":0,"total":1,"message":"Loading receiver-prepared arrays"})).into_response();
    }
    Json(json!({"state":"unavailable","products":[],"message":"Receiver-prepared arrays are not yet published for this day."})).into_response()
}
fn bands(station: &str) -> Value {
    let v: Value = serde_json::from_str(include_str!("../../../web/transmitters.json")).unwrap();
    v[station].clone()
}
async fn days(State(app): State<Shared>) -> Json<Value> {
    let db = app.db.lock().unwrap();
    let mut q = db
        .prepare("SELECT DISTINCT day FROM sources ORDER BY day DESC")
        .unwrap();
    let v = q
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect::<Vec<_>>();
    Json(json!(v))
}
async fn health(State(app): State<Shared>) -> Json<Value> {
    let db = app.db.lock().unwrap();
    // CPU badges need telemetry, not the product axes or full-day evidence.
    let mut q=db.prepare("WITH b AS (SELECT station,day,generated,metadata FROM bundles UNION ALL SELECT station,day,generated,metadata FROM latest_bundles) SELECT station,generated,json_extract(metadata,'$.health') FROM b WHERE generated=(SELECT MAX(generated) FROM b newer WHERE newer.station=b.station) ORDER BY station").unwrap();
    let rows = q
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, f64>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .unwrap();
    let stations: Vec<_> = rows
        .filter_map(|r| r.ok())
        .filter_map(|(station, generated, s)| {
            serde_json::from_str::<Value>(&s)
                .ok()
                .map(|health| json!({"station":station,"generated_unix":generated,"health":health}))
        })
        .collect();
    Json(
        json!({"ok":true,"stations":stations,"archive_listings_per_http_request":0,"renderer":"browser Rustplotlib/WASM/WebGL","server_reduces_observations":false}),
    )
}
#[derive(Deserialize)]
struct LimitSelection {
    #[serde(flatten)]
    selection: Selection,
    #[serde(default)]
    summary: bool,
    // Fixed UTC instant for map replay; omitted means live/latest behavior.
    at: Option<f64>,
}
async fn limits(
    State(app): State<Shared>,
    Query(query): Query<LimitSelection>,
) -> Result<Json<Value>, StatusCode> {
    let s = query.selection;
    let selected_date = chrono::NaiveDate::parse_from_str(&s.day, "%Y-%m-%d")
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let midnight = selected_date
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc()
        .timestamp() as f64;
    if query
        .at
        .is_some_and(|t| !t.is_finite() || t < midnight || t >= midnight + 86400.)
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    let data: Option<String> = app
        .db
        .lock()
        .unwrap()
        .query_row(
            "SELECT limits FROM bundles WHERE station=?1 AND day=?2",
            params![s.station, s.day],
            |r| r.get(0),
        )
        .ok();
    let mut raw: Value = data
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(json!([]));
    let latest: Option<String> = app
        .db
        .lock()
        .unwrap()
        .query_row(
            "SELECT limits FROM latest_bundles WHERE station=?1 AND day=?2",
            params![s.station, s.day],
            |r| r.get(0),
        )
        .ok();
    if let Some(rows) = latest.and_then(|v| serde_json::from_str::<Vec<Value>>(&v).ok()) {
        if let Some(raw) = raw.as_array_mut() {
            raw.extend(rows);
        }
    }
    let metadata = positions();
    let records = Value::Array(
        raw.as_array()
            .into_iter()
            .flatten()
            .cloned()
            .map(|mut row| {
                let tx = row["transmitter"].as_str().unwrap_or("").to_owned();
                if metadata[&tx]["quantitative_eligible"] == false {
                    row["quantitative_eligible"] = json!(false);
                }
                if let Some(label) = metadata[&tx]["source_label"].as_str() {
                    row["source_label"] = json!(label);
                }
                path_quality::measurement_quality(row)
            })
            .collect(),
    );
    let assessed_unix = query
        .at
        .unwrap_or_else(|| chrono::Utc::now().timestamp() as f64);
    // Only path assessment is cut at the selected instant. Full-day evidence
    // stays available for RTI cadence and history, without a second download.
    let eligible = query.at.map(|at| {
        Value::Array(
            records
                .as_array()
                .into_iter()
                .flatten()
                .filter(|row| row["time"].as_f64().is_some_and(|t| t <= at))
                .cloned()
                .collect(),
        )
    });
    let paths = path_quality::assess(eligible.as_ref().unwrap_or(&records), &s.day, assessed_unix);
    let mut response = json!({"experimental":true,"label":"Detected echo-band limits","threshold_db":13,"minimum_adjacent_bins":3,"paths":paths,"assessed_unix":assessed_unix,"positions":positions()});
    // The map consumes only the latest path assessment. History remains opt-in
    // through the original default response for receiver RTIs and band history.
    if !query.summary {
        response["records"] = records;
    }
    Ok(Json(response))
}
fn positions() -> Value {
    serde_json::from_str(include_str!("../../../web/paths.json")).unwrap()
}
fn with_station_status(mut health: Value, station: &str) -> Value {
    if iono_browser::RECEIVERS.contains(&station) {
        let file = PathBuf::from("/var/www/html/iono")
            .join(format!("station_status_latest-{station}.json"));
        if let Ok(bytes) = fs::read(file) {
            if let Ok(v) = serde_json::from_slice::<Value>(&bytes) {
                for name in ["disks", "ringbuffer", "output", "ok", "generated_unix"] {
                    health[name] = v[name].clone();
                }
            }
        }
    }
    health
}
async fn data(
    State(app): State<Shared>,
    UrlPath(id): UrlPath<String>,
    headers: HeaderMap,
) -> Response {
    if id.len() != 24 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    if headers
        .get(header::IF_NONE_MATCH)
        .is_some_and(|s| s.as_bytes() == format!("\"{id}\"").as_bytes())
    {
        return StatusCode::NOT_MODIFIED.into_response();
    }
    let path = app.store.join("arrays").join(format!("{id}.bin.gz"));
    let Ok(file) = tokio::fs::File::open(path).await else {
        return (
            StatusCode::NOT_FOUND,
            "Array cache expired; refresh day metadata.",
        )
            .into_response();
    };
    app.db
        .lock()
        .unwrap()
        .execute(
            "UPDATE arrays SET used=?2 WHERE id=?1",
            params![id, chrono::Utc::now().timestamp()],
        )
        .ok();
    let stream = tokio_util::io::ReaderStream::with_capacity(file, 65536);
    (
        [
            (header::CONTENT_TYPE, "application/octet-stream".to_string()),
            (header::CONTENT_ENCODING, "gzip".into()),
            (
                header::CACHE_CONTROL,
                "public, max-age=31536000, immutable".into(),
            ),
            (header::ETAG, format!("\"{id}\"")),
        ],
        Body::from_stream(stream),
    )
        .into_response()
}
#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> Result<()> {
    let env =
        |key: &str, default: &str| PathBuf::from(std::env::var(key).unwrap_or(default.into()));
    let app = setup(
        env("IONO_STORE", "/var/lib/iono2-browser"),
        env("IONO_ARCHIVE", "/mnt/shovel/ionosonde"),
        env("IONO_INBOX", "/var/lib/iono2-browser/inbox"),
    )?;
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).is_some_and(|s| s == "import") {
        for p in &args[2..] {
            import(&app, Path::new(p))?;
        }
        return Ok(());
    }
    poll(app.clone());
    let web = env("IONO_WEB", "web");
    let router = Router::new()
        .route("/api/day", get(day))
        .route("/api/archive-days", get(days))
        .route("/api/data/{id}", get(data))
        .route("/api/health", get(health))
        .route("/api/frequency-limits", get(limits))
        .fallback_service(tower_http::services::ServeDir::new(web))
        .layer(tower_http::compression::CompressionLayer::new())
        .with_state(app);
    let listener = tokio::net::TcpListener::bind(
        std::env::var("IONO_LISTEN").unwrap_or("127.0.0.1:8796".into()),
    )
    .await?;
    axum::serve(listener, router).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    #[test]
    fn metadata_cache_is_byte_bounded_and_preserves_each_receivers_latest() {
        let t = tempfile::tempdir().unwrap();
        fs::create_dir(t.path().join("archive")).unwrap();
        let app = setup(
            t.path().join("store"),
            t.path().join("archive"),
            t.path().join("inbox"),
        )
        .unwrap();
        let db = app.db.lock().unwrap();
        assert!(
            db.query_row("PRAGMA max_page_count", [], |r| r.get::<_, u64>(0))
                .unwrap()
                >= 131072
        );
        let payload = "x".repeat(3 * 1024 * 1024);
        for i in 0..24 {
            db.execute(
                "INSERT INTO bundles VALUES('DOB',?1,'archive','rev',?2,'[]',?3)",
                params![i.to_string(), payload, i],
            )
            .unwrap();
        }
        db.execute(
            "INSERT INTO bundles VALUES('KHO','old','archive','rev','{}','[]',-1)",
            [],
        )
        .unwrap();
        assert!(
            db.query_row("PRAGMA page_count", [], |r| r.get::<_, u64>(0))
                .unwrap()
                > 16384
        );
        prune_metadata(&db).unwrap();
        let size: u64 = db
            .query_row(
                "SELECT SUM(length(metadata)+length(limits)) FROM bundles",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(size <= 32 * 1024 * 1024);
        for (station, day) in [("DOB", "23"), ("KHO", "old")] {
            assert!(db
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM bundles WHERE station=?1 AND day=?2)",
                    params![station, day],
                    |r| r.get::<_, bool>(0)
                )
                .unwrap());
        }
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM sources", [], |r| r.get::<_, u64>(0))
                .unwrap(),
            0
        );
    }
    fn fixture(root: &Path) -> (Shared, PathBuf, String) {
        let archive = root.join("archive");
        fs::create_dir(&archive).unwrap();
        let app = setup(root.join("store"), archive.clone(), root.join("inbox")).unwrap();
        let grid = iono_core::Grid {
            id: String::new(),
            station: "TGO".into(),
            transmitter: "Test".into(),
            kind: "rti".into(),
            mode: "O".into(),
            timestamp: 1792627200.,
            nx: 2,
            ny: 2,
            x: vec![1792627230., 1792627290.],
            y: vec![10., 20.],
            xlabel: "Time (UTC)".into(),
            ylabel: "Range (km)".into(),
            color_label: "SNR (dB)".into(),
            low: 0.,
            high: 20.,
            values: vec![1., f32::NAN, 3., 4.],
        };
        let mut bundle = Bundle {
            schema: iono_browser::SCHEMA.into(),
            station: "TGO".into(),
            day: "2026-10-22".into(),
            generated_unix: 1792627300.,
            partial: false,
            products: vec![iono_browser::Product {
                grid,
                quantity: "snr".into(),
                palette: "greys".into(),
                occupied_minutes: 2,
            }],
            evidence: vec![],
            assumed_paths: vec![
                json!({"transmitter":"US","source_label":"US (assumed)","receiver":"TGO","time":1792627290.,"lower_mhz":5.,"upper_mhz":12.,"assumed":true,"method":"range-gated-sweep-envelope-v1"}),
            ],
            health: json!({"time":1792627300}),
        };
        let path = archive.join("browser_day-TGO-1792627200.h5");
        iono_browser::write(&path, &mut bundle).unwrap();
        let id = bundle.products[0].grid.id.clone();
        import(&app, &path).unwrap();
        (app, path, id)
    }
    #[tokio::test]
    async fn receiver_associations_survive_import_without_bloating_the_day_catalogue() {
        let t = tempfile::tempdir().unwrap();
        let (app, _, _) = fixture(t.path());
        let response = limits(
            State(app.clone()),
            Query(LimitSelection {
                selection: Selection {
                    day: "2026-10-22".into(),
                    station: "TGO".into(),
                },
                summary: true,
                at: Some(1792627300.),
            }),
        )
        .await
        .unwrap()
        .0;
        assert_eq!(response["paths"][0]["transmitter"], "US");
        assert_eq!(response["paths"][0]["assumed"], true);
        assert_eq!(response["paths"][0]["upper_mhz"], 12.);
        let metadata = selected(
            &app,
            &Selection {
                day: "2026-10-22".into(),
                station: "TGO".into(),
            },
        )
        .unwrap();
        assert!(metadata.assumed_paths.is_empty());
    }
    #[tokio::test]
    async fn pituffik_keeps_source_identity_provisional_and_withholds_quantitative_limits() {
        let t = tempfile::tempdir().unwrap();
        let (app, _, _) = fixture(t.path());
        let records = json!([{"transmitter":"THJ76","receiver":"TGO","time":1792627290.,"lower_mhz":3.,"upper_mhz":8.}]);
        app.db
            .lock()
            .unwrap()
            .execute("UPDATE bundles SET limits=?1", [records.to_string()])
            .unwrap();
        let response = limits(
            State(app),
            Query(LimitSelection {
                selection: Selection {
                    day: "2026-10-22".into(),
                    station: "TGO".into(),
                },
                summary: true,
                at: Some(1792627300.),
            }),
        )
        .await
        .unwrap()
        .0;
        let path = &response["paths"][0];
        assert_eq!(path["transmitter"], "THJ76");
        assert_eq!(path["source_label"], "Pituffik (Thule; provisional)");
        assert_eq!(path["measurement_quality"], "commissioning");
        assert_eq!(path["quantitative_eligible"], false);
        assert!(path["upper_mhz"].is_null());
        assert_eq!(path["diagnostic_band_mhz"], json!([3., 8.]));
    }
    #[tokio::test]
    async fn map_summary_keeps_path_assessment_without_sending_history() {
        let t = tempfile::tempdir().unwrap();
        let (app, _, _) = fixture(t.path());
        let records = json!([
            {"transmitter":"SGO","receiver":"TGO","time":1792627200.,"lower_mhz":3.,"upper_mhz":9.},
            {"transmitter":"SGO","receiver":"TGO","time":1792627500.,"lower_mhz":null,"upper_mhz":null}
        ]);
        app.db
            .lock()
            .unwrap()
            .execute("UPDATE bundles SET limits=?1", [records.to_string()])
            .unwrap();
        let request = |summary| {
            Query(LimitSelection {
                selection: Selection {
                    day: "2026-10-22".into(),
                    station: "TGO".into(),
                },
                summary,
                at: None,
            })
        };
        let full = limits(State(app.clone()), request(false)).await.unwrap().0;
        let compact = limits(State(app.clone()), request(true)).await.unwrap().0;
        assert_eq!(
            full["records"],
            Value::Array(
                records
                    .as_array()
                    .unwrap()
                    .iter()
                    .cloned()
                    .map(path_quality::measurement_quality)
                    .collect()
            )
        );
        assert!(compact.get("records").is_none());
        assert_eq!(compact["paths"], full["paths"]);
        assert_eq!(compact["positions"], full["positions"]);
        assert!(compact["paths"][0]["lower_mhz"].is_null());
        let health = health(State(app)).await.0;
        assert_eq!(health["stations"][0]["health"], json!({"time":1792627300}));
        assert!(health["stations"][0].get("products").is_none());
    }
    #[tokio::test]
    async fn fixed_time_uses_only_past_observations_and_preserves_daily_evidence() {
        let t = tempfile::tempdir().unwrap();
        let (app, _, _) = fixture(t.path());
        let start = 1792627200.;
        let records = json!([
            {"transmitter":"SGO","receiver":"TGO","time":start+60.,"lower_mhz":3.,"upper_mhz":9.},
            {"transmitter":"SGO","receiver":"TGO","time":start+300.,"lower_mhz":null,"upper_mhz":null}
        ]);
        app.db
            .lock()
            .unwrap()
            .execute("UPDATE bundles SET limits=?1", [records.to_string()])
            .unwrap();
        for (offset, expected) in [
            (0., None),
            (60., Some(9.)),
            (299., Some(9.)),
            (300., None),
            (1201., None),
        ] {
            let response = limits(
                State(app.clone()),
                Query(LimitSelection {
                    selection: Selection {
                        day: "2026-10-22".into(),
                        station: "TGO".into(),
                    },
                    summary: false,
                    at: Some(start + offset),
                }),
            )
            .await
            .unwrap()
            .0;
            assert_eq!(
                response["records"],
                Value::Array(
                    records
                        .as_array()
                        .unwrap()
                        .iter()
                        .cloned()
                        .map(path_quality::measurement_quality)
                        .collect()
                )
            );
            if offset == 0. {
                assert_eq!(response["paths"], json!([]));
            } else {
                assert_eq!(response["paths"][0]["upper_mhz"].as_f64(), expected);
                assert!(response["paths"][0]["time"].as_f64().unwrap() <= start + offset);
                assert_eq!(
                    response["paths"][0]["freshness"],
                    if offset > 1200. { "stale" } else { "current" }
                );
            }
        }
        for at in [start - 1., start + 86400., f64::NAN] {
            assert!(matches!(
                limits(
                    State(app.clone()),
                    Query(LimitSelection {
                        selection: Selection {
                            day: "2026-10-22".into(),
                            station: "TGO".into()
                        },
                        summary: true,
                        at: Some(at),
                    })
                )
                .await,
                Err(StatusCode::BAD_REQUEST)
            ));
        }
    }
    #[tokio::test]
    async fn cached_day_and_numeric_payload_survive_archive_unavailability() {
        let t = tempfile::tempdir().unwrap();
        let (app, path, id) = fixture(t.path());
        fs::rename(path.parent().unwrap(), t.path().join("archive-offline")).unwrap();
        let manifest = day(
            State(app.clone()),
            Query(Selection {
                day: "2026-10-22".into(),
                station: "TGO".into(),
            }),
        )
        .await;
        assert_eq!(manifest.status(), StatusCode::OK);
        let m: Value = serde_json::from_slice(
            &axum::body::to_bytes(manifest.into_body(), 1_000_000)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(m["state"], "ready");
        assert_eq!(m["archive_reads_per_request"], 0);
        let response = data(State(app), UrlPath(id), HeaderMap::new()).await;
        assert_eq!(response.headers()[header::CONTENT_ENCODING], "gzip");
        let bytes = axum::body::to_bytes(response.into_body(), 10000)
            .await
            .unwrap();
        let mut decoded = vec![];
        flate2::read::GzDecoder::new(&bytes[..])
            .read_to_end(&mut decoded)
            .unwrap();
        let values: Vec<_> = decoded
            .chunks_exact(4)
            .map(|v| f32::from_le_bytes(v.try_into().unwrap()))
            .collect();
        assert_eq!(values[0], 1.);
        assert!(values[1].is_nan());
        assert_eq!(&values[2..], &[3., 4.]);
    }
    #[tokio::test]
    async fn evicted_array_is_rehydrated_from_one_known_bundle() {
        let t = tempfile::tempdir().unwrap();
        let (app, path, id) = fixture(t.path());
        fs::remove_file(app.store.join("arrays").join(format!("{id}.bin.gz"))).unwrap();
        let response = day(
            State(app.clone()),
            Query(Selection {
                day: "2026-10-22".into(),
                station: "TGO".into(),
            }),
        )
        .await;
        let p: Value = serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), 10000)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(p["state"], "loading");
        let ticket = app.inbox.join("request-TGO-2026-10-22.json");
        let v: Value = serde_json::from_slice(&fs::read(ticket).unwrap()).unwrap();
        assert_eq!(
            v["path"],
            path.canonicalize().unwrap().to_string_lossy().as_ref()
        );
        import(&app, &path).unwrap();
        assert!(app
            .store
            .join("arrays")
            .join(format!("{id}.bin.gz"))
            .exists());
        assert!(path.exists());
    }
    #[test]
    fn live_stage_import_does_not_touch_the_network_archive() {
        let t = tempfile::tempdir().unwrap();
        let (app, path, _) = fixture(t.path());
        let staged = app
            .store
            .join("incoming")
            .join("browser_day-TGO-1792627200.h5");
        fs::copy(&path, &staged).unwrap();
        let source = app.archive.join(path.file_name().unwrap());
        app.db
            .lock()
            .unwrap()
            .execute("DELETE FROM bundles", [])
            .unwrap();
        fs::rename(path.parent().unwrap(), t.path().join("archive-offline")).unwrap();
        import_source(&app, &source, Some(&staged)).unwrap();
        assert!(selected(
            &app,
            &Selection {
                station: "TGO".into(),
                day: "2026-10-22".into()
            }
        )
        .is_ok());
        let outside = t.path().join("outside.h5");
        fs::copy(&staged, &outside).unwrap();
        assert!(import_source(&app, &source, Some(&outside)).is_err());
    }
    #[test]
    fn priority_ionograms_survive_later_bulk_import_without_erasing_rti() {
        let t = tempfile::tempdir().unwrap();
        let (app, path, _) = fixture(t.path());
        let query = Selection {
            station: "TGO".into(),
            day: "2026-10-22".into(),
        };
        let mut latest = selected(&app, &query).unwrap();
        latest.products[0].grid.values = vec![1., f32::NAN, 3., 4.];
        latest.partial = true;
        latest.generated_unix += 120.;
        latest.assumed_paths.clear();
        latest.products[0].grid.kind = "prn".into();
        latest.products[0].quantity = "ionogram".into();
        latest.products[0].grid.timestamp += 120.;
        let expected = latest.products[0].grid.timestamp;
        let latest_path = path.with_file_name("browser_latest-TGO-1792627200.h5");
        iono_browser::write_transport(&latest_path, &mut latest).unwrap();
        import(&app, &latest_path).unwrap();
        let merged = selected(&app, &query).unwrap();
        assert_eq!(merged.products.len(), 2);
        assert!(merged.products.iter().any(|p| p.grid.kind == "rti"));
        assert_eq!(
            merged
                .products
                .iter()
                .find(|p| p.grid.kind == "prn")
                .unwrap()
                .grid
                .timestamp,
            expected
        );
        // A bulk transfer that started earlier must not replace a newer sounding.
        let h = hdf5::File::open(&path).unwrap();
        let meta = h
            .dataset("browser_metadata")
            .unwrap()
            .read_scalar::<hdf5::types::VarLenUnicode>()
            .unwrap();
        drop(h);
        let mut old: Bundle = serde_json::from_str(meta.as_str()).unwrap();
        old.generated_unix += 240.;
        old.products[0].grid.values = vec![1.; 4];
        iono_browser::write(&path, &mut old).unwrap();
        import(&app, &path).unwrap();
        let merged = selected(&app, &query).unwrap();
        assert_eq!(merged.products.len(), 2);
        assert_eq!(
            merged
                .products
                .iter()
                .find(|p| p.grid.kind == "prn")
                .unwrap()
                .grid
                .timestamp,
            expected
        );
        let id = &latest.products[0].grid.id;
        let cached = app.store.join("arrays").join(format!("{id}.bin.gz"));
        fs::remove_file(&cached).unwrap();
        import(&app, &latest_path).unwrap();
        assert!(
            cached.exists(),
            "priority array must rehydrate from its exact source"
        );
        let gzip = fs::read(&cached).unwrap();
        let mut decoder = flate2::read::GzDecoder::new(gzip.as_slice());
        let mut bytes = vec![];
        std::io::Read::read_to_end(&mut decoder, &mut bytes).unwrap();
        let expected: Vec<u8> = [1f32, f32::NAN, 3., 4.]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        assert_eq!(
            bytes, expected,
            "serialization must preserve measured cells and NaN gaps"
        );
    }
}

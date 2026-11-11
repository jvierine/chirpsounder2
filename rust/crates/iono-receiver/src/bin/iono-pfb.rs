//! Receiver-local 32-bin shared frontend. Unix socket transports only selected
//! 100 ksample/s complex64 voltage; raw RF never leaves the receiver computer.
use anyhow::{ensure, Context, Result};
use iono_core::shared_pfb::Cache;
use iono_receiver::frontend::Request;
use rdigital_rf::{CacheOptions, Reader};
use serde::Deserialize;
use std::{
    fs,
    io::{Read, Write},
    os::unix::{
        fs::PermissionsExt,
        net::{UnixListener, UnixStream},
    },
    path::Path,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
#[derive(Deserialize)]
struct Config {
    data_dir: String,
    channel: String,
    #[serde(default)]
    socket: String,
    tiles: usize,
}
struct Engine {
    cache: Cache,
    reader: Reader,
    config: Config,
    last_report: Instant,
}
fn session(mut stream: UnixStream, engine: Arc<Mutex<Engine>>) -> Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(120)))?;
    stream.set_write_timeout(Some(Duration::from_secs(10)))?;
    loop {
        let mut header = [0u8; 4];
        match stream.read_exact(&mut header) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(()),
            Err(e) => return Err(e.into()),
        }
        let n = u32::from_le_bytes(header) as usize;
        ensure!(n > 0 && n <= 1024, "invalid request length");
        let mut bytes = vec![0u8; n];
        stream.read_exact(&mut bytes)?;
        let result = (|| -> Result<Vec<_>> {
            let request: Request = serde_json::from_slice(&bytes)?;
            let mut guard = engine
                .lock()
                .map_err(|_| anyhow::anyhow!("PFB engine poisoned"))?;
            let e = &mut *guard;
            ensure!(request.channel == e.config.channel, "unknown RF channel");
            let z = e.cache.read(
                request.start,
                request.count,
                request.origin,
                request.offset_hz,
                request.bandwidth_hz,
                |start, count| match e.reader.read_complex32(start, count, &e.config.channel, 0) {
                    Ok(z) => Ok(z),
                    Err(error) => {
                        if e.reader
                            .bounds(&e.config.channel)
                            .ok()
                            .flatten()
                            .is_some_and(|bounds| start < bounds.0)
                        {
                            Err(iono_receiver::frontend::InputExpired.into())
                        } else {
                            Err(error)
                        }
                    }
                },
            );
            if e.last_report.elapsed() > Duration::from_secs(10) {
                println!("{}", serde_json::to_string(&e.cache.stats)?);
                e.last_report = Instant::now();
            }
            z
        })();
        let (status, bytes) = match result {
            Ok(z) => {
                let mut b = Vec::with_capacity(z.len() * 8);
                for z in z {
                    b.extend_from_slice(&z.re.to_le_bytes());
                    b.extend_from_slice(&z.im.to_le_bytes());
                }
                (0u32, b)
            }
            Err(e) => {
                eprintln!("PFB request failed: {e:#}");
                (
                    if e.is::<iono_receiver::frontend::InputExpired>() {
                        2u32
                    } else {
                        1u32
                    },
                    format!("{e:#}")
                        .into_bytes()
                        .into_iter()
                        .take(2048)
                        .collect(),
                )
            }
        };
        stream.write_all(&status.to_le_bytes())?;
        stream.write_all(&(bytes.len() as u32).to_le_bytes())?;
        stream.write_all(&bytes)?;
    }
}
fn main() -> Result<()> {
    let path = std::env::args()
        .nth(1)
        .context("usage: iono-pfb CONFIG.json")?;
    let mut config: Config = serde_json::from_slice(&fs::read(path)?)?;
    if let Some(socket) = std::env::args().nth(2) {
        config.socket = socket;
    }
    ensure!(!config.socket.is_empty(), "missing socket path");
    if Path::new(&config.socket).exists() {
        ensure!(
            UnixStream::connect(&config.socket).is_err(),
            "PFB socket already active"
        );
        fs::remove_file(&config.socket)?;
    }
    let mut reader = Reader::open([&config.data_dir])?;
    ensure!(
        reader.properties(&config.channel)?.sample_rate_numerator == 25_000_000
            && reader.properties(&config.channel)?.sample_rate_denominator == 1,
        "PFB needs 25 MHz input"
    );
    reader.set_cache_options(CacheOptions {
        minimum_bytes: 1024 * 1024,
        max_open_files: 4,
    })?;
    reader.set_cache_byte_limit(16 * 1024 * 1024)?;
    let cache = Cache::new(config.tiles)?;
    let listener = UnixListener::bind(&config.socket)?;
    fs::set_permissions(&config.socket, fs::Permissions::from_mode(0o600))?;
    let engine = Arc::new(Mutex::new(Engine {
        cache,
        reader,
        config,
        last_report: Instant::now(),
    }));
    let active = Arc::new(AtomicUsize::new(0));
    for stream in listener.incoming() {
        let stream = stream?;
        if active.load(Ordering::Relaxed) >= 64 {
            continue;
        }
        active.fetch_add(1, Ordering::Relaxed);
        let active = active.clone();
        let engine = engine.clone();
        std::thread::Builder::new()
            .stack_size(512 * 1024)
            .spawn(move || {
                if let Err(e) = session(stream, engine) {
                    eprintln!("PFB client disconnected: {e:#}");
                }
                active.fetch_sub(1, Ordering::Relaxed);
            })?;
    }
    Ok(())
}

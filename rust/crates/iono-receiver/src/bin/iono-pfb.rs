//! Receiver-local 32-bin shared frontend. Unix socket transports only selected
//! 100 ksample/s complex64 voltage; raw RF never leaves the receiver computer.
use anyhow::{ensure, Context, Result};
use iono_core::shared_pfb::Cache;
use iono_receiver::frontend::Request;
use num_complex::Complex32 as Z;
use rdigital_rf::{CacheOptions, Reader};
use serde::Deserialize;
use std::collections::VecDeque;
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
        mpsc::{self, SyncSender},
        Arc,
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
    #[serde(default = "raw_frames")]
    raw_frames: usize,
}
fn raw_frames() -> usize {
    32
}
const RAW_FRAME: u64 = 2_500_000; //100 ms. Avoid repeatedly decompressing RF chunks.
struct RawCache {
    frames: VecDeque<(u64, Vec<Z>)>,
    capacity: usize,
    hits: u64,
    misses: u64,
}
impl RawCache {
    fn new(capacity: usize) -> Result<Self> {
        ensure!((1..=64).contains(&capacity), "invalid raw cache capacity");
        Ok(Self {
            frames: VecDeque::new(),
            capacity,
            hits: 0,
            misses: 0,
        })
    }
    fn read<F>(&mut self, start: u64, n: usize, mut source: F) -> Result<Vec<Z>>
    where
        F: FnMut(u64, usize) -> Result<Vec<Z>>,
    {
        let mut out = Vec::with_capacity(n);
        while out.len() < n {
            let at = start + out.len() as u64;
            let base = at / RAW_FRAME * RAW_FRAME;
            let lo = (at - base) as usize;
            let count = (RAW_FRAME as usize - lo).min(n - out.len());
            let frame = if let Some(i) = self.frames.iter().position(|(s, _)| *s == base) {
                self.hits += 1;
                self.frames.remove(i).unwrap()
            } else {
                self.misses += 1;
                match source(base, RAW_FRAME as usize) {
                    Ok(z) => {
                        ensure!(z.len() == RAW_FRAME as usize, "short raw frame");
                        (base, z)
                    }
                    // Read-ahead must never turn a valid requested interval
                    // into a gap or invent unavailable future voltage.
                    Err(_) => {
                        let z = source(at, count)?;
                        ensure!(z.len() == count, "short raw interval");
                        out.extend_from_slice(&z);
                        continue;
                    }
                }
            };
            out.extend_from_slice(&frame.1[lo..lo + count]);
            if self.frames.len() >= self.capacity {
                self.frames.pop_front();
            }
            self.frames.push_back(frame);
        }
        Ok(out)
    }
}
struct Engine {
    cache: Cache,
    reader: Reader,
    raw: RawCache,
    config: Config,
    last_report: Instant,
}
type Job = (Request, SyncSender<Result<Vec<Z>>>);
fn process(e: &mut Engine, request: Request) -> Result<Vec<Z>> {
    ensure!(request.channel == e.config.channel, "unknown RF channel");
    let z = e.cache.read(
        request.start,
        request.count,
        request.origin,
        request.offset_hz,
        request.bandwidth_hz,
        |start, count| {
            e.raw.read(start, count, |start, count| {
                match e.reader.read_complex32(start, count, &e.config.channel, 0) {
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
                }
            })
        },
    );
    if e.last_report.elapsed() > Duration::from_secs(10) {
        println!(
            "{}",
            serde_json::json!({"coarse":e.cache.stats,"raw_hits":e.raw.hits,"raw_misses":e.raw.misses})
        );
        e.last_report = Instant::now();
    }
    z
}
fn session(mut stream: UnixStream, engine: SyncSender<Job>) -> Result<()> {
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
        let request: Request = serde_json::from_slice(&bytes)?;
        let (reply, answer) = mpsc::sync_channel(1);
        engine
            .send((request, reply))
            .context("PFB engine unavailable")?;
        let result = answer
            .recv_timeout(Duration::from_secs(60))
            .context("PFB engine deadline")?;
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
    let raw = RawCache::new(config.raw_frames)?;
    let mut state = Engine {
        cache,
        reader,
        raw,
        config,
        last_report: Instant::now(),
    };
    // FIFO prevents one backfilling client from reacquiring a mutex indefinitely.
    // At most one outstanding job per connection, bounded by 64 connections.
    let (engine, jobs) = mpsc::sync_channel::<Job>(64);
    std::thread::spawn(move || {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            while let Ok((request, reply)) = jobs.recv() {
                let _ = reply.send(process(&mut state, request));
            }
        }));
        // A failed engine must restart the service, not leave a live socket
        // accepting requests which can never be processed.
        std::process::exit(1);
    });
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rf_read_ahead_reuses_blocks_and_stays_bounded() {
        let mut c = RawCache::new(2).unwrap();
        let mut calls = 0;
        for start in [1000, 257000, 513000, 769000] {
            let z = c
                .read(start, 264000, |s, n| {
                    calls += 1;
                    Ok((0..n).map(|i| Z::new((s + i as u64) as f32, 0.)).collect())
                })
                .unwrap();
            assert_eq!(z[0].re, start as f32);
            assert_eq!(z[263999].re, (start + 263999) as f32);
        }
        assert_eq!(calls, 1);
        for start in [RAW_FRAME * 2, RAW_FRAME * 3, RAW_FRAME * 4] {
            c.read(start, 1, |_, n| Ok(vec![Z::default(); n])).unwrap();
        }
        assert_eq!(c.frames.len(), 2);
    }
    #[test]
    fn unavailable_read_ahead_does_not_hide_valid_interval_or_fill_a_gap() {
        let mut c = RawCache::new(1).unwrap();
        assert!(c
            .read(100, 10, |_, n| if n > 10 {
                anyhow::bail!("outside retained data")
            } else {
                Ok(vec![Z::new(1., 0.); n])
            })
            .is_ok());
        assert!(c.read(100, 10, |_, _| anyhow::bail!("real gap")).is_err());
        assert!(c.frames.is_empty());
    }
}

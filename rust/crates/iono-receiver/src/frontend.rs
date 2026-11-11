//! Explicit opt-in shared PFB protocol. No automatic fallback on missing voltage
//! or daemon failure: such errors must remain visible rather than invent data.
use anyhow::{ensure, Context, Result};
use iono_core::{prn::Channelizer, shared_pfb::MAX_OUTPUT};
use num_complex::Complex32 as Z;
use rdigital_rf::Reader;
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    io::{Read, Write},
    os::unix::net::UnixStream,
    time::Duration,
};
#[derive(Debug)]
pub struct InputExpired;
impl std::fmt::Display for InputExpired {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "processing fell behind the retained voltage window")
    }
}
impl std::error::Error for InputExpired {}
#[derive(Serialize, Deserialize)]
pub struct Request {
    pub start: u64,
    pub count: usize,
    pub origin: u64,
    pub offset_hz: f64,
    pub bandwidth_hz: f64,
    pub channel: String,
}
thread_local! {static DIRECT: RefCell<std::collections::VecDeque<((u64,u64),Channelizer)>>=const{RefCell::new(std::collections::VecDeque::new())};}
thread_local! {static CONNECTION:RefCell<Option<(String,UnixStream)>>=const{RefCell::new(None)};}
pub fn enabled() -> bool {
    std::env::var_os("IONO_PFB_SOCKET").is_some()
}
pub fn remote(
    start: u64,
    count: usize,
    origin: u64,
    offset: f64,
    bandwidth: f64,
    channel: &str,
) -> Result<Option<Vec<Z>>> {
    let Some(path) = std::env::var_os("IONO_PFB_SOCKET") else {
        return Ok(None);
    };
    let path = path.to_str().context("non-UTF8 PFB socket")?;
    Ok(Some(exchange(
        path,
        &Request {
            start,
            count,
            origin,
            offset_hz: offset,
            bandwidth_hz: bandwidth,
            channel: channel.into(),
        },
    )?))
}
pub fn exchange(path: &str, request: &Request) -> Result<Vec<Z>> {
    ensure!(
        request.count > 0 && request.count <= MAX_OUTPUT,
        "invalid PFB output count"
    );
    let payload = serde_json::to_vec(request)?;
    ensure!(payload.len() <= 1024, "oversize request");
    CONNECTION.with(|cell| {
        let mut connection = cell.borrow_mut();
        if connection.as_ref().is_none_or(|(p, _)| p != path) {
            let stream = UnixStream::connect(path).context("shared PFB unavailable")?;
            stream.set_read_timeout(Some(Duration::from_secs(60)))?;
            stream.set_write_timeout(Some(Duration::from_secs(10)))?;
            *connection = Some((path.into(), stream));
        }
        let stream = &mut connection.as_mut().unwrap().1;
        let result = (|| {
            stream.write_all(&(payload.len() as u32).to_le_bytes())?;
            stream.write_all(&payload)?;
            let mut header = [0u8; 8];
            stream.read_exact(&mut header)?;
            let status = u32::from_le_bytes(header[..4].try_into().unwrap());
            let n = u32::from_le_bytes(header[4..].try_into().unwrap()) as usize;
            ensure!(n <= MAX_OUTPUT * 8, "oversize response");
            let mut bytes = vec![0u8; n];
            stream.read_exact(&mut bytes)?;
            if status == 2 {
                return Err(InputExpired.into());
            }
            ensure!(
                status == 0,
                "shared PFB: {}",
                String::from_utf8_lossy(&bytes)
            );
            ensure!(n == request.count * 8, "short PFB voltage response");
            let z = bytes
                .chunks_exact(8)
                .map(|b| {
                    Z::new(
                        f32::from_le_bytes(b[..4].try_into().unwrap()),
                        f32::from_le_bytes(b[4..].try_into().unwrap()),
                    )
                })
                .collect();
            Ok(z)
        })();
        if result.is_err() {
            *connection = None;
        }
        result
    })
}
/// Preserve the previous window's nominal grid/phase. PFB explicitly matches
/// the boxcar centre; padded window samples are still trimmed by each decoder.
pub fn read_window(
    r: &mut Reader,
    start: u64,
    n: usize,
    channel: &str,
    relative: i64,
    offset: f64,
    bandwidth: f64,
) -> Result<Vec<Z>> {
    let origin = start
        .checked_add_signed(-relative)
        .context("invalid phase origin")?;
    if let Some(z) = remote(start, n / 250, origin, offset, bandwidth, channel)? {
        return Ok(z);
    }
    let raw = r.read_complex32(start, n, channel, 0)?;
    DIRECT.with(|cell| {
        let mut plans = cell.borrow_mut();
        let key = (offset.to_bits(), bandwidth.to_bits());
        let plan = if let Some(i) = plans.iter().position(|(k, _)| *k == key) {
            plans.remove(i).unwrap().1
        } else {
            Channelizer::new(offset, bandwidth)?
        };
        let z = plan.process(&raw, relative);
        if plans.len() >= 8 {
            plans.pop_front();
        }
        plans.push_back((key, plan));
        Ok(z)
    })
}

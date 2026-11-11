//! Bounded same-voltage comparison of direct vs shared PFB including RF I/O,
//! socket copies and server CPU. Writes HDF5 only, never station products.
use anyhow::{ensure, Context, Result};
use iono_core::prn::Channelizer;
use iono_receiver::frontend::{exchange, Request};
use rdigital_rf::Reader;
use serde::Deserialize;
use std::{
    fs,
    time::{Instant, SystemTime, UNIX_EPOCH},
};
#[derive(Deserialize)]
struct Config {
    data_dir: String,
    channel: String,
    socket: String,
}
fn cpu(pid: &str) -> Result<f64> {
    let s = fs::read_to_string(format!("/proc/{pid}/stat"))?;
    let fields: Vec<_> = s
        .rsplit_once(')')
        .context("invalid process stat")?
        .1
        .split_whitespace()
        .collect();
    unsafe extern "C" {
        fn sysconf(name: i32) -> i64;
    }
    // Linux _SC_CLK_TCK=2. This tool is station-only; never assume tick size.
    let hz = unsafe { sysconf(2) };
    ensure!(hz > 0, "invalid clock ticks");
    Ok((fields[11].parse::<u64>()? + fields[12].parse::<u64>()?) as f64 / hz as f64)
}
fn main() -> Result<()> {
    let a: Vec<_> = std::env::args().collect();
    ensure!(
        a.len() == 4,
        "usage: iono-pfb-compare CONFIG SERVER_PID OUTPUT.h5"
    );
    let c: Config = serde_json::from_slice(&fs::read(&a[1])?)?;
    let mut r = Reader::open([&c.data_dir])?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs_f64();
    let base = ((now - 8.) * 25e6) as u64 / 256000 * 256000;
    let h = hdf5::File::create(&a[3])?;
    let mut rows = vec![];
    for count in [1usize, 4, 8, 16, 32] {
        for trial in 0..3 {
            let start = base + (trial * 2_560_000 + count * 256000) as u64;
            let frequencies: Vec<_> = (0..count)
                .map(|i| (i as f64 - (count as f64 - 1.) / 2.) * 700e3)
                .collect();
            let mut direct = vec![];
            let mut shared = vec![];
            let mut times = [0.; 4];
            for implementation in if trial % 2 == 0 { [0, 1] } else { [1, 0] } {
                let client = cpu("self")?;
                let server = cpu(&a[2])?;
                let timer = Instant::now();
                for &offset in &frequencies {
                    let z = if implementation == 0 {
                        let raw = r.read_complex32(start - 4000, 2_508_000, &c.channel, 0)?;
                        Channelizer::new(offset, 80e3)?.process(&raw, -4000)[16..10016].to_vec()
                    } else {
                        exchange(
                            &c.socket,
                            &Request {
                                start,
                                count: 10000,
                                origin: start,
                                offset_hz: offset,
                                bandwidth_hz: 80e3,
                                channel: c.channel.clone(),
                            },
                        )?
                    };
                    if implementation == 0 {
                        direct.push(z)
                    } else {
                        shared.push(z)
                    }
                }
                times[implementation * 2] = cpu("self")? - client
                    + if implementation == 1 {
                        cpu(&a[2])? - server
                    } else {
                        0.
                    };
                times[implementation * 2 + 1] = timer.elapsed().as_secs_f64();
            }
            let mut error = 0f64;
            let mut energy = 0f64;
            for (a, b) in direct.iter().flatten().zip(shared.iter().flatten()) {
                error += (*a - *b).norm_sqr() as f64;
                energy += a.norm_sqr() as f64;
            }
            let rms = (error / energy.max(1e-30)).sqrt();
            println!("channels={count} trial={trial} direct_cpu={:.3} shared_cpu={:.3} direct_wall={:.3} shared_wall={:.3} relative_rms={rms:.4}",times[0],times[2],times[1],times[3]);
            rows.extend_from_slice(&[
                count as f64,
                trial as f64,
                times[0],
                times[1],
                times[2],
                times[3],
                rms,
            ]);
        }
    }
    h.new_dataset::<f64>()
        .shape([15, 7])
        .create("trials")?
        .write_raw(&rows)?;
    h.new_attr::<hdf5::types::VarLenUnicode>().create("columns")?.write_scalar(&"channels,trial,direct_cpu_s,direct_wall_s,shared_total_cpu_s,shared_wall_s,relative_voltage_rms".parse::<hdf5::types::VarLenUnicode>()?)?;
    h.new_attr::<u64>()
        .create("input_sample_index")?
        .write_scalar(&base)?;
    h.new_attr::<hdf5::types::VarLenUnicode>().create("scope")?.write_scalar(&"Same recorded RF; includes reader, socket and daemon CPU; excludes sounder decoding. CPU sampled via Linux clock ticks. Cache may reuse overlapping windows.".parse::<hdf5::types::VarLenUnicode>()?)?;
    Ok(())
}

//! Real DigitalRF/HDF5 -> daemon -> Unix socket -> coded-sounder decoders.
//! No receiver or network service is accessed; temporary fixtures are removed.
#![cfg(unix)]
use hdf5::{
    types::{CompoundField, CompoundType, TypeDescriptor, VarLenUnicode},
    File, H5Type,
};
use iono_core::{
    dsp::Digisonde,
    prn::{self, Decoder},
    pulsed::PulseDecoder,
};
use iono_receiver::frontend::{exchange, Request};
use num_complex::Complex32 as Z;
use std::{
    f64::consts::TAU,
    fs,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
#[repr(C)]
#[derive(Clone, Copy)]
struct Iq {
    r: f32,
    i: f32,
}
unsafe impl H5Type for Iq {
    fn type_descriptor() -> TypeDescriptor {
        TypeDescriptor::Compound(CompoundType {
            fields: vec![
                CompoundField::typed::<f32>("r", 0, 0),
                CompoundField::typed::<f32>("i", 4, 1),
            ],
            size: 8,
        })
    }
}
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());
struct Fixture {
    _serial: std::sync::MutexGuard<'static, ()>,
    root: PathBuf,
    daemon: Child,
    socket: String,
    origin: u64,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.daemon.kill();
        let _ = self.daemon.wait();
        let _ = fs::remove_dir_all(&self.root);
    }
}
impl Fixture {
    fn new(waveform: &[Z], lag: usize, offset: f64, count: usize) -> Self {
        let serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let root = std::path::Path::new("/tmp").join(format!(
            "iono-pfb-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let channel = root.join("ch0");
        fs::create_dir_all(&channel).unwrap();
        let epoch = 1700000000u64;
        let first = epoch * 25_000_000;
        let origin = first + 1_000_137;
        {
            let p = File::create(channel.join("drf_properties.h5")).unwrap();
            for (k, v) in [
                ("sample_rate_numerator", 25000000u64),
                ("sample_rate_denominator", 1),
                ("num_subchannels", 1),
                ("is_complex", 1),
                ("H5Tget_class", 1),
                ("H5Tget_size", 4),
                ("file_cadence_millisecs", 1000),
                ("subdir_cadence_secs", 3600),
            ] {
                p.new_attr::<u64>()
                    .create(k)
                    .unwrap()
                    .write_scalar(&v)
                    .unwrap();
            }
            for (k, v) in [
                ("digital_rf_version", "2.6.0"),
                ("epoch", "1970-01-01T00:00:00Z"),
            ] {
                p.new_attr::<VarLenUnicode>()
                    .create(k)
                    .unwrap()
                    .write_scalar(&v.parse::<VarLenUnicode>().unwrap())
                    .unwrap();
            }
            let directory = channel.join(
                chrono::DateTime::from_timestamp((epoch / 3600 * 3600) as i64, 0)
                    .unwrap()
                    .format("%Y-%m-%dT%H-%M-%S")
                    .to_string(),
            );
            fs::create_dir(&directory).unwrap();
            let h = File::create(directory.join(format!("rf@{epoch}.000.h5"))).unwrap();
            let n = 1_000_137 + count * 250 + 600_000;
            let raw: Vec<_> = (0..n)
                .map(|i| {
                    let t = i as i64 - 1_000_137;
                    let j = t.div_euclid(250) - lag as i64;
                    let v = waveform[j.rem_euclid(waveform.len() as i64) as usize];
                    let a = TAU * offset * t as f64 / 25e6;
                    let z = v * Z::new(a.cos() as f32, a.sin() as f32);
                    Iq { r: z.re, i: z.im }
                })
                .collect();
            h.new_dataset::<Iq>()
                .shape((n, 1))
                .create("rf_data")
                .unwrap()
                .write_raw(&raw)
                .unwrap();
            h.new_dataset::<u64>()
                .shape((1, 2))
                .create("rf_data_index")
                .unwrap()
                .write_raw(&[first, 0])
                .unwrap();
        }
        let socket = root.join("pfb.sock").to_str().unwrap().to_owned();
        let config = root.join("pfb.json");
        fs::write(
            &config,
            serde_json::to_vec(
                &serde_json::json!({"data_dir":root,"channel":"ch0","socket":socket,"tiles":4}),
            )
            .unwrap(),
        )
        .unwrap();
        let mut daemon = Command::new(env!("CARGO_BIN_EXE_iono-pfb"))
            .arg(config)
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        for _ in 0..500 {
            if std::os::unix::net::UnixStream::connect(&socket).is_ok() {
                return Self {
                    _serial: serial,
                    root,
                    daemon,
                    socket,
                    origin,
                };
            }
            if let Some(s) = daemon.try_wait().unwrap() {
                panic!("PFB daemon startup failed: {s}")
            };
            std::thread::sleep(Duration::from_millis(10));
        }
        let _ = daemon.kill();
        panic!("PFB startup timeout");
    }
    fn read(&self, count: usize, offset: f64, bw: f64) -> Vec<Z> {
        exchange(
            &self.socket,
            &Request {
                start: self.origin,
                count,
                origin: self.origin,
                offset_hz: offset,
                bandwidth_hz: bw,
                channel: "ch0".into(),
            },
        )
        .unwrap()
    }
}
fn peak(s: &[f32]) -> usize {
    s.iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .unwrap()
        .0
}
#[test]
fn prn_rf_hdf5_socket_cached_inverse_and_missing_data_errors() {
    let code = prn::code(4, 10000, 10, 400).unwrap();
    let f = Fixture::new(&code, 47, -8_984_375., 10000);
    let z = f.read(10000, -8_984_375., 100e3);
    let mut d = Decoder::new_matrix(&code, 0, 128).unwrap();
    for p in 0..8 {
        d.push(
            &z.iter()
                .map(|v| *v * Z::from_polar(1., p as f32 * 0.4))
                .collect::<Vec<_>>(),
        )
        .unwrap();
    }
    assert!(peak(&d.finish().unwrap()).abs_diff(47) <= 1);
    let mut request = Request {
        start: f.origin,
        count: 100,
        origin: f.origin,
        offset_hz: -8_984_375.,
        bandwidth_hz: 100e3,
        channel: "wrong".into(),
    };
    assert!(exchange(&f.socket, &request)
        .unwrap_err()
        .to_string()
        .contains("unknown RF channel"));
    request.channel = "ch0".into();
    request.start += 25_000_000;
    assert!(
        exchange(&f.socket, &request).is_err(),
        "missing data was filled"
    );
    request.start = f.origin - 25_000_000;
    request.origin = request.start;
    assert!(exchange(&f.socket, &request)
        .unwrap_err()
        .is::<iono_receiver::frontend::InputExpired>());
    // Failure closes the client's cached connection; the next call reconnects.
    assert!(f
        .read(100, -8_984_375., 100e3)
        .iter()
        .all(|z| z.re.is_finite()));
}
#[test]
fn digisonde_rf_hdf5_socket_preserves_complementary_pair() {
    let nr = 1000;
    let waveform = iono_core::digisonde_match::template(nr * 8, 100e3, 10000, 3);
    let f = Fixture::new(&waveform, 73, 4_296_875., 16000);
    let z = f.read(8000, 4_296_875., 30000.);
    let d = Digisonde::new(10000);
    let decoded: Vec<_> = z
        .chunks_exact(nr)
        .enumerate()
        .map(|(i, z)| d.decode_baseband_ipp(z.to_vec(), i, None))
        .collect();
    let power = d.power(&decoded);
    assert!(peak(&power[0]).abs_diff(73) <= 1);
    let epoch = f.origin as f64 / 25e6;
    let config = f.root.join("digisonde.json");
    let output = f.root.join("digisonde-products");
    fs::write(&config,serde_json::to_vec(&serde_json::json!({"station":"TEST","transmitter":"SyntheticDigi","data_dir":f.root,"output_dir":output,"channel":"ch0","freq_start_hz":16796875.,"freq_stop_hz":16896875.,"freq_step_hz":50000.,"ipp_us":10000,"n_ipp":8,"offset_us":0.,"transmit_delay_us":0.,"snr_threshold_db":0.})).unwrap()).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_iono-receiver"))
        .arg("digisonde")
        .arg(config)
        .arg("--once")
        .arg("--start")
        .arg(format!("{epoch:.9}"))
        .env("IONO_PFB_SOCKET", &f.socket)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let day = chrono::DateTime::from_timestamp(epoch as i64, 0)
        .unwrap()
        .format("%Y-%m-%d")
        .to_string();
    let products = iono_core::read_products(&output.join(day).join(format!(
        "digisonde_ionogram-SyntheticDigi-TEST-{}.h5",
        epoch as u64
    )))
    .unwrap();
    assert!(products
        .iter()
        .any(|g| g.values.iter().any(|v| v.is_finite())));
}
#[test]
fn verso_rf_hdf5_socket_preserves_regularized_echo() {
    let native: Vec<_> = include_bytes!("../../../deploy/verso/SGO_bauds_complex128.bin")
        .chunks_exact(16)
        .map(|b| {
            Z::new(
                f64::from_le_bytes(b[..8].try_into().unwrap()) as f32,
                f64::from_le_bytes(b[8..].try_into().unwrap()) as f32,
            )
        })
        .collect();
    let code = iono_core::pulsed::resample_code(&native, 62500, 100000).unwrap();
    let mut d = PulseDecoder::regularized(&code, 128, 0.01).unwrap();
    let n = d.input_len();
    let mut waveform = vec![Z::default(); n];
    let at = d.input_prefix() + 47;
    waveform[at..at + code.len()].copy_from_slice(&code);
    let f = Fixture::new(&waveform, 0, 2_734_375., n);
    let z = f.read(n, 2_734_375., 62500.);
    for _ in 0..8 {
        d.push(&z).unwrap();
    }
    assert!(peak(&d.finish().unwrap().0).abs_diff(47) <= 1);
}
#[test]
fn native_prn_worker_writes_complete_sparse_ionogram_through_shared_frontend() {
    let code = prn::code(4, 1600, 10, 400).unwrap();
    let f = Fixture::new(&code, 47, -8_984_375., 6400);
    let epoch = f.origin as f64 / 25e6;
    let config = f.root.join("worker.json");
    let output = f.root.join("products");
    fs::write(&config,serde_json::to_vec(&serde_json::json!({"station":"TEST","transmitter":"SyntheticPRN","data_dir":f.root,"output_dir":output,"channel":"ch0","frequencies_hz":[3_515_625.],"code_indices":[0],"codes":[{"seed":4,"length":1600,"pulse_length":10,"ipp":400,"bandwidth_hz":100000.,"control_seeds":[1]}],"dwell_s":0.064,"period_s":1.,"range_max_km":500.,"writer_grace_s":0.25,"estimator":"least-squares","least_squares_solver":"cached-inverse","retain_candidate_snr":true})).unwrap()).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_iono-prn"))
        .arg(config)
        .arg("--once")
        .arg("--start")
        .arg(format!("{epoch:.9}"))
        .env("IONO_PFB_SOCKET", &f.socket)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let day = chrono::DateTime::from_timestamp(epoch as i64, 0)
        .unwrap()
        .format("%Y-%m-%d")
        .to_string();
    let h = File::open(output.join(day).join(format!(
        "prn_ionogram-SyntheticPRN-TEST-{}.h5",
        epoch as u64
    )))
    .unwrap();
    assert_eq!(
        h.dataset("frequency_complete")
            .unwrap()
            .read_raw::<u8>()
            .unwrap(),
        vec![1]
    );
    let snr = h
        .dataset("candidate_snr")
        .unwrap()
        .read_raw::<f32>()
        .unwrap();
    assert!(peak(&snr).abs_diff(47) <= 1);
    assert!(!h
        .dataset("SNR_sparse_values")
        .unwrap()
        .read_raw::<f32>()
        .unwrap()
        .is_empty());
}

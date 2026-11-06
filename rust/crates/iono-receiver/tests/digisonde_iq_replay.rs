//! End-to-end offline replay: no recorder/ring-buffer access is permitted.
use hdf5::File;
use std::{
    fs,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};
#[test]
fn cached_channelized_iq_recovers_code_without_opening_rf() {
    let root = std::env::temp_dir().join(format!(
        "iono-iq-replay-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&root).unwrap();
    let source = root.join("input.h5");
    let config = root.join("config.json");
    let output = root.join("out");
    let epoch = 1793990000u64;
    {
        let h = File::create(&source).unwrap();
        h.create_group("iq").unwrap();
        h.new_attr::<f64>()
            .create("iq_carrier_search_hz")
            .unwrap()
            .write_scalar(&0.)
            .unwrap();
        h.new_dataset_builder()
            .with_data(&[epoch * 25_000_000])
            .create("iq/sample_index")
            .unwrap();
        h.new_dataset_builder()
            .with_data(&[4e6f64])
            .create("iq/carrier_hz")
            .unwrap();
        let code: Vec<[f32; 2]> = iono_core::digisonde_match::template(16000, 100e3, 10000, 3)
            .iter()
            .map(|z| [z.re, z.im])
            .collect();
        h.new_dataset::<[f32; 2]>()
            .shape([1, 16000])
            .create("iq/baseband_complex32")
            .unwrap()
            .write_raw(&code)
            .unwrap();
    }
    fs::write(&config, serde_json::to_vec(&serde_json::json!({"data_dir":"/this-ring-must-not-be-opened", "output_dir":output,"channel":"ch0","ipp_candidates_us":[10000],"doppler_search_hz":0})).unwrap()).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_iono-digisonde-verify"))
        .arg(&config)
        .arg("--iq")
        .arg(&source)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let h = File::open(output.join(format!("digisonde_code_matches-{epoch}.h5"))).unwrap();
    assert_eq!(
        h.attr("successful_windows")
            .unwrap()
            .read_scalar::<u64>()
            .unwrap(),
        1
    );
    assert_eq!(
        h.dataset("ipp_us").unwrap().read_raw::<u32>().unwrap(),
        vec![10000]
    );
    assert!(h.dataset("coherence").unwrap().read_raw::<f32>().unwrap()[0] > 0.99);
    assert!(h.dataset("lag_s").unwrap().read_raw::<f64>().unwrap()[0] < 0.00002);
    drop(h);
    let summary = root.join("summary.h5");
    let result = Command::new(env!("CARGO_BIN_EXE_iono-digisonde-verify"))
        .arg("--summary")
        .arg(&source)
        .arg(&summary)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let h = File::open(summary).unwrap();
    assert!(h.dataset("iq/baseband_complex32").is_err());
    assert_eq!(
        h.dataset("iq/sample_index")
            .unwrap()
            .read_raw::<u64>()
            .unwrap(),
        vec![epoch * 25_000_000]
    );
    drop(h);
    fs::remove_dir_all(root).unwrap();
}

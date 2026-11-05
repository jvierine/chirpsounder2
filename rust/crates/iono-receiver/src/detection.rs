use super::*;
use iono_core::chirp::Detector;
pub fn run(c: &Config, once: bool) -> Result<()> {
    let n = 5_000_000;
    // Two bounded FFT workers cover consecutive acquisition blocks. One reader
    // owns the HDF5 cache; at most two jobs wait and four results can accumulate.
    type Job = (u64, u64, u64, Vec<Complex32>);
    let (result_tx, result_rx) = std::sync::mpsc::sync_channel::<
        Result<(
            u64,
            u64,
            u64,
            Vec<iono_core::chirp::Detection>,
            Vec<radar::Column>,
        )>,
    >(4);
    let mut jobs = Vec::new();
    for _ in 0..2 {
        let (tx, rx) = std::sync::mpsc::sync_channel::<Job>(1);
        jobs.push(tx);
        let result_tx = result_tx.clone();
        let c = c.clone();
        thread::spawn(move || {
            let mut d = Detector::new(n, c.sample_rate, c.center_freq, vec![100000., 125000.]);
            while let Ok((sequence, block, skipped, raw)) = rx.recv() {
                let mut columns = vec![];
                let t = block as f64 * n as f64 / c.sample_rate;
                let found = d.seek_with_spectra(&raw, 13., 5, 200000., |rate, fft| {
                    if !c.radar_sources.is_empty() {
                        columns.extend(radar::project(&c, t, rate, fft));
                    }
                });
                if result_tx
                    .send(Ok((sequence, block, skipped, found, columns)))
                    .is_err()
                {
                    break;
                }
            }
        });
    }
    let input_config = c.clone();
    thread::spawn(move || {
        let c = input_config;
        let mut r = match reader(&c) {
            Ok(r) => r,
            Err(e) => {
                let _ = result_tx.send(Err(e));
                return;
            }
        };
        let mut block = ((now() - 2.) * c.sample_rate / n as f64).floor() as u64;
        let mut worker = 0;
        let mut sequence = 0;
        loop {
            let oldest = ((now() - 5.) * c.sample_rate / n as f64).floor() as u64;
            let skipped = oldest.saturating_sub(block);
            block = block.max(oldest);
            match read_live(&mut r, &c, block * n as u64, n) {
                Ok(raw) => {
                    if jobs[worker].send((sequence, block, skipped, raw)).is_err() {
                        break;
                    }
                    worker = (worker + 1) % jobs.len();
                    sequence += 1;
                }
                Err(e) => {
                    eprintln!("missing detector input at block {block}: {e:#}");
                    if once {
                        let _ = result_tx.send(Err(e));
                        return;
                    }
                }
            }
            block += 1;
        }
    });
    let mut bucket = 0u64;
    let mut rows: Vec<f64> = Vec::new();
    let mut last_post = 0.;
    let mut skipped_total = 0;
    let mut expected = 0;
    let mut pending = std::collections::BTreeMap::new();
    let mut radar = radar::Accumulator::default();
    loop {
        let (block, skipped, found, columns) = loop {
            if let Some(value) = pending.remove(&expected) {
                break value;
            }
            let (sequence, block, skipped, found, columns) =
                result_rx.recv().context("detector workers stopped")??;
            pending.insert(sequence, (block, skipped, found, columns));
            ensure!(pending.len() <= 8, "detector reorder queue exceeded bound");
        };
        expected += 1;
        skipped_total += skipped;
        let start = block * n as u64;
        let t = start as f64 / c.sample_rate;
        radar.update(c, columns, t)?;
        let current = (t / 900.).floor() as u64 * 900;
        if current != bucket {
            if bucket != 0 {
                save_bucket(c, bucket, &rows)?;
            }
            bucket = current;
            rows.clear();
            let path = output(c, &format!("cdetections-{}", c.station), bucket as f64)?;
            if let Ok(h) = File::open(path) {
                if let Ok(ds) = h.dataset("data") {
                    if ds.size() < 500_000 {
                        rows = ds.read_raw::<f64>().unwrap_or_default();
                    }
                }
            }
        }
        for (index, det) in found.iter().enumerate() {
            let epoch = t - det.frequency_hz / det.rate_hz_s;
            let path = output(
                c,
                &format!(
                    "chirp-{}-{}-f{}-{}",
                    c.channel,
                    (det.rate_hz_s / 1000.) as u32,
                    index,
                    start
                ),
                epoch.floor(),
            )?;
            let tmp = path.with_extension("h5.partial");
            {
                let h = File::create(&tmp)?;
                dataset(&h, "f0", det.frequency_hz)?;
                dataset(&h, "chirp_rate", det.rate_hz_s)?;
                dataset(&h, "chirp_time", epoch)?;
                dataset(&h, "sample_rate", c.sample_rate)?;
                dataset(&h, "snr", det.snr as f64)?;
                h.new_dataset::<u64>().create("i0")?.write_scalar(&start)?;
                dataset(&h, "n_samples", n as f64)?;
                text(&h, "channel", &c.channel)?;
                text(&h, "station_name", &c.station)?;
                h.flush()?;
            }
            std::fs::rename(tmp, path)?;
            rows.extend([epoch, t, det.frequency_hz, det.rate_hz_s, det.snr as f64]);
            ensure!(
                rows.len() <= 500_000,
                "detection bucket exceeds bounded size"
            );
        }
        if now() - last_post >= 60. || once {
            save_bucket(c, bucket, &rows)?;
            last_post = now();
            println!(
                "{} detector active: {} accumulated detections, input age {:.2}s, skipped {skipped_total} older blocks since last report",
                c.station,
                rows.len() / 5,
                now() - t
            );
            skipped_total = 0;
        }
        if once {
            return Ok(());
        }
    }
}

fn save_bucket(c: &Config, bucket: u64, rows: &[f64]) -> Result<()> {
    let path = output(c, &format!("cdetections-{}", c.station), bucket as f64)?;
    let tmp = path.with_extension("h5.partial");
    {
        let h = File::create(&tmp)?;
        if rows.is_empty() {
            h.new_dataset::<f64>().shape([0, 5]).create("data")?;
        } else {
            h.new_dataset::<f64>()
                .shape([rows.len() / 5, 5])
                .deflate(3)
                .create("data")?
                .write_raw(rows)?;
        }
        text(&h, "processor", "iono-receiver/0.1.0")?;
        dataset(&h, "updated_unix", now())?;
        h.flush()?;
    }
    publish(c, &tmp, &path)
}

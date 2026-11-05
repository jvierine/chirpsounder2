use super::*;
pub fn run(c: &Config, t: f64) -> Result<()> {
    let rate = 500008.4;
    let dec = 625;
    let sr_dec = c.sample_rate / dec as f64;
    let ds = C / rate;
    let nfft = ((sr_dec * ds / 2000. / 2.).floor() as usize) * 2;
    let hop = ((50000. / rate) * sr_dec).floor() as usize;
    let indices: Vec<_> = (0..500)
        .filter(|&i| {
            let f = rate * i as f64 * hop as f64 / sr_dec;
            f > 0.5e6 && f < 16e6
        })
        .collect();
    let needed = indices[indices.len() - 1] * hop + 6 * (hop / 13) + nfft + 1;
    let block = 4000;
    let n_windows = needed.div_ceil(block);
    let mut z = Vec::with_capacity(n_windows * block);
    let mut r = reader(c)?;
    let i0 = epoch_sample(t, c.sample_rate as u64)?;
    for b in 0..n_windows {
        let offset = (b * block * dec) as u64;
        let raw = read_live(&mut r, c, i0 + offset, block * dec + 2 * dec)?;
        z.extend(iono_core::lfm::dechirp(
            &raw,
            offset,
            block,
            dec,
            c.sample_rate,
            c.center_freq,
            rate,
        ));
    }
    let p = iono_core::lfm::spectra(&z, nfft, hop, &indices);
    let ridx: Vec<_> = (0..nfft)
        .filter(|&k| {
            let f = (k as f64 - nfft as f64 / 2.) * sr_dec / nfft as f64;
            let r = ds * f;
            r > 200000. && r < 1500000.
        })
        .collect();
    let mut snr = Vec::with_capacity(indices.len() * ridx.len());
    let mut noise = Vec::with_capacity(indices.len());
    for row in p.chunks_exact(nfft) {
        let nf = median(row);
        noise.push(nf);
        for &k in &ridx {
            let v = (row[k] - nf) / nf;
            snr.push(if v >= 2. { v } else { f32::NAN })
        }
    }
    let path = output(
        c,
        &format!("lfm_ionogram-SGO-{}-{}-001", c.station, c.channel),
        t,
    )?;
    let tmp = path.with_extension("h5.partial");
    {
        let h = File::create(&tmp)?;
        h.new_dataset::<f32>()
            .shape([indices.len(), ridx.len()])
            .deflate(3)
            .create("SNR")?
            .write_raw(&snr)?;
        let freqs: Vec<_> = indices
            .iter()
            .map(|&i| rate * i as f64 * hop as f64 / sr_dec)
            .collect();
        let ranges: Vec<_> = ridx
            .iter()
            .map(|&k| ds * (k as f64 - nfft as f64 / 2.) * sr_dec / nfft as f64)
            .collect();
        h.new_dataset_builder().with_data(&freqs).create("freqs")?;
        h.new_dataset_builder()
            .with_data(&ranges)
            .create("ranges")?;
        h.new_dataset_builder()
            .with_data(&noise)
            .create("noise_floor")?;
        dataset(&h, "t0", t)?;
        dataset(&h, "rate", rate)?;
        dataset(&h, "id", 1.)?;
        dataset(&h, "sr", sr_dec)?;
        dataset(&h, "range_offset_applied", 1.)?;
        text(&h, "station_name", &c.station)?;
        text(&h, "txname", "SGO")?;
        text(&h, "ch", &c.channel)?;
        text(&h, "processor", "iono-receiver/0.1.0")?;
        text(
            &h,
            "noise_floor_units",
            "unnormalized FFT power; SNR uses per-frequency median",
        )?;
        h.flush()?;
    }
    publish(c, &tmp, &path)
}
pub fn verify(path: &Path) -> Result<()> {
    let h = File::open(path)?;
    let re = h.dataset("raw_re")?.read_raw::<f32>()?;
    let im = h.dataset("raw_im")?.read_raw::<f32>()?;
    let raw: Vec<_> = re
        .iter()
        .zip(im)
        .map(|(&r, i)| Complex32::new(r, i))
        .collect();
    let dec = iono_core::scalar(&h, "decimation", 625.) as usize;
    let n = iono_core::scalar(&h, "n_out", 20.) as usize;
    let offset = iono_core::scalar(&h, "sample_offset", 0.) as u64;
    let z = iono_core::lfm::dechirp(&raw, offset, n, dec, 25e6, 12.5e6, 500008.4);
    let out = File::create(path.with_extension("rust.h5"))?;
    out.new_dataset_builder()
        .with_data(&z.iter().map(|v| v.re).collect::<Vec<_>>())
        .create("z_re")?;
    out.new_dataset_builder()
        .with_data(&z.iter().map(|v| v.im).collect::<Vec<_>>())
        .create("z_im")?;
    Ok(())
}

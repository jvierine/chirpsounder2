//! Bounded, receiver-local coarse voltage cache shared by all coded sounders.
//! A tile is computed once for all 32 bins. Missing raw voltage is never cached
//! as a successful tile. No LFM worker calls this frontend.
use crate::pfb::{Coarse, Pfb, Request};
use anyhow::{ensure, Context, Result};
use num_complex::Complex32 as Z;
use std::{collections::VecDeque, f64::consts::TAU, sync::Arc};
pub const CHANNELS: usize = 32;
pub const TILE: u64 = 256_000; // 10.24 ms; divisible by 250 and M.
pub const PAD: u64 = 4000;
pub const MAX_OUTPUT: usize = 65_536;
#[derive(Default, Clone, Copy, serde::Serialize)]
pub struct Stats {
    pub hits: u64,
    pub misses: u64,
    pub input_samples: u64,
}
pub struct Cache {
    coarse: Pfb,
    tiles: VecDeque<(u64, Arc<Coarse>)>,
    plans: VecDeque<((u64, u64), Pfb)>,
    capacity: usize,
    pub stats: Stats,
}
impl Cache {
    pub fn new(capacity: usize) -> Result<Self> {
        ensure!(
            (1..=128).contains(&capacity),
            "cache must contain 1..128 tiles"
        );
        let requests: Vec<_> = (0..CHANNELS)
            .map(|k| Request {
                offset_hz: (if k < CHANNELS / 2 {
                    k as i64
                } else {
                    k as i64 - CHANNELS as i64
                }) as f64
                    * 25e6
                    / CHANNELS as f64,
                bandwidth_hz: 100e3,
            })
            .collect();
        // The Nyquist bin itself is used for valid signals straddling +/-Fs/2
        // in complex sampling; construct just-inside the edge then use its bin.
        let mut requests = requests;
        requests[CHANNELS / 2].offset_hz += 100e3;
        Ok(Self {
            coarse: Pfb::new(CHANNELS, &requests)?,
            tiles: VecDeque::new(),
            plans: VecDeque::new(),
            capacity,
            stats: Stats::default(),
        })
    }
    /// `origin` is the decoder's integer phase epoch (not f32 Unix time).
    /// Return the same nominal output grid as the old channelizer, explicitly
    /// preserving its +4.5-sample FIR-centre convention. Source closure owns I/O.
    pub fn read<F>(
        &mut self,
        start: u64,
        count: usize,
        origin: u64,
        offset: f64,
        bandwidth: f64,
        mut source: F,
    ) -> Result<Vec<Z>>
    where
        F: FnMut(u64, usize) -> Result<Vec<Z>>,
    {
        ensure!(count > 0 && count <= MAX_OUTPUT, "invalid output extent");
        ensure!(
            start
                .checked_add((count * 250) as u64 + TILE + PAD)
                .is_some(),
            "sample extent overflow"
        );
        ensure!(
            (start as i128 - origin as i128).unsigned_abs() < 25_000_000 * 86_400,
            "phase origin more than a day away"
        );
        let key = (offset.to_bits(), bandwidth.to_bits());
        let plan = if let Some(at) = self.plans.iter().position(|(k, _)| *k == key) {
            self.plans.remove(at).unwrap().1
        } else {
            Pfb::new(
                CHANNELS,
                &[Request {
                    offset_hz: offset,
                    bandwidth_hz: bandwidth,
                }],
            )?
        };
        let result = self.read_with(&plan, start, count, origin, offset, &mut source);
        if self.plans.len() >= 64 {
            self.plans.pop_front();
        }
        self.plans.push_back((key, plan));
        result
    }
    fn read_with<F>(
        &mut self,
        plan: &Pfb,
        start: u64,
        count: usize,
        origin: u64,
        offset: f64,
        source: &mut F,
    ) -> Result<Vec<Z>>
    where
        F: FnMut(u64, usize) -> Result<Vec<Z>>,
    {
        let signed = (offset / (25e6 / CHANNELS as f64)).round() as i64;
        let slot = signed.rem_euclid(CHANNELS as i64) as usize;
        let mut values = Vec::with_capacity(count);
        while values.len() < count {
            let at = start
                .checked_add((values.len() * 250) as u64)
                .context("sample overflow")?;
            let tile = at / TILE * TILE;
            let n = ((tile + TILE - 1 - at) / 250 + 1).min((count - values.len()) as u64) as usize;
            let coarse = if let Some(i) = self.tiles.iter().position(|(t, _)| *t == tile) {
                self.stats.hits += 1;
                let entry = self.tiles.remove(i).unwrap();
                let c = entry.1.clone();
                self.tiles.push_back(entry);
                c
            } else {
                self.stats.misses += 1;
                let first = tile.checked_sub(PAD).context("tile before sample zero")?;
                let raw = match source(first, (TILE + 2 * PAD) as usize) {
                    Ok(raw) => raw,
                    Err(_) => {
                        let first = at.checked_sub(PAD).context("read before sample zero")?;
                        let raw = source(first, n * 250 + 2 * PAD as usize)?;
                        ensure!(
                            raw.iter().all(|z| z.re.is_finite() && z.im.is_finite()),
                            "missing voltage in requested interval"
                        );
                        let relative = i64::try_from(first as i128 - origin as i128)?;
                        let coarse = plan.coarse(&raw, relative);
                        let z = plan.extract_bin(
                            coarse.first_sample,
                            &coarse.bins[0],
                            0,
                            i64::try_from(at as i128 - origin as i128)?,
                            n,
                            9,
                        );
                        ensure!(
                            z.values
                                .iter()
                                .all(|z| z.re.is_finite() && z.im.is_finite()),
                            "unsupported requested interval"
                        );
                        values.extend(z.values);
                        continue;
                    }
                };
                ensure!(
                    raw.len() == (TILE + 2 * PAD) as usize,
                    "incomplete raw tile"
                );
                ensure!(
                    raw.iter().all(|z| z.re.is_finite() && z.im.is_finite()),
                    "missing voltage in tile"
                );
                let c = Arc::new(self.coarse.coarse(&raw, -(PAD as i64)));
                self.stats.input_samples += raw.len() as u64;
                if self.tiles.len() >= self.capacity {
                    self.tiles.pop_front();
                }
                self.tiles.push_back((tile, c.clone()));
                c
            };
            // Compute epoch differences as integers BEFORE conversion to float.
            let delta =
                i64::try_from(tile as i128 - origin as i128).context("phase origin too distant")?;
            ensure!(
                delta.unsigned_abs() < 25_000_000 * 86_400,
                "phase origin more than a day away"
            );
            let first = i64::try_from(at as i128 - origin as i128)?;
            let z = plan.extract_bin(
                coarse.first_sample + delta,
                &coarse.bins[slot],
                0,
                first,
                n,
                9,
            );
            // Coarse oscillator origin is tile, residual oscillator origin is
            // decoder epoch. This exact modulo rotation joins the two clocks.
            let angle =
                -TAU * signed as f64 * delta.rem_euclid(CHANNELS as i64) as f64 / CHANNELS as f64;
            let rotation = Z::new(angle.cos() as f32, angle.sin() as f32);
            for z in z.values {
                ensure!(z.re.is_finite() && z.im.is_finite(), "unsupported FIR edge");
                values.push(z * rotation);
            }
        }
        Ok(values)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_32_bins_reuse_one_voltage_tile_and_preserve_epoch_phase() {
        let mut cache = Cache::new(2).unwrap();
        let origin = 44_867_654_321_000_013u64 / TILE * TILE + 13;
        let start = origin + 1337;
        for k in 0..32 {
            let offset = (k as f64 - 15.5) * 700e3;
            // Shared broadband deterministic input, independent analytic tones.
            let out = cache
                .read(start, 500, origin, offset, 80e3, |s, n| {
                    Ok((0..n)
                        .map(|i| {
                            let t = (s as i128 + i as i128 - origin as i128) as f64;
                            (0..32)
                                .map(|j| {
                                    Z::from_polar(
                                        1.,
                                        (TAU * ((j as f64 - 15.5) * 700e3 + 2000.) * t / 25e6)
                                            as f32,
                                    )
                                })
                                .sum()
                        })
                        .collect())
                })
                .unwrap();
            // Independent demodulated expected carrier, includes centre shift.
            let expected: Vec<_> = (0..out.len())
                .map(|i| {
                    Z::from_polar(
                        1.,
                        (TAU * 2000. * (1337. + i as f64 * 250. + 4.5) / 25e6) as f32,
                    )
                })
                .collect();
            let error = (out
                .iter()
                .zip(expected)
                .map(|(a, b)| (*a - b).norm_sqr())
                .sum::<f32>()
                / out.len() as f32)
                .sqrt();
            assert!(error < 0.003, "bin {k} RMS {error}");
        }
        assert_eq!(cache.stats.misses, 1);
        assert_eq!(cache.stats.hits, 31);
    }
    #[test]
    fn unrelated_gap_in_tile_does_not_hide_valid_short_interval() {
        let mut c = Cache::new(1).unwrap();
        let z = c
            .read(16 * TILE + 5000, 10, 16 * TILE, 0., 30e3, |_, n| {
                if n > TILE as usize {
                    anyhow::bail!("unrelated gap")
                } else {
                    Ok(vec![Z::new(1., 0.); n])
                }
            })
            .unwrap();
        assert!(z.iter().all(|z| (*z - Z::new(1., 0.)).norm() < 1e-5));
        assert!(c
            .read(u64::MAX - 1, 10, 0, 0., 30e3, |_, _| unreachable!())
            .is_err());
    }
    #[test]
    fn eviction_errors_and_frame_boundaries() {
        let mut c = Cache::new(1).unwrap();
        let origin = 4_000_000u64;
        let mut source = |_: u64, n: usize| Ok(vec![Z::new(1., 0.); n]);
        let start = 16 * TILE - 137;
        let out = c.read(start, 50, origin, 0., 30e3, &mut source).unwrap();
        assert!(out.iter().all(|z| (*z - Z::new(1., 0.)).norm() < 1e-5));
        assert_eq!(c.stats.misses, 2);
        assert!(c
            .read(start, 50, origin, 0., 30e3, |_, _| anyhow::bail!(
                "sample gap"
            ))
            .is_err());
        assert!(c.read(start, 0, origin, 0., 30e3, &mut source).is_err());
        assert!(c.read(start, 1, origin, 0., 101e3, &mut source).is_err());
    }
}

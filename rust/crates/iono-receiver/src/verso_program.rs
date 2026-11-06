//! SGO VERSO schedule supplied in March 2026. The May follow-up clarified
//! that the waveform may be shared/published. Private correspondence is not
//! part of the distributable scientific program.
use anyhow::{ensure, Context, Result};
use hdf5::File;
use num_complex::Complex32 as Z;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{fs, path::Path};
pub const SITES: [&str; 4] = ["Joutseno", "Kuusamo", "Vaasa", "Sodankyla"];
pub const RATE: u64 = 62500;
pub const BAUD_SAMPLES: u64 = 625;
#[derive(Deserialize)]
struct Schedule {
    exp_runlist: Vec<(f64, u32, f64, String)>,
    #[serde(rename = "IPP")]
    ipp: Vec<u64>,
    groups_subch: Vec<Vec<f64>>,
}
#[derive(Clone, Debug)]
pub struct Group {
    pub start_samples: u64,
    pub duration_samples: u64,
    pub ipp_samples: u64,
    pub site: u8,
    pub frequencies: Vec<f64>,
}
pub struct Program {
    pub code: Vec<Z>,
    pub groups: Vec<Group>,
    pub sha256: String,
}
pub fn text(h: &File, name: &str, value: &str) -> Result<()> {
    h.new_dataset::<hdf5::types::VarLenUnicode>()
        .create(name)?
        .write_scalar(&value.parse::<hdf5::types::VarLenUnicode>()?)?;
    Ok(())
}
pub fn scalar(h: &File, name: &str, value: f64) -> Result<()> {
    h.new_dataset::<f64>().create(name)?.write_scalar(&value)?;
    Ok(())
}
impl Program {
    fn parse(yaml: &[u8], binary: &[u8]) -> Result<Self> {
        ensure!(
            yaml.len() <= 1024 * 1024 && binary.len() == 625 * 16,
            "unsupported VERSO attachment size"
        );
        let s: Schedule = serde_yaml_ng::from_slice(yaml)?;
        ensure!(
            !s.exp_runlist.is_empty()
                && s.exp_runlist.len() <= 100
                && s.ipp.len() == s.exp_runlist.len()
                && s.groups_subch.len() == s.ipp.len(),
            "schedule axes disagree"
        );
        let code = binary
            .chunks_exact(16)
            .map(|b| {
                Z::new(
                    f64::from_le_bytes(b[..8].try_into().unwrap()) as f32,
                    f64::from_le_bytes(b[8..].try_into().unwrap()) as f32,
                )
            })
            .collect();
        let mut groups = vec![];
        let mut start = 0;
        for (((duration, _waveform_id, carrier, site), ipp), frequencies) in
            s.exp_runlist.into_iter().zip(s.ipp).zip(s.groups_subch)
        {
            ensure!(
                duration.is_finite()
                    && duration > 0.
                    && duration <= 10.
                    && (duration * RATE as f64).fract().abs() < 1e-6
                    && carrier.is_finite(),
                "invalid group duration/tuning"
            );
            let site = match site.as_str() {
                "JOU" => 0,
                "KUU" => 1,
                "VAA" => 2,
                "SDT" => 3,
                "XXX" => 255,
                _ => anyhow::bail!("unknown transmitter {site}"),
            };
            let duration_samples = (duration * RATE as f64).round() as u64;
            groups.push(Group {
                start_samples: start,
                duration_samples,
                ipp_samples: ipp,
                site,
                frequencies,
            });
            start += duration_samples;
        }
        let p = Self {
            code,
            groups,
            sha256: String::new(),
        };
        p.validate()?;
        Ok(p)
    }
    fn validate(&self) -> Result<()> {
        ensure!(
            self.code.len() == 625
                && self
                    .code
                    .iter()
                    .all(|v| v.re.is_finite() && v.im.is_finite())
                && self.code.iter().map(|v| v.norm_sqr()).sum::<f32>() > 0.,
            "invalid supplied waveform"
        );
        let mut end = 0;
        for g in &self.groups {
            ensure!(
                g.start_samples == end
                    && g.duration_samples > 0
                    && g.ipp_samples > 0
                    && !g.frequencies.is_empty()
                    && g.frequencies.len() <= 12,
                "invalid group geometry"
            );
            ensure!(
                g.frequencies
                    .iter()
                    .all(|f| f.is_finite() && (0.1e6..24.9e6).contains(f)),
                "invalid RF frequencies"
            );
            if g.site != 255 {
                ensure!(
                    g.site < 4
                        && g.ipp_samples >= g.frequencies.len() as u64 * BAUD_SAMPLES
                        && g.duration_samples % g.ipp_samples == 0
                        && (8..=100).contains(&(g.duration_samples / g.ipp_samples)),
                    "invalid TDM repetition geometry"
                );
            }
            end = end
                .checked_add(g.duration_samples)
                .context("schedule overflow")?;
        }
        ensure!(
            end == 240 * RATE,
            "schedule must include silent slots and total 240 seconds"
        );
        Ok(())
    }
    pub fn import(yaml: &Path, binary: &Path, output: &Path) -> Result<()> {
        let y = fs::read(yaml)?;
        let b = fs::read(binary)?;
        let p = Self::parse(&y, &b)?;
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent)?;
        }
        let tmp = output.with_extension("partial.h5");
        let h = File::create(&tmp)?;
        macro_rules! put {
            ($n:expr,$v:expr) => {
                h.new_dataset_builder().with_data(&$v).create($n)?;
            };
        }
        put!("code_real", p.code.iter().map(|v| v.re).collect::<Vec<_>>());
        put!("code_imag", p.code.iter().map(|v| v.im).collect::<Vec<_>>());
        put!(
            "group_start_samples",
            p.groups.iter().map(|g| g.start_samples).collect::<Vec<_>>()
        );
        put!(
            "group_duration_samples",
            p.groups
                .iter()
                .map(|g| g.duration_samples)
                .collect::<Vec<_>>()
        );
        put!(
            "group_ipp_samples",
            p.groups.iter().map(|g| g.ipp_samples).collect::<Vec<_>>()
        );
        put!(
            "group_site_id",
            p.groups.iter().map(|g| g.site).collect::<Vec<_>>()
        );
        let mut offsets = vec![0u64];
        let mut frequencies = vec![];
        for g in &p.groups {
            frequencies.extend_from_slice(&g.frequencies);
            offsets.push(frequencies.len() as u64);
        }
        put!("frequency_offsets", offsets);
        put!("frequency_hz", frequencies);
        scalar(&h, "native_sample_rate_hz", RATE as f64)?;
        scalar(&h, "channel_stagger_samples", BAUD_SAMPLES as f64)?;
        scalar(&h, "period_s", 240.)?;
        scalar(&h, "start_offset_s", 235.)?;
        text(&h, "schema", "sgo-verso-program-v1")?;
        text(
            &h,
            "site_ids",
            "0=Joutseno,1=Kuusamo,2=Vaasa,3=Sodankyla,255=silence",
        )?;
        text(
            &h,
            "source_yaml_sha256",
            &format!("{:x}", Sha256::digest(&y)),
        )?;
        text(
            &h,
            "source_waveform_sha256",
            &format!("{:x}", Sha256::digest(&b)),
        )?;
        h.flush()?;
        drop(h);
        fs::rename(tmp, output)?;
        Ok(())
    }
    pub fn read(path: &Path) -> Result<Self> {
        let h = File::open(path)?;
        ensure!(
            iono_core::scalar(&h, "native_sample_rate_hz", 0.) == RATE as f64
                && iono_core::scalar(&h, "channel_stagger_samples", 0.) == BAUD_SAMPLES as f64
                && iono_core::scalar(&h, "period_s", 0.) == 240.
                && iono_core::scalar(&h, "start_offset_s", 0.) == 235.,
            "unsupported VERSO clock or timing"
        );
        ensure!(
            iono_core::text(&h, "schema", "") == "sgo-verso-program-v1",
            "unknown VERSO program schema"
        );
        ensure!(
            h.dataset("code_real")?.size() == 625
                && h.dataset("group_site_id")?.size() <= 100
                && h.dataset("frequency_hz")?.size() <= 1200,
            "program exceeds bounds"
        );
        let re = h.dataset("code_real")?.read_raw::<f32>()?;
        let im = h.dataset("code_imag")?.read_raw::<f32>()?;
        ensure!(re.len() == im.len(), "waveform axes disagree");
        let start = h.dataset("group_start_samples")?.read_raw::<u64>()?;
        let duration = h.dataset("group_duration_samples")?.read_raw::<u64>()?;
        let ipp = h.dataset("group_ipp_samples")?.read_raw::<u64>()?;
        let site = h.dataset("group_site_id")?.read_raw::<u8>()?;
        let offsets = h.dataset("frequency_offsets")?.read_raw::<u64>()?;
        let f = h.dataset("frequency_hz")?.read_raw::<f64>()?;
        let n = site.len();
        ensure!(
            start.len() == n
                && duration.len() == n
                && ipp.len() == n
                && offsets.len() == n + 1
                && offsets.first() == Some(&0)
                && offsets.last() == Some(&(f.len() as u64))
                && offsets.windows(2).all(|w| w[0] < w[1]),
            "program axes disagree"
        );
        let groups = (0..n)
            .map(|i| Group {
                start_samples: start[i],
                duration_samples: duration[i],
                ipp_samples: ipp[i],
                site: site[i],
                frequencies: f[offsets[i] as usize..offsets[i + 1] as usize].to_vec(),
            })
            .collect();
        let p = Self {
            code: re.into_iter().zip(im).map(|(r, i)| Z::new(r, i)).collect(),
            groups,
            sha256: format!("{:x}", Sha256::digest(fs::read(path)?)),
        };
        p.validate()?;
        Ok(p)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    const YAML: &[u8] = include_bytes!("../../../deploy/verso/SGO.yaml");
    const CODE: &[u8] = include_bytes!("../../../deploy/verso/SGO_bauds_complex128.bin");
    #[test]
    fn supplied_schedule_keeps_silence_split_stations_and_channel_offsets() {
        let p = Program::parse(YAML, CODE).unwrap();
        assert_eq!(p.groups.len(), 44);
        assert_eq!(p.groups[26].start_samples, 151 * RATE);
        assert_eq!(p.groups[38].start_samples, 211 * RATE);
        assert_eq!(p.groups[32].site, 2);
        assert_eq!(p.groups[25].site, 255);
        assert_eq!(p.groups[0].ipp_samples, 7500);
        assert_eq!(p.groups[26].ipp_samples, 6250);
        assert_eq!(p.groups[0].frequencies[0], 8791500.); // Not the hardware tuning column.
        assert_eq!(p.groups[0].frequencies.len(), 12);
        assert_eq!(11 * BAUD_SAMPLES, 6875);
    }
}

//! Experimental shared, overlapping multirate channelizer. Not wired to workers.
//!
//! Each requested passband chooses a sparse path through decimate-by-two nodes
//! centred at -Fs/4, 0 or +Fs/4 of their parent. The extra centre branch supplies
//! overlap: a signal is never assigned across a halfband filter transition.
//! A fractional FIR finishes at exactly 100 ksample/s; powers of two alone cannot
//! convert 25 MHz to 100 kHz. All outputs share the input frame's phase origin.
use crate::{dsp::fir_taps, simd};
use anyhow::{ensure, Result};
use num_complex::Complex32 as Z;
use std::{collections::BTreeMap, f64::consts::TAU};
const FS: f64 = 25_000_000.;
const PASS: f64 = 0.22;
const HALF_TAPS: usize = 95;
const FINAL_TAPS: usize = 191;
const DEPTH: u32 = 7;

#[derive(Clone, Copy)]
pub struct Request {
    pub offset_hz: f64,
    pub bandwidth_hz: f64,
}
struct Node {
    parent: usize,
    sign: i32,
    depth: u32,
    center: f64,
}
struct Leaf {
    node: usize,
    offset: f64,
}
struct Buffer {
    first: i64,
    stride: i64,
    values: Vec<Z>,
}
pub struct Output {
    pub first_sample: i64,
    pub values: Vec<Z>,
}
/// One plan processes several simultaneous requests from the same contiguous
/// frame. It does not share work across independent receiver processes by itself.
pub struct OverlappingTree {
    nodes: Vec<Node>,
    leaves: Vec<Leaf>,
    half: Vec<Z>,
    final_filters: BTreeMap<(u32, u64), Vec<Vec<Z>>>,
    bandwidths: Vec<f64>,
    uses: Vec<usize>,
}
impl OverlappingTree {
    pub fn new(requests: &[Request]) -> Result<Self> {
        ensure!(
            !requests.is_empty() && requests.len() <= 128,
            "request count must be 1..128"
        );
        let mut nodes = vec![Node {
            parent: 0,
            sign: 0,
            depth: 0,
            center: 0.,
        }];
        let mut children = BTreeMap::new();
        let mut leaves = vec![];
        let mut filters = BTreeMap::new();
        for r in requests {
            ensure!(
                r.offset_hz.is_finite() && r.bandwidth_hz > 0. && r.bandwidth_hz <= 100_000.,
                "invalid channel"
            );
            ensure!(
                r.offset_hz.abs() + r.bandwidth_hz / 2. < FS / 2.,
                "outside recorded band"
            );
            ensure!(r.offset_hz.abs()+r.bandwidth_hz/2. <= 0.47*FS,
                "experimental tree excludes the extreme ADC edges; use the direct channelizer there");
            let mut parent = 0;
            for depth in 1..=DEPTH {
                let parent_fs = FS / (1u32 << (depth - 1)) as f64;
                let sign = ((r.offset_hz - nodes[parent].center) / (parent_fs / 4.))
                    .round()
                    .clamp(-1., 1.) as i32;
                let center = nodes[parent].center + sign as f64 * parent_fs / 4.;
                // The ENTIRE signal bandwidth, not just its carrier, must fit.
                // Otherwise stop here and finish from the wider parent stream.
                if (r.offset_hz - center).abs() + r.bandwidth_hz / 2. > PASS * parent_fs {
                    break;
                }
                let child = *children.entry((parent, sign)).or_insert_with(|| {
                    let i = nodes.len();
                    nodes.push(Node {
                        parent,
                        sign,
                        depth,
                        center,
                    });
                    i
                });
                parent = child;
            }
            let depth = nodes[parent].depth;
            let stride = 1u32 << depth;
            filters
                .entry((depth, r.bandwidth_hz.to_bits()))
                .or_insert_with(|| {
                    (0..stride)
                        .map(|phase| {
                            fractional_taps(
                                FINAL_TAPS,
                                (r.bandwidth_hz / 2.).min(45_000.) / (FS / stride as f64),
                                phase as f64 / stride as f64,
                            )
                        })
                        .collect()
                });
            leaves.push(Leaf {
                node: parent,
                offset: r.offset_hz - nodes[parent].center,
            });
        }
        let mut uses = vec![0; nodes.len()];
        for n in &nodes[1..] {
            uses[n.parent] += 1;
        }
        for l in &leaves {
            uses[l.node] += 1;
        }
        let half = fir_taps(HALF_TAPS, 0.25, 8.6)
            .into_iter()
            .map(|h| Z::new(h, h))
            .collect();
        Ok(Self {
            nodes,
            leaves,
            half,
            final_filters: filters,
            bandwidths: requests.iter().map(|r| r.bandwidth_hz).collect(),
            uses,
        })
    }
    pub fn node_count(&self) -> usize {
        self.nodes.len() - 1
    }
    pub fn depths(&self) -> Vec<u32> {
        self.leaves
            .iter()
            .map(|l| self.nodes[l.node].depth)
            .collect()
    }
    /// `first_sample` is a relative integer index in a shared 25 MHz frame
    /// origin, not a floating-point Unix epoch. Supply real overlap at both
    /// ends. Unsupported filter edges and input gaps remain NaN; they are never
    /// padded with invented voltage. Frame-origin metadata must accompany any
    /// future inter-process transport so Doppler coherence can be preserved.
    pub fn process(&self, raw: &[Z], first_sample: i64) -> Vec<Output> {
        if raw.is_empty() {
            return self
                .leaves
                .iter()
                .map(|_| Output {
                    first_sample,
                    values: vec![],
                })
                .collect();
        }
        let mut buffers: Vec<Option<Buffer>> = (0..self.nodes.len()).map(|_| None).collect();
        // Root is borrowed below; no full-band copy is needed.
        let mut uses = self.uses.clone();
        for i in 1..self.nodes.len() {
            let n = &self.nodes[i];
            let (values, first, stride) = if n.parent == 0 {
                (raw, first_sample, 1)
            } else {
                let b = buffers[n.parent].as_ref().unwrap();
                (b.values.as_slice(), b.first, b.stride)
            };
            let child = halfband(values, first, stride, n.sign, &self.half);
            buffers[i] = Some(child);
            uses[n.parent] -= 1;
            if n.parent > 0 && uses[n.parent] == 0 {
                buffers[n.parent] = None;
            }
        }
        let mut result = Vec::with_capacity(self.leaves.len());
        for (index, l) in self.leaves.iter().enumerate() {
            let (values, first, stride) = if l.node == 0 {
                (raw, first_sample, 1)
            } else {
                let b = buffers[l.node].as_ref().unwrap();
                (b.values.as_slice(), b.first, b.stride)
            };
            let fs = FS / stride as f64;
            let mut phase = -TAU * l.offset * first as f64 / FS;
            let step = -TAU * l.offset / fs;
            let mixed: Vec<_> = values
                .iter()
                .enumerate()
                .map(|(i, z)| {
                    // Re-anchor in f64 rather than accumulating f32 oscillator drift.
                    if i % 4096 == 0 {
                        phase = (-TAU * l.offset * (first + (i as i64) * stride) as f64 / FS)
                            .rem_euclid(TAU);
                    }
                    let q = Z::new(phase.cos() as f32, phase.sin() as f32);
                    phase += step;
                    *z * q
                })
                .collect();
            let filters =
                &self.final_filters[&(self.nodes[l.node].depth, self.bandwidths[index].to_bits())];
            let first_out =
                first_sample.div_euclid(250) + i64::from(first_sample.rem_euclid(250) != 0);
            let end = (first_sample + raw.len() as i64 - 1).div_euclid(250);
            let mut out = vec![Z::new(f32::NAN, f32::NAN); (end - first_out + 1).max(0) as usize];
            for (j, z) in out.iter_mut().enumerate() {
                let at = (first_out + j as i64) * 250 - first;
                let center = at.div_euclid(stride);
                let fraction = at.rem_euclid(stride) as usize;
                let lo = center - FINAL_TAPS as i64 / 2;
                let hi = lo + FINAL_TAPS as i64;
                if lo >= 0 && hi <= mixed.len() as i64 {
                    *z = simd::weighted_sum(&mixed[lo as usize..hi as usize], &filters[fraction]);
                }
            }
            result.push(Output {
                first_sample: first_out * 250,
                values: out,
            });
            uses[l.node] -= 1;
            if l.node > 0 && uses[l.node] == 0 {
                buffers[l.node] = None;
            }
        }
        result
    }
}
fn halfband(raw: &[Z], first: i64, stride: i64, sign: i32, taps: &[Z]) -> Buffer {
    let mixed: Vec<_> = raw
        .iter()
        .enumerate()
        .map(
            |(i, &z)| match (sign as i64 * (first / stride + i as i64)).rem_euclid(4) {
                0 => z,
                1 => Z::new(z.im, -z.re),
                2 => -z,
                _ => Z::new(-z.im, z.re),
            },
        )
        .collect();
    let next = stride * 2;
    let start = first.div_euclid(next) + i64::from(first.rem_euclid(next) != 0);
    let end = (first + (raw.len() as i64 - 1) * stride).div_euclid(next);
    let mut values = vec![Z::new(f32::NAN, f32::NAN); (end - start + 1).max(0) as usize];
    for (i, z) in values.iter_mut().enumerate() {
        let center = ((start + i as i64) * next - first) / stride;
        let lo = center - taps.len() as i64 / 2;
        let hi = lo + taps.len() as i64;
        if lo >= 0 && hi <= raw.len() as i64 {
            *z = simd::weighted_sum(&mixed[lo as usize..hi as usize], taps);
        }
    }
    Buffer {
        first: start * next,
        stride: next,
        values,
    }
}
fn fractional_taps(n: usize, cutoff: f64, fraction: f64) -> Vec<Z> {
    fn i0(x: f64) -> f64 {
        let (mut sum, mut term) = (1., 1.);
        for k in 1..60 {
            term *= x * x / (4. * (k * k) as f64);
            sum += term;
            if term < sum * 1e-16 {
                break;
            }
        }
        sum
    }
    let mut h = vec![];
    let denom = i0(8.6);
    for k in 0..n {
        let t = k as f64 - (n - 1) as f64 / 2. - fraction;
        let u = 2. * k as f64 / (n - 1) as f64 - 1.;
        let sinc = if t.abs() < 1e-14 {
            2. * cutoff
        } else {
            (TAU * cutoff * t).sin() / (std::f64::consts::PI * t)
        };
        h.push(sinc * i0(8.6 * (1. - u * u).max(0.).sqrt()) / denom);
    }
    let sum: f64 = h.iter().sum();
    h.into_iter()
        .map(|v| Z::new((v / sum) as f32, (v / sum) as f32))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_assigned_passband_has_overlap_margin() {
        let requests: Vec<_> = (-110..=110)
            .map(|i| Request {
                offset_hz: i as f64 * 100_000.,
                bandwidth_hz: 80_000.,
            })
            .collect();
        for chunk in requests.chunks(64) {
            let tree = OverlappingTree::new(chunk).unwrap();
            for (r, l) in chunk.iter().zip(&tree.leaves) {
                let mut node = l.node;
                while node > 0 {
                    let n = &tree.nodes[node];
                    let parent_fs = FS / (1u32 << (n.depth - 1)) as f64;
                    assert!(
                        (r.offset_hz - n.center).abs() + r.bandwidth_hz / 2. <= PASS * parent_fs
                    );
                    node = n.parent;
                }
            }
        }
    }
    #[test]
    fn empty_input_and_unavailable_samples_stay_unavailable() {
        let tree = OverlappingTree::new(&[Request {
            offset_hz: 0.,
            bandwidth_hz: 80_000.,
        }])
        .unwrap();
        assert!(tree.process(&[], 0)[0].values.is_empty());
        let missing = vec![Z::new(f32::NAN, f32::NAN); 100_000];
        assert!(tree.process(&missing, 0)[0]
            .values
            .iter()
            .all(|z| z.re.is_nan()));
    }
    #[test]
    fn unsupported_adc_edges_are_rejected_instead_of_aliased() {
        assert!(OverlappingTree::new(&[Request {
            offset_hz: 12_400_000.,
            bandwidth_hz: 80_000.
        }])
        .is_err());
    }
}

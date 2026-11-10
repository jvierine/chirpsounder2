//! Paired whole-channelizer benchmark; the runner supplies current production
//! sources and the exact pre-change SIMD baseline in a temporary standalone crate.
use num_complex::Complex32 as Z;
use std::{hint::black_box, time::Instant};
mod channelizer_after;
mod channelizer_before;
mod dsp;
mod simd_after;
mod simd_before;
#[repr(C)]
struct Ts {
    sec: i64,
    ns: i64,
}
unsafe extern "C" {
    fn clock_gettime(clock: i32, time: *mut Ts) -> i32;
}
fn cpu() -> f64 {
    let mut t = Ts { sec: 0, ns: 0 };
    assert_eq!(unsafe { clock_gettime(3, &mut t) }, 0);
    t.sec as f64 + t.ns as f64 * 1e-9
}
fn main() -> anyhow::Result<()> {
    let before = channelizer_before::Channelizer::new(1_700_000., 80_000.)?;
    let after = channelizer_after::Channelizer::new(1_700_000., 80_000.)?;
    // A complete 100 ms PRN code period plus real read-overlap at 25 MHz.
    let raw: Vec<_> = (0..2_502_000)
        .map(|i| Z::new((i as f32 * 0.031).sin(), (i as f32 * 0.071).cos()))
        .collect();
    // Independently compare a carrier whose chunk phase is not an integer turn.
    let off_before = channelizer_before::Channelizer::new(1_712_345.67, 80_000.)?;
    let off_after = channelizer_after::Channelizer::new(1_712_345.67, 80_000.)?;
    let off_reference = off_before.process(&raw, 1_234_567);
    let off_actual = off_after.process(&raw, 1_234_567);
    let off_error = off_reference
        .iter()
        .zip(&off_actual)
        .map(|(a, b)| (*a - *b).norm())
        .fold(0f32, f32::max);
    assert!(
        off_error < 2e-6,
        "off-grid channelizer mismatch {off_error}"
    );
    println!("{{\"off_grid_max_abs\":{off_error}}}");
    let reference = before.process(&raw, -1000);
    let actual = after.process(&raw, -1000);
    let max_error = reference
        .iter()
        .zip(&actual)
        .map(|(a, b)| (*a - *b).norm())
        .fold(0f32, f32::max);
    assert!(max_error < 2e-6, "channelizer mismatch {max_error}");
    println!("{{\"correctness_max_abs\":{max_error},\"backend\":\"{}\",\"input_samples\":{},\"output_samples\":{}}}",simd_after::backend(),raw.len(),actual.len());
    for trial in 0..15 {
        for implementation in if trial % 2 == 0 { [0, 1] } else { [1, 0] } {
            let begin = Instant::now();
            let c = cpu();
            let output = if implementation == 0 {
                before.process(black_box(&raw), -1000)
            } else {
                after.process(black_box(&raw), -1000)
            };
            let cpu_s = cpu() - c;
            let wall_s = begin.elapsed().as_secs_f64();
            black_box(output);
            println!("{{\"work\":\"channelizer_100ms\",\"trial\":{trial},\"implementation\":\"{}\",\"cpu_s\":{cpu_s:.9},\"wall_s\":{wall_s:.9}}}",if implementation==0 {"baseline"}else{"assembly"});
        }
    }
    Ok(())
}

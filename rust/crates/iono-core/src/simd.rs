//! Runtime-dispatched complex64 (two f32 components) kernels. FMA is optional;
//! the Ivy Bridge KHO receiver retains compatible AVX and SSE3 fallbacks.
use num_complex::Complex32 as Z;

pub fn backend() -> &'static str {
    #[cfg(target_arch = "x86_64")]
    {
        if std::is_x86_feature_detected!("avx") && std::is_x86_feature_detected!("fma") {
            return "avx+fma";
        }
        if std::is_x86_feature_detected!("avx") {
            return "avx";
        }
        if std::is_x86_feature_detected!("sse3") {
            return "sse3";
        }
    }
    "scalar"
}

pub fn multiply(a: &[Z], b: &[Z], out: &mut [Z]) {
    assert_eq!(a.len(), b.len());
    assert_eq!(a.len(), out.len());
    #[cfg(target_arch = "x86_64")]
    unsafe {
        if std::is_x86_feature_detected!("avx") && std::is_x86_feature_detected!("fma") {
            return avx_fma(a, b, out);
        }
        if std::is_x86_feature_detected!("avx") {
            return avx(a, b, out);
        }
        if std::is_x86_feature_detected!("sse3") {
            return sse3(a, b, out);
        }
    }
    scalar(a, b, out);
}
fn scalar(a: &[Z], b: &[Z], out: &mut [Z]) {
    for ((&a, &b), o) in a.iter().zip(b).zip(out) {
        *o = a * b;
    }
}
/// Real FIR dot product; weights are duplicated into real/imaginary lanes.
pub fn weighted_sum(a: &[Z], weights: &[Z]) -> Z {
    assert_eq!(a.len(), weights.len());
    #[cfg(target_arch = "x86_64")]
    unsafe {
        if std::is_x86_feature_detected!("avx") && std::is_x86_feature_detected!("fma") {
            return dot_avx_fma(a, weights);
        }
        if std::is_x86_feature_detected!("avx") {
            return dot_avx(a, weights);
        }
        if std::is_x86_feature_detected!("sse3") {
            return dot_sse(a, weights);
        }
    }
    a.iter().zip(weights).map(|(a, w)| *a * w.re).sum()
}
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx,fma")]
unsafe fn dot_avx_fma(a: &[Z], w: &[Z]) -> Z {
    use std::arch::x86_64::*;
    // Independent accumulators hide the fused multiply-add dependency latency.
    let mut sums = [_mm256_setzero_ps(); 4];
    let block = a.len() / 16 * 16;
    for i in (0..block).step_by(16) {
        for (lane, sum) in sums.iter_mut().enumerate() {
            let at = i + lane * 4;
            *sum = _mm256_fmadd_ps(
                _mm256_loadu_ps(a.as_ptr().add(at).cast()),
                _mm256_loadu_ps(w.as_ptr().add(at).cast()),
                *sum,
            );
        }
    }
    let mut sum = _mm256_add_ps(
        _mm256_add_ps(sums[0], sums[1]),
        _mm256_add_ps(sums[2], sums[3]),
    );
    let n = a.len() / 4 * 4;
    for i in (block..n).step_by(4) {
        sum = _mm256_fmadd_ps(
            _mm256_loadu_ps(a.as_ptr().add(i).cast()),
            _mm256_loadu_ps(w.as_ptr().add(i).cast()),
            sum,
        );
    }
    let mut s = [0f32; 8];
    _mm256_storeu_ps(s.as_mut_ptr(), sum);
    Z::new(s[0] + s[2] + s[4] + s[6], s[1] + s[3] + s[5] + s[7])
        + a[n..]
            .iter()
            .zip(&w[n..])
            .map(|(a, w)| *a * w.re)
            .sum::<Z>()
}
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx,fma")]
unsafe fn avx_fma(a: &[Z], b: &[Z], out: &mut [Z]) {
    use std::arch::x86_64::*;
    let n = a.len() / 4 * 4;
    for i in (0..n).step_by(4) {
        let x = _mm256_loadu_ps(a.as_ptr().add(i).cast());
        let y = _mm256_loadu_ps(b.as_ptr().add(i).cast());
        _mm256_storeu_ps(
            out.as_mut_ptr().add(i).cast(),
            _mm256_fmaddsub_ps(
                _mm256_moveldup_ps(x),
                y,
                _mm256_mul_ps(_mm256_movehdup_ps(x), _mm256_permute_ps(y, 0xb1)),
            ),
        );
    }
    scalar(&a[n..], &b[n..], &mut out[n..]);
}
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx")]
unsafe fn dot_avx(a: &[Z], w: &[Z]) -> Z {
    use std::arch::x86_64::*;
    // Four independent chains expose add/multiply throughput on non-FMA AVX
    // CPUs (notably Ivy Bridge). Unaligned loads are intentional: FIR windows
    // advance by 25 complex samples, so alignment changes on every output.
    // Safety: each iteration reads exactly 16 in-bounds complex values from
    // both slices; all clobbers are declared, no stack or memory is written.
    let block = a.len() / 16 * 16;
    let mut sum: __m256;
    std::arch::asm!(
        "vxorps ymm0, ymm0, ymm0",
        "vxorps ymm1, ymm1, ymm1",
        "vxorps ymm2, ymm2, ymm2",
        "vxorps ymm3, ymm3, ymm3",
        "test {blocks}, {blocks}",
        "jz 3f",
        "2:",
        "vmovups ymm4, [{a}]",
        "vmovups ymm5, [{a} + 32]",
        "vmulps ymm4, ymm4, [{w}]",
        "vmulps ymm5, ymm5, [{w} + 32]",
        "vaddps ymm0, ymm0, ymm4",
        "vaddps ymm1, ymm1, ymm5",
        "vmovups ymm4, [{a} + 64]",
        "vmovups ymm5, [{a} + 96]",
        "vmulps ymm4, ymm4, [{w} + 64]",
        "vmulps ymm5, ymm5, [{w} + 96]",
        "vaddps ymm2, ymm2, ymm4",
        "vaddps ymm3, ymm3, ymm5",
        "add {a}, 128",
        "add {w}, 128",
        "dec {blocks}",
        "jnz 2b",
        "3:",
        "vaddps ymm0, ymm0, ymm1",
        "vaddps ymm2, ymm2, ymm3",
        "vaddps ymm0, ymm0, ymm2",
        a = inout(reg) a.as_ptr() => _,
        w = inout(reg) w.as_ptr() => _,
        blocks = inout(reg) block / 16 => _,
        out("ymm0") sum,
        out("ymm1") _, out("ymm2") _, out("ymm3") _,
        out("ymm4") _, out("ymm5") _,
        options(nostack, readonly),
    );
    let n = a.len() / 4 * 4;
    for i in (block..n).step_by(4) {
        sum = _mm256_add_ps(
            sum,
            _mm256_mul_ps(
                _mm256_loadu_ps(a.as_ptr().add(i).cast()),
                _mm256_loadu_ps(w.as_ptr().add(i).cast()),
            ),
        );
    }
    let mut s = [0f32; 8];
    _mm256_storeu_ps(s.as_mut_ptr(), sum);
    Z::new(s[0] + s[2] + s[4] + s[6], s[1] + s[3] + s[5] + s[7])
        + a[n..]
            .iter()
            .zip(&w[n..])
            .map(|(a, w)| *a * w.re)
            .sum::<Z>()
}
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse3")]
unsafe fn dot_sse(a: &[Z], w: &[Z]) -> Z {
    use std::arch::x86_64::*;
    let mut sum = _mm_setzero_ps();
    let n = a.len() / 2 * 2;
    for i in (0..n).step_by(2) {
        sum = _mm_add_ps(
            sum,
            _mm_mul_ps(
                _mm_loadu_ps(a.as_ptr().add(i).cast()),
                _mm_loadu_ps(w.as_ptr().add(i).cast()),
            ),
        );
    }
    let mut s = [0f32; 4];
    _mm_storeu_ps(s.as_mut_ptr(), sum);
    Z::new(s[0] + s[2], s[1] + s[3])
        + a[n..]
            .iter()
            .zip(&w[n..])
            .map(|(a, w)| *a * w.re)
            .sum::<Z>()
}
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx")]
unsafe fn avx(a: &[Z], b: &[Z], out: &mut [Z]) {
    use std::arch::x86_64::*;
    let n = a.len() / 4 * 4;
    for i in (0..n).step_by(4) {
        let x = _mm256_loadu_ps(a.as_ptr().add(i).cast());
        let y = _mm256_loadu_ps(b.as_ptr().add(i).cast());
        let re = _mm256_moveldup_ps(x);
        let im = _mm256_movehdup_ps(x);
        let swap = _mm256_permute_ps(y, 0xb1);
        _mm256_storeu_ps(
            out.as_mut_ptr().add(i).cast(),
            _mm256_addsub_ps(_mm256_mul_ps(re, y), _mm256_mul_ps(im, swap)),
        );
    }
    scalar(&a[n..], &b[n..], &mut out[n..]);
}
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse3")]
unsafe fn sse3(a: &[Z], b: &[Z], out: &mut [Z]) {
    use std::arch::x86_64::*;
    let n = a.len() / 2 * 2;
    for i in (0..n).step_by(2) {
        let x = _mm_loadu_ps(a.as_ptr().add(i).cast());
        let y = _mm_loadu_ps(b.as_ptr().add(i).cast());
        _mm_storeu_ps(
            out.as_mut_ptr().add(i).cast(),
            _mm_addsub_ps(
                _mm_mul_ps(_mm_moveldup_ps(x), y),
                _mm_mul_ps(_mm_movehdup_ps(x), _mm_shuffle_ps(y, y, 0xb1)),
            ),
        );
    }
    scalar(&a[n..], &b[n..], &mut out[n..]);
}

// Pre-optimization baseline; excluded from production binaries.
#[cfg(all(test, target_arch = "x86_64"))]
#[target_feature(enable = "avx")]
unsafe fn dot_avx_serial_reference(a: &[Z], w: &[Z]) -> Z {
    use std::arch::x86_64::*;
    let mut sum = _mm256_setzero_ps();
    let n = a.len() / 4 * 4;
    for i in (0..n).step_by(4) {
        sum = _mm256_add_ps(
            sum,
            _mm256_mul_ps(
                _mm256_loadu_ps(a.as_ptr().add(i).cast()),
                _mm256_loadu_ps(w.as_ptr().add(i).cast()),
            ),
        );
    }
    let mut s = [0f32; 8];
    _mm256_storeu_ps(s.as_mut_ptr(), sum);
    Z::new(s[0] + s[2] + s[4] + s[6], s[1] + s[3] + s[5] + s[7])
        + a[n..]
            .iter()
            .zip(&w[n..])
            .map(|(a, w)| *a * w.re)
            .sum::<Z>()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn close(actual: &[Z], expected: &[Z]) {
        for (a, e) in actual.iter().zip(expected) {
            assert!((*a - *e).norm() <= 2e-6 * (1. + e.norm()), "{a:?} != {e:?}");
        }
    }
    #[test]
    fn simd_matches_scalar_including_unaligned_tails() {
        let a: Vec<_> = (0..41)
            .map(|i| Z::new(i as f32 * 0.1, -(i as f32).sin()))
            .collect();
        let b: Vec<_> = a.iter().map(|v| Z::new(-v.im, v.re + 0.7)).collect();
        for n in 0..40 {
            let mut expected = vec![Z::default(); n];
            scalar(&a[1..1 + n], &b[1..1 + n], &mut expected);
            let mut actual = expected.clone();
            multiply(&a[1..1 + n], &b[1..1 + n], &mut actual);
            close(&actual, &expected);
            let w: Vec<_> = b[1..1 + n].iter().map(|v| Z::new(v.re, v.re)).collect();
            let dot = a[1..1 + n]
                .iter()
                .zip(&w)
                .map(|(v, w)| *v * w.re)
                .sum::<Z>();
            assert!((weighted_sum(&a[1..1 + n], &w) - dot).norm() < 1e-3);
            #[cfg(target_arch = "x86_64")]
            unsafe {
                if std::is_x86_feature_detected!("sse3") {
                    sse3(&a[1..1 + n], &b[1..1 + n], &mut actual);
                    assert_eq!(actual, expected);
                }
                if std::is_x86_feature_detected!("avx") {
                    avx(&a[1..1 + n], &b[1..1 + n], &mut actual);
                    close(&actual, &expected);
                    assert!((dot_avx(&a[1..1 + n], &w) - dot).norm() < 1e-3);
                }
                if std::is_x86_feature_detected!("avx") && std::is_x86_feature_detected!("fma") {
                    avx_fma(&a[1..1 + n], &b[1..1 + n], &mut actual);
                    close(&actual, &expected);
                    assert!((dot_avx_fma(&a[1..1 + n], &w) - dot).norm() < 1e-3);
                }
            }
        }
    }
    #[test]
    fn fir_length_191_matches_double_precision_reference() {
        let a: Vec<_> = (0..192)
            .map(|i| Z::new((i as f32 * 0.13).sin(), (i as f32 * 0.27).cos()))
            .collect();
        let w: Vec<_> = (0..191)
            .map(|i| {
                let v = (i as f32 * 0.19).sin() / 191.;
                Z::new(v, v)
            })
            .collect();
        let reference: num_complex::Complex64 = a[1..]
            .iter()
            .zip(&w)
            .map(|(a, w)| num_complex::Complex64::new(a.re as f64, a.im as f64) * w.re as f64)
            .sum();
        let value = weighted_sum(&a[1..], &w);
        assert!(
            (num_complex::Complex64::new(value.re as f64, value.im as f64) - reference).norm()
                < 1e-6
        );
    }
    #[test]
    #[cfg(target_arch = "x86_64")]
    fn avx_assembly_matches_f64_for_all_tails_and_offsets() {
        if !std::is_x86_feature_detected!("avx") {
            return;
        }
        for offset in 0..8 {
            for n in 0..520 {
                let a: Vec<_> = (0..n + offset)
                    .map(|i| Z::new((i as f32 * 0.131).sin(), (i as f32 * 0.173).cos()))
                    .collect();
                let w: Vec<_> = (0..n + offset)
                    .map(|i| {
                        let h = (i as f32 * 0.31).sin() / 191.;
                        Z::new(h, h)
                    })
                    .collect();
                let x = &a[offset..];
                let h = &w[offset..];
                let reference: num_complex::Complex64 = x
                    .iter()
                    .zip(h)
                    .map(|(z, h)| {
                        num_complex::Complex64::new(z.re as f64, z.im as f64) * h.re as f64
                    })
                    .sum();
                let value = unsafe { dot_avx(x, h) };
                assert!(
                    (num_complex::Complex64::new(value.re as f64, value.im as f64) - reference)
                        .norm()
                        < 2e-6,
                    "n={n} offset={offset}"
                );
            }
        }
    }

    /// Same-process, alternating-order comparison with the exact former AVX
    /// implementation. No HDF5, FFT or receiver-I/O speedup is implied.
    #[test]
    #[ignore = "paired receiver benchmark; release mode, --ignored --nocapture"]
    #[cfg(all(target_arch = "x86_64", target_os = "linux"))]
    fn benchmark_avx_assembly_paired() {
        use std::{hint::black_box, time::Instant};
        if !std::is_x86_feature_detected!("avx") {
            return;
        }
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
        let a: Vec<_> = (0..2_097_153)
            .map(|i| Z::new((i as f32 * 0.013).sin(), (i as f32 * 0.017).cos()))
            .collect();
        let w: Vec<_> = (0..191)
            .map(|i| {
                let h = (i as f32 * 0.031).sin() / 191.;
                Z::new(h, h)
            })
            .collect();
        type Dot = unsafe fn(&[Z], &[Z]) -> Z;
        let functions: [Dot; 2] = [dot_avx_serial_reference, dot_avx];
        // Warm both paths before timing. Hot-kernel and strided FIR workloads
        // share data and run on one thread; checksum/black_box prevent DCE.
        for f in functions {
            for _ in 0..1000 {
                black_box(unsafe { f(&a[1..192], &w) });
            }
        }
        for (work, iterations, stride, span) in [
            ("fir191_hot", 200_000, 0, 1),
            ("fir191_stream", 65_536, 25, 2_000_000),
        ] {
            for trial in 0..11 {
                for index in if trial % 2 == 0 { [0, 1] } else { [1, 0] } {
                    let f = functions[index];
                    let mut checksum = Z::default();
                    let start = Instant::now();
                    let c = cpu();
                    for i in 0..iterations {
                        let at = 1 + (i * stride) % span;
                        checksum += unsafe { f(black_box(&a[at..at + 191]), black_box(&w)) };
                    }
                    let cpu_s = cpu() - c;
                    let wall_s = start.elapsed().as_secs_f64();
                    black_box(checksum);
                    println!("{{\"work\":\"{work}\",\"trial\":{trial},\"implementation\":\"{}\",\"iterations\":{iterations},\"cpu_s\":{cpu_s:.9},\"wall_s\":{wall_s:.9},\"checksum_re\":{:.7}}}",if index==0 {"baseline"}else{"assembly"},checksum.re);
                }
            }
        }
    }
    #[test]
    #[ignore = "receiver CPU benchmark; run in release mode with --ignored --nocapture"]
    #[cfg(target_arch = "x86_64")]
    fn benchmark_receiver_kernels() {
        use std::{hint::black_box, time::Instant};
        let a: Vec<_> = (0..16385)
            .map(|i| Z::new((i as f32 * 0.1).sin(), 0.3))
            .collect();
        let b: Vec<_> = a.iter().map(|z| Z::new(z.re, z.re)).collect();
        let mut out = vec![Z::default(); 16384];
        let mut kernels: Vec<(
            &str,
            unsafe fn(&[Z], &[Z], &mut [Z]),
            unsafe fn(&[Z], &[Z]) -> Z,
        )> = vec![];
        if std::is_x86_feature_detected!("sse3") {
            kernels.push(("sse3", sse3, dot_sse));
        }
        if std::is_x86_feature_detected!("avx") {
            kernels.push(("avx", avx, dot_avx));
        }
        if std::is_x86_feature_detected!("avx") && std::is_x86_feature_detected!("fma") {
            kernels.push(("avx+fma", avx_fma, dot_avx_fma));
        }
        for (name, mul, dot) in kernels {
            let start = Instant::now();
            for _ in 0..2000 {
                unsafe { mul(black_box(&a[1..]), black_box(&b[1..]), black_box(&mut out)) };
            }
            let multiply = start.elapsed().as_secs_f64();
            let start = Instant::now();
            for _ in 0..2_000_000 {
                black_box(unsafe { dot(black_box(&a[1..192]), black_box(&b[1..192])) });
            }
            println!(
                "{name}: multiply_32768000_s={multiply:.6} fir_191_x2000000_s={:.6}",
                start.elapsed().as_secs_f64()
            );
        }
        println!("runtime selected {}", backend());
    }
}

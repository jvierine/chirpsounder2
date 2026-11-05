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

//! Benchmark complete complex64 x; setup is separate from recurring computation.
use anyhow::{ensure, Result};
use iono_core::{prn, prn_matrix::Factor, simd};
use num_complex::Complex32 as Z;
use rustfft::{Fft, FftPlanner};
use std::{sync::Arc, time::Instant};
struct Rhs {
    n: usize,
    first: usize,
    nr: usize,
    scale: f32,
    kernel: Vec<Z>,
    work: Vec<Z>,
    filtered: Vec<Z>,
    scratch: Vec<Z>,
    fft: Arc<dyn Fft<f32>>,
    ifft: Arc<dyn Fft<f32>>,
}
impl Rhs {
    fn new(code: &[Z], first: usize, nr: usize) -> Self {
        let n = code.len();
        let mut p = FftPlanner::new();
        let fft = p.plan_fft_forward(n);
        let ifft = p.plan_fft_inverse(n);
        let mut kernel = code.to_vec();
        fft.process(&mut kernel);
        for z in &mut kernel {
            *z = z.conj();
        }
        let scratch = vec![
            Z::default();
            fft.get_inplace_scratch_len()
                .max(ifft.get_inplace_scratch_len())
        ];
        Self {
            n,
            first,
            nr,
            scale: 1. / (n as f32 * code.iter().map(|z| z.norm_sqr()).sum::<f32>()),
            kernel,
            work: vec![Z::default(); n],
            filtered: vec![Z::default(); n],
            scratch,
            fft,
            ifft,
        }
    }
    fn compute(&mut self, input: &[Z], out: &mut [Z]) {
        for (v, b) in input
            .chunks_exact(self.n)
            .zip(out.chunks_exact_mut(self.nr))
        {
            self.work.copy_from_slice(v);
            self.fft
                .process_with_scratch(&mut self.work, &mut self.scratch);
            simd::multiply(&self.work, &self.kernel, &mut self.filtered);
            self.ifft
                .process_with_scratch(&mut self.filtered, &mut self.scratch);
            for (x, y) in b
                .iter_mut()
                .zip(&self.filtered[self.first..self.first + self.nr])
            {
                *x = *y * self.scale;
            }
        }
    }
}
#[repr(C)]
struct Time {
    sec: i64,
    ns: i64,
}
unsafe extern "C" {
    fn clock_gettime(id: i32, time: *mut Time) -> i32;
}
fn cpu() -> f64 {
    let mut t = Time { sec: 0, ns: 0 };
    unsafe {
        clock_gettime(2, &mut t);
    }
    t.sec as f64 + t.ns as f64 * 1e-9
}
fn main() -> Result<()> {
    let a: Vec<_> = std::env::args().collect();
    ensure!(
        a.len() == 5,
        "usage: iono-prn-bench FIRST_LAG RANGE_BINS OUTPUT.h5 REPEATS"
    );
    let first: usize = a[1].parse()?;
    let nr: usize = a[2].parse()?;
    let repeats: usize = a[4].parse()?;
    ensure!((9..=101).contains(&repeats), "need 9..101 repetitions");
    let code = prn::code(10, 10000, 400, 400)?;
    let n = code.len();
    ensure!(first + nr <= n, "invalid window");
    let t = Instant::now();
    let factor = Factor::new(&code, nr)?;
    let factor_setup = t.elapsed().as_secs_f64();
    let t = Instant::now();
    let inverse = factor.inverse()?;
    let inverse_setup = t.elapsed().as_secs_f64();
    let t = Instant::now();
    let dense = factor.estimator(&code, first)?;
    let dense_setup = t.elapsed().as_secs_f64();
    let t = Instant::now();
    let mut rhs = Rhs::new(&code, first, nr);
    let rhs_setup = t.elapsed().as_secs_f64();
    let h = hdf5::File::create_excl(&a[3])?;
    let text = |name: &str, s: &str| -> Result<()> {
        let v: hdf5::types::VarLenUnicode = s.parse()?;
        h.new_dataset::<hdf5::types::VarLenUnicode>()
            .create(name)?
            .write_scalar(&v)?;
        Ok(())
    };
    for (key, value) in [
        ("rhs_setup_s", rhs_setup),
        ("code_length", n as f64),
        ("first_lag", first as f64),
        ("range_bins", nr as f64),
        ("factor_setup_s", factor_setup),
        ("inverse_additional_setup_s", inverse_setup),
        ("B_additional_setup_s", dense_setup),
        ("factor_cache_bytes", factor.bytes() as f64),
        ("inverse_cache_bytes", inverse.bytes() as f64),
        ("B_cache_bytes", dense.bytes() as f64),
    ] {
        h.new_dataset::<f64>().create(key)?.write_scalar(&value)?;
    }
    text("backend", factor.backend())?;
    text("simd", simd::backend())?;
    text(
        "hostname",
        &std::fs::read_to_string("/etc/hostname").unwrap_or_default(),
    )?;
    text(
        "cpu",
        &std::fs::read_to_string("/proc/cpuinfo")
            .unwrap_or_default()
            .lines()
            .find(|s| s.starts_with("model name"))
            .unwrap_or("unknown"),
    )?;
    text("interpretation","Identical mean-centred inputs and finite range columns; unregularized complex64 least squares. Cached B uses GEMV for one period and GEMM for eight. Cached inverse uses FFT A^H z plus GEMV/GEMM. Factor uses FFT A^H z plus POTRS. FFT matched-only is a speed baseline, not an equivalent least-squares estimate. Setup reported separately. Single BLAS thread where supported; wall time includes live station contention. No receiver configuration or delay changed.")?;
    for batch in [1usize, 8] {
        let mut input = vec![Z::default(); n * batch];
        for (p, v) in input.chunks_exact_mut(n).enumerate() {
            for i in 0..n {
                v[i] = code[(i + n - first - nr / 3) % n] * Z::from_polar(1., p as f32 * 0.37)
                    + code[(i + n - first - 2 * nr / 3) % n]
                        * Z::from_polar(0.031, p as f32 * (-0.21))
                    + Z::new(
                        (i as f32 * 0.137).sin() * 0.0001,
                        (i as f32 * 0.217).cos() * 0.0001,
                    );
            }
            let mean = v.iter().copied().sum::<Z>() / n as f32;
            for z in v {
                *z -= mean;
            }
        }
        let mut reference = vec![Z::default(); nr * batch];
        dense.multiply(&input, &mut reference)?;
        let g = h.create_group(&format!("batch_{batch}"))?;
        let methods = [
            "cached_B",
            "fft_factor_solve",
            "fft_cached_inverse",
            "fft_matched_only",
        ];
        let mut outputs = vec![vec![Z::default(); nr * batch]; 4];
        let mut samples = vec![Vec::new(); 4];
        let mut b = vec![Z::default(); nr * batch];
        // Rotate method order to balance cache and thermal effects.
        for r in 0..repeats + 5 {
            for offset in 0..4 {
                let k = (r + offset) % 4;
                let output = &mut outputs[k];
                let start = Instant::now();
                let start_cpu = cpu();
                match k {
                    0 => dense.multiply(&input, output)?,
                    1 => {
                        rhs.compute(&input, output);
                        factor.solve_batch(output)?;
                    }
                    2 => {
                        rhs.compute(&input, &mut b);
                        inverse.multiply(&b, output)?;
                    }
                    _ => rhs.compute(&input, output),
                }
                let c = cpu() - start_cpu;
                let wall = start.elapsed().as_secs_f64();
                std::hint::black_box(output);
                if r >= 5 {
                    samples[k].extend_from_slice(&[wall, c]);
                }
            }
        }
        for (k, method) in methods.into_iter().enumerate() {
            let output = &outputs[k];
            let times = &samples[k];
            let max = output
                .iter()
                .zip(&reference)
                .map(|(x, y)| (*x - *y).norm())
                .fold(0f32, f32::max);
            let rel = (output
                .iter()
                .zip(&reference)
                .map(|(x, y)| (*x - *y).norm_sqr() as f64)
                .sum::<f64>()
                / reference.iter().map(|y| y.norm_sqr() as f64).sum::<f64>())
            .sqrt();
            ensure!(
                method == "fft_matched_only" || rel < 5e-4,
                "{method} failed complex x agreement: {rel}"
            );
            let ds = g.new_dataset::<f64>().shape([repeats, 2]).create(method)?;
            ds.write_raw(times)?;
            ds.new_attr::<f64>()
                .create("relative_l2_error")?
                .write_scalar(&rel)?;
            ds.new_attr::<f64>()
                .create("max_absolute_error")?
                .write_scalar(&(max as f64))?;
            let mut wall: Vec<_> = times.chunks_exact(2).map(|v| v[0]).collect();
            wall.sort_by(f64::total_cmp);
            let mut c: Vec<_> = times.chunks_exact(2).map(|v| v[1]).collect();
            c.sort_by(f64::total_cmp);
            println!("batch={batch} {method}: median wall {:.3} ms; CPU {:.3} ms; relative x error {rel:.3e}",wall[repeats/2]*1000.,c[repeats/2]*1000.);
        }
    }
    text(
        "peak_memory",
        &std::fs::read_to_string("/proc/self/status")
            .unwrap_or_default()
            .lines()
            .filter(|s| s.starts_with("VmHWM:") || s.starts_with("VmRSS:"))
            .collect::<Vec<_>>()
            .join("\n"),
    )?;
    println!("setup factor {factor_setup:.3}s, inverse +{inverse_setup:.3}s, B +{dense_setup:.3}s; cache B {} MiB, inverse/factor {} MiB",dense.bytes()/1048576,inverse.bytes()/1048576);
    Ok(())
}

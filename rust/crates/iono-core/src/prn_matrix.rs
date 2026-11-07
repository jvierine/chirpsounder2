//! Exact finite-range complex64 least squares: (A^H A) x = A^H z.
//! A[t,r] = code[(t-r) mod N], as in ionosonde/prc_lib.py.
//! Construct a Cholesky factor once, then cache its inverse for production
//! multiplication; retain factor and full-operator variants for comparison.
use anyhow::{ensure, Result};
use libloading::Library;
use num_complex::Complex32 as Z;
use rustfft::FftPlanner;
use std::sync::{Arc, OnceLock};
type Potrf = unsafe extern "C" fn(*const u8, *const i32, *mut Z, *const i32, *mut i32);
type Potrs = unsafe extern "C" fn(
    *const u8,
    *const i32,
    *const i32,
    *const Z,
    *const i32,
    *mut Z,
    *const i32,
    *mut i32,
);
type Gemm = unsafe extern "C" fn(
    *const u8,
    *const u8,
    *const i32,
    *const i32,
    *const i32,
    *const Z,
    *const Z,
    *const i32,
    *const Z,
    *const i32,
    *const Z,
    *mut Z,
    *const i32,
);
type Gemv = unsafe extern "C" fn(
    *const u8,
    *const i32,
    *const i32,
    *const Z,
    *const Z,
    *const i32,
    *const Z,
    *const i32,
    *const Z,
    *mut Z,
    *const i32,
);
struct Lapack {
    _lib: Library,
    factor: Potrf,
    solve: Potrs,
    gemm: Gemm,
    gemv: Gemv,
    name: String,
}
fn lapack() -> Result<Arc<Lapack>> {
    static API: OnceLock<Result<Arc<Lapack>, String>> = OnceLock::new();
    API.get_or_init(|| {
        let mut names: Vec<String> = std::env::var("IONO_PRN_LAPACK").ok().into_iter().collect();
        names.extend(
            [
                "libopenblas.so.0",
                "liblapack.so.3",
                "/System/Library/Frameworks/Accelerate.framework/Accelerate",
            ]
            .into_iter()
            .map(String::from),
        );
        for name in names {
            unsafe {
                if let Ok(lib) = Library::new(&name) {
                    let Ok(factor) = lib.get::<Potrf>(b"cpotrf_\0").map(|v| *v) else {
                        continue;
                    };
                    let Ok(solve) = lib.get::<Potrs>(b"cpotrs_\0").map(|v| *v) else {
                        continue;
                    };
                    let Ok(gemm) = lib.get::<Gemm>(b"cgemm_\0").map(|v| *v) else {
                        continue;
                    };
                    let Ok(gemv) = lib.get::<Gemv>(b"cgemv_\0").map(|v| *v) else {
                        continue;
                    };
                    // Bound BLAS parallelism inside the station's existing CPU slice.
                    if let Ok(set) =
                        lib.get::<unsafe extern "C" fn(i32)>(b"openblas_set_num_threads\0")
                    {
                        set(1);
                    }
                    return Ok(Arc::new(Lapack {
                        _lib: lib,
                        factor,
                        solve,
                        gemm,
                        gemv,
                        name: name.into(),
                    }));
                }
            }
        }
        Err("complex64 LAPACK unavailable (OpenBLAS or LAPACK required)".into())
    })
    .clone()
    .map_err(anyhow::Error::msg)
}
pub struct Factor {
    n: usize,
    lower: Vec<Z>,
    api: Arc<Lapack>,
}
impl Factor {
    pub fn backend(&self) -> &str {
        &self.api.name
    }
    pub fn bytes(&self) -> usize {
        self.lower.len() * 8
    }
    pub fn solve_batch(&self, rhs: &mut [Z]) -> Result<()> {
        ensure!(
            !rhs.is_empty() && rhs.len() % self.n == 0,
            "invalid RHS batch"
        );
        let n = self.n as i32;
        let columns = (rhs.len() / self.n) as i32;
        let mut info = 0;
        unsafe {
            (self.api.solve)(
                b"L".as_ptr(),
                &n,
                &columns,
                self.lower.as_ptr(),
                &n,
                rhs.as_mut_ptr(),
                &n,
                &mut info,
            );
        }
        ensure!(info == 0, "matrix batch solve failed: {info}");
        Ok(())
    }
    pub fn inverse(&self) -> Result<Matrix> {
        let mut values = vec![Z::default(); self.n * self.n];
        for i in 0..self.n {
            values[i + i * self.n] = Z::new(1., 0.);
        }
        self.solve_batch(&mut values)?;
        Ok(Matrix {
            rows: self.n,
            cols: self.n,
            values,
            api: self.api.clone(),
        })
    }
    pub fn estimator(&self, code: &[Z], first: usize) -> Result<Matrix> {
        ensure!(first + self.n <= code.len(), "invalid estimator window");
        let energy = code.iter().map(|z| z.norm_sqr()).sum::<f32>();
        let mut values = vec![Z::default(); self.n * code.len()];
        for t in 0..code.len() {
            for r in 0..self.n {
                values[r + t * self.n] =
                    code[(t + code.len() - first - r) % code.len()].conj() / energy;
            }
        }
        self.solve_batch(&mut values)?;
        Ok(Matrix {
            rows: self.n,
            cols: code.len(),
            values,
            api: self.api.clone(),
        })
    }
    pub fn new(code: &[Z], n: usize) -> Result<Self> {
        ensure!(
            n > 1 && n <= 2048 && n <= code.len(),
            "matrix range width outside 2..2048 bins"
        );
        let energy = code.iter().map(|z| z.norm_sqr()).sum::<f32>();
        ensure!(energy.is_finite() && energy > 0., "invalid code energy");
        let mut p = FftPlanner::<f32>::new();
        let mut corr = code.to_vec();
        p.plan_fft_forward(code.len()).process(&mut corr);
        for z in &mut corr {
            *z = Z::new(z.norm_sqr(), 0.);
        }
        p.plan_fft_inverse(code.len()).process(&mut corr);
        let scale = 1. / (code.len() as f32 * energy);
        let mut lower = vec![Z::default(); n * n];
        for col in 0..n {
            for row in col..n {
                lower[row + col * n] = corr[row - col] * scale;
            }
        }
        for i in 0..n {
            lower[i + i * n] = Z::new(1., 0.);
        }
        let api = lapack()?;
        let mut info = 0;
        let ni = n as i32;
        unsafe {
            (api.factor)(b"L".as_ptr(), &ni, lower.as_mut_ptr(), &ni, &mut info);
        }
        ensure!(info==0,"PRN normal matrix is not positive definite: LAPACK {info}; no silent matched-filter fallback");
        Ok(Self { n, lower, api })
    }
    /// rhs is A^H z / code energy; factor has the same normalization.
    pub fn solve(&self, rhs: &mut [Z]) -> Result<()> {
        ensure!(rhs.len() == self.n, "matrix RHS length mismatch");
        let n = self.n as i32;
        let one = 1;
        let mut info = 0;
        unsafe {
            (self.api.solve)(
                b"L".as_ptr(),
                &n,
                &one,
                self.lower.as_ptr(),
                &n,
                rhs.as_mut_ptr(),
                &n,
                &mut info,
            );
        }
        ensure!(
            info == 0 && rhs.iter().all(|z| z.re.is_finite() && z.im.is_finite()),
            "PRN matrix solve failed: {info}"
        );
        Ok(())
    }
}
pub struct Matrix {
    rows: usize,
    cols: usize,
    values: Vec<Z>,
    api: Arc<Lapack>,
}
impl Matrix {
    pub fn bytes(&self) -> usize {
        self.values.len() * 8
    }
    pub fn multiply(&self, input: &[Z], output: &mut [Z]) -> Result<()> {
        ensure!(
            !input.is_empty()
                && input.len() % self.cols == 0
                && output.len() == input.len() / self.cols * self.rows,
            "invalid matrix product dimensions"
        );
        let m = self.rows as i32;
        let k = self.cols as i32;
        let n = (input.len() / self.cols) as i32;
        let alpha = Z::new(1., 0.);
        let beta = Z::default();
        let one = 1;
        unsafe {
            if n == 1 {
                (self.api.gemv)(
                    b"N".as_ptr(),
                    &m,
                    &k,
                    &alpha,
                    self.values.as_ptr(),
                    &m,
                    input.as_ptr(),
                    &one,
                    &beta,
                    output.as_mut_ptr(),
                    &one,
                );
            } else {
                (self.api.gemm)(
                    b"N".as_ptr(),
                    b"N".as_ptr(),
                    &m,
                    &n,
                    &k,
                    &alpha,
                    self.values.as_ptr(),
                    &m,
                    input.as_ptr(),
                    &k,
                    &beta,
                    output.as_mut_ptr(),
                    &m,
                );
            }
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn matrix_recovers_two_echoes_from_normal_equations() {
        let code = crate::prn::code(10, 1000, 500, 500).unwrap();
        let n = 40;
        let f = Factor::new(&code, n).unwrap();
        let mut truth = vec![Z::default(); n];
        truth[5] = Z::new(1., 0.2);
        truth[27] = Z::new(0.01, -0.03);
        let mut rhs = vec![Z::default(); n];
        let e = code.iter().map(|z| z.norm_sqr()).sum::<f32>();
        for i in 0..code.len() {
            let y = (0..n)
                .map(|r| code[(i + code.len() - r) % code.len()] * truth[r])
                .sum::<Z>();
            for r in 0..n {
                rhs[r] += code[(i + code.len() - r) % code.len()].conj() * y / e;
            }
        }
        f.solve(&mut rhs).unwrap();
        assert!(rhs.iter().zip(truth).all(|(a, b)| (*a - b).norm() < 2e-5));
    }
}

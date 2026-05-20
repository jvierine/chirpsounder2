#include "chirp_downconvert.h"
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>

#ifndef M_PI
#define M_PI 3.14159265358979323846
#endif

static volatile float sink_re = 0.0f;
static volatile float sink_im = 0.0f;

void consume(double chirpt, double dt, complex_float *sintab, int tabl,
             complex_float *in, complex_float *out_buffer, int n_out, int dec,
             int dec2, double f0, double rate, float *wfun, int n_threads);

static double now_seconds(void)
{
  struct timespec ts;
  clock_gettime(CLOCK_MONOTONIC, &ts);
  return (double)ts.tv_sec + 1e-9*(double)ts.tv_nsec;
}

static inline int phase_table_index(double chirpt, int tabl, double f0, double rate)
{
  int64_t idx = (int64_t)(tabl*(f0 + 0.5*rate*chirpt)*chirpt);

  if((tabl & (tabl - 1)) == 0)
    return (int)(idx & (int64_t)(tabl - 1));

  idx %= tabl;
  if(idx < 0)
    idx += tabl;

  return (int)idx;
}

static void fill_test_data(complex_float *in, float *wfun, complex_float *sintab,
                           int dec2, int tabl)
{
  for(int i=0; i<dec2; i++)
  {
    in[i].re = sinf(0.013f*(float)i) + 0.1f*cosf(0.071f*(float)i);
    in[i].im = cosf(0.017f*(float)i) - 0.1f*sinf(0.053f*(float)i);
    wfun[i] = 0.5f - 0.5f*cosf(2.0f*(float)M_PI*(float)i/(float)(dec2 - 1));
  }

  for(int i=0; i<tabl; i++)
  {
    float ph = -2.0f*(float)M_PI*(float)i/(float)tabl;
    sintab[i].re = cosf(ph);
    sintab[i].im = sinf(ph);
  }
}

static double bench_phase_index(int n_iter, int dec2, double dt,
                                int tabl, double f0, double rate)
{
  int acc = 0;
  double t0 = now_seconds();

  for(int iter=0; iter<n_iter; iter++)
  {
    double chirpt = (double)iter*(double)dec2*dt;
    for(int i=0; i<dec2; i++)
      acc += phase_table_index(chirpt + (double)i*dt, tabl, f0, rate);
  }

  double t1 = now_seconds();
  sink_re += (float)(acc & 0xff);
  return t1 - t0;
}

static double bench_fir_mac(int n_iter, complex_float *in, float *wfun, int dec2)
{
  complex_float acc;
  double t0 = now_seconds();

  for(int iter=0; iter<n_iter; iter++)
  {
    acc.re = 0.0f;
    acc.im = 0.0f;
    for(int i=0; i<dec2; i++)
    {
      acc.re += in[i].re*wfun[i];
      acc.im += in[i].im*wfun[i];
    }
    sink_re += acc.re;
    sink_im += acc.im;
  }

  double t1 = now_seconds();
  return t1 - t0;
}

static double bench_lookup_mix(int n_iter, complex_float *in, complex_float *sintab,
                               int dec2, double dt, int tabl, double f0, double rate)
{
  complex_float acc;
  double t0 = now_seconds();

  for(int iter=0; iter<n_iter; iter++)
  {
    double chirpt = (double)iter*(double)dec2*dt;
    acc.re = 0.0f;
    acc.im = 0.0f;
    for(int i=0; i<dec2; i++)
    {
      complex_float p = sintab[phase_table_index(chirpt + (double)i*dt, tabl, f0, rate)];
      acc.re += in[i].re*p.re - in[i].im*p.im;
      acc.im += in[i].im*p.re + in[i].re*p.im;
    }
    sink_re += acc.re;
    sink_im += acc.im;
  }

  double t1 = now_seconds();
  return t1 - t0;
}

static double bench_full_fir(int n_iter, complex_float *in, float *wfun,
                             complex_float *sintab, int dec2, double dt,
                             int tabl, double f0, double rate)
{
  complex_float acc;
  double t0 = now_seconds();

  for(int iter=0; iter<n_iter; iter++)
  {
    double chirpt = (double)iter*(double)dec2*dt;
    acc.re = 0.0f;
    acc.im = 0.0f;
    for(int i=0; i<dec2; i++)
    {
      complex_float p = sintab[phase_table_index(chirpt + (double)i*dt, tabl, f0, rate)];
      float zr = in[i].re*wfun[i];
      float zi = in[i].im*wfun[i];
      acc.re += zr*p.re - zi*p.im;
      acc.im += zi*p.re + zr*p.im;
    }
    sink_re += acc.re;
    sink_im += acc.im;
  }

  double t1 = now_seconds();
  return t1 - t0;
}

static double bench_recursive_phase_update(int n_iter, int dec2, double dt,
                                           double f0, double rate)
{
  double t0 = now_seconds();
  double domega = 2.0*M_PI*rate*dt*dt;
  double ar = cos(domega);
  double ai = sin(domega);

  for(int iter=0; iter<n_iter; iter++)
  {
    double chirpt = (double)iter*(double)dec2*dt;
    double pr = cos(2.0*M_PI*(f0 + 0.5*rate*chirpt)*chirpt);
    double pi = sin(2.0*M_PI*(f0 + 0.5*rate*chirpt)*chirpt);
    double omega = 2.0*M_PI*(f0 + rate*chirpt)*dt;
    double sr = cos(omega);
    double si = sin(omega);

    for(int i=0; i<dec2; i++)
    {
      double nr = pr*sr - pi*si;
      double ni = pi*sr + pr*si;
      pr = nr;
      pi = ni;

      double nsr = sr*ar - si*ai;
      double nsi = si*ar + sr*ai;
      sr = nsr;
      si = nsi;
    }
    sink_re += (float)pr;
    sink_im += (float)pi;
  }

  double t1 = now_seconds();
  return t1 - t0;
}

static double bench_consume_block_size(int total_output_samples,
                                       int block_output_samples,
                                       int dec,
                                       int dec2,
                                       double dt,
                                       int tabl,
                                       double f0,
                                       double rate,
                                       complex_float *in,
                                       complex_float *out,
                                       complex_float *sintab,
                                       float *wfun,
                                       int n_threads)
{
  double chirpt = 0.0;
  int produced = 0;
  double t0 = now_seconds();

  while(produced < total_output_samples)
  {
    int n_out = block_output_samples;
    if(produced + n_out > total_output_samples)
      n_out = total_output_samples - produced;

    consume(chirpt, dt, sintab, tabl, in, out, n_out, dec, dec2, f0, rate,
            wfun, n_threads);
    chirpt += (double)n_out*(double)dec*dt;
    produced += n_out;
  }

  double t1 = now_seconds();
  sink_re += out[0].re;
  sink_im += out[0].im;
  return t1 - t0;
}

static void run_block_size_benchmark(int dec,
                                     int dec2,
                                     double dt,
                                     int tabl,
                                     double f0,
                                     double rate,
                                     complex_float *sintab,
                                     float *wfun)
{
  const int total_output_samples = 120000;
  const int n_threads = 1;
  const int block_sizes[] = {125, 250, 500, 1000, 2000, 4000, 8000, 16000};
  const int n_block_sizes = (int)(sizeof(block_sizes)/sizeof(block_sizes[0]));
  int max_block = block_sizes[n_block_sizes - 1];
  int max_input = max_block*dec + dec2;
  complex_float *in = calloc((size_t)max_input, sizeof(complex_float));
  complex_float *out = calloc((size_t)max_block, sizeof(complex_float));

  if(in == NULL || out == NULL)
  {
    fprintf(stderr, "allocation failed\n");
    exit(1);
  }

  for(int i=0; i<max_input; i++)
  {
    in[i].re = sinf(0.013f*(float)i) + 0.1f*cosf(0.071f*(float)i);
    in[i].im = cosf(0.017f*(float)i) - 0.1f*sinf(0.053f*(float)i);
  }

  printf("\nBlock-size benchmark using consume(): total_output_samples=%d threads=%d\n",
         total_output_samples, n_threads);
  printf("block_output_samples,calls,seconds,us_per_output_sample,ns_per_input_sample,speedup_vs_1000\n");

  double baseline = 0.0;
  double times[n_block_sizes];
  for(int i=0; i<n_block_sizes; i++)
  {
    int block = block_sizes[i];
    double t = bench_consume_block_size(total_output_samples, block, dec, dec2,
                                        dt, tabl, f0, rate, in, out, sintab,
                                        wfun, n_threads);
    times[i] = t;
    if(block == 1000)
      baseline = t;
  }

  for(int i=0; i<n_block_sizes; i++)
  {
    int block = block_sizes[i];
    int calls = (total_output_samples + block - 1)/block;
    double input_samples = (double)total_output_samples*(double)dec;
    printf("%d,%d,%.6f,%.3f,%.3f,%.3f\n",
           block,
           calls,
           times[i],
           1e6*times[i]/(double)total_output_samples,
           1e9*times[i]/input_samples,
           baseline/times[i]);
  }

  free(in);
  free(out);
}

int main(int argc, char **argv)
{
  int dec = 625;
  int filter_len = 2;
  int dec2 = dec*filter_len;
  int tabl = 8192;
  int n_iter = 20000;
  double dt = 1.0/25e6;
  double f0 = -12.5e6;
  double rate = 500.0084e3;

  if(argc > 1)
    n_iter = atoi(argv[1]);

  complex_float *in = calloc((size_t)dec2, sizeof(complex_float));
  complex_float *sintab = calloc((size_t)tabl, sizeof(complex_float));
  float *wfun = calloc((size_t)dec2, sizeof(float));
  if(in == NULL || sintab == NULL || wfun == NULL)
  {
    fprintf(stderr, "allocation failed\n");
    return 1;
  }

  fill_test_data(in, wfun, sintab, dec2, tabl);

  double samples = (double)n_iter*(double)dec2;
  double t_phase = bench_phase_index(n_iter, dec2, dt, tabl, f0, rate);
  double t_fir = bench_fir_mac(n_iter, in, wfun, dec2);
  double t_lookup = bench_lookup_mix(n_iter, in, sintab, dec2, dt, tabl, f0, rate);
  double t_full = bench_full_fir(n_iter, in, wfun, sintab, dec2, dt, tabl, f0, rate);
  double t_recursive = bench_recursive_phase_update(n_iter, dec2, dt, f0, rate);

  printf("KHO-like FIR microbenchmark: dec=%d filter_len=%d taps/output=%d iterations=%d\n",
         dec, filter_len, dec2, n_iter);
  printf("stage,seconds,ns_per_input_sample,relative_to_full\n");
  printf("phase_index_double,%.6f,%.3f,%.3f\n", t_phase, 1e9*t_phase/samples, t_phase/t_full);
  printf("fir_weight_mac_only,%.6f,%.3f,%.3f\n", t_fir, 1e9*t_fir/samples, t_fir/t_full);
  printf("lookup_and_complex_mix,%.6f,%.3f,%.3f\n", t_lookup, 1e9*t_lookup/samples, t_lookup/t_full);
  printf("full_current_fir_inner_loop,%.6f,%.3f,1.000\n", t_full, 1e9*t_full/samples);
  printf("recursive_phase_update_double,%.6f,%.3f,%.3f\n", t_recursive, 1e9*t_recursive/samples, t_recursive/t_full);
  printf("sink,%f,%f\n", sink_re, sink_im);

  run_block_size_benchmark(dec, dec2, dt, tabl, f0, rate, sintab, wfun);

  free(in);
  free(sintab);
  free(wfun);
  return 0;
}

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
void consume_recursive(double chirpt, double dt, complex_float *sintab, int tabl,
                       complex_float *in, complex_float *out_buffer, int n_out,
                       int dec, int dec2, double f0, double rate, float *wfun,
                       int n_threads);

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

static complex_float make_chirp_sample(double t, double f0, double rate)
{
  double phase = 2.0*M_PI*(f0 + 0.5*rate*t)*t;
  complex_float z;
  z.re = (float)cos(phase);
  z.im = (float)sin(phase);
  return z;
}

static void fill_chirp_block(complex_float *in, int n_in, double start_t,
                             double dt, double f0, double rate)
{
  for(int i=0; i<n_in; i++)
    in[i] = make_chirp_sample(start_t + (double)i*dt, f0, rate);
}

static double complex_abs_float(complex_float z)
{
  return sqrt((double)z.re*(double)z.re + (double)z.im*(double)z.im);
}

static double complex_arg_product(complex_float a, complex_float b)
{
  double re = (double)a.re*(double)b.re + (double)a.im*(double)b.im;
  double im = (double)a.im*(double)b.re - (double)a.re*(double)b.im;
  return atan2(im, re);
}

static double wrap_phase(double phase)
{
  while(phase > M_PI)
    phase -= 2.0*M_PI;
  while(phase < -M_PI)
    phase += 2.0*M_PI;
  return phase;
}

static void consume_recursive_float_block(double chirpt,
                                          double dt,
                                          complex_float *in,
                                          complex_float *out_buffer,
                                          int n_out,
                                          int dec,
                                          int dec2,
                                          double f0,
                                          double rate,
                                          float *wfun)
{
  double phase_accel_phase = -2.0*M_PI*rate*dt*dt;
  float phase_accel_re = (float)cos(phase_accel_phase);
  float phase_accel_im = (float)sin(phase_accel_phase);

  for(int out_idx=0; out_idx<n_out; out_idx++)
  {
    double t = chirpt + (double)(dec*out_idx)*dt;
    double phase = -2.0*M_PI*(f0 + 0.5*rate*t)*t;
    double phase_step = -2.0*M_PI*((f0 + rate*t)*dt + 0.5*rate*dt*dt);
    float pr = (float)cos(phase);
    float pi = (float)sin(phase);
    float sr = (float)cos(phase_step);
    float si = (float)sin(phase_step);
    float acc_re = 0.0f;
    float acc_im = 0.0f;
    int in0 = out_idx*dec;

    for(int dec_idx=0; dec_idx<dec2; dec_idx++)
    {
      float zr = in[in0 + dec_idx].re*wfun[dec_idx];
      float zi = in[in0 + dec_idx].im*wfun[dec_idx];

      acc_re += zr*pr - zi*pi;
      acc_im += zi*pr + zr*pi;

      float npr = pr*sr - pi*si;
      float npi = pi*sr + pr*si;
      pr = npr;
      pi = npi;

      float nsr = sr*phase_accel_re - si*phase_accel_im;
      float nsi = si*phase_accel_re + sr*phase_accel_im;
      sr = nsr;
      si = nsi;
    }

    out_buffer[out_idx].re = acc_re;
    out_buffer[out_idx].im = acc_im;
  }
}

static void run_recursive_accuracy_benchmark(int dec,
                                             int dec2,
                                             double dt,
                                             int tabl,
                                             double f0,
                                             double rate,
                                             complex_float *sintab,
                                             float *wfun)
{
  const int block = 4000;
  const int n_blocks = 320;
  const int n_threads = 1;
  const int n_in = block*dec + dec2;
  complex_float *in = calloc((size_t)n_in, sizeof(complex_float));
  complex_float *out_lookup = calloc((size_t)block, sizeof(complex_float));
  complex_float *out_recursive = calloc((size_t)block, sizeof(complex_float));
  complex_float *out_recursive_float = calloc((size_t)block, sizeof(complex_float));

  if(in == NULL || out_lookup == NULL || out_recursive == NULL || out_recursive_float == NULL)
  {
    fprintf(stderr, "allocation failed\n");
    exit(1);
  }

  double max_abs_err = 0.0;
  double rms_abs_err = 0.0;
  double max_phase_err = 0.0;
  double rms_phase_err = 0.0;
  double sum_phase_step_err = 0.0;
  double max_phase_step_err = 0.0;
  double float_max_abs_err = 0.0;
  double float_rms_abs_err = 0.0;
  double float_max_phase_err = 0.0;
  double float_rms_phase_err = 0.0;
  double float_sum_phase_step_err = 0.0;
  double float_max_phase_step_err = 0.0;
  int n_samples = 0;
  int n_step = 0;
  double prev_phase_err = 0.0;
  double float_prev_phase_err = 0.0;
  int have_prev = 0;
  int float_have_prev = 0;

  double chirpt = 0.0;
  double t0_lookup = now_seconds();
  for(int block_idx=0; block_idx<n_blocks; block_idx++)
  {
    double block_time = (double)(block_idx*block*dec)*dt;
    fill_chirp_block(in, n_in, block_time, dt, -f0, rate);
    consume(chirpt, dt, sintab, tabl, in, out_lookup, block, dec, dec2, f0,
            rate, wfun, n_threads);
    chirpt += (double)block*(double)dec*dt;
  }
  double lookup_seconds = now_seconds() - t0_lookup;

  chirpt = 0.0;
  double t0_recursive = now_seconds();
  for(int block_idx=0; block_idx<n_blocks; block_idx++)
  {
    double block_time = (double)(block_idx*block*dec)*dt;
    fill_chirp_block(in, n_in, block_time, dt, -f0, rate);
    consume_recursive(chirpt, dt, sintab, tabl, in, out_recursive, block, dec,
                      dec2, f0, rate, wfun, n_threads);
    chirpt += (double)block*(double)dec*dt;

    for(int i=0; i<block; i++)
    {
      complex_float err;
      err.re = out_recursive[i].re - out_lookup[i].re;
      err.im = out_recursive[i].im - out_lookup[i].im;
      double abs_err = complex_abs_float(err);
      double phase_err_signed = complex_arg_product(out_recursive[i], out_lookup[i]);
      double phase_err = fabs(phase_err_signed);
      if(abs_err > max_abs_err)
        max_abs_err = abs_err;
      if(phase_err > max_phase_err)
        max_phase_err = phase_err;
      rms_abs_err += abs_err*abs_err;
      rms_phase_err += phase_err*phase_err;
      n_samples++;

      if(have_prev)
      {
        double phase_step_err = fabs(wrap_phase(phase_err_signed - prev_phase_err));
        sum_phase_step_err += phase_step_err;
        if(phase_step_err > max_phase_step_err)
          max_phase_step_err = phase_step_err;
        n_step++;
      }
      prev_phase_err = phase_err_signed;
      have_prev = 1;
    }
  }
  double recursive_seconds = now_seconds() - t0_recursive;

  chirpt = 0.0;
  double t0_recursive_float = now_seconds();
  for(int block_idx=0; block_idx<n_blocks; block_idx++)
  {
    double block_time = (double)(block_idx*block*dec)*dt;
    fill_chirp_block(in, n_in, block_time, dt, -f0, rate);
    consume_recursive_float_block(chirpt, dt, in, out_recursive_float, block,
                                  dec, dec2, f0, rate, wfun);
    chirpt += (double)block*(double)dec*dt;

    for(int i=0; i<block; i++)
    {
      complex_float err;
      err.re = out_recursive_float[i].re - out_recursive[i].re;
      err.im = out_recursive_float[i].im - out_recursive[i].im;
      double abs_err = complex_abs_float(err);
      double phase_err_signed = complex_arg_product(out_recursive_float[i], out_recursive[i]);
      double phase_err = fabs(phase_err_signed);
      if(abs_err > float_max_abs_err)
        float_max_abs_err = abs_err;
      if(phase_err > float_max_phase_err)
        float_max_phase_err = phase_err;
      float_rms_abs_err += abs_err*abs_err;
      float_rms_phase_err += phase_err*phase_err;

      if(float_have_prev)
      {
        double phase_step_err = fabs(wrap_phase(phase_err_signed - float_prev_phase_err));
        float_sum_phase_step_err += phase_step_err;
        if(phase_step_err > float_max_phase_step_err)
          float_max_phase_step_err = phase_step_err;
      }
      float_prev_phase_err = phase_err_signed;
      float_have_prev = 1;
    }
  }
  double recursive_float_seconds = now_seconds() - t0_recursive_float;

  rms_abs_err = sqrt(rms_abs_err/(double)n_samples);
  rms_phase_err = sqrt(rms_phase_err/(double)n_samples);
  double mean_phase_step_err = n_step > 0 ? sum_phase_step_err/(double)n_step : 0.0;
  double sr_dec = 1.0/(dt*(double)dec);
  double mean_freq_offset_hz = mean_phase_step_err*sr_dec/(2.0*M_PI);
  double max_freq_offset_hz = max_phase_step_err*sr_dec/(2.0*M_PI);
  float_rms_abs_err = sqrt(float_rms_abs_err/(double)n_samples);
  float_rms_phase_err = sqrt(float_rms_phase_err/(double)n_samples);
  double float_mean_phase_step_err = n_step > 0 ? float_sum_phase_step_err/(double)n_step : 0.0;
  double float_mean_freq_offset_hz = float_mean_phase_step_err*sr_dec/(2.0*M_PI);
  double float_max_freq_offset_hz = float_max_phase_step_err*sr_dec/(2.0*M_PI);

  printf("\nRecursive FIR accuracy vs lookup FIR over %.3f s / %.3f MHz sweep\n",
         (double)(n_blocks*block*dec)*dt,
         rate*(double)(n_blocks*block*dec)*dt/1e6);
  printf("lookup_seconds,recursive_double_seconds,recursive_float_seconds,double_speedup,float_speedup\n");
  printf("%.6f,%.6f,%.6f,%.3f,%.3f\n", lookup_seconds, recursive_seconds,
         recursive_float_seconds, lookup_seconds/recursive_seconds,
         recursive_seconds/recursive_float_seconds);
  printf("rms_abs_err,max_abs_err,rms_phase_err_rad,max_phase_err_rad,mean_freq_offset_hz,max_freq_offset_hz\n");
  printf("%.9g,%.9g,%.9g,%.9g,%.9g,%.9g\n",
         rms_abs_err,
         max_abs_err,
         rms_phase_err,
         max_phase_err,
         mean_freq_offset_hz,
         max_freq_offset_hz);
  printf("\nFloat recursive FIR accuracy vs double recursive FIR\n");
  printf("rms_abs_err,max_abs_err,rms_phase_err_rad,max_phase_err_rad,mean_freq_offset_hz,max_freq_offset_hz\n");
  printf("%.9g,%.9g,%.9g,%.9g,%.9g,%.9g\n",
         float_rms_abs_err,
         float_max_abs_err,
         float_rms_phase_err,
         float_max_phase_err,
         float_mean_freq_offset_hz,
         float_max_freq_offset_hz);

  free(in);
  free(out_lookup);
  free(out_recursive);
  free(out_recursive_float);
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
  run_recursive_accuracy_benchmark(dec, dec2, dt, tabl, f0, rate, sintab, wfun);

  free(in);
  free(sintab);
  free(wfun);
  return 0;
}

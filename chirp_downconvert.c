#include "chirp_downconvert.h"
#include <stdint.h>
#include <stdio.h>
#include <pthread.h>
#include <stdlib.h>
#include <math.h>
#ifdef __AVX2__
#include <immintrin.h>
#endif

#ifndef M_PI
#define M_PI 3.14159265358979323846
#endif

typedef struct complex_double_str {
  double re;
  double im;
} complex_double;

static inline int phase_table_index(double chirpt, int tabl, double f0, double rate)
{
  int64_t idx = (int64_t)(tabl*(f0+0.5*rate*chirpt)*chirpt) % tabl;

  if(idx < 0)
    idx = tabl+idx;

  return (int)idx;
}

void complex_mul(complex_float *a, complex_float *res)
{
  float tmp;
  //res = (a.re + 1i*a.im)*(res.re + 1i*res.im)
  //res.re = (a.re*res.re) - a.im*res*im
  //res.im = (a.im*res.re) + a.re*res.im
  tmp = res->re;
  res->re = a->re*tmp - a->im*res->im;
  res->im = a->im*tmp + a->re*res->im;
}

void complex_add(complex_float *a, complex_float *res)
{
  res->re = res->re + a->re;
  res->im = res->im + a->im;
}

void add_and_advance_phasor(double chirpt, complex_float *sintab, int tabl, complex_float *a, complex_float *mean, double f0, double rate)
{
  complex_float tmp;

  // this is faster
  int idx = phase_table_index(chirpt, tabl, f0, rate);

  tmp = sintab[idx];
  complex_mul(a, &tmp);
  complex_add(&tmp, mean);
}

static inline void add_windowed_phasor_sum(double chirpt,
                                           double dt,
                                           complex_float *sintab,
                                           int tabl,
                                           complex_float *in,
                                           float *wfun,
                                           int dec2,
                                           complex_float *out_sample,
                                           double f0,
                                           double rate)
{
  int dec_idx = 0;

#ifdef __AVX2__
  __m256 acc = _mm256_setzero_ps();

  for(; dec_idx <= dec2 - 4; dec_idx += 4)
  {
    double t0 = chirpt + (double)(dec_idx + 0)*dt;
    double t1 = chirpt + (double)(dec_idx + 1)*dt;
    double t2 = chirpt + (double)(dec_idx + 2)*dt;
    double t3 = chirpt + (double)(dec_idx + 3)*dt;

    complex_float p0 = sintab[phase_table_index(t0, tabl, f0, rate)];
    complex_float p1 = sintab[phase_table_index(t1, tabl, f0, rate)];
    complex_float p2 = sintab[phase_table_index(t2, tabl, f0, rate)];
    complex_float p3 = sintab[phase_table_index(t3, tabl, f0, rate)];

    __m256 z = _mm256_loadu_ps((float *)&in[dec_idx]);
    __m256 w = _mm256_set_ps(wfun[dec_idx + 3], wfun[dec_idx + 3],
                             wfun[dec_idx + 2], wfun[dec_idx + 2],
                             wfun[dec_idx + 1], wfun[dec_idx + 1],
                             wfun[dec_idx + 0], wfun[dec_idx + 0]);
    z = _mm256_mul_ps(z, w);

    __m256 pr = _mm256_set_ps(p3.re, p3.re, p2.re, p2.re,
                              p1.re, p1.re, p0.re, p0.re);
    __m256 pi = _mm256_set_ps(p3.im, p3.im, p2.im, p2.im,
                              p1.im, p1.im, p0.im, p0.im);
    __m256 zswap = _mm256_permute_ps(z, 0xb1);
    __m256 prod_reim = _mm256_mul_ps(z, pr);
    __m256 prod_cross = _mm256_mul_ps(zswap, pi);

    acc = _mm256_add_ps(acc, _mm256_addsub_ps(prod_reim, prod_cross));
  }

  float accv[8];
  _mm256_storeu_ps(accv, acc);
  out_sample->re += accv[0] + accv[2] + accv[4] + accv[6];
  out_sample->im += accv[1] + accv[3] + accv[5] + accv[7];
#endif

  for(; dec_idx < dec2; dec_idx++)
  {
    complex_float tmp = in[dec_idx];
    tmp.re = tmp.re*wfun[dec_idx];
    tmp.im = tmp.im*wfun[dec_idx];
    add_and_advance_phasor(chirpt + (double)dec_idx*dt,
                           sintab,
                           tabl,
                           &tmp,
                           out_sample,
                           f0,
                           rate);
  }
}

void test(complex_float *sintab, int n)
{
  for(int i=0; i<n ; i++)
  {
    printf("%d %f %f\n",i,sintab[i].re,sintab[i].im);
  }
}
struct arg_struct {
  double chirpt;
  double dt;
  complex_float *sintab;
  int tabl;
  complex_float *in;
  complex_float *out_buffer;
  int n_out;
  int dec;
  int dec2;
  double f0;
  double rate;
  float *wfun;
  int rank;
  int size;
};

void *consume_one(void *args)
{
  struct arg_struct *a = args;
  double chirpt=a->chirpt;
  double dt=a->dt;
  complex_float *sintab=a->sintab;
  complex_float *in=a->in;
  complex_float *out_buffer=a->out_buffer;
  int n_out = a->n_out;
  int tabl = a->tabl;
  int dec=a->dec;
  int dec2=a->dec2;
  double f0=a->f0;
  double rate=a->rate;
  float *wfun=a->wfun;
  int rank=a->rank;
  int size=a->size;
    
  /*
    parallel consume
   */
  // complex_float *in = (complex_float *) input_items[0];
  complex_float out_sample;
  int i;
  double chirpt0;
  chirpt0=chirpt;

  for(int out_idx=rank; out_idx<n_out; out_idx+=size)
  {
    out_sample.re=0.0;out_sample.im=0.0;
    chirpt=((double)dec*out_idx)*dt + chirpt0;
    i=out_idx*dec;
    /* 
       better lpf with a user defined window function (e.g., windowed ideal LPF)
    */
    add_windowed_phasor_sum(chirpt, dt, sintab, tabl, &in[i], wfun, dec2,
                            &out_sample, f0, rate);
    out_buffer[out_idx]=out_sample;
  }
  pthread_exit(NULL);
  return(NULL);
}

void consume(double chirpt, double dt, complex_float *sintab, int tabl, complex_float *in, complex_float *out_buffer, int n_out, int dec, int dec2, double f0, double rate, float *wfun, int n_threads)
{
  pthread_t *proc_threads;
  struct arg_struct *a;
  a=(struct arg_struct *)malloc(sizeof(struct arg_struct)*n_threads);
  proc_threads=(pthread_t *)malloc(sizeof(pthread_t)*n_threads);
  for(int i=0; i<n_threads; i++)
  {
    
    a[i].chirpt=chirpt;
    a[i].dt=dt;
    a[i].sintab=sintab;
    a[i].tabl=tabl;
    a[i].in=in;
    a[i].out_buffer=out_buffer;
    a[i].n_out=n_out;
    a[i].dec=dec;
    a[i].dec2=dec2;
    a[i].f0=f0;
    a[i].rate=rate;
    a[i].wfun=wfun;
    a[i].rank=i;
    a[i].size=n_threads;
    pthread_create(&proc_threads[i], NULL, consume_one, (void *)&a[i]);
  }


  for(int i=0; i<n_threads; i++)
  {
    pthread_join(proc_threads[i],NULL);
  }
  free(a);
  free(proc_threads);
}

void consume_cic(double chirpt, double dt, complex_float *sintab, int tabl, complex_float *in, complex_float *out_buffer, int n_out, int dec, double f0, double rate, complex_float *integrator_state, complex_float *comb_state, int n_stages)
{
  complex_float mixed;
  complex_float comb;
  complex_float prev;
  double t = chirpt;
  double gain = 1.0;
  int out_idx = 0;

  for(int i=0; i<n_stages; i++)
  {
    gain *= (double)dec;
  }

  for(int sample_idx=0; sample_idx<(n_out*dec); sample_idx++)
  {
    mixed = in[sample_idx];
    add_and_advance_phasor(t, sintab, tabl, &mixed, &integrator_state[0], f0, rate);

    for(int stage=1; stage<n_stages; stage++)
    {
      integrator_state[stage].re += integrator_state[stage-1].re;
      integrator_state[stage].im += integrator_state[stage-1].im;
    }

    if(((sample_idx + 1) % dec) == 0)
    {
      comb = integrator_state[n_stages-1];
      for(int stage=0; stage<n_stages; stage++)
      {
        prev = comb_state[stage];
        comb_state[stage] = comb;
        comb.re -= prev.re;
        comb.im -= prev.im;
      }
      out_buffer[out_idx].re = comb.re / gain;
      out_buffer[out_idx].im = comb.im / gain;
      out_idx++;
    }

    t += dt;
  }
}

static inline complex_float complex_mul_value(complex_float a, complex_float b)
{
  complex_float res;
  res.re = a.re*b.re - a.im*b.im;
  res.im = a.im*b.re + a.re*b.im;
  return res;
}

static inline void complex_accumulate(complex_float *acc, complex_float x)
{
  acc->re += x.re;
  acc->im += x.im;
}

static inline complex_float downconvert_sample(complex_float x, complex_double phase)
{
  complex_float res;
  res.re = (float)((double)x.re*phase.re - (double)x.im*phase.im);
  res.im = (float)((double)x.im*phase.re + (double)x.re*phase.im);
  return res;
}

static inline void advance_phase(complex_double *phase, complex_double phase_step)
{
  double re = phase->re*phase_step.re - phase->im*phase_step.im;
  double im = phase->im*phase_step.re + phase->re*phase_step.im;
  phase->re = re;
  phase->im = im;
}

static void digisonde_downconvert_boxcar(complex_float *in,
                                         complex_float *out,
                                         int n_out,
                                         int dec,
                                         complex_double phase,
                                         complex_double phase_step)
{
  for(int out_idx=0; out_idx<n_out; out_idx++)
  {
    complex_float sum;
    sum.re = 0.0;
    sum.im = 0.0;

    for(int dec_idx=0; dec_idx<dec; dec_idx++)
    {
      int in_idx = out_idx*dec + dec_idx;
      complex_accumulate(&sum, downconvert_sample(in[in_idx], phase));
      advance_phase(&phase, phase_step);
    }

    out[out_idx].re = sum.re / (float)dec;
    out[out_idx].im = sum.im / (float)dec;
  }
}

static void digisonde_downconvert_average(complex_float *in,
                                          complex_float *out,
                                          int n_out,
                                          int dec,
                                          complex_double *phase,
                                          complex_double phase_step)
{
  for(int out_idx=0; out_idx<n_out; out_idx++)
  {
    complex_float sum;
    sum.re = 0.0;
    sum.im = 0.0;

    for(int dec_idx=0; dec_idx<dec; dec_idx++)
    {
      int in_idx = out_idx*dec + dec_idx;
      complex_accumulate(&sum, downconvert_sample(in[in_idx], *phase));
      advance_phase(phase, phase_step);
    }

    out[out_idx].re = sum.re / (float)dec;
    out[out_idx].im = sum.im / (float)dec;
  }
}

static void digisonde_downconvert_average10(complex_float *in,
                                            complex_float *out,
                                            int n_out,
                                            complex_double *phase,
                                            complex_double phase_step)
{
  for(int out_idx=0; out_idx<n_out; out_idx++)
  {
    int in_idx = out_idx*10;
    complex_float sum;
    sum.re = 0.0;
    sum.im = 0.0;
    int dec_idx = 0;

#ifdef __AVX2__
    __m256 acc = _mm256_setzero_ps();
    for(; dec_idx <= 6; dec_idx += 4)
    {
      complex_double p0 = *phase;
      advance_phase(phase, phase_step);
      complex_double p1 = *phase;
      advance_phase(phase, phase_step);
      complex_double p2 = *phase;
      advance_phase(phase, phase_step);
      complex_double p3 = *phase;
      advance_phase(phase, phase_step);

      __m256 z = _mm256_loadu_ps((float *)&in[in_idx + dec_idx]);
      __m256 pr = _mm256_set_ps((float)p3.re, (float)p3.re,
                                (float)p2.re, (float)p2.re,
                                (float)p1.re, (float)p1.re,
                                (float)p0.re, (float)p0.re);
      __m256 pi = _mm256_set_ps((float)p3.im, (float)p3.im,
                                (float)p2.im, (float)p2.im,
                                (float)p1.im, (float)p1.im,
                                (float)p0.im, (float)p0.im);
      __m256 zswap = _mm256_permute_ps(z, 0xb1);
      __m256 prod_reim = _mm256_mul_ps(z, pr);
      __m256 prod_cross = _mm256_mul_ps(zswap, pi);

      acc = _mm256_add_ps(acc, _mm256_addsub_ps(prod_reim, prod_cross));
    }

    float accv[8];
    _mm256_storeu_ps(accv, acc);
    sum.re += accv[0] + accv[2] + accv[4] + accv[6];
    sum.im += accv[1] + accv[3] + accv[5] + accv[7];
#endif

    for(; dec_idx<10; dec_idx++)
    {
      complex_accumulate(&sum, downconvert_sample(in[in_idx + dec_idx], *phase));
      advance_phase(phase, phase_step);
    }

    out[out_idx].re = sum.re * 0.1f;
    out[out_idx].im = sum.im * 0.1f;
  }
}

static void digisonde_cic_decimate(complex_float *in,
                                   complex_float *out,
                                   int n_in,
                                   int dec,
                                   int n_stages)
{
  complex_float *integrator_state = calloc(n_stages, sizeof(complex_float));
  complex_float *comb_state = calloc(n_stages, sizeof(complex_float));
  int out_idx = 0;
  double gain = 1.0;

  for(int stage=0; stage<n_stages; stage++)
  {
    gain *= (double)dec;
  }

  for(int sample_idx=0; sample_idx<n_in; sample_idx++)
  {
    integrator_state[0].re += in[sample_idx].re;
    integrator_state[0].im += in[sample_idx].im;

    for(int stage=1; stage<n_stages; stage++)
    {
      integrator_state[stage].re += integrator_state[stage-1].re;
      integrator_state[stage].im += integrator_state[stage-1].im;
    }

    if(((sample_idx + 1) % dec) == 0)
    {
      complex_float comb = integrator_state[n_stages-1];
      for(int stage=0; stage<n_stages; stage++)
      {
        complex_float prev = comb_state[stage];
        comb_state[stage] = comb;
        comb.re -= prev.re;
        comb.im -= prev.im;
      }
      out[out_idx].re = comb.re / (float)gain;
      out[out_idx].im = comb.im / (float)gain;
      out_idx++;
    }
  }

  free(integrator_state);
  free(comb_state);
}

static void digisonde_downconvert_fir10_25(complex_float *in,
                                           complex_float *out,
                                           int n_out,
                                           int n_in,
                                           float *fir_taps,
                                           int n_taps,
                                           complex_double phase,
                                           complex_double phase_step)
{
  int stage1_dec = 10;
  int stage2_dec = 25;
  int n_stage1 = n_in / stage1_dec;
  int tap_center = (n_taps - 1) / 2;
  complex_float *stage1 = malloc(sizeof(complex_float)*n_stage1);

  digisonde_downconvert_average10(in,
                                  stage1,
                                  n_stage1,
                                  &phase,
                                  phase_step);

  for(int out_idx=0; out_idx<n_out; out_idx++)
  {
    int center_idx = out_idx*stage2_dec;
    complex_float sum;
    sum.re = 0.0;
    sum.im = 0.0;
    int tap_idx = 0;

#ifdef __AVX2__
    __m256 acc = _mm256_setzero_ps();
    for(; tap_idx <= n_taps - 4; tap_idx += 4)
    {
      int input_idx = center_idx + tap_idx - tap_center;
      if(input_idx >= 0 && input_idx + 3 < n_stage1)
      {
        __m256 z = _mm256_loadu_ps((float *)&stage1[input_idx]);
        __m256 w = _mm256_set_ps(fir_taps[tap_idx + 3], fir_taps[tap_idx + 3],
                                 fir_taps[tap_idx + 2], fir_taps[tap_idx + 2],
                                 fir_taps[tap_idx + 1], fir_taps[tap_idx + 1],
                                 fir_taps[tap_idx + 0], fir_taps[tap_idx + 0]);
        acc = _mm256_add_ps(acc, _mm256_mul_ps(z, w));
      }
      else
      {
        for(int j=0; j<4; j++)
        {
          int edge_idx = input_idx + j;
          if(edge_idx >= 0 && edge_idx < n_stage1)
          {
            sum.re += stage1[edge_idx].re * fir_taps[tap_idx + j];
            sum.im += stage1[edge_idx].im * fir_taps[tap_idx + j];
          }
        }
      }
    }

    float accv[8];
    _mm256_storeu_ps(accv, acc);
    sum.re += accv[0] + accv[2] + accv[4] + accv[6];
    sum.im += accv[1] + accv[3] + accv[5] + accv[7];
#endif

    for(; tap_idx<n_taps; tap_idx++)
    {
      int input_idx = center_idx + tap_idx - tap_center;
      if(input_idx >= 0 && input_idx < n_stage1)
      {
        sum.re += stage1[input_idx].re * fir_taps[tap_idx];
        sum.im += stage1[input_idx].im * fir_taps[tap_idx];
      }
    }

    out[out_idx] = sum;
  }

  free(stage1);
}

static void digisonde_downconvert_cic_staged(complex_float *in,
                                             complex_float *out,
                                             int n_out,
                                             int n_in,
                                             int dec,
                                             int n_stages,
                                             complex_double phase,
                                             complex_double phase_step)
{
  if(dec == 250)
  {
    int stage1_dec = 10;
    int stage2_dec = 25;
    int n_stage1 = n_in / stage1_dec;
    complex_float *stage1 = malloc(sizeof(complex_float)*n_stage1);

    digisonde_downconvert_average10(in,
                                    stage1,
                                    n_stage1,
                                    &phase,
                                    phase_step);
    digisonde_cic_decimate(stage1,
                           out,
                           n_stage1,
                           stage2_dec,
                           n_stages);
    free(stage1);
  }
  else
  {
    complex_float *mixed = malloc(sizeof(complex_float)*n_in);
    for(int i=0; i<n_in; i++)
    {
      mixed[i] = downconvert_sample(in[i], phase);
      advance_phase(&phase, phase_step);
    }
    digisonde_cic_decimate(mixed, out, n_in, dec, n_stages);
    free(mixed);
  }
}

void digisonde_downconvert_decimate(complex_float *in,
                                    complex_float *out,
                                    int n_in,
                                    int dec,
                                    double frequency_offset,
                                    double sample_rate,
                                    float initial_phase_re,
                                    float initial_phase_im,
                                    int filter_strategy,
                                    float *fir_taps,
                                    int n_taps,
                                    int cic_stages)
{
  int n_out = n_in / dec;
  double dphase = -2.0*M_PI*frequency_offset/sample_rate;
  complex_double phase;
  complex_double phase_step;

  phase.re = (double)initial_phase_re;
  phase.im = (double)initial_phase_im;
  phase_step.re = cos(dphase);
  phase_step.im = sin(dphase);

  if(filter_strategy == 0)
  {
    digisonde_downconvert_boxcar(in, out, n_out, dec, phase, phase_step);
  }
  else if(filter_strategy == 1)
  {
    digisonde_downconvert_cic_staged(in, out, n_out, n_in, dec, cic_stages,
                                     phase, phase_step);
  }
  else
  {
    digisonde_downconvert_fir10_25(in, out, n_out, n_in, fir_taps, n_taps,
                                   phase, phase_step);
  }
}

#include "chirp_downconvert.h"
#include <stdint.h>
#include <stdio.h>
#include <pthread.h>
#include <stdlib.h>
#ifdef __AVX2__
#include <immintrin.h>
#endif

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

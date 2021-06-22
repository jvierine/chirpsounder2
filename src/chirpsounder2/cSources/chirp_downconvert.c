#include "chirp_downconvert.h"
#include <stdint.h>
#include <stdio.h>
#include <pthread.h>
#include <stdlib.h>
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
  int64_t idx = (int64_t)(tabl*(f0+0.5*rate*chirpt)*chirpt) % tabl;
  
  if(idx < 0)
    idx = tabl+idx;
      
  tmp = sintab[idx];
  complex_mul(a, &tmp);
  complex_add(&tmp, mean);
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
  complex_float tmp;
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
    for(int dec_idx=0; dec_idx<dec2; dec_idx++)
    {
      /* dechirp and low-pass filter by averaging */
      /* window function to improve filter peformance */
      tmp=in[i];
      tmp.re=tmp.re*wfun[dec_idx];
      tmp.im=tmp.im*wfun[dec_idx];      
      add_and_advance_phasor(chirpt, sintab, tabl, &tmp, &out_sample, f0, rate);
      chirpt+=dt;
      i++;
    }
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
}

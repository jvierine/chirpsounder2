#include "chirp_downconvert.h"
#include <stdint.h>
#include <stdio.h>
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
  int64_t tabll=tabl;
  int64_t idx = (int64_t)(tabl*(f0+0.5*rate*chirpt)*chirpt) % tabl;
  
  //  if(idx < 0)
  //  idx = tabl+idx;
      
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

void consume(double chirpt, double dt, complex_float *sintab, int tabl, complex_float *in, complex_float *out_buffer, int n_out, int dec, int dec2, double f0, double rate, float *wfun)
{
  // complex_float *in = (complex_float *) input_items[0];
  complex_float out_sample;
  complex_float tmp;
  int i;
  double chirpt0;
  chirpt0=chirpt;

  i=0;
  for(int out_idx=0; out_idx<n_out; out_idx++)
  {
    out_sample.re=0.0;out_sample.im=0.0;
    chirpt=((double)dec*out_idx)*dt + chirpt0;
    i=out_idx*dec;
    /* 
       better lpf 
       we use boxcar filter for efficiency, but use a longer one for better low
       pass filtering.
       we could add a window function here if we wanted to!
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
}

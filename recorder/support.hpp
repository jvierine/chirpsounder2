#pragma once
#include <algorithm>
#include <cmath>
#include <complex>
#include <cstdint>
#include <limits>
#include <stdexcept>
#include <string>
#include <vector>
namespace recorder {
struct Mode {
 uint64_t rate; double center; bool eight_bit;
 static Mode parse(const std::string& name) {
  if(name=="25mhz")return {25000000,12500000.,false};
  if(name=="50mhz")return {50000000,25000000.,true};
  throw std::invalid_argument("mode must be 25mhz or 50mhz");
 }
};
struct Counters {
 uint64_t received_samples=0, rail_samples=0, rail_components=0, near_rail_samples=0;
 uint64_t uhd_overflow_events=0, missing_samples=0, backward_packets=0, conversion_clips=0, startup_missing_samples=0;
 int32_t maximum_abs_component=0; double sum_power=0.;
 template<class T>void add(const std::complex<T>* p,size_t n) {
  constexpr int32_t lo=std::numeric_limits<T>::lowest(),hi=std::numeric_limits<T>::max();
  uint64_t rails=0,components=0,near=0;int32_t maximum=0;double power=0.;
  for(size_t i=0;i<n;i++){
   const int32_t re=p[i].real(),im=p[i].imag();
   const bool a=re==lo||re==hi,b=im==lo||im==hi;
   rails+=a||b;components+=int(a)+int(b);
   maximum=std::max(maximum,std::max(std::abs(re),std::abs(im)));
   near+=std::abs(re)>=hi-1||std::abs(im)>=hi-1;
   power+=double(re)*re+double(im)*im;
  }
  received_samples+=n;rail_samples+=rails;rail_components+=components;near_rail_samples+=near;
  maximum_abs_component=std::max(maximum_abs_component,maximum);sum_power+=power;
 }
};
// Causal 33-tap half-band filter. Drop its initial transient; timestamp outputs
// at the centre of the filter, compensating its 16-input-sample group delay.
// The quarter-rate mixer moves 0..25 MHz into a 12.5 MHz-centred output view.
class Low25 {
 std::complex<int32_t> ring[33]{};double taps[33]{};uint64_t next=0,seen=0;
public:
 uint64_t conversion_clips=0;
 Low25(){double sum=0.;for(int k=0;k<33;k++){
  const int m=k-16;const double x=m==0?.5:std::sin(3.141592653589793*.5*m)/(3.141592653589793*m);
  taps[k]=x*(.54-.46*std::cos(2*3.141592653589793*k/32));sum+=taps[k];
 }for(auto& t:taps)t/=sum;}
 void reset(uint64_t index){next=index;seen=0;}
 void process(uint64_t index,const std::complex<int8_t>* in,size_t n,
              std::vector<std::complex<int16_t>>& out,uint64_t& first) {
  if(index!=next)reset(index);out.clear();conversion_clips=0;
  for(size_t j=0;j<n;j++){
   uint64_t t=index+j;int32_t a=in[j].real(),b=in[j].imag();std::complex<int32_t> z;
   switch(t&3){case 0:z={a,b};break;case 1:z={-b,a};break;case 2:z={-a,-b};break;default:z={b,-a};}
   ring[t%33]=z;++seen;
   if(seen<33||(t&1))continue;
   double re=0,im=0;
   for(int k=0;k<33;k++){const auto v=ring[(t-uint64_t(k))%33];re+=taps[k]*v.real();im+=taps[k]*v.imag();}
   if(out.empty())first=(t-16)/2;
   // Wide intermediates and i16 output avoid -(-128) and sum overflow.
   auto narrow=[&](double value){
    const int64_t wide=std::llround(value*128.);
    if(wide < -32768 || wide > 32767)++conversion_clips;
    return int16_t(std::clamp<int64_t>(wide,-32768,32767));
   };
   out.emplace_back(narrow(re),narrow(im));
  }next=index+n;
 }
};
}

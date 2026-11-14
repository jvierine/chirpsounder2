#include <unistd.h>

#include "../recorder/health.hpp"
#include <cassert>
#include <filesystem>
#include <iostream>
using namespace recorder;
static double tone(double rf){
 Low25 d;std::vector<std::complex<int8_t>> v(20000);const uint64_t start=50000000;
 for(size_t n=0;n<v.size();n++){double p=2*3.141592653589793*(rf-25e6)*double(start+n)/50e6;v[n]={int8_t(std::lround(50*std::cos(p))),int8_t(std::lround(50*std::sin(p)))};}
 std::vector<std::complex<int16_t>> out;uint64_t first=0;d.process(start,v.data(),v.size(),out,first);
 assert(first==(start+16)/2);assert(out.size()==9984);
 double power=0.;for(auto z:out)power+=std::norm(std::complex<double>(z.real(),z.imag()));
 if(rf==5e6){
  for(size_t n=0;n<out.size();n+=113){double p=2*3.141592653589793*(rf-12.5e6)*double(first+n)/25e6;
   const std::complex<double> target=50.*128.*std::complex<double>(std::cos(p),std::sin(p));
   assert(std::abs(std::complex<double>(out[n].real(),out[n].imag())-target)<100.);
  }
 }
 return power/out.size();
}
int main(){
 assert(Mode::parse("50mhz").rate==50000000);assert(Mode::parse("25mhz").center==12.5e6);
 bool bad=false;try{Mode::parse("51mhz");}catch(...){bad=true;}assert(bad);
 Counters c;std::complex<int8_t> a[]={{-128,0},{127,-128},{1,2}};c.add(a,3);
 assert(c.received_samples==3&&c.rail_samples==2&&c.rail_components==3&&c.maximum_abs_component==128);
 const double low=tone(5e6),alias=tone(30e6);std::cerr<<"alias ratio "<<alias/low<<" dB "<<10*std::log10(alias/low)<<std::endl;assert(alias/low<1./2500.);
 Low25 d;std::complex<int8_t> v[100]{};std::vector<std::complex<int16_t>> out;uint64_t first=0;
 d.process(50000000,v,100,out,first);d.process(50001000,v,100,out,first);assert(first==25000508);
 const std::string root="/tmp/recorder-test-"+std::to_string(getpid());
 {Health h(root,"test",0,50,8);std::complex<int8_t> x[100]{};
  h.observe(50*3599+49,x,100);h.flush();}
 unsigned files=0;for(auto& e:std::filesystem::directory_iterator(root)){
  assert(e.path().extension()==".h5");++files;hid_t h=H5Fopen(e.path().c_str(),H5F_ACC_RDONLY,H5P_DEFAULT);
  hid_t d=H5Dopen2(h,"received_samples",H5P_DEFAULT);uint64_t n=0;H5Dread(d,H5T_NATIVE_UINT64,H5S_ALL,H5S_ALL,H5P_DEFAULT,&n);
  assert(n==1||n==99);H5Dclose(d);H5Fclose(h);
 }assert(files==2);std::filesystem::remove_all(root);
 std::cout<<"recorder tests passed; upper-band alias suppression dB "<<10*std::log10(alias/low)<<"\n";
}

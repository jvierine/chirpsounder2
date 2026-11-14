#pragma once
#include "support.hpp"
#include <hdf5.h>
#include <ctime>
#include <filesystem>
#include <iostream>
#include <iomanip>
#include <sstream>
namespace recorder {
class Health {
 std::string dir,uuid;size_t channel;uint64_t rate,hour=UINT64_MAX,last_report=0,first=0,last=0;int bits;Counters c;
 template<class T>void scalar(hid_t h,const char* name,hid_t type,T value){
  hid_t s=H5Screate(H5S_SCALAR),d=H5Dcreate2(h,name,type,s,H5P_DEFAULT,H5P_DEFAULT,H5P_DEFAULT);
  if(d<0||H5Dwrite(d,type,H5S_ALL,H5S_ALL,H5P_DEFAULT,&value)<0)throw std::runtime_error("recorder health write failed");
  H5Dclose(d);H5Sclose(s);
 }
public:
 Health(std::string path,std::string id,size_t ch,uint64_t sr,int width):dir(path),uuid(id),channel(ch),rate(sr),bits(width){std::filesystem::create_directories(dir);}
 Counters& counters(){return c;}
 void flush(bool complete=false){if(hour==UINT64_MAX)return;
  std::time_t epoch=hour*3600;std::tm tm{};gmtime_r(&epoch,&tm);std::ostringstream name;
  name<<dir<<"/recorder-"<<std::put_time(&tm,"%Y%m%dT%H")<<"-ch"<<channel<<"-"<<uuid<<".h5";
  std::string tmp=name.str()+".partial";hid_t h=H5Fcreate(tmp.c_str(),H5F_ACC_TRUNC,H5P_DEFAULT,H5P_DEFAULT);
  if(h<0)throw std::runtime_error("cannot create recorder hourly health file");
  scalar(h,"schema_version",H5T_NATIVE_INT,1);scalar(h,"hour_start_unix",H5T_NATIVE_UINT64,uint64_t(epoch));
  scalar(h,"complete",H5T_NATIVE_INT,int(complete));scalar(h,"sample_rate_hz",H5T_NATIVE_UINT64,rate);
  scalar(h,"component_bits",H5T_NATIVE_INT,bits);scalar(h,"first_sample",H5T_NATIVE_UINT64,first);scalar(h,"last_sample_exclusive",H5T_NATIVE_UINT64,last);
#define COUNT(field) scalar(h,#field,H5T_NATIVE_UINT64,c.field)
  COUNT(received_samples);COUNT(rail_samples);COUNT(rail_components);COUNT(near_rail_samples);COUNT(uhd_overflow_events);COUNT(missing_samples);COUNT(backward_packets);COUNT(conversion_clips);
#undef COUNT
  scalar(h,"maximum_abs_component",H5T_NATIVE_INT32,c.maximum_abs_component);
  scalar(h,"mean_complex_power",H5T_NATIVE_DOUBLE,c.received_samples?c.sum_power/c.received_samples:0.);
  scalar(h,"rail_sample_fraction",H5T_NATIVE_DOUBLE,c.received_samples?double(c.rail_samples)/c.received_samples:0.);
  scalar(h,"updated_unix",H5T_NATIVE_UINT64,uint64_t(std::time(nullptr)));
  H5Fclose(h);std::filesystem::rename(tmp,name.str());last_report=std::time(nullptr);
 }
 template<class T>void observe(uint64_t index,const std::complex<T>* data,size_t n){
  while(n){const uint64_t bucket=index/rate/3600;
   if(bucket!=hour){flush(true);hour=bucket;c=Counters{};first=index;}
   const size_t take=std::min<uint64_t>(n,(bucket+1)*3600*rate-index);
   c.add(data,take);last=index+take;index+=take;data+=take;n-=take;
  }
  if(uint64_t(std::time(nullptr))-last_report>=60)flush();
 }
 ~Health(){try{flush(false);}catch(const std::exception& e){std::cerr<<e.what()<<std::endl;}}
};
}

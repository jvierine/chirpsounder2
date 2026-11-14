// Modified to thread multiple channels and run Ettus Octoclock.
// Lawrence Coleman, 2023
//
// This is based on one of the UHD driver examples.
// Juha Vierinen, 2021
//
// Copyright 2010-2011 Ettus Research LLC
// Copyright 2018 Ettus Research, a National Instruments Company
//
// SPDX-License-Identifier: GPL-3.0-or-later
//

#include <uhd/usrp/multi_usrp.hpp>
#include <uhd/usrp_clock/multi_usrp_clock.hpp>
#include <uhd/utils/safe_main.hpp>
#include <uhd/utils/thread.hpp>
#include <boost/algorithm/string.hpp>
#include <boost/format.hpp>
#include <boost/program_options.hpp>
#include <atomic>
#include <array>
#include <complex>
#include <cstdlib>
#include <iomanip>
#include <thread>
#include <iostream>
#include <fstream>
#include <mutex>
#include <random>
#include <sstream>
#include <vector>
#include <algorithm>
#include <unistd.h>
#include <digital_rf.h>
#include "recorder/health.hpp"
#include <csignal>
#include <cmath>
#include <type_traits>

#define NO_WRITE_DRF 1

namespace po = boost::program_options;

using namespace uhd::usrp_clock;
using namespace uhd::usrp;
using namespace std;

std::atomic<bool> shutdown_requested(false);
std::atomic<bool> fatal_error(false);
std::mutex shutdown_log_mutex;

void request_shutdown(const std::string& reason)
{
    fatal_error.store(true);
    bool expected = false;
    if (shutdown_requested.compare_exchange_strong(expected, true)) {
        std::lock_guard<std::mutex> lock(shutdown_log_mutex);
        std::cerr << reason << std::endl;
    }
}

// Digital RF keeps returning errors after a fatal I/O failure, for example
// when the output disk fills. Ask all channel threads to stop so the USRP
// streamers and Digital RF writers can be cleaned up before the process exits.
bool write_digital_rf_or_request_shutdown(Digital_rf_write_object* data_object,
                                          uint64_t sample_index,
                                          void* data,
                                          uint64_t vector_length,
                                          size_t chan)
{
    int result = digital_rf_write_hdf5(
        data_object, sample_index, data, vector_length);
    if (result != 0) {
        request_shutdown(str(boost::format(
            "Fatal Digital RF write error on channel %1% at sample index %2%; "
            "digital_rf_write_hdf5 returned %3%. Stopping recorder.")
            % chan % sample_index % result));
        return false;
    }
    return true;
}

std::string random_uuid_v4()
{
    std::array<unsigned char, 16> bytes;
    std::random_device rd;
    for (auto& byte : bytes) {
        byte = static_cast<unsigned char>(rd());
    }

    bytes[6] = static_cast<unsigned char>((bytes[6] & 0x0f) | 0x40);
    bytes[8] = static_cast<unsigned char>((bytes[8] & 0x3f) | 0x80);

    std::ostringstream uuid;
    uuid << std::hex << std::setfill('0');
    for (size_t i = 0; i < bytes.size(); i++) {
        if (i == 4 || i == 6 || i == 8 || i == 10) {
            uuid << '-';
        }
        uuid << std::setw(2) << static_cast<unsigned int>(bytes[i]);
    }
    return uuid.str();
}

void get_usrp_time(multi_usrp::sptr usrp, size_t mboard, std::vector<int64_t>* times)
{
    (*times)[mboard] = usrp->get_time_now(mboard).get_full_secs();
}

bool has_value(const std::vector<std::string>& values, const std::string& value)
{
    return std::find(values.begin(), values.end(), value) != values.end();
}

bool has_sensor(multi_usrp::sptr usrp, const std::string& sensor, size_t mboard)
{
    std::vector<std::string> sensors = usrp->get_mboard_sensor_names(mboard);
    return has_value(sensors, sensor);
}

bool all_mboards_support_gpsdo(multi_usrp::sptr usrp)
{
    for (size_t mb = 0; mb < usrp->get_num_mboards(); mb++) {
        if (!has_value(usrp->get_clock_sources(mb), "gpsdo") ||
            !has_value(usrp->get_time_sources(mb), "gpsdo")) {
            return false;
        }
    }
    return true;
}

bool wait_for_gps_lock(multi_usrp::sptr usrp, int timeout_sec)
{
    auto start_time = std::chrono::steady_clock::now();
    bool all_gps_locked = false;
    while (!all_gps_locked) {
        all_gps_locked = true;
        for (size_t mb = 0; mb < usrp->get_num_mboards(); mb++) {
            if (!has_sensor(usrp, "gps_locked", mb)) {
                continue;
            }
            bool gps_locked = usrp->get_mboard_sensor("gps_locked", mb).to_bool();
            std::cout << boost::format(" * mboard %d gps_locked: %s")
                         % mb % (gps_locked ? "true" : "false")
                      << std::endl;
            if (!gps_locked) {
                all_gps_locked = false;
            }
        }

        if (!all_gps_locked) {
            if (timeout_sec >= 0) {
                auto elapsed = std::chrono::duration_cast<std::chrono::seconds>(
                    std::chrono::steady_clock::now() - start_time).count();
                if (elapsed >= timeout_sec) {
                    std::cout << "GPSDO did not lock within "
                              << timeout_sec
                              << " seconds; continuing without GPS lock."
                              << std::endl;
                    return false;
                }
            }
            std::cout << "Waiting for GPSDO lock." << std::endl;
            std::this_thread::sleep_for(std::chrono::seconds(10));
        }
    }
    return true;
}


struct RecorderOptions {
    recorder::Mode mode;
    std::string stats_dir,compat_dir;
    double peak=1.;
};

template<class T>
void record_channel(size_t chan,const std::string& outdir,multi_usrp::sptr usrp,
                    uhd::time_spec_t pps,std::chrono::steady_clock::time_point end_time,
                    const RecorderOptions& options)
{
    const auto mode=options.mode;
    const std::string uuid=random_uuid_v4();
    recorder::Health health(options.stats_dir,uuid,chan,mode.rate,sizeof(T)*8,options.peak);
    uhd::stream_args_t args(mode.eight_bit?"sc8":"sc16",mode.eight_bit?"sc8":"sc16");
    args.channels={chan};
    if(mode.eight_bit)args.args["peak"]=std::to_string(options.peak);
    auto stream=usrp->get_rx_stream(args);
    const double start=pps.get_real_secs()+2.;
    const uint64_t global_start=uint64_t(uhd::time_spec_t(start).to_ticks(double(mode.rate)));
    auto writer=[&](const std::string& root,hid_t type,uint64_t rate,uint64_t first){
        const auto path=root+"/ch"+std::to_string(chan);
        std::filesystem::create_directories(path);
        auto* w=digital_rf_create_write_hdf5(const_cast<char*>(path.c_str()),type,
            3600,1000,first,rate,1,const_cast<char*>(uuid.c_str()),0,0,1,1,0,1);
        if(!w)throw std::runtime_error("Could not create Digital RF writer: "+path);
        return w;
    };
    auto* raw=writer(outdir,mode.eight_bit?H5T_NATIVE_INT8:H5T_NATIVE_INT16,mode.rate,global_start);
    Digital_rf_write_object* low=nullptr;
    if(!options.compat_dir.empty())low=writer(options.compat_dir,H5T_NATIVE_INT16,25000000,global_start/2);
    recorder::Low25 decimator;
    std::vector<std::complex<int16_t>> reduced;
    uhd::stream_cmd_t command(uhd::stream_cmd_t::STREAM_MODE_START_CONTINUOUS);
    command.stream_now=false;command.time_spec=uhd::time_spec_t(start);stream->issue_stream_cmd(command);
    std::vector<std::complex<T>> data(65536);
    uhd::rx_metadata_t md;
    uint64_t expected=global_start,pending_overflows=0;
    unsigned empty=0;
    std::cout<<"CAPTURE mode="<<(mode.eight_bit?"50mhz":"25mhz")
             <<" rate="<<mode.rate<<" center_hz="<<mode.center<<" bits="<<sizeof(T)*8
             <<" start_unix="<<std::setprecision(16)<<start<<" uuid="<<uuid<<std::endl;
    try {
        while(!shutdown_requested.load()&&std::chrono::steady_clock::now()<end_time){
            const size_t n=stream->recv(data.data(),data.size(),md,empty?0.2:3.1,false);
            if(md.error_code==uhd::rx_metadata_t::ERROR_CODE_OVERFLOW)++pending_overflows;
            else if(md.error_code!=uhd::rx_metadata_t::ERROR_CODE_NONE&&md.error_code!=uhd::rx_metadata_t::ERROR_CODE_TIMEOUT)
                throw std::runtime_error("UHD receive error: "+md.strerror());
            if(!n){if(++empty>100)throw std::runtime_error("No samples for 100 receive calls");continue;}
            empty=0;
            if(!md.has_time_spec)throw std::runtime_error("Received samples without a timestamp");
            const uint64_t index=uint64_t(md.time_spec.to_ticks(double(mode.rate)));
            if(index<expected){++health.counters().backward_packets;continue;}
            health.observe(index,data.data(),n);
            auto& c=health.counters();c.uhd_overflow_events+=pending_overflows;pending_overflows=0;
            if(index>expected){
                if(expected==global_start && c.received_samples==n)c.startup_missing_samples+=index-expected;
                else c.missing_samples+=index-expected;
                decimator.reset(index);
            }
            // Actual packet timestamps/lengths define storage. Never fill a gap
            // by writing another copy of the most recently received samples.
            if(!write_digital_rf_or_request_shutdown(raw,index-global_start,data.data(),n,chan))break;
            if constexpr(std::is_same_v<T,int8_t>){
                if(low){uint64_t first=0;decimator.process(index,data.data(),n,reduced,first);c.conversion_clips+=decimator.conversion_clips;
                    if(!reduced.empty()&&!write_digital_rf_or_request_shutdown(low,first-global_start/2,reduced.data(),reduced.size(),chan))break;}
            }
            expected=index+n;
        }
        health.counters().uhd_overflow_events+=pending_overflows;
        health.flush();
    }catch(const std::exception& ex){request_shutdown(ex.what());}
    uhd::stream_cmd_t stop(uhd::stream_cmd_t::STREAM_MODE_STOP_CONTINUOUS);
    try{stream->issue_stream_cmd(stop);}catch(const std::exception& ex){request_shutdown(ex.what());}
    digital_rf_close_write_hdf5(raw);if(low)digital_rf_close_write_hdf5(low);
}

void streaming_by_channel(size_t chan,double rate,std::string subdev,std::string outdir,
                         multi_usrp::sptr usrp,uhd::time_spec_t pps,
                         std::chrono::steady_clock::time_point end_time,RecorderOptions options)
{
    try{
        if(options.mode.eight_bit)record_channel<int8_t>(chan,outdir,usrp,pps,end_time,options);
        else record_channel<int16_t>(chan,outdir,usrp,pps,end_time,options);
    }catch(const std::exception& ex){request_shutdown(ex.what());}
}

int UHD_SAFE_MAIN(int argc, char* argv[])
{
    std::string usrp_args;
    std::string outdir;
    std::string subdev;
    double rate;
    std::string channel_list;
    int gps_lock_timeout_sec;
    std::string mode_name,stats_dir,compat_dir;
    double seconds,peak;

    po::options_description desc("Allowed options");
    // clang-format off
    desc.add_options()
        ("help", "help message")
        ("usrp_args", po::value<std::string>(&usrp_args)->default_value("addr0=192.168.10.2,recv_buff_size=500000000"),"ettus device args")
        ("outdir", po::value<std::string>(&outdir)->default_value("/dev/shm/hf25"), "output directory")
        ("subdev", po::value<std::string>(&subdev)->default_value("A:A"), "subdevice")
        ("mode", po::value<std::string>(&mode_name)->default_value("25mhz"), "25mhz: 0..25 MHz sc16; 50mhz: 0..50 MHz sc8")
        ("rate", po::value<double>(&rate)->default_value(25e6), "compatibility alias: 25e6 or 50e6 only")
        ("stats-dir", po::value<std::string>(&stats_dir)->default_value("/data0/recorder-health"), "persistent hourly HDF5 rail/overflow counters")
        ("compat25-outdir", po::value<std::string>(&compat_dir)->default_value(""), "optional 0..25 MHz i16 compatibility ring for 50mhz mode")
        ("peak", po::value<double>(&peak)->default_value(1.), "sc8 full-scale fraction, (0,1]; 1 preserves maximum ADC headroom")
        ("seconds", po::value<double>(&seconds)->default_value(86400.), "bounded run duration in seconds")
        ("channels", po::value<std::string>(&channel_list)->default_value("0"), "which channel(s) to use (specify \"0\", \"1\", \"0,1\", etc)")
        ("gps-lock-timeout", po::value<int>(&gps_lock_timeout_sec)->default_value(-1), "seconds to wait for internal GPSDO lock before continuing; use -1 to wait indefinitely")
    ;
    
    // clang-format on
    po::variables_map vm;
    po::store(po::parse_command_line(argc, argv, desc), vm);
    po::notify(vm);
    if(vm.count("help")){std::cout<<desc<<std::endl;return EXIT_SUCCESS;}
    if(!vm["rate"].defaulted()){
        std::string implied=rate==25e6?"25mhz":rate==50e6?"50mhz":"invalid";
        if(!vm["mode"].defaulted()&&mode_name!=implied)throw std::invalid_argument("rate conflicts with mode");
        mode_name=implied;
    }
    const auto mode=recorder::Mode::parse(mode_name);rate=double(mode.rate);
    if(!std::isfinite(seconds)||seconds<=0||!std::isfinite(peak)||peak<=0||peak>1)
        throw std::invalid_argument("Invalid duration or sc8 peak");
    if(!compat_dir.empty()&&(!mode.eight_bit||std::filesystem::weakly_canonical(compat_dir)==std::filesystem::weakly_canonical(outdir)))
        throw std::invalid_argument("Compatibility output needs 50mhz mode and a separate directory");
    const RecorderOptions options{mode,stats_dir,compat_dir,peak};
    std::signal(SIGINT,[](int){shutdown_requested.store(true);});
    std::signal(SIGTERM,[](int){shutdown_requested.store(true);});

    std::cout << boost::format("\nCreating the USRP device with: %s") % usrp_args
              << std::endl;
    multi_usrp::sptr usrp = multi_usrp::make(usrp_args);


    
    std::vector<std::string> serials;
    for (size_t ch = 0; ch < usrp->get_num_mboards(); ch++) {
        serials.push_back(usrp->get_usrp_tx_info(ch)["mboard_serial"]);
    }
    bool using_internal_gpsdo = all_mboards_support_gpsdo(usrp);
    if (using_internal_gpsdo) {
        std::cout << "Using internal GPSDO clock and PPS." << std::endl;
        for (size_t mb = 0; mb < usrp->get_num_mboards(); mb++) {
            usrp->set_clock_source("gpsdo", mb);
            usrp->set_time_source("gpsdo", mb);
        }
        bool gps_locked = wait_for_gps_lock(usrp, gps_lock_timeout_sec);
        if (!gps_locked) {
            std::cout << "WARNING: GPSDO is not locked; using GPSDO reference/PPS if present, but setting USRP time from PC time." << std::endl;
        }
    } else {
        std::cout << "Internal GPSDO not available on all mboards; using external 10 MHz and PPS." << std::endl;
        for (size_t mb = 0; mb < usrp->get_num_mboards(); mb++) {
            usrp->set_clock_source("external", mb);
            usrp->set_time_source("external", mb);
        }
    }

    std::cout << std::endl << "Checking USRP devices for lock." << std::endl;
    bool all_locked = true;
    for (size_t ch = 0; ch < usrp->get_num_mboards(); ch++) {
        std::string ref_locked = usrp->get_mboard_sensor("ref_locked", ch).value;
        std::cout << boost::format(" * %s: %s") % serials[ch] % ref_locked << std::endl;

        if (ref_locked != "true")
            all_locked = false;
    }
    if (not all_locked)
        std::cout << std::endl << "WARNING: One or more devices not locked." << std::endl;

    std::vector<std::string> channel_strings;
    std::vector<size_t> channel_nums;
    boost::split(channel_strings, channel_list, boost::is_any_of("\"',"));
    for (size_t ch = 0; ch < channel_strings.size(); ch++) {
        size_t chan = std::stoi(channel_strings[ch]);
	std::cout << chan << std::endl;
        if (chan >= usrp->get_tx_num_channels() or chan >= usrp->get_rx_num_channels()) {
            throw std::runtime_error("Invalid channel(s) specified.");
        } else
            channel_nums.push_back(std::stoi(channel_strings[ch]));
    }
    std::cout << "done channels" << std::endl;

    // Quit after 24 hours to ensure the next service invocation starts from a
    // fresh synchronized PPS edge.
    auto start_time = std::chrono::steady_clock::now();
    auto run_duration = std::chrono::duration_cast<std::chrono::steady_clock::duration>(std::chrono::duration<double>(seconds));
    auto end_time = start_time + run_duration;

    uhd::time_spec_t last_pps = usrp->get_time_last_pps();
    std::cout << "last_pps: " << last_pps.get_real_secs() << "\n";
    std::cout << "waiting for next pps\n";
    while (usrp->get_time_last_pps() == last_pps) {
      std::this_thread::sleep_for(std::chrono::milliseconds(5));
    }
    
    std::this_thread::sleep_for(std::chrono::milliseconds(200));
    auto now = std::chrono::system_clock::now();
    auto secs = std::chrono::time_point_cast<std::chrono::seconds>(now);
    auto frac = std::chrono::duration<double>(now - secs).count();

    time_t pc_secs = secs.time_since_epoch().count();
    
    std::cout << "PC time now: " << pc_secs << " + " << frac << " sec\n";
    std::cout << "Setting USRP time to: " << pc_secs+1 << " at next PPS\n";
    // schedule time reset on next PPS, regardless of whether PPS comes from
    // the internal GPSDO or the external PPS input.
    usrp->set_time_next_pps(uhd::time_spec_t(pc_secs + 1));

    // Wait for it to apply
    // The wait is 2 seconds because N-Series has a known issue where
    // the time at the last PPS does not properly update at the PPS edge
    // when the time is actually set.
    std::this_thread::sleep_for(std::chrono::seconds(2));

    uhd::time_spec_t time_last_pps = usrp->get_time_last_pps();
    printf("USRP time now %1.4f USRP last pps %1.4f\n",usrp->get_time_now().get_real_secs(),time_last_pps.get_real_secs());

    for(size_t mb=0;mb<usrp->get_num_mboards();++mb)usrp->set_rx_subdev_spec(subdev,mb);
    for(const size_t ch:channel_nums){
        usrp->set_rx_rate(rate,ch);usrp->set_rx_freq(mode.center,ch);
        if(std::abs(usrp->get_rx_rate(ch)-rate)>.1||std::abs(usrp->get_rx_freq(ch)-mode.center)>1.)
            throw std::runtime_error("USRP coerced the requested sample rate or centre frequency");
    }
    std::vector<std::thread> threads;
    for(const size_t ch:channel_nums)
        threads.emplace_back(streaming_by_channel,ch,rate,subdev,outdir,usrp,time_last_pps,end_time,options);

    for(auto& thread : threads){
        thread.join();
    }

    usrp.reset();

    if (fatal_error.load()) {
        return EXIT_FAILURE;
    }

    return EXIT_SUCCESS;
}

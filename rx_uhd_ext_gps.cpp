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

#define NO_WRITE_DRF 1

namespace po = boost::program_options;

using namespace uhd::usrp_clock;
using namespace uhd::usrp;
using namespace std;

std::atomic<bool> shutdown_requested(false);
std::mutex shutdown_log_mutex;

void request_shutdown(const std::string& reason)
{
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
                                          short* data,
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

void streaming_by_channel(size_t chan,double rate,std::string subdev,std::string outdir, multi_usrp::sptr usrp, uhd::time_spec_t time_last_pps, std::chrono::steady_clock::time_point end_time)
{
    Digital_rf_write_object * data_object = NULL;
    uint64_t vector_leading_edge_index = 0;
    uint64_t global_start_index;
    int result;
    std::vector<size_t> channel_number;
    channel_number.push_back(chan);
    uint64_t sample_rate_numerator = 25000000;
    uint64_t sample_rate_denominator = 1;
    uint64_t subdir_cadence = 3600;
    uint64_t millseconds_per_file = 1000; 
    int compression_level = 0;
    int checksum = 0;
    int is_complex = 1;
    int is_continuous = 1;
    int num_subchannels = 1;
    int marching_periods = 1;
    std::string uuid = random_uuid_v4();
    uint64_t vector_length = 363;

    usrp->set_rx_rate(rate,chan);
    usrp->set_rx_freq(12.5e6,chan);
    usrp->set_rx_subdev_spec(subdev,chan);

    // Each thread owns one UHD receive streamer and one Digital RF writer.
    uhd::stream_args_t stream_args("sc16", "sc16");
    stream_args.channels             = channel_number;
    uhd::rx_streamer::sptr rx_stream = usrp->get_rx_stream(stream_args);

    // Start all channel streamers on the same future USRP timestamp.
    double tstart=time_last_pps.get_real_secs()+2.0;
    uhd::time_spec_t ts_t0=uhd::time_spec_t(tstart);
    printf("Streaming start at %f\n",time_last_pps.get_real_secs()+2.0);

    // Digital RF sample indices are absolute Unix-time sample numbers.
    global_start_index = (uint64_t)((uint64_t)tstart * (long double)sample_rate_numerator/sample_rate_denominator);
    printf("%lu",global_start_index);

    std::string ch_dir = outdir+"/ch"+std::to_string(chan);
    std::cout << "Writing complex short to multiple files and subdirectories in " << ch_dir << std::endl;
    std::string mkdir_cmd = "mkdir -p "+ch_dir;
    std::cout << mkdir_cmd << std::endl;
    result = system(mkdir_cmd.c_str());
    if (result != 0) {
      request_shutdown(str(boost::format(
          "Failed to create output directory %1% for channel %2%. Stopping recorder.")
          % ch_dir % chan));
      return;
    }

    data_object = digital_rf_create_write_hdf5((char *)ch_dir.c_str(),
					       H5T_NATIVE_SHORT,
					       subdir_cadence,
					       millseconds_per_file,
					       global_start_index,
					       sample_rate_numerator,
					       sample_rate_denominator,
					       (char *)uuid.c_str(),
					       compression_level,
					       checksum,
					       is_complex,
					       num_subchannels,
					       is_continuous,
					       marching_periods);

    if (!data_object){
      request_shutdown(str(boost::format(
          "No Digital RF writer created for channel %1%. Stopping recorder.")
          % chan));
      return;
    }

    uhd::stream_cmd_t stream_cmd(uhd::stream_cmd_t::STREAM_MODE_START_CONTINUOUS);
    stream_cmd.stream_now = false;
    stream_cmd.time_spec  = ts_t0;

    rx_stream->issue_stream_cmd(stream_cmd);

    uhd::rx_metadata_t md;

    std::vector<std::complex<short>> buff(rx_stream->get_max_num_samps());
    std::vector<void*> buffs;
    buffs.push_back(&buff.front());

    // The first recv() blocks until the timed stream command starts.
    double timeout = 3.0 + 0.1;

    uint64_t packet_i=0;
    uint64_t prev_tl=0;
    uint64_t samp_diff=363;
    int n_empty=0;
    while (!shutdown_requested.load() && std::chrono::steady_clock::now() < end_time)
    {
      size_t num_rx_samps = rx_stream->recv(buffs, buff.size(), md, timeout, true);

      if (shutdown_requested.load()) {
        break;
      }

      if(num_rx_samps  == 363){
        n_empty=0;
        uint64_t tl=(uint64_t)md.time_spec.get_full_secs()*sample_rate_numerator;
        tl=tl + (uint64_t)(md.time_spec.get_frac_secs()*((double)sample_rate_numerator));

        if(prev_tl!=0)
        {
          samp_diff = tl-prev_tl;
        }
      
        short *a = (short *)buff.data();
      
        if(samp_diff == 363)
        {
          if (!write_digital_rf_or_request_shutdown(
                  data_object, vector_leading_edge_index + packet_i*363, a, vector_length, chan)) {
            break;
          }
          packet_i+=1;
        }
        else
        {
          // Preserve the absolute Digital RF sample index across packet gaps.
          int n_packets = samp_diff/363;
          printf("samp_diff %ld number of packets %d\n",samp_diff,n_packets);
          for(int pi = 0 ; pi < n_packets; pi++)
          {
            if (!write_digital_rf_or_request_shutdown(
                    data_object, vector_leading_edge_index + packet_i*363, a, vector_length, chan)) {
              break;
            }
            packet_i+=1;
          }
        
        }
        prev_tl=tl;
      }
      else
      {
        printf("got no data in recv %d\n",n_empty);
        n_empty+=1;
        if(n_empty > 10)
        {
          request_shutdown(str(boost::format(
              "Channel %1% received no data for 10 consecutive recv calls. Stopping recorder.")
              % chan));
          break;
        }
      }

      timeout = 0.1;
    }

    try {
      uhd::stream_cmd_t stop_cmd(uhd::stream_cmd_t::STREAM_MODE_STOP_CONTINUOUS);
      rx_stream->issue_stream_cmd(stop_cmd);
    } catch (const std::exception& ex) {
      std::lock_guard<std::mutex> lock(shutdown_log_mutex);
      std::cerr << "Failed to stop streamer for channel " << chan
                << ": " << ex.what() << std::endl;
    }
    digital_rf_close_write_hdf5(data_object);
    std::cout << "Channel " << chan << " finished 24h streaming.\n";
}

int UHD_SAFE_MAIN(int argc, char* argv[])
{
    std::string usrp_args;
    std::string outdir;
    std::string subdev;
    double rate;
    std::string channel_list;
    int gps_lock_timeout_sec;

    po::options_description desc("Allowed options");
    // clang-format off
    desc.add_options()
        ("help", "help message")
        ("usrp_args", po::value<std::string>(&usrp_args)->default_value("addr0=192.168.10.2,recv_buff_size=500000000"),"ettus device args")
        ("outdir", po::value<std::string>(&outdir)->default_value("/dev/shm/hf25"), "output directory")
        ("subdev", po::value<std::string>(&subdev)->default_value("A:A"), "subdevice")
        ("rate", po::value<double>(&rate)->default_value(25e6), "rate of incoming samples")
        ("channels", po::value<std::string>(&channel_list)->default_value("0"), "which channel(s) to use (specify \"0\", \"1\", \"0,1\", etc)")
        ("gps-lock-timeout", po::value<int>(&gps_lock_timeout_sec)->default_value(300), "seconds to wait for internal GPSDO lock before continuing; use -1 to wait indefinitely")
    ;
    
    // clang-format on
    po::variables_map vm;
    po::store(po::parse_command_line(argc, argv, desc), vm);
    po::notify(vm);

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
    auto run_duration = std::chrono::hours(24);
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

    std::vector<std::thread> threads;
    for(size_t ch=0 ; ch < usrp->get_num_mboards(); ch++){
      threads.push_back(std::thread(streaming_by_channel, std::stoi(channel_strings[ch]), rate, subdev, outdir, usrp, time_last_pps, end_time));
    }  
    
    for(auto& thread : threads){
        thread.join();
    }

    usrp.reset();

    if (shutdown_requested.load()) {
        return EXIT_FAILURE;
    }

    return EXIT_SUCCESS;
}

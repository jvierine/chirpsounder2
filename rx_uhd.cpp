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
#include <complex>
#include <thread>
#include <iostream>
#include <fstream>
#include <vector>
#include <unistd.h>
#include <digital_rf.h>

#define NO_WRITE_DRF 1

namespace po = boost::program_options;

using namespace uhd::usrp_clock;
using namespace uhd::usrp;
using namespace std;

void get_usrp_time(multi_usrp::sptr usrp, size_t mboard, std::vector<int64_t>* times)
{
    (*times)[mboard] = usrp->get_time_now(mboard).get_full_secs();
}

void streaming_by_channel(size_t chan,double rate,std::string subdev,std::string outdir, multi_usrp::sptr usrp, uhd::time_spec_t time_last_pps)
{
    Digital_rf_write_object * data_object = NULL; /* main object created by init */
    uint64_t vector_leading_edge_index = 0; /* index of the sample being written starting at zero with the first sample recorded */
    uint64_t global_start_index; /* start sample (unix time * sample_rate) of first measurement - set below */
    int i, result;
    std::vector<size_t> channel_number;
    channel_number.push_back(chan);
    uint64_t sample_rate_numerator = 25000000; /* 25 MHz sample rate */
    uint64_t sample_rate_denominator = 1;
    uint64_t subdir_cadence = 3600;
    uint64_t millseconds_per_file = 1000; 
    int compression_level = 0; /* low level of compression */
    int checksum = 0; /* no checksum */
    int is_complex = 1; /* complex values */
    int is_continuous = 1; /* continuous data written */
    int num_subchannels = 1; /* only one subchannel */
    int marching_periods = 1; /* marching periods when writing */
    char uuid[100] = "Fake UUID - use a better one!";
    uint64_t vector_length = 363; /* one packet */

    usrp->set_rx_rate(rate,chan);
    usrp->set_rx_freq(12.5e6,chan);
    usrp->set_rx_subdev_spec(subdev,chan);

    // create a receive streamers for this thread's channel
    uhd::stream_args_t stream_args("sc16", "sc16"); // complex shorts
    stream_args.channels             = channel_number;
    uhd::rx_streamer::sptr rx_stream = usrp->get_rx_stream(stream_args);

    //    std::this_thread::sleep_for(std::chrono::seconds(2));
    // setup streaming
    double tstart=time_last_pps.get_real_secs()+2.0;
    uhd::time_spec_t ts_t0=uhd::time_spec_t(tstart);
    printf("Streaming start at %f\n",time_last_pps.get_real_secs()+2.0);
    
    // start recording at global_start_sample
    global_start_index = (uint64_t)((uint64_t)tstart * (long double)sample_rate_numerator/sample_rate_denominator);
    printf("%lu",global_start_index);

    std::string ch_dir = outdir+"/ch"+std::to_string(chan);
    std::cout << "Writing complex short to multiple files and subdirectores in " << ch_dir << std::endl;
    std::string mkdir_cmd = "mkdir -p "+ch_dir;
    std::cout << mkdir_cmd << std::endl;
    result = system(mkdir_cmd.c_str());

    data_object = digital_rf_create_write_hdf5((char *)ch_dir.c_str(),
					       H5T_NATIVE_SHORT,
					       subdir_cadence,
					       millseconds_per_file,
					       global_start_index,
					       sample_rate_numerator,
					       sample_rate_denominator,
					       uuid,
					       compression_level,
					       checksum,
					       is_complex,
					       num_subchannels,
					       is_continuous,
					       marching_periods);

    if (!data_object){
      printf("no data object created\n");
      exit(-1);
    }

    uhd::stream_cmd_t stream_cmd(uhd::stream_cmd_t::STREAM_MODE_START_CONTINUOUS);
    // stream_cmd.num_samps  = total_num_samps;
    stream_cmd.stream_now = false;
    stream_cmd.time_spec  = ts_t0;

    rx_stream->issue_stream_cmd(stream_cmd);

    // metadata
    uhd::rx_metadata_t md;


    // allocate buffer to receive with samples
    std::vector<std::complex<short>> buff(rx_stream->get_max_num_samps());
    std::vector<void*> buffs;
    buffs.push_back(&buff.front());

    // the first call to recv() will block this many seconds before receiving
    double timeout = 3.0 + 0.1; // timeout (delay before receive + padding)

    size_t num_acc_samps = 0; // number of accumulated samples
    uint64_t packet_i=0;
    uint64_t prev_tl=0;
    uint64_t samp_diff=363;
    int n_empty=0;
    while (1)
    {
      // receive a single packet
      size_t num_rx_samps = rx_stream->recv(buffs, buff.size(), md, timeout, true);

      if(num_rx_samps  == 363){
        n_empty=0;
        uint64_t tl=(uint64_t)md.time_spec.get_full_secs()*sample_rate_numerator;
        tl=tl + (uint64_t)(md.time_spec.get_frac_secs()*((double)sample_rate_numerator));

        //      printf("tl %ld prev %ld\n",tl,prev_tl);
        if(prev_tl!=0)
        {
          samp_diff = tl-prev_tl;
        }
      
        // pointer to short array
        short *a = (short *)buff.data();
      
        if(samp_diff == 363)
        {
          //	printf("%d\n",data_short[0]);
          result = digital_rf_write_hdf5(data_object, vector_leading_edge_index + packet_i*363, a, vector_length);
          packet_i+=1;
        }
        else
        {
          int n_packets = samp_diff/363;
          printf("samp_diff %ld number of packets %d\n",samp_diff,n_packets);
          for(int pi = 0 ; pi < n_packets; pi++)
          {
            result = digital_rf_write_hdf5(data_object, vector_leading_edge_index + packet_i*363, a, vector_length);
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
          exit(0);
        }
          /*	if (md.error_code != uhd::rx_metadata_t::ERROR_CODE_NONE) {
           throw std::runtime_error(str(boost::format("Receiver error %s") % md.strerror()));
          }
          */

      }
          // use a small timeout for subsequent packets
      timeout = 0.1;

          // handle the error code
          /*
          if (md.error_code == uhd::rx_metadata_t::ERROR_CODE_TIMEOUT)
      break;
          if (md.error_code != uhd::rx_metadata_t::ERROR_CODE_NONE) {
      throw std::runtime_error(str(boost::format("Receiver error %s") % md.strerror()));
          }
          */
          // check md.time_stamp

    }
}

int UHD_SAFE_MAIN(int argc, char* argv[])
{
    // variables to be set by po
    std::string usrp_args;
    std::string clock_args;
    std::string outdir;
    std::string killdir;
    std::string wire;
    std::string subdev;
    uint32_t max_interval, num_tests;
    double seconds_in_future;
    size_t total_num_samps;
    double rate;
    std::string channel_list;

    // setup the program options
    po::options_description desc("Allowed options");
    // clang-format off
    desc.add_options()
        ("help", "help message")
        ("usrp_args", po::value<std::string>(&usrp_args)->default_value("addr0=192.168.10.2,recv_buff_size=500000000"),"ettus device args")
        ("clock_args",po::value<std::string>(&clock_args)->default_value("addr=192.168.10.3"),"octoclock address args")
        ("outdir", po::value<std::string>(&outdir)->default_value("/dev/shm/hf25"), "output directory")
        ("killdir", po::value<std::string>(&killdir)->default_value("/home/sdr/chirpsounder2"), "kill directory")
        ("wire", po::value<std::string>(&wire)->default_value(""), "the over the wire type, sc16, sc8, etc")
        ("subdev", po::value<std::string>(&subdev)->default_value("A:A"), "subdevice")
        ("secs", po::value<double>(&seconds_in_future)->default_value(1.5), "number of seconds in the future to receive")
        ("nsamps", po::value<size_t>(&total_num_samps)->default_value(10000), "total number of samples to receive")
        ("rate", po::value<double>(&rate)->default_value(25e6), "rate of incoming samples")
        ("dilv", "specify to disable inner-loop verbose")
        ("channels", po::value<std::string>(&channel_list)->default_value("0"), "which channel(s) to use (specify \"0\", \"1\", \"0,1\", etc)")
        ("max-interval", po::value<uint32_t>(&max_interval)->default_value(10000), "Maximum interval between comparisons (in ms)")
        ("num-tests", po::value<uint32_t>(&num_tests)->default_value(2), "Number of times to compare device times")
    ;
    
    // clang-format on
    po::variables_map vm;
    po::store(po::parse_command_line(argc, argv, desc), vm);
    po::notify(vm);

    bool verbose = vm.count("dilv") == 0;

    // Create a Multi-USRP-Clock device (currently OctoClock only)
    std::cout << boost::format("\nCreating the Clock device with: %s") % clock_args
              << std::endl;
    multi_usrp_clock::sptr clock = multi_usrp_clock::make(clock_args);

    // Make sure Clock configuration is correct
    if (clock->get_sensor("gps_detected").value == "false") {
        throw uhd::runtime_error("No GPSDO detected on Clock.");
    }
    if (clock->get_sensor("using_ref").value != "internal") {
        throw uhd::runtime_error("Clock must be using an internal reference.");
    }

    // Create a Multi-USRP device
    std::cout << boost::format("\nCreating the USRP device with: %s") % usrp_args
              << std::endl;
    multi_usrp::sptr usrp = multi_usrp::make(usrp_args);

    // Store USRP device serials for useful output
    std::vector<std::string> serials;
    for (size_t ch = 0; ch < usrp->get_num_mboards(); ch++) {
        serials.push_back(usrp->get_usrp_tx_info(ch)["mboard_serial"]);
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

    // detect which channels to use
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

    // Get GPS time to initially set USRP devices
    std::cout << std::endl
              << "Querying Clock for time and setting USRP times..." << std::endl
              << std::endl;

    const time_t gps_time = clock->get_sensor("gps_time").to_int();
    usrp->set_time_next_pps(uhd::time_spec_t(gps_time + 1));

    // Wait for it to apply
    // The wait is 2 seconds because N-Series has a known issue where
    // the time at the last PPS does not properly update at the PPS edge
    // when the time is actually set.
    std::this_thread::sleep_for(std::chrono::seconds(2));

    uhd::time_spec_t time_last_pps = usrp->get_time_last_pps();
    printf("USRP time now %1.4f USRP last pps %1.4f\n",usrp->get_time_now().get_real_secs(),time_last_pps.get_real_secs());

    // Threading for each channel
    std::vector<std::thread> threads;
    for(size_t ch=0 ; ch < usrp->get_num_mboards(); ch++){
        threads.push_back(std::thread(streaming_by_channel, std::stoi(channel_strings[ch]), rate, subdev, outdir, usrp, time_last_pps));
    }  
    
    // Join threads
    for(auto& thread : threads){
        thread.join();
    }

    return EXIT_SUCCESS;
}

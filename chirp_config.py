import numpy as n
import os
try:
    import configparser
except ImportError as e:
    import configparser2 as configparser

import json


class chirp_config:
    def __init__(self, fname=None, read_shared=True):
        cf = configparser.ConfigParser()
        explicit_ringbuffer_max_age_min = False
        explicit_ringbuffer_max_age_sec = False
        shared_station_info = {}
        shared_station_links = []
        # initialize with default values

#                        # 70 seconds is the default cleanup age
#ringbuffer_max_age_sec=70
# delete older files
#ringbuffer_cleanup=true


        cf["config"] = {"channel": '''
["ch0"]
''',
                        "sample_rate": "25000000.0",
                        "center_freq": "12.5e6",
                        "data_dir": '"/mnt/data/juha/hf25"',
                        "kill_path": '"/home/sdr/chirpsounder2/kill.txt"',
 #                       "chirp_rep_times": "[300.0,300.0,300.0,60.0]",
                        "output_dir": '"./chirp2"',
                        "receiver_station_name": '"station_name"',
                        "plot_timings": "false",
                        "realtime": "false",
                        "ringbuffer_max_age_min":"4",
                        "ringbuffer_max_age_sec":"70",
                        "ringbuffer_cleanup":"false",
                        "serendipitous": "false",
                        }
        cf["detection"] = {
            "threshold_snr": "13.0",
            "max_simultaneous_detections": "5",
            "min_detections": "3",            
            "step": "1",
            "n_samples_per_block": "5000000",
            "minimum_frequency_spacing": "0.2e6",
            "chirp_rates": "[50e3,100e3,125e3,500.0084e3]",
            "debug_timings": "false",            
        }
        cf["lfm"] = {
            "range_resolution": "2e3",
            "frequency_resolution": "50e3",
            "maximum_analysis_frequency": "25e6",
            "minimum_analysis_frequency": "0.0",
            "max_range_extent": "2000e3",
            "min_range": "200e3",
            "storage_snr_threshold": "2",                        
            "max_range": "1500e3",
            "manual_range_extent": "false",
            "save_raw_voltage": "false",
            "fast_boxcar_filter": "false",
            "downconversion_filter": '"fir"',
            "cic_stages": "2",
            "n_downconversion_threads": "4"            ,
            "downconversion_block_samples": "4000",
            "min_freq": "0",
            "max_freq": "25e6",
            "manual_freq_extent": "false",
            "decimation": "1250",
            "sounder_timings": '[{"chirp-rate":500.0084e3,"rep":60.0,"chirpt":54.0016,"id":5}]'
            }
        cf["transfer"]={
                        "copy_to_server": "false",
                        "copy_destination": "none",
            }
        cf["rtf"] = {
            "links": "[]",
        }

        cf["stations"]={
            "station_info":'''

{"SGO":{"name":"SGO",
	             "lat":67.36369337350563,
	             "lon":26.634311805059543},
	      "TGO":{"name":"TGO",
	             "lat":69.66174439007057,
		     "lon":18.939127366530286},
	      "Ramfjordmoen":{"name":"Ramfjordmoen",
	                      "lat":69.58187184247221,
			      "lon":19.220853348827067},
	      "ROTHR":{"name":"ROTHR",
	             "lat":36.11793762278912, 
		     "lon":-82.5807086169964},
	      "JORN":{"name":"JORN", 
	             "lat":-23.853424758715892,
		     "lon":125.07620821000198}
	      }
	      ''',
          "links":'''
 [["SGO","TGO"],
	["Ramfjordmoen","TGO"],
	["ROTHR","TGO"],
	["JORN","TGO"]]
          '''
            }
        
        if fname != None:
            if os.path.exists(fname):
                fname = os.path.abspath(fname)
                shared_fname = os.path.join(os.path.dirname(fname), "server.ini")
                if read_shared and shared_fname != fname and os.path.exists(shared_fname):
                    print("reading %s" % (shared_fname))
                    cf.read(shared_fname)
                    shared_cf = configparser.ConfigParser()
                    shared_cf.read(shared_fname)
                    explicit_ringbuffer_max_age_min |= shared_cf.has_option("config", "ringbuffer_max_age_min")
                    explicit_ringbuffer_max_age_sec |= shared_cf.has_option("config", "ringbuffer_max_age_sec")
                    if shared_cf.has_option("stations", "station_info"):
                        shared_station_info = json.loads(shared_cf["stations"]["station_info"])
                    if shared_cf.has_option("stations", "links"):
                        shared_station_links = json.loads(shared_cf["stations"]["links"])
                print("reading %s" % (fname))
                cf.read(fname)
                file_cf = configparser.ConfigParser()
                file_cf.read(fname)
                explicit_ringbuffer_max_age_min |= file_cf.has_option("config", "ringbuffer_max_age_min")
                explicit_ringbuffer_max_age_sec |= file_cf.has_option("config", "ringbuffer_max_age_sec")
            else:
                print(
                    "configuration file %s doesn't exist. using default values" % (fname))
        #print("keys",list(cf.keys()))
        self.fname = fname
        self.plot_timings = json.loads(cf["config"]["plot_timings"])
        self.copy_to_server = json.loads(cf["transfer"]["copy_to_server"])

        self.ringbuffer_max_age_min=json.loads(cf["config"]["ringbuffer_max_age_min"])#:"300",
        if explicit_ringbuffer_max_age_sec:
            self.ringbuffer_max_age_sec=json.loads(cf["config"]["ringbuffer_max_age_sec"])
        elif explicit_ringbuffer_max_age_min:
            self.ringbuffer_max_age_sec=self.ringbuffer_max_age_min * 60
        else:
            self.ringbuffer_max_age_sec=json.loads(cf["config"]["ringbuffer_max_age_sec"])
        self.ringbuffer_cleanup=json.loads(cf["config"]["ringbuffer_cleanup"])#":"false",

        
        self.debug_timings = json.loads(cf["detection"]["debug_timings"])
#        print(cf["stations"]["station_info"])
        self.station_info = shared_station_info
        self.station_info.update(json.loads(cf["stations"]["station_info"]))
        self.station_links = shared_station_links + [
            link for link in json.loads(cf["stations"]["links"])
            if link not in shared_station_links
        ]
        self.rtf_links = json.loads(cf["rtf"]["links"])

        self.manual_range_extent = json.loads(
            cf["lfm"]["manual_range_extent"])

        self.manual_freq_extent = json.loads(
            cf["lfm"]["manual_freq_extent"])

        self.serendipitous = json.loads(cf["config"]["serendipitous"])
        self.storage_snr_threshold = json.loads(cf["lfm"]["storage_snr_threshold"])
        self.min_detections = int(json.loads(cf["detection"]["min_detections"]))
        
        self.sounder_timings = json.loads(cf["lfm"]["sounder_timings"])
        self.decimation = json.loads(cf["lfm"]["decimation"])
#        self.chirp_rep_times = json.loads(cf["config"]["chirp_rep_times"])
        self.realtime = json.loads(cf["config"]["realtime"])
        self.save_raw_voltage = json.loads(cf["lfm"]["save_raw_voltage"])
        self.fast_boxcar_filter = json.loads(cf["lfm"]["fast_boxcar_filter"])
        self.downconversion_filter = json.loads(cf["lfm"]["downconversion_filter"])
        if self.downconversion_filter not in ["fir", "boxcar", "cic"]:
            raise ValueError("downconversion_filter must be 'fir', 'boxcar', or 'cic'")
        self.cic_stages = json.loads(cf["lfm"]["cic_stages"])
        self.data_dir = json.loads(cf["config"]["data_dir"])
        print(self.data_dir)

#        self.snr_threshold = json.loads(cf["config"]["snr_threshold"])
        
        self.kill_path = json.loads(cf["config"]["kill_path"])
        try:
            self.copy_destination = json.loads(
                cf["transfer"]["copy_destination"])
        except:
            print("couldn't read copy destination")
            pass
        self.station_name = json.loads(cf["config"]["receiver_station_name"])

        self.n_downconversion_threads = json.loads(
            cf["lfm"]["n_downconversion_threads"])
        self.downconversion_block_samples = int(json.loads(
            cf["lfm"]["downconversion_block_samples"]))
        self.max_range_extent = json.loads(cf["lfm"]["max_range_extent"])

        self.max_range = json.loads(cf["lfm"]["max_range"])
        self.min_range = json.loads(cf["lfm"]["min_range"])

        self.max_freq = json.loads(cf["lfm"]["max_freq"])
        self.min_freq = json.loads(cf["lfm"]["min_freq"])

        self.n_samples_per_block = json.loads(
            cf["detection"]["n_samples_per_block"])
        self.sample_rate = json.loads(cf["config"]["sample_rate"])
        self.center_freq = json.loads(cf["config"]["center_freq"])
        self.chirp_rates = json.loads(cf["detection"]["chirp_rates"])
        self.range_resolution = json.loads(cf["lfm"]["range_resolution"])
        self.frequency_resolution = json.loads(
            cf["lfm"]["frequency_resolution"])
#        print(cf["config"]["channel"])
        self.channel = json.loads(cf["config"]["channel"])
        self.step = json.loads(cf["detection"]["step"])
        self.maximum_analysis_frequency = json.loads(
            cf["lfm"]["maximum_analysis_frequency"])
        self.minimum_analysis_frequency = json.loads(
            cf["lfm"]["minimum_analysis_frequency"])
        self.output_dir = json.loads(cf["config"]["output_dir"])

        try:
            os.mkdir(self.output_dir)
        except:
            pass

        if not os.path.exists(self.output_dir):
            print("Output directory %s doesn't exists and cannot be created" %
                  (self.output_dir))
            #exit(0)

        # the minimum distance in frequency between detections
        # (avoid multiple detections of the same chirp)
        self.minimum_frequency_spacing = json.loads(
            cf["detection"]["minimum_frequency_spacing"])
        self.df = (float(self.sample_rate) / float(self.n_samples_per_block))
        # minimum spacing of detections in fft bins
        self.mfsi = int(self.minimum_frequency_spacing / self.df)

        # how many chirps can we detect simultaneously
        self.max_simultaneous_detections = json.loads(
            cf["detection"]["max_simultaneous_detections"])
        # the smallest normalized snr that is detected
        self.threshold_snr = json.loads(cf["detection"]["threshold_snr"])

        self.fvec = n.fft.fftshift(n.fft.fftfreq(self.n_samples_per_block,
                                                 d=1.0 / float(self.sample_rate))) + self.center_freq

    def __str__(self):
        out = "Configuration\n"
        for e in dir(self):
            if not callable(getattr(self, e)) and not e.startswith("__"):
                out += "%s = %s\n" % (e, getattr(self, e))
        return (out)


if __name__ == "__main__":
    import sys
    cc = chirp_config(sys.argv[1])
    print(cc)
    

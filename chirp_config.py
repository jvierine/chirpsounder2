import numpy as n
import os

class chirp_config:
    def __init__(self,
                 data_dir="/mnt/data/juha/hf25",
                 channel="cha",
                 n_samples_per_block=2500000,
                 sample_rate=25000000.0,
                 center_freq=12.5e6,
                 chirp_rates=[50e3,100e3,125e3,500.0084e3,550e3],
                 minimum_frequency_spacing=0.2e6,
                 threshold_snr=15.0,
                 max_simultaneous_detections=5,
                 save_bandwidth=20e3,    # how much bandwidth do we store around detected peak
                 range_resolution=1000.0,    
                 frequency_resolution=100000.0,
                 output_dir="chirp_out"):

        self.n_samples_per_block=n_samples_per_block
        self.sample_rate=sample_rate
        self.center_freq=center_freq
        self.chirp_rates=chirp_rates
        self.range_resolution=range_resolution
        self.frequency_resolution=frequency_resolution
        self.data_dir=data_dir
        self.channel=channel
        os.system("mkdir -p %s"%(output_dir))
        self.output_dir=output_dir
        # the minimum distance in frequency between detections
        # (avoid multiple detections of the same chirp)
        self.minimum_frequency_spacing=minimum_frequency_spacing
        self.df=(float(sample_rate)/float(n_samples_per_block))
        self.mfsi=int(minimum_frequency_spacing/self.df) # minimum spacing of detections in fft bins

        # how many chirps can we detect simultaneously
        self.max_simultaneous_detections=max_simultaneous_detections
        # the smallest normalized snr that is detected
        self.threshold_snr=threshold_snr

        self.fvec=n.fft.fftshift(n.fft.fftfreq(n_samples_per_block,
                                               d=1.0/float(sample_rate)))+center_freq
        
        self.save_bandwidth=save_bandwidth
        n_bins_to_save=int(save_bandwidth/self.df)
        self.save_freq_idx=n.arange(-int(n_bins_to_save/2),int(n_bins_to_save/2),dtype=n.int)
        self.save_len=len(self.save_freq_idx)

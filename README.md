# Chirp Sounder 2

This software can be used to detect chirp sounders over the air, and to calculate ionograms. The software relies on digital rf recordings of HF. 

The software consists of several parts:
 - detect_chirps.py  # this is used to find chirps using a chirp-rate matched filterbank
 - find_timings.py # this is used to cluster detections and determine what chirp timings and chirp rates exist
 - calc_ionograms.py # this is used to calculate ionograms based on parameters
 - plot_ionograms.py # plot calculated ionograms

Version:
Tested on Python 2.7.

Usage:
1) configure config_config.py (make sure you have the right center frequency, sample-rate, data directory, and channel name)
2) run detect_chirps.py
3) run find_timings.py
4) run calc_ionograms.py
5) run plot_ionograms.py

<!--
# Examples

<img src="./examples/chirpdet0.png" width="60%"/>

<img src="./examples/chirpdet1.png" width="60%"/>

<img src="./examples/chirpdet2.png" width="60%"/>
-->

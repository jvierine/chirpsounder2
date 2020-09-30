# Chirp Sounder 2

This software can be used to detect chirp sounders over the air, and to calculate ionograms. The software relies on digital rf recordings of HF. 

The software consists of several parts:
 - detect_chirps.py  # this is used to find chirps using a chirp-rate matched filterbank
 - find_timings.py # this is used to cluster detections and determine what chirp timings and chirp rates exist
 - calc_ionograms.py # this is used to calculate ionograms based on parameters
 - plot_ionograms.py # plot calculated ionograms


<!--
# Examples

<img src="./examples/chirpdet0.png" width="60%"/>

<img src="./examples/chirpdet1.png" width="60%"/>

<img src="./examples/chirpdet2.png" width="60%"/>
-->

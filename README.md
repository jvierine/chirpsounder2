# Chirp Sounder 2

This software can be used to detect chirp sounders and over-the-horizon radar transmissions over the air, and to calculate ionograms from them. The software relies on <a href="https://github.com/MITHaystack/digital_rf">Digital RF</a> recordings of HF. 

This is a new implementation of the <a href="https://github.com/jvierine/chirpsounder">GNU Chirp Sounder</a>. This new version allows you to now automatically find chirps without knowledge of what the timing and chirp-rate is. You can still figure out the true distance if you have a GPSDO, as most sounders start at a full second. 

The software consists of several parts:
 - detect_chirps.py  # this is used to find chirps using a chirp-rate matched filterbank
 - find_timings.py # this is used to cluster detections and determine what chirp timings and chirp rates exist
 - calc_ionograms.py # this is used to calculate ionograms based on parameters
 - plot_ionograms.py # plot calculated ionograms

## Installation
See dependencies.txt for instructions on how to build the dependencies (tested on Ubuntu 18 & 20)
You need to compile the chirp downconversion library, which is written in C.
```
make 
```
There is no packaging or other installation needed. You just run the scripts in place. 

Python packages that are required: pyfftw, numpy, scipy, matplotlib, digital_rf, mpi4py, h5py. Tested on Python 2.7.17 and Python 3.6.9. 

## Usage:
1) Make a data capture with THOR (comes with <a href="https://github.com/MITHaystack/digital_rf">DigitalRF</a>), a USRP N2x0, a GPSDO, and a broadband HF antenna in a quiet location. I recommend using a 12.5 MHz center frequency and a 25 MHz sampling rate. Here is an example command to kick off a recording: 

```
thor.py -m 192.168.10.3 -d "A:A" -c cha -f 12.5e6 -r 25e6 /dev/shm/hf25 
```

Tip: You can use a RAM disk ring buffer to avoid dropped packets on slower computers and hard disks. This is not necessary, as the chirp analysis will be okay with dropped packets. Here's an example of how you can use rsync to shovel a digital rf recording on the fly from a ram disk to a hard disk.

```
# copy digital rf from ram disk to permanent storage:
while true; do rsync -av --remove-source-files --exclude=tmp*
--progress /dev/shm/hf25/cha /data_out/hf25/ ; sleep 1 ; done
```

2) configure by copying the example1.ini to e.g., configuration.ini. Edit the file to make sure you have the right center frequency, sample-rate, data directory, and channel name. I've only tested 25 MHz sample-rate and 12.5 MHz center-frequency so far.
```
[config]

# channel name for the digital rf recording
channel="cha"

# the sample rate of the digital rf recording
sample_rate=25000000.0

# the center frequency of the digital rf recording
center_freq=12.5e6

# the location of the digital_rf recording
data_dir="/data_out/hf25"

# detection
threshold_snr=13.0

# how many chirps can we at most detect simultaneously
max_simultaneous_detections=5

# how sparsely do we search for chirps (1 .. N) 1 is slowest, but the most sensitive
# every Nth block is analyzed 
step=10            

# how many samples per block are coherently integrated on chirp detection
n_samples_per_block=5000000

minimum_frequency_spacing=0.2e6
# what chirp rates do we look for
chirp_rates=[50e3,100e3,125e3,500.0084e3]

# this is where all the data files are produced in
output_dir="./chirp2"

# what is the range resolution of the ionograms
range_resolution=2e3

# what is the frequency step of the ionogram
frequency_resolution=50e3

# what is the range extent around the strongest echo that is stored
max_range_extent=2000e3

# how many threads are used when chirp downconverting
n_downconversion_threads=4
```

3) Detect chirps on the recording. can be parallelized with MPI to speed things up if you have lots of CPUs and a very fast disk. If you don't have a fast disk, using too many processes may actually reduce performance due to trashing. 
```
mpirun -np 4 python detect_chirps.py configuration.ini
```

4) Run find_timings.py to cluster together multiple detections of the same chirp to create a database of chirp timings
```
python find_timings.py configuration.ini
```

5) Run calc_ionograms.py to generate ionograms based on the timings that were found. Can be paralellized with MPI. Keep in mind that adding a lot of processes may be detrimental to performance, due to the 100 MB/s read requirement. If you have a slow disk, don't use too many processes here! Each MPI process is additionally multi-threaded, with the number of threads configured in the configuration file
```
python calc_ionograms.py configuration.ini
```

6) run plot_ionograms.py to create plots
```
python plot_ionograms.py configuration.ini
```


## Examples

All of these are observed in Northern Norway (Skibotn). I typically see around 100 ionograms per hour in a recording.

US ROTHR (hard to tell which one, as I'm so far away)

<img src="https://raw.githubusercontent.com/jvierine/chirpsounder2/python-packaging/examples//example00.png" width="60%"/>

Sodankylä geophysical observatory vertical sounding ionosonde

<img src="https://raw.githubusercontent.com/jvierine/chirpsounder2/python-packaging/examples//example01.png" width="60%"/>

US ROTHR (hard to tell which one, as I'm so far away)

<img src="https://raw.githubusercontent.com/jvierine/chirpsounder2/python-packaging/examples//example02.png" width="60%"/>

Sodankylä geophysical observatory vertical sounding ionosonde

<img src="https://raw.githubusercontent.com/jvierine/chirpsounder2/python-packaging/examples//example03.png" width="60%"/>

Australian JORN. Very far away! I see many of these at the right time of day. 

<img src="https://raw.githubusercontent.com/jvierine/chirpsounder2/python-packaging/examples//example04.png" width="60%"/>

US ROTHR (hard to tell which one, as I'm so far away)

<img src="https://raw.githubusercontent.com/jvierine/chirpsounder2/python-packaging/examples//example05.png" width="60%"/>

## Links

You can also use your sound card and HAM radio to detect chirps using the <a href="https://www.andrewsenior.me.uk/chirpview">Chirpview</a> program.

University of Twente operates a WebSDR, which is capable of <a href="http://websdr.ewi.utwente.nl:8901/chirps/">tracking known chirp sounders</a>

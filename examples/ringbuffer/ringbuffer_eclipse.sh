#!/usr/bin/bash
#
# start a ringbuffer
#
# sync to ntp time
sudo ntpdate ntp.uit.no
# start digital rf acquisition
./rx_uhd_ext_gps --outdir=/dev/shm/hf25 --rate=25e6 &
sleep 10
# setup ringbuffer
drf ringbuffer -z 3000MB /dev/shm/hf25 -p 2 &
sleep 10

# Calculate ionograms using known timings
# use two parallel threads. one for SGO and one for HAARP
mpirun.mpich -np 2 python calc_ionograms.py conf/ringbuffer_eclipse.ini &
sleep 10

# plot ionograms
python plot_ionograms.py conf/ringbuffer_eclipse.ini

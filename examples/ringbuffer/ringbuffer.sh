#!/usr/bin/bash
#
# start a ringbuffer
#
sudo ntpdate ntp.uit.no
./rx_uhd_ext_gps --outdir=/dev/shm/hf25 --rate=25e6 &
sleep 10
drf ringbuffer -z 2000MB /dev/shm/hf25 -p 2 &
sleep 10
# detect chirps
mpirun.mpich -np 2 python detect_chirps.py ringbuffer.ini &
sleep 10
# calculate ionograms
python calc_ionograms.py ringbuffer.ini &
sleep 10
# plot ionograms
python plot_ionograms.py ringbuffer.ini

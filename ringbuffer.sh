#!/usr/bin/bash
#
# start a ringbuffer
#
sudo ntpdate ntp.uit.no
thor.py -m 192.168.10.4 -d A:A -c cha -f 12.5e6 -r 25e6 /dev/shm/hf25 &
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

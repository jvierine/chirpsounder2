#!/usr/bin/bash
#
# start a ringbuffer
#
# sync to ntp time
sudo ntpdate ntp.uit.no
# start digital rf acquisition
thor.py -m 192.168.10.2 -d A:A -c cha -f 12.5e6 -r 25e6 /dev/shm/hf25 &
sleep 10
# setup ringbuffer
drf ringbuffer -z 1700MB /dev/shm/hf25 -p 2 &
sleep 10

# Calculate ionograms using known timings
# use two parallel threads. one for SGO and one for HAARP
mpirun.mpich -np 2 python calc_ionograms.py sgo.ini &
sleep 10

# plot ionograms
python plot_ionograms.py conf/sgo.ini

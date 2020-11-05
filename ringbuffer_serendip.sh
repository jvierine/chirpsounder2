#!/usr/bin/bash
#
# analyze unknown ionograms serendipitously (start analyzing when a new chirp is detected)
#
# make sure this is the right mpirun command (you might need mpirun instead of mpirun.mpich)
MPIRUN=mpirun.mpich
NTP_SERVER=ntp.uit.no
RINGBUFFER_DIR=/dev/shm/hf25
SAMPLE_RATE=10e6
CENTER_FREQ=10e6
# start a ringbuffer
#
sudo ntpdate $NTP_SERVER
rm -Rf $RINGBUFFER_DIR
thor.py -m 192.168.10.4 -d A:A -c cha -f $CENTER_FREQ -r $SAMPLE_RATE $RINGBUFFER_DIR &
sleep 10

drf ringbuffer -z 3800MB $RINGBUFFER_DIR -p 2 &
sleep 10

# detect chirps
#mpirun.mpich -np 2 python detect_chirps.py ringbuffer_serendip.ini &
#sleep 10

# find timings
#python find_timings.py ringbuffer_serendip.ini &
#sleep 10

# calculate ionograms
#mpirun.mpich -np 2 python calc_ionograms.py ringbuffer_serendip.ini &
#sleep 10
# plot ionograms
#python plot_ionograms.py ringbuffer_serendip.ini

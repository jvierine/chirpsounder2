#!/usr/bin/bash
#
# analyze unknown ionograms serendipitously (start analyzing when a new chirp is detected)
#
# make sure this is the right mpirun command (you might need mpirun instead of mpirun.mpich)
MPIRUN=mpirun.mpich
NTP_SERVER=ntp.uit.no
# ram disk buffer for fast i/o.
# if you have a fast SSD or raid, you can also use that
RINGBUFFER_DIR=/dev/shm/hf25
SAMPLE_RATE=10e6
CENTER_FREQ=5e6
# make this about 200 MB less than half your RAM size
# with an SSD this can be more
# more is better, as we can analyze more
RINGBUFFER_SIZE=3800MB
# start a ringbuffer
#
sudo ntpdate $NTP_SERVER
rm -Rf $RINGBUFFER_DIR
thor.py -m 192.168.10.4 -d A:A -c cha -f $CENTER_FREQ -r $SAMPLE_RATE $RINGBUFFER_DIR &
sleep 10

drf ringbuffer -z $RINGBUFFER_SIZE $RINGBUFFER_DIR -p 2 &
sleep 10

# detect chirps
$MPIRUN -np 2 python detect_chirps.py ringbuffer_serendip.ini &
sleep 10

# find timings
python find_timings.py ringbuffer_serendip.ini &
sleep 10

# calculate ionograms
$MPIRUN -np 4 python calc_ionograms.py ringbuffer_serendip.ini &
#sleep 10

# plot ionograms
python plot_ionograms.py ringbuffer_serendip.ini &

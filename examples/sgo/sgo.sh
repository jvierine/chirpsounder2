#!/usr/bin/bash
#
# start a ringbuffer
#
CONFFILE=/home/chrpsdr/src/chirpsounder2/examples/sgo/sgo.ini
DDIR=/dev/shm/hf25
# sync to ntp time
sudo ntpdate ntp.uit.no

# setup ringbuffer
drf ringbuffer -z 1700MB $DDIR -p 2 &


# Calculate ionograms using known timings
# use two parallel threads. one for SGO and one for HAARP
python3 calc_ionograms.py $CONFFILE &
sleep 10

# plot ionograms
python3 plot_ionograms.py $CONFFILE &

while true;
do
    # start digital rf acquisition
    thor.py -m 192.168.10.2 -d A:A -c cha -f 12.5e6 -r 25e6 $DDIR 
    sleep 10
done
    

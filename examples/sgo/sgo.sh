#!/usr/bin/bash
#
# start a ringbuffer
#
CONFFILE=/home/chrpsdr/src/chirpsounder2/examples/sgo/sgo.ini
DDIR=/dev/shm/hf25
mkdir -p logs
# sync to ntp time
echo "NTPDATE"
sudo ntpdate ntp.uit.no

# setup ringbuffer
echo "Ringbuffer"
drf ringbuffer -z 1700MB $DDIR -p 2 >logs/ringbuffer.log 2>&1 &


# Calculate ionograms using known timings
# use two parallel threads. one for SGO and one for HAARP
echo "Ionogram calc"
python3 calc_ionograms.py $CONFFILE >logs/calc_ionograms.log 2>&1 &
sleep 10

# plot ionograms
echo "Plot ionograms"
python3 plot_ionograms.py $CONFFILE >logs/plot_ionograms.log 2>&1 &

while true;
do
    echo "Starting THOR"
    # start digital rf acquisition
    thor.py -m 192.168.10.2 -d A:A -c cha -f 12.5e6 -r 25e6 $DDIR  >logs/thor.log 2>&1
    sleep 10
done
    

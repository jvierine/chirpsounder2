#!/usr/bin/bash
#
# start a ringbuffer
#
INSTALL_PATH=/home/$USER/src/chirpsounder2
cd $INSTALL_PATH
# stop all processes
./stop_ringbuffer.sh
CONFFILE=/home/$USER/src/chirpsounder2/examples/sgo/sgo_iva.ini
DDIR=/dev/shm/hf25
mkdir -p logs

# delete old data from ram disk
rm -Rf $DDIR
mkdir -p $DDIR

# sync to ntp time not needed, if you run ntpd
#echo "NTPDATE"
#sudo ntpdate ntp.uit.no

# setup ringbuffer
echo "Ringbuffer"
drf ringbuffer -z 30000MB $DDIR -p 2 >logs/ringbuffer.log 2>&1 &


# Calculate ionograms using known timings
# use two parallel threads. one for SGO and one for HAARP
echo "Ionogram calc"
python3 calc_ionograms.py $CONFFILE >logs/calc_ionograms.log 2>&1 &
sleep 10

# plot ionograms
echo "Plot ionograms"
python3 plot_ionograms.py $CONFFILE >logs/plot_ionograms.log 2>&1 &


echo "startgin backup"
sh ./examples/sgo/backup_iva.sh >logs/backup.log 2>&1 &

while true;
do
    echo "Starting THOR"
    # start digital rf acquisition with custom c++ program that uses the uhd driver directly, skipping gnuradio
    ./rx_uhd >logs/thor.log 2>&1
    sleep 10
done
    

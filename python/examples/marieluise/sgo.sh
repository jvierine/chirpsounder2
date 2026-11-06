#!/usr/bin/bash
#
# start a ringbuffer
#
INSTALL_PATH=/home/$USER/src/chirpsounder2
cd $INSTALL_PATH
# stop all processes
sh stop_ringbuffer.sh
CONFFILE=/home/$USER/src/chirpsounder2/examples/marieluise/sgo_tgo.ini
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
# use two parallel threads. one for SGO
# based on known timing of transmitted chirp, calculate m_d[n] = m[n*decimation]\epsilon^*[n*decimation], decimate it and save to file
echo "Ionogram calc"
python3 calc_ionograms.py $CONFFILE >logs/calc_ionograms.log 2>&1 &
sleep 10

# plot ionograms
# take a spectrogram of m_d[n] and figure mapping between frequency, time, and ionogram frequency and range.
echo "Plot ionograms"
python3 plot_ionograms.py $CONFFILE >logs/plot_ionograms.log 2>&1 &



while true;
do
    echo "Starting THOR"
    # start digital rf acquisition with custom c++ program that uses the uhd driver directly, skipping gnuradio
    # digitize with 25 MHz and store to disk (create a stream of m[n])
    ./rx_uhd >logs/thor.log 2>&1
    sleep 10
done
    

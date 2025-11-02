#!/usr/bin/bash
#
# ntp-public.uit.no
# ntp.uit.no
#
# Create ionograms without knowledge of ionogram timings. Figure out the timings
# by listening to signals on the antenna.
# 
#

INSTALL_PATH=/home/hfrx2/src/chirpsounder2

# make sure this is the right mpirun command (you might need mpirun instead of mpirun.mpich)
MPIRUN=mpirun

# ram disk buffer for fast i/o.
# if you have a fast SSD or raid, you can also use that
RINGBUFFER_DIR=/dev/shm/hf25
SAMPLE_RATE=25e6
CENTER_FREQ=12.5e6
CONF_FILE=$INSTALL_PATH/examples/marieluise/marieluise.ini

cd $INSTALL_PATH
# kill possibly existing runtime
# stop all processes
./stop_ringbuffer.sh
# delete old data from ram disk
rm -Rf $RINGBUFFER_DIR

mkdir -p logs

# ram disk size. this should be compatible than your ram size
# at least about 2 GB less than the amount of ram available.
RINGBUFFER_SIZE=30000MB

# start ring buffer
drf ringbuffer -z $RINGBUFFER_SIZE $RINGBUFFER_DIR -p 2 > logs/ringbuffer.log 2>&1 &
echo "Starting ringbuffer"

# detect chirps
# two processes seems to be enough to keep up with realtime
$MPIRUN -np 4 python3 detect_chirps.py $CONF_FILE > logs/detect.log 2>&1 &
echo "Starting detect_chirps.py"

# find timings
python3 find_timings.py $CONF_FILE > logs/timings.log 2>&1 &
echo "Starting find_timings.py"
# calculate ionograms
# seems like four parallel processes work.
# this means we can process four ionograms simultaneously!
$MPIRUN --oversubscribe -np 4 python3 calc_ionograms.py $CONF_FILE > logs/ionograms.log 2>&1 &

echo "Starting calc_ionograms.py"

# plot ionograms
python3 plot_ionograms.py $CONF_FILE > logs/plot_ionograms.log &
echo "Starting plot_ionograms.py"

echo "Starting rx_uhd"
while true;
do
    ./rx_uhd --outdir=$RINGBUFFER_DIR > logs/thor.log 2>&1 
    sleep 5
done


#!/bin/bash
#
# Create ionograms without knowledge of ionogram timings. Figure out the timings
# by listening to signals on the antenna.
#
#
# This is the directory where the python programs are:
INSTALL_PATH=/home/aeronauts/uhd/host/build/chirpsounder2/

# make sure this is the right mpirun command (you might need mpirun instead of mpirun.mpich)
MPIRUN=mpirun

# ram disk buffer for fast i/o.
# if you have a fast SSD or raid, you can also use that
# note that this is defined in /etc/fstab so that this disk is created
# 
# when computer is started. /dev/shm only had 32 GB.
RINGBUFFER_DIR=/mnt/tmpfs
# delete old data from ram disk
#rm -Rf $RINGBUFFER_DIR/
SAMPLE_RATE=25e6
CENTER_FREQ=20e6
CONF_FILE=$INSTALL_PATH/examples/oahu/aeroauto.ini

cd $INSTALL_PATH
mkdir -p logs
#mkdir -p $OUTPUT_DIR/logs

# ram disk size. this should be compatible than your ram size
# at least about 2 GB less than the amount of ram available.
RINGBUFFER_SIZE=42000MB

# start ring buffer
# deletes oldest file once data in directory is over $RINGBUFFER_SIZE
drf ringbuffer -z $RINGBUFFER_SIZE $RINGBUFFER_DIR -p 1 > logs/ringbuffer.log 2>&1 &
echo "Starting ringbuffer"

# detect chirps
# two processes seems to be enough to keep up with realtime
# detects snippets of chirp transmissions everywhere in the band (0..25 MHz)
# records when snippets are found.
$MPIRUN -np 4 python3 detect_chirps.py $CONF_FILE > logs/detect.log 2>&1 &
echo "Starting detect_chirps.py"

# find timings
python3 find_timings.py $CONF_FILE > logs/timings.log 2>&1 &
echo "Starting find_timings.py"
# calculate ionograms
# seems like four parallel processes work.
# this means we can process four ionograms simultaneously!
$MPIRUN -np 4 python3 calc_ionograms.py $CONF_FILE > logs/ionograms.log 2>&1 &

echo "Starting calc_ionograms.py"

# plot ionograms
python3 plot_ionograms.py $CONF_FILE > logs/plot_ionograms.log &
echo "Starting plot_ionograms.py"

echo "Starting rx_uhd"
while true;
do
    echo "starting recorder"
    ./rx_uhd --outdir=$RINGBUFFER_DIR > logs/thor.log 2>&1 
    sleep 5
done


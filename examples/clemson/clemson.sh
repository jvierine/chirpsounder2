#!/bin/sh
#
# Create ionograms without knowledge of ionogram timings. Figure out the timings
# by listening to signals on the antenna.
# 

sysctl -w net.core.wmem_max=500000000
sysctl -w net.core.rmem_max=500000000
sysctl -w net.core.wmem_default=500000000
sysctl -w net.core.rmem_default=500000000


INSTALL_PATH=/home/sdr/stokes

# make sure this is the right mpirun command (you might need mpirun instead of mpirun.mpich)
MPIRUN=mpirun
CH0="ch0"
CH1="ch1"
# ram disk buffer for fast i/o.
# if you have a fast SSD or raid, you can also use that
RINGBUFFER_DIR=/mnt/ramdisk/hf25
# delete old data from ram disk
rm -rf $RINGBUFFER_DIR
SAMPLE_RATE=25e6
CENTER_FREQ=12.5e6
CONF_FILE=$INSTALL_PATH/clemson.ini

cd $INSTALL_PATH
./stop_ringbuffer.sh
mkdir -p logs

# ram disk size. this should be compatible than your ram size
# at least about 2 GB less than the amount of ram available.
RINGBUFFER_SIZE=88000MB

# start ring buffer
/home/sdr/.local/bin/drf ringbuffer -z $RINGBUFFER_SIZE $RINGBUFFER_DIR -p 2 > logs/ringbuffer.log 2>&1 &

echo "Starting ringbuffer"

# detect chirps
# two processes seems to be enough to keep up with realtime
$MPIRUN -np 4 python3 detect_chirps.py $CONF_FILE > logs/detect.log 2>&1 &
echo "Starting detect_chirps.py"

# find timings
python3 find_timings.py $CONF_FILE $CH0 > logs/timings.log 2>&1 &
python3 find_timings.py $CONF_FILE $CH1 > logs/timings.log 2>&1 &
echo "Starting find_timings.py"
# calculate ionograms
# seems like four parallel processes work.
# this means we can process four ionograms simultaneously!
$MPIRUN -np 4 python3 calc_ionograms2.py $CONF_FILE $CH0 > logs/ionograms.log 2>&1 &
$MPIRUN -np 4 python3 calc_ionograms2.py $CONF_FILE $CH1 > logs/ionograms.log 2>&1 &
echo "Starting calc_ionograms scripts"

# plot ionograms
python3 plot_ionograms.py $CONF_FILE > logs/plot_ionograms.log &
echo "Starting plot_ionograms.py"

# plot Stokes Parameter V
#python3 plot_stokesV.py $CONF_FILE > logs/plot_stokesV.log &
#echo "Starting plot_stokesV.py"

echo "Starting rx_uhd"
./rx_uhd --outdir=$RINGBUFFER_DIR > logs/thor.log 2>&1

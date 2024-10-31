#!/bin/sh
#
# Create ionograms without knowledge of ionogram timings. Script to demonstrate
# multichannel capability.
#
# run these commands once per boot to improve performance
# sysctl -w net.core.wmem_max=500000000
# sysctl -w net.core.rmem_max=500000000
# sysctl -w net.core.wmem_default=500000000
# sysctl -w net.core.rmem_default=500000000


INSTALL_PATH=/home/sdr/chirpsounder2

# make sure this is the right mpirun command (you might need mpirun instead of mpirun.mpich)
MPIRUN=mpirun

# ram disk buffer for fast i/o.
# if you have a fast SSD or raid, you can also use that
RINGBUFFER_DIR=/mnt/ramdisk/hf25
# delete old data from ram disk
rm -rf $RINGBUFFER_DIR
SAMPLE_RATE=25e6
CENTER_FREQ=12.5e6
CONF_FILE=$INSTALL_PATH/examples/clemson/clemson.ini
CLOCKARGS="addr0=192.168.10.2,addr1=192.168.10.4,recv_buff_size=500000000"

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
$MPIRUN -np 4 python3 detect_chirps.py $CONF_FILE > logs/detect.log 2>&1 &
echo "Starting detect_chirps.py"

# find timings
python3 find_timings.py $CONF_FILE > logs/timings.log 2>&1 &
echo "Starting find_timings.py"

# calculate ionograms
$MPIRUN -np 8 python3 calc_ionograms.py $CONF_FILE > logs/ionograms.log 2>&1 &
echo "Starting calc_ionograms scripts"

# plot ionograms
python3 plot_ionograms.py $CONF_FILE > logs/plot_ionograms.log &
echo "Starting plot_ionograms.py"

echo "Starting rx_uhd"
./rx_uhd --outdir=$RINGBUFFER_DIR --usrp_args=$CLOCKARGS --channels="0,1" > logs/thor.log 2>&1

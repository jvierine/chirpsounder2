#!/usr/bin/bash
#
# ntp-public.uit.no
# ntp.uit.no
#
# Create ionograms without knowledge of ionogram timings. Figure out the timings
# by listening to signals on the antenna.
# 
#

INSTALL_PATH=/home/j/src/chirpsounder2
# venv
source /home/j/src/chirpsounder2/.venv/bin/activate

# ram disk buffer for fast i/o.
# if you have a fast SSD or raid, you can also use that
RINGBUFFER_DIR=/dev/shm/hf25
SAMPLE_RATE=25e6
CENTER_FREQ=12.5e6
UHD_ARGS="addr0=192.168.10.2,recv_buff_size=500000000"
CONF_FILE=$INSTALL_PATH/examples/marieluise/kho.ini
DATA_MOUNT=/media/j/DATADRIVE1
OUTPUT_DIR=/data0

cd $INSTALL_PATH

if ! mountpoint -q "$DATA_MOUNT" || [ ! -w "$OUTPUT_DIR" ]; then
    echo "KHO data disk is not mounted and writable at $OUTPUT_DIR" >&2
    exit 1
fi

# kill possibly existing runtime
# stop all processes
./stop_ringbuffer.sh || true
# delete old data from ram disk
#rm -Rf $RINGBUFFER_DIR
mkdir -p $RINGBUFFER_DIR

mkdir -p logs

# ram disk size. this should be compatible than your ram size
# at least about 2 GB less than the amount of ram available.
#RINGBUFFER_SIZE=30000MB
echo "iono_housekeeping.py"
python3 iono_housekeeping.py --config $CONF_FILE > logs/housekeeping.log 2>&1 &

# All RF analysis and plot arrays are native Rust workers.
systemctl --user start iono-rust-digisonde.service iono-rust-lfm.service iono-rust-detect.service iono-rust-spectrum.service

echo "station_monitor.py"
python3 station_monitor.py --config $CONF_FILE > logs/station_monitor.log 2>&1 &

echo "Starting rx_uhd with internal GPSDO if available, otherwise external 1 PPS and 10 MHz. Restarting in 24 hours."
while true;
do
    # TBD: change cpp program so that ini file defined USRP setup!
    ./rx_uhd_ext_gps --outdir=$RINGBUFFER_DIR --usrp_args="$UHD_ARGS" > logs/kho.log 2>&1
    sleep 5
    echo "Restarting recording (every 24 hours)."
    echo "Rotating logs"
    logrotate $INSTALL_PATH/examples/marieluise/tgo-logrotate.conf -s logs/rotate.status
done

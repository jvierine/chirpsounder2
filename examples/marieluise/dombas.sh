#!/usr/bin/bash
#
# ntp-public.uit.no
# ntp.uit.no
#
# Create ionograms without knowledge of ionogram timings. Figure out the timings
# by listening to signals on the antenna.
# 
#

INSTALL_PATH=/home/tgo/src/chirpsounder2
# venv
source /home/tgo/ionosonde/bin/activate

# ram disk buffer for fast i/o.
# if you have a fast SSD or raid, you can also use that
RINGBUFFER_DIR=/dev/shm/hf25
SAMPLE_RATE=25e6
CENTER_FREQ=12.5e6
CONF_FILE=$INSTALL_PATH/examples/marieluise/dombas.ini

cd $INSTALL_PATH
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
systemctl --user start iono-rust-digisonde.service iono-rust-lfm.service iono-rust-detect.service iono-rust-spectrum.service iono-digisonde@juliusruh-day.service iono-digisonde@juliusruh-night.service iono-digisonde@dourbes.service iono-digisonde@dourbes-night.service iono-digisonde@dourbes-regular-05.service iono-digisonde@dourbes-night-regular-05.service iono-digisonde@dourbes-regular-10.service iono-digisonde@dourbes-night-regular-10.service iono-digisonde@chilton.service

echo "station_monitor.py"
python3 station_monitor.py --config $CONF_FILE > logs/station_monitor.log 2>&1 &

echo "Starting rx_uhd with external 1 PPS and 10 MHz. Restarting in 24 hours."
while true;
do
    # TBD: change cpp program so that ini file defined USRP setup!
    ./rx_uhd_ext_gps --outdir=$RINGBUFFER_DIR > logs/thor.log 2>&1 
    sleep 5
    echo "Restarting recording (every 24 hours)."
    echo "Rotating logs"
    logrotate $INSTALL_PATH/examples/marieluise/tgo-logrotate.conf -s logs/rotate.status
done

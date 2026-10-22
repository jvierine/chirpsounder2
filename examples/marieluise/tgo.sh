#!/usr/bin/bash
#
# ntp-public.uit.no
# ntp.uit.no
#
# Create ionograms without knowledge of ionogram timings. Figure out the timings
# by listening to signals on the antenna.
# 
#

INSTALL_PATH=/home/hfrx2/src/git/chirpsounder2
# venv
#source /home/tgo/ionosonde/bin/activate
# make sure this is the right mpirun command (you might need mpirun instead of mpirun.mpich)
MPIRUN=mpirun

# ram disk buffer for fast i/o.
# if you have a fast SSD or raid, you can also use that
RINGBUFFER_DIR=/dev/shm/hf25
SAMPLE_RATE=25e6
CENTER_FREQ=12.5e6
CONF_FILE=$INSTALL_PATH/examples/marieluise/tgo.ini

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
echo "sync_iono_data.py"
python3 sync_iono_data.py --config $CONF_FILE > logs/sync.log 2>&1 &
echo "iono_housekeeping.py"
python3 iono_housekeeping.py --config $CONF_FILE > logs/housekeeping.log 2>&1 &

# Experimental Rust DSP and HDF5 products. C++ UHD acquisition stays below.
# Each worker is supervised independently and follows the receiver service.
systemctl --user start iono-rust-detect.service iono-rust-digisonde.service \
    iono-rust-juliusruh-day.service iono-rust-juliusruh-night.service \
    iono-rust-lfm.service iono-rust-spectrum.service

# Native Rustplotlib images and reduced products are computed on this receiver.
systemctl --user start iono-local-plots.service

echo "station_monitor.py"
python3 -u station_monitor.py --config "$CONF_FILE" --period-s 60 \
    --process "recorder=rx_uhd_ext_gps|rx_uhd" \
    --process "detect_chirps=iono-receiver detect " \
    --process "receive_digisonde=iono-receiver digisonde " \
    --process "calc_ionograms=iono-receiver lfm " \
    --process "spectrum=iono-receiver spectrum " \
    --process "sync_iono_data=sync_iono_data.py" > logs/station_monitor.log 2>&1 &

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

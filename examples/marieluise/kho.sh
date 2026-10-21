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
# make sure this is the right mpirun command (you might need mpirun instead of mpirun.mpich)
MPIRUN=mpirun

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
echo "sync_iono_data.py"
python3 sync_iono_data.py --config $CONF_FILE > logs/sync.log 2>&1 &
echo "iono_housekeeping.py"
python3 iono_housekeeping.py --config $CONF_FILE > logs/housekeeping.log 2>&1 &

echo "detections2metadata.py"
python3 detections2metadata.py --config $CONF_FILE > logs/detections2metadata.log 2>&1 &

echo "receive_digisonde.py"
python3 receive_digisonde.py --config $CONF_FILE > logs/digisonde.log 2>&1 &
#python3 receive_digisonde.py --config $CONF_FILE --sounder Juliusruh > logs/digisonde_julius_day.log 2>&1 &
#python3 receive_digisonde.py --config $CONF_FILE --sounder JuliusruhN > logs/digisonde_julius_night.log 2>&1 &
#python3 receive_digisonde.py --config $CONF_FILE --sounder Chilton > logs/digisonde_chilton.log 2>&1 &
#python3 receive_digisonde.py --config $CONF_FILE --sounder Dourbes > logs/digisonde_dourbes.log 2>&1 &

echo "plot_rtf.py"
python3 plot_rtf.py --config $CONF_FILE > logs/plot_rtf.log 2>&1 &

echo "plot_detectionfiles.py"
python3 plot_detectionfiles.py --config $CONF_FILE > logs/plot_detectionfiles.log 2>&1 &

echo "plot_rf_spec.py"
bash supervise_worker.sh rf-spectrum-KHO python3 -u plot_rf_spec.py "$CONF_FILE" --refresh-sec 60 > logs/plot_rf_spec.log 2>&1 &

echo "detect_chirps.py"
python3 detect_chirps.py --config $CONF_FILE > logs/detect.log 2>&1 &

echo "calc_ionograms.py"
python3 calc_ionograms.py --config $CONF_FILE > logs/ionograms.log 2>&1 &

echo "plot_ionograms.py"
python3 plot_ionograms.py --config $CONF_FILE > logs/plot_ionograms.log 2>&1 &

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

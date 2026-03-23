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
./stop_ringbuffer.sh
# delete old data from ram disk
rm -Rf $RINGBUFFER_DIR

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
python3 receive_digisonde.py --config $CONF_FILE --sounder Juliusruh > logs/digisonde_julius_day.log 2>&1 &
python3 receive_digisonde.py --config $CONF_FILE --sounder JuliusruhN > logs/digisonde_julius_night.log 2>&1 &

#echo "plot_rtf.py"
#python3 plot_rtf.py --config examples/marieluise/tgo.ini --sounding_path SGO,TGO  > logs/plot_rtf_sgotgo.log 2>&1 &
#python3 plot_rtf.py --config examples/marieluise/tgo.ini --sounding_path Ramfjordmoen,TGO  > logs/plot_rtf_rfmtgo.log 2>&1 &

#echo "plot_detectionfiles.py"
#python3 plot_detectionfiles.py --config $CONF_FILE > logs/plot_detectionfiles.log 2>&1 &

echo "detect_chirps.py"
$MPIRUN -np 2 python3 detect_chirps.py --config $CONF_FILE > logs/detect.log 2>&1 &

echo "calc_ionograms.py"
python3 calc_ionograms.py --config $CONF_FILE > logs/ionograms.log 2>&1 &

echo "plot_ionograms.py"
python3 plot_ionograms.py --config $CONF_FILE > logs/plot_ionograms.log > logs/plot_ionograms.log 2>&1 &

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

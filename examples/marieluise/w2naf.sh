#!/usr/bin/bash
#
# W2NAF receive-only chirpsounder startup.
#
# This station initially records the 0-25 MHz HF band and runs
# detect_chirps.py for ROTHR/JORN-like 100 kHz and 125 kHz sounders.
#

set -euo pipefail

INSTALL_PATH=/home/hamsci/src/chirpsounder2
if [ -f "$INSTALL_PATH/.venv/bin/activate" ]; then
    source "$INSTALL_PATH/.venv/bin/activate"
fi

MPIRUN=mpirun
RINGBUFFER_DIR=/mnt/ramdisk/hf25
CONF_FILE=$INSTALL_PATH/examples/marieluise/w2naf.ini
LOGROTATE_CONF=$INSTALL_PATH/examples/marieluise/tgo-logrotate.conf
UHD_ARGS="addr0=192.168.10.2,recv_buff_size=500000000"

cd "$INSTALL_PATH"

./stop_ringbuffer.sh || true
mkdir -p "$RINGBUFFER_DIR"
mkdir -p /home/hamsci/data/ionosonde
mkdir -p logs

GPS_LOCK_TIMEOUT=$(python3 - <<PY
import chirp_config as cc
conf = cc.chirp_config("$CONF_FILE", verbose=False, build_fvec=False)
print(conf.gps_lock_timeout_sec if not conf.require_gps_lock else -1)
PY
)

echo "sync_iono_data.py"
python3 sync_iono_data.py --config "$CONF_FILE" > logs/sync.log 2>&1 &

echo "iono_housekeeping.py"
python3 iono_housekeeping.py --config "$CONF_FILE" > logs/housekeeping.log 2>&1 &

echo "detections2metadata.py"
python3 detections2metadata.py --config "$CONF_FILE" > logs/detections2metadata.log 2>&1 &

echo "plot_detectionfiles.py"
python3 plot_detectionfiles.py --config "$CONF_FILE" > logs/plot_detectionfiles.log 2>&1 &

echo "detect_chirps.py"
$MPIRUN -np 2 python3 detect_chirps.py --config "$CONF_FILE" > logs/detect.log 2>&1 &

echo "station_monitor.py"
python3 station_monitor.py --config "$CONF_FILE" > logs/station_monitor.log 2>&1 &

echo "Starting rx_uhd_ext_gps for W2NAF USRP N200 at 192.168.10.2. Restarting every 24 hours."
while true;
do
    ./rx_uhd_ext_gps --outdir="$RINGBUFFER_DIR" --usrp_args="$UHD_ARGS" --gps-lock-timeout="$GPS_LOCK_TIMEOUT" > logs/w2naf.log 2>&1
    sleep 5
    echo "Restarting recording."
    echo "Rotating logs"
    logrotate "$LOGROTATE_CONF" -s logs/rotate.status || true
done

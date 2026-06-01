#!/usr/bin/env bash
set -u

RINGBUFFER_DIR="${RINGBUFFER_DIR:-/dev/shm/hf25}"
STOP_PATTERNS=(
    "python.*sync_iono_data.py"
    "python.*iono_housekeeping.py"
    "python.*detections2metadata.py"
    "python.*receive_digisonde.py"
    "python.*plot_rtf.py"
    "python.*plot_detectionfiles.py"
    "python.*detect_chirps.py"
    "mpirun.*detect_chirps.py"
    "python.*calc_ionograms.py"
    "python.*plot_ionograms.py"
    "python.*station_monitor.py"
    "python.*find_timings"
    "python.*drf"
    "rx_uhd"
)

matching_pids() {
    local pattern="$1"
    local pid

    pgrep -f "$pattern" 2>/dev/null | while read -r pid; do
        if [[ -n "$pid" && "$pid" != "$$" ]]; then
            printf '%s\n' "$pid"
        fi
    done
}

stop_pattern() {
    local pattern="$1"
    local signal="$2"
    local pids

    pids="$(matching_pids "$pattern" | tr '\n' ' ')"
    if [[ -z "$pids" ]]; then
        return
    fi

    echo "Sending ${signal} to ${pattern}: ${pids}"
    kill "-${signal}" $pids 2>/dev/null || true
}

for pattern in "${STOP_PATTERNS[@]}"; do
    stop_pattern "$pattern" TERM
done

sleep 2

for pattern in "${STOP_PATTERNS[@]}"; do
    stop_pattern "$pattern" KILL
done

# remove remaining data in ringbuffer
rm -Rf "$RINGBUFFER_DIR"

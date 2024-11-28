#!/bin/bash

while true;
do
    echo "Starting THOR"
    # start digital rf acquisition with custom c++ program that uses the uhd driver directly, skipping gnuradio
    ./rx_uhd >logs/thor.log 2>&1
    sleep 10

    # copy digital rf from ram disk to permanent storage:
    rsync -av --remove-source-files --exclude=tmp*
    --progress /dev/shm/hf25/cha /chirpsounder2/data_out/hf25/
    sleep 1
done

#!/bin/bash

while true;
do
    echo "Starting THOR"
    # start digital rf acquisition with custom c++ program that uses the uhd driver directly, skipping gnuradio
    ./rx_uhd >logs/thor.log 2>&1
    sleep 10
done
    

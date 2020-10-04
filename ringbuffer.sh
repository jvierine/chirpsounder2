#!/usr/bin/bash


sudo ntpdate ntp.uit.no
thor.py -m 192.168.10.4 -d A:A -c cha -f 12.5e6 -r 25e6 /dev/shm/hf25 &
drf ringbuffer -z 2000MB /dev/shm/hf25 -p 2 

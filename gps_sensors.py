#!/usr/bin/env python3
#
# Demonstrate the use of internal gpsdo and set launch time on a N200.
# 11 Nov 2020 - Updated by W2NAF to work with GnuRadio 3.8.1
#
from gnuradio import gr, gru, uhd
import time, math, os
from time import strftime, gmtime
import datetime
import calendar
import signal, errno
import sys
import subprocess

u = uhd.usrp_source(
            ",".join(("ADDR=192.168.10.2", "")),
            uhd.stream_args(
                cpu_format="fc32",
                args='',
                channels=list(range(0,1)),
            ),
        )

u.set_clock_source("gpsdo")
print(u.get_mboard_sensor("gps_gpgga"))
print(u.get_mboard_sensor("gps_gprmc"))
print(u.get_mboard_sensor("gps_time"))
print(u.get_mboard_sensor("gps_locked"))  # is the gps clock locked?
print(u.get_mboard_sensor("gps_servo"))
print(u.get_mboard_sensor("ref_locked"))  
print(u.get_mboard_sensor("mimo_locked"))
tnow = u.get_time_last_pps().get_real_secs()
tstart = math.ceil(tnow)+10.0
print("Time of last PPS %1.2f Starting sampling at %1.2f"%(tnow,tstart))
#u.set_start_time(uhd.time_spec(tstart))
#ns = gr.null_sink(gr.sizeof_gr_complex)
#fg = gr.top_block()
#fg.connect((u,0),(ns,0))
#fg.start()
#while True:
#    time.sleep(1)

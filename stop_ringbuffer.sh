
# stop all processes doing realtime chirp analysis
kill `ps ax |grep detect_chirps |grep python |awk '{print $1}'|xargs`
kill `ps ax |grep find_timings |grep python |awk '{print $1}'|xargs`
kill `ps ax |grep thor.py|grep python |awk '{print $1}' |xargs`
kill `ps ax |grep plot_ionograms.py|grep python |awk '{print $1}' |xargs`
kill `ps ax |grep calc_ionograms.py|grep python |awk '{print $1}' |xargs`
kill `ps ax |grep drf|grep python |awk '{print $1}' |xargs`

# remove remaining data in ringbuffer
rm -Rf /dev/shm/hf25
kill `ps ax |grep rx_uhd |awk '{print $1}' |xargs`

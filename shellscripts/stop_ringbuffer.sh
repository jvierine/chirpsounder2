
# stop all processes doing realtime chirp analysis
kill `ps ax |grep detect_chirps |grep python |gawk '{print $1}'|xargs`
kill `ps ax |grep find_timings |grep python |gawk '{print $1}'|xargs`
kill `ps ax |grep thor.py|grep python |gawk '{print $1}' |xargs`
kill `ps ax |grep plot_ionograms.py|grep python |gawk '{print $1}' |xargs`
kill `ps ax |grep calc_ionograms.py|grep python |gawk '{print $1}' |xargs`
kill `ps ax |grep drf|grep python |gawk '{print $1}' |xargs`
# remove remaining data in ringbuffer
rm -Rf /dev/shm/hf25

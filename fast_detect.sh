#
# Off-line processing of a recording of HF obtained using thor.py
#
CONF_FILE=fast_detect.ini

# how many CPU cores do you have
N_CPUS=48
# we use 2 cores for each mpi process when calculating an ionogram
N_CPUS_I=`expr $N_CPUS / 2`


# find chirps with unknown chirp timings
mpirun -np $N_CPUS python3 detect_chirps.py $CONF_FILE

# cluster detections and find chirp soundings that
# are relatively certain to be chirp soundings
python3 find_timings.py $CONF_FILE

# calculate ionograms
mpirun -np $N_CPUS_I python3 calc_ionograms.py $CONF_FILE

# plot the calculated ionograms.
python3 plot_ionograms.py $CONF_FILE

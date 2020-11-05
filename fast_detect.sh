# how many CPU cores do you have
N_CPUS=4
# we use 2 cores for each mpi process when calculating an ionogram
N_CPUS_I=`expr $N_CPUS / 2`
echo $N_CPUS
echo $N_CPUS_I

CONF_FILE=fast_detect.ini

# find chirps with unknown chirp timings
mpirun -np $N_CPUS python3 detect_chirps.py $CONF_FILE

# cluster detections and find chirp soundings that
# are relatively certain to be chirp soundings
python3 find_timings.py $CONF_FILE

# calculate ionograms
mpirun -np $N_CPUS_I python3 calc_ionograms.py $CONF_FILE

python3 plot_ionograms.py $CONF_FILE

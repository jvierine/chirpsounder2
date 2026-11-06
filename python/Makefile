# Retired ctypes DSP helper; retained for historical reproducibility only.
libdownconvert.so: chirp_downconvert.c chirp_downconvert.h
	gcc -shared -fpic -O3 -march=native -ffast-math -fno-strict-aliasing -o $@ chirp_downconvert.c -pthread -lm
benchmark_downconvert: benchmark_downconvert.c chirp_downconvert.c chirp_downconvert.h
	gcc -O3 -march=native -ffast-math -fno-strict-aliasing -o $@ benchmark_downconvert.c chirp_downconvert.c -pthread -lm

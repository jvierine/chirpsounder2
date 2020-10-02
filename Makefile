all:
	#gcc -shared -fpic -O3  -o libdownconvert.so chirp_downconvert.c
	gcc -shared -fpic -fopt-info-vec  -O3 -mavx  -ffast-math -ftree-vectorizer-verbose=2  -ftree-vectorize  -o libdownconvert.so chirp_downconvert.c

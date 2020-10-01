all:
	gcc -shared -fpic -O3  -o libdownconvert.so chirp_downconvert.c

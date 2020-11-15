all:
	#gcc -shared -fpic -O3  -o libdownconvert.so chirp_downconvert.c
	gcc -shared -fpic -O3  -o libdownconvert.so chirp_downconvert.c -pthread
	g++ -L/usr/lib/x86_64-linux-gnu/hdf5/serial -I/usr/include/hdf5/serial -o rx_uhd rx_uhd.cpp -pthread -luhd -lboost_program_options -lboost_system -lboost_thread -lboost_date_time -lboost_regex -lboost_serialization -lhdf5 -ldigital_rf

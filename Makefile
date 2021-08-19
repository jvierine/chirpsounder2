all:
	#gcc -shared -fpic -O3  -o libdownconvert.so chirp_downconvert.c
	gcc -shared -fpic -O3  -o libdownconvert.so chirp_downconvert.c -pthread
	g++ `pkg-config --cflags uhd hdf5 digital_rf` -o rx_uhd rx_uhd.cpp -pthread  -lboost_program_options -lboost_system -lboost_thread -lboost_date_time -lboost_regex -lboost_serialization -ldigital_rf `pkg-config --libs uhd hdf5 digital_rf`

all: libdownconvert.so rx_uhd rx_uhd_ext_gps

libdownconvert.so: chirp_downconvert.c chirp_downconvert.h
	gcc -shared -fpic -O3 -march=native -ffast-math -fno-strict-aliasing -o libdownconvert.so chirp_downconvert.c -pthread -lm

benchmark_downconvert: benchmark_downconvert.c chirp_downconvert.c chirp_downconvert.h
	gcc -O3 -march=native -ffast-math -fno-strict-aliasing -o benchmark_downconvert benchmark_downconvert.c chirp_downconvert.c -pthread -lm

rx_uhd: rx_uhd.cpp
	g++ `pkg-config --cflags uhd hdf5 digital_rf` -o rx_uhd rx_uhd.cpp -pthread  -lboost_program_options -lboost_system -lboost_thread -lboost_date_time -lboost_regex -lboost_serialization -ldigital_rf `pkg-config --libs uhd hdf5 digital_rf`

rx_uhd_ext_gps: rx_uhd_ext_gps.cpp
	g++ `pkg-config --cflags uhd hdf5 digital_rf` -o rx_uhd_ext_gps rx_uhd_ext_gps.cpp -pthread  -lboost_program_options -lboost_system -lboost_thread -lboost_date_time -lboost_regex -lboost_serialization -ldigital_rf `pkg-config --libs uhd hdf5 digital_rf`

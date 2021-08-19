all:
	g++ \
	 ./rx_uhd.cpp \
	-o $(INSTALL_PATH) \
	-L/usr/lib/x86_64-linux-gnu/hdf5/serial \
	-I/usr/include/hdf5/serial \
	-pthread \
	-luhd \
	-lboost_program_options \
	-lboost_system \
	-lboost_thread \
	-lboost_date_time \
	-lboost_regex \
	-lboost_serialization \
	-lhdf5 \
	-ldigital_rf

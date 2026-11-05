// Setup-time UHD sensor readout; do not open a second multi_usrp while recording.
#include <uhd/usrp/multi_usrp.hpp>
#include <boost/property_tree/ptree.hpp>
#include <boost/property_tree/json_parser.hpp>
#include <boost/algorithm/string.hpp>
#include <hdf5.h>
#include <chrono>
#include <cmath>
#include <cstdio>
#include <iostream>
#include <thread>
#include <vector>

static bool coordinates(const std::string& sentence, double& lat, double& lon) {
    const auto star = sentence.find('*');
    if (sentence.empty() || sentence[0] != '$' || star == std::string::npos) return false;
    unsigned char checksum = 0;
    for (size_t i = 1; i < star; ++i) checksum ^= sentence[i];
    if (checksum != std::stoul(sentence.substr(star + 1), nullptr, 16)) return false;
    std::vector<std::string> fields;
    boost::split(fields, sentence.substr(0, star), boost::is_any_of(","));
    if (fields.size() < 10 || (fields[0] != "$GPGGA" && fields[0] != "$GNGGA") ||
        fields[6].empty() || std::stoi(fields[6]) == 0) return false;
    auto degrees = [](const std::string& text) {
        const double value = std::stod(text);
        if (!std::isfinite(value) || value < 0 || std::fmod(value, 100.) >= 60.)
            throw std::runtime_error("invalid GPS coordinate");
        return std::floor(value / 100.) + std::fmod(value, 100.) / 60.;
    };
    if ((fields[3] != "N" && fields[3] != "S") ||
        (fields[5] != "E" && fields[5] != "W")) return false;
    lat = degrees(fields[2]) * (fields[3] == "S" ? -1 : 1);
    lon = degrees(fields[4]) * (fields[5] == "W" ? -1 : 1);
    return std::abs(lat) <= 90 && std::abs(lon) <= 180;
}

static void write_location(const char* path, double lat, double lon, double epoch) {
    const std::string temporary = std::string(path) + ".partial";
    const hid_t file = H5Fcreate(temporary.c_str(), H5F_ACC_TRUNC, H5P_DEFAULT, H5P_DEFAULT);
    if (file < 0) throw std::runtime_error("cannot create station location HDF5");
    const hid_t scalar = H5Screate(H5S_SCALAR);
    bool ok = true;
    for (const auto& item : std::vector<std::pair<const char*, double>>{
             {"latitude_deg", lat}, {"longitude_deg", lon}, {"gps_time_unix", epoch}}) {
        const hid_t ds = H5Dcreate2(file, item.first, H5T_IEEE_F64LE, scalar,
                                   H5P_DEFAULT, H5P_DEFAULT, H5P_DEFAULT);
        if (ds < 0) { ok = false; break; }
        ok &= H5Dwrite(ds, H5T_NATIVE_DOUBLE, H5S_ALL, H5S_ALL, H5P_DEFAULT, &item.second) >= 0;
        H5Dclose(ds);
    }
    H5Sclose(scalar);
    ok &= H5Fclose(file) >= 0;
    if (!ok || std::rename(temporary.c_str(), path))
        throw std::runtime_error("cannot commit station location HDF5");
}

int main(int argc, char** argv) {
    try {
        const auto radio = uhd::usrp::multi_usrp::make(argc > 1 ? argv[1] : "addr=192.168.10.2");
        const bool wait_for_fix = argc > 3 && std::string(argv[3]) == "--wait";
        for (;;) {
        boost::property_tree::ptree result, sensors;
        result.put("queried_unix", std::chrono::duration_cast<std::chrono::seconds>(
            std::chrono::system_clock::now().time_since_epoch()).count());
        const auto info = radio->get_usrp_rx_info(0);
        for (const auto& name : info.keys())
            result.put("hardware." + name, info[name]);
        result.put("clock_source", radio->get_clock_source(0));
        result.put("time_source", radio->get_time_source(0));
        for (const auto& name : radio->get_mboard_sensor_names(0)) {
            try { sensors.put(name, radio->get_mboard_sensor(name, 0).value); }
            catch (const std::exception& e) { sensors.put(name + "_error", e.what()); }
        }
        result.add_child("sensors", sensors);
        double lat = 0, lon = 0;
        bool fix = false;
        try {
            const double gps_epoch = sensors.get<double>("gps_time");
            fix = sensors.get<std::string>("gps_locked") == "true" &&
                  std::abs(gps_epoch - result.get<double>("queried_unix")) <= 2 &&
                  coordinates(sensors.get<std::string>("gps_gpgga"), lat, lon);
            if (fix) {
                result.put("latitude_deg", lat);
                result.put("longitude_deg", lon);
                write_location("/data0/station-location.h5", lat, lon, gps_epoch);
            }
        } catch (const std::exception&) { fix = false; }
        result.put("valid_position_and_time", fix);
        if (argc > 2) {
            const std::string temporary = std::string(argv[2]) + ".partial";
            boost::property_tree::write_json(temporary, result);
            if (std::rename(temporary.c_str(), argv[2])) throw std::runtime_error("cannot commit UHD status");
        }
        else boost::property_tree::write_json(std::cout, result);
        if (!wait_for_fix || fix) break;
        std::cerr << "Waiting for valid GPS position, GPS lock and UTC time.\n";
        std::this_thread::sleep_for(std::chrono::seconds(10));
        }
        return 0;
    } catch (const std::exception& e) {
        std::cerr << e.what() << '\n';
        return 1;
    }
}

import serial
import time

ser = serial.Serial("/dev/ttyACM0", 9600, timeout=1)

gps_state = {
    "utc": None,
    "date": None,
    "fix": "NO",
    "fix_quality": "0",
    "fix_type": "1",
    "sats_used": "0",
    "sats_view": "0",
    "lat": None,
    "lon": None,
    "alt": None,
}


def parse_nmea(line):
    fields = line.split(",")

    if line.startswith("$GNRMC"):
        gps_state["utc"] = fields[1]
        gps_state["fix"] = "YES" if fields[2] == "A" else "NO"
        gps_state["date"] = fields[9]
        if fields[3]:
            gps_state["lat"] = f"{fields[3]} {fields[4]}"
        if fields[5]:
            gps_state["lon"] = f"{fields[5]} {fields[6]}"

    elif line.startswith("$GNGGA"):
        gps_state["utc"] = fields[1]
        gps_state["fix_quality"] = fields[6]
        gps_state["sats_used"] = fields[7]
        if fields[9]:
            gps_state["alt"] = fields[9] + " m"

    elif line.startswith("$GNGSA"):
        gps_state["fix_type"] = fields[2]

    elif line.startswith("$GPGSV"):
        gps_state["sats_view"] = fields[3]


def pretty_print():
    print(
        f"UTC: {gps_state['utc']} | "
        f"FIX: {gps_state['fix']} | "
        f"TYPE: {gps_state['fix_type']} | "
        f"USED: {gps_state['sats_used']} | "
        f"VIEW: {gps_state['sats_view']} | "
        f"LAT: {gps_state['lat']} | "
        f"LON: {gps_state['lon']} | "
        f"ALT: {gps_state['alt']}"
    )


last_print = 0

while True:
    line = ser.readline().decode(errors="ignore").strip()
    if not line:
        continue

    parse_nmea(line)

    now = time.time()
    if now - last_print > 2:
        pretty_print()
        last_print = now

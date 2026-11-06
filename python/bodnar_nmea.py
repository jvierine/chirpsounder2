import serial
import time

def parse_rmc(fields):
    return {
        "type": "RMC",
        "utc": fields[1],
        "status": "valid" if fields[2] == "A" else "no_fix",
        "lat": fields[3] + " " + fields[4] if fields[3] else None,
        "lon": fields[5] + " " + fields[6] if fields[5] else None,
        "speed_knots": fields[7] or None,
        "course": fields[8] or None,
        "date": fields[9] or None,
    }

def parse_gga(fields):
    return {
        "type": "GGA",
        "utc": fields[1],
        "lat": fields[2] + " " + fields[3] if fields[2] else None,
        "lon": fields[4] + " " + fields[5] if fields[4] else None,
        "fix_quality": fields[6],
        "sats_used": fields[7],
        "hdop": fields[8],
        "altitude_m": fields[9] or None,
    }

def parse_gsa(fields):
    used_prns = [f for f in fields[3:15] if f]
    return {
        "type": "GSA",
        "mode": fields[1],
        "fix_type": fields[2],   # 1=no fix, 2=2D, 3=3D
        "used_prns": used_prns,
        "pdop": fields[15] if len(fields) > 15 else None,
        "hdop": fields[16] if len(fields) > 16 else None,
        "vdop": fields[17].split("*")[0] if len(fields) > 17 else None,
    }

def parse_gsv(fields):
    sats_in_view = fields[3]
    sats = []
    for i in range(4, len(fields), 4):
        prn = fields[i] if i < len(fields) else ""
        elev = fields[i + 1] if i + 1 < len(fields) else ""
        az = fields[i + 2] if i + 2 < len(fields) else ""
        snr = fields[i + 3].split("*")[0] if i + 3 < len(fields) else ""
        if prn:
            sats.append({
                "prn": prn,
                "elevation": elev,
                "azimuth": az,
                "snr": snr or None,
            })
    return {
        "type": "GSV",
        "msg_num": fields[2],
        "total_msgs": fields[1],
        "sats_in_view": sats_in_view,
        "satellites": sats,
    }

def parse_nmea(line):
    if not line.startswith("$"):
        return None

    fields = line.strip().split(",")
    sentence = fields[0][3:]  # e.g. RMC, GGA, GSA, GSV

    if sentence == "RMC":
        return parse_rmc(fields)
    elif sentence == "GGA":
        return parse_gga(fields)
    elif sentence == "GSA":
        return parse_gsa(fields)
    elif sentence == "GSV":
        return parse_gsv(fields)
    return None

def to_int(value, default=0):
    try:
        return int(value)
    except (TypeError, ValueError):
        return default

def format_utc(utc_str):
    if not utc_str:
        return "--:--:--"
    try:
        # hhmmss.ss -> hh:mm:ss
        return f"{utc_str[0:2]}:{utc_str[2:4]}:{utc_str[4:6]}"
    except Exception:
        return utc_str

def format_date(date_str):
    if not date_str or len(date_str) != 6:
        return "--/--/--"
    # ddmmyy -> dd/mm/yy
    return f"{date_str[0:2]}/{date_str[2:4]}/{date_str[4:6]}"

state = {
    "utc": None,
    "date": None,
    "status": "no_fix",
    "lat": None,
    "lon": None,
    "speed_knots": None,
    "course": None,
    "fix_quality": "0",
    "sats_used": "0",
    "hdop": None,
    "altitude_m": None,
    "fix_type": "1",
    "used_prns": [],
    "pdop": None,
    "vdop": None,
    "sats_in_view": "0",
    "gsv_satellites": [],
    "pps_valid": False,
}

def update_state(parsed):
    t = parsed["type"]

    if t == "RMC":
        state["utc"] = parsed["utc"]
        state["date"] = parsed["date"]
        state["status"] = parsed["status"]
        state["lat"] = parsed["lat"]
        state["lon"] = parsed["lon"]
        state["speed_knots"] = parsed["speed_knots"]
        state["course"] = parsed["course"]

    elif t == "GGA":
        state["utc"] = parsed["utc"]
        state["lat"] = parsed["lat"] or state["lat"]
        state["lon"] = parsed["lon"] or state["lon"]
        state["fix_quality"] = parsed["fix_quality"]
        state["sats_used"] = parsed["sats_used"]
        state["hdop"] = parsed["hdop"]
        state["altitude_m"] = parsed["altitude_m"]

    elif t == "GSA":
        state["fix_type"] = parsed["fix_type"]
        state["used_prns"] = parsed["used_prns"]
        state["pdop"] = parsed["pdop"]
        state["hdop"] = parsed["hdop"] or state["hdop"]
        state["vdop"] = parsed["vdop"]

    elif t == "GSV":
        state["sats_in_view"] = parsed["sats_in_view"]
        state["gsv_satellites"] = parsed["satellites"]

    # PPS valid only when there is an actual valid fix
    state["pps_valid"] = (
        state["status"] == "valid" and
        to_int(state["fix_quality"]) >= 1 and
        to_int(state["fix_type"]) >= 2
    )

def pretty_print():
    gps_lock = "LOCKED" if state["status"] == "valid" else "NO FIX"
    pps = "VALID" if state["pps_valid"] else "INVALID"

    print("=" * 72)
    print(f"UTC Time     : {format_utc(state['utc'])}")
    print(f"Date         : {format_date(state['date'])}")
    print(f"GPS Status   : {gps_lock}")
    print(f"PPS          : {pps}")
    print(f"Fix Quality  : {state['fix_quality']}")
    print(f"Fix Type     : {state['fix_type']}   (1=no fix, 2=2D, 3=3D)")
    print(f"Sats Used    : {state['sats_used']}")
    print(f"Sats In View : {state['sats_in_view']}")
    print(f"Latitude     : {state['lat'] or '-'}")
    print(f"Longitude    : {state['lon'] or '-'}")
    print(f"Altitude     : {state['altitude_m'] or '-'} m")
    print(f"HDOP         : {state['hdop'] or '-'}")
    print(f"PDOP         : {state['pdop'] or '-'}")
    print(f"VDOP         : {state['vdop'] or '-'}")
    print(f"Used PRNs    : {', '.join(state['used_prns']) if state['used_prns'] else '-'}")

    if state["gsv_satellites"]:
        print("Satellites   :")
        for sat in state["gsv_satellites"]:
            print(
                f"  PRN {sat['prn']:>2} | "
                f"EL {sat['elevation'] or '-':>2} | "
                f"AZ {sat['azimuth'] or '-':>3} | "
                f"SNR {sat['snr'] or '-'}"
            )

ser = serial.Serial("/dev/ttyACM0", 9600, timeout=1)

last_print = 0
print_interval = 2.0  # seconds

while True:
    line = ser.readline().decode(errors="ignore").strip()
    if not line:
        continue

    parsed = parse_nmea(line)
    if not parsed:
        continue

    update_state(parsed)

    now = time.time()
    if now - last_print >= print_interval:
        pretty_print()
        last_print = now


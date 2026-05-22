#!/usr/bin/env python3
"""Write a lightweight station health JSON file for the web dashboard."""

from __future__ import annotations

import argparse
import fnmatch
import json
import os
import shutil
import tempfile
import time
from datetime import datetime, timezone

import chirp_config as cc
import chirpsounder_version as csversion

try:
    import digital_rf as drf
except ImportError:  # pragma: no cover - digital_rf is installed on stations
    drf = None

try:
    import psutil
except ImportError:  # pragma: no cover - psutil is in requirements.txt
    psutil = None

try:
    import requests
except ImportError:  # pragma: no cover - requests is in requirements.txt
    requests = None

DEFAULT_UPLOAD_URL = "http://4.235.86.214/upload.php"


DEFAULT_PROCESS_GROUPS = [
    "recorder=rx_uhd_ext_gps|rx_uhd|thor.py",
    "detect_chirps=detect_chirps.py",
    "detections2metadata=detections2metadata.py",
    "receive_digisonde=receive_digisonde.py",
    "calc_ionograms=calc_ionograms.py",
    "plot_ionograms=plot_ionograms.py",
    "plot_rtf=plot_rtf.py",
    "plot_detectionfiles=plot_detectionfiles.py",
    "sync_iono_data=sync_iono_data.py",
]


def utc_now_iso(now: float | None = None) -> str:
    if now is None:
        now = time.time()
    return datetime.fromtimestamp(now, timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def parse_process_group(value: str) -> tuple[str, list[str]]:
    if "=" in value:
        name, patterns = value.split("=", 1)
    else:
        name, patterns = value, value
    pattern_list = [pattern.strip() for pattern in patterns.split("|") if pattern.strip()]
    if not name.strip() or not pattern_list:
        raise ValueError("process groups must look like name=pattern|pattern")
    return name.strip(), pattern_list


def process_command_lines() -> list[str]:
    if psutil is None:
        return []
    commands = []
    for proc in psutil.process_iter(["pid", "cmdline"]):
        try:
            cmdline = proc.info.get("cmdline") or []
        except (psutil.NoSuchProcess, psutil.AccessDenied):
            continue
        if not cmdline:
            continue
        commands.append(" ".join(str(part) for part in cmdline))
    return commands


def check_processes(process_groups: list[tuple[str, list[str]]]) -> list[dict]:
    commands = process_command_lines()
    checks = []
    for name, patterns in process_groups:
        matches = [
            command
            for command in commands
            if any(pattern in command for pattern in patterns)
            and "station_monitor.py" not in command
        ]
        checks.append(
            {
                "name": name,
                "ok": bool(matches),
                "patterns": patterns,
                "count": len(matches),
            }
        )
    return checks


def newest_matching_file(root: str, patterns: tuple[str, ...]) -> tuple[str | None, float | None]:
    newest_path = None
    newest_mtime = None
    if not os.path.isdir(root):
        return newest_path, newest_mtime
    for dirpath, _, filenames in os.walk(root):
        for filename in filenames:
            if not any(fnmatch.fnmatch(filename, pattern) for pattern in patterns):
                continue
            path = os.path.join(dirpath, filename)
            try:
                mtime = os.path.getmtime(path)
            except OSError:
                continue
            if newest_mtime is None or mtime > newest_mtime:
                newest_path = path
                newest_mtime = mtime
    return newest_path, newest_mtime


def disk_status(path: str, label: str) -> dict:
    status = {"label": label, "path": path, "ok": False}
    try:
        usage = shutil.disk_usage(path)
    except OSError as exc:
        status["error"] = str(exc)
        return status
    used_fraction = usage.used / usage.total if usage.total else 1.0
    status.update(
        {
            "ok": True,
            "total_bytes": usage.total,
            "used_bytes": usage.used,
            "free_bytes": usage.free,
            "used_percent": 100.0 * used_fraction,
        }
    )
    return status


def digital_rf_ringbuffer_status(conf, max_age_s: float) -> dict | None:
    if drf is None or not os.path.isdir(conf.data_dir):
        return None

    now = time.time()
    channels = [str(channel) for channel in getattr(conf, "channel", [])]
    checks = []
    newest_sample_unix = None

    try:
        reader = drf.DigitalRFReader(conf.data_dir)
    except Exception as exc:
        return {"digital_rf_error": str(exc)}

    for channel in channels:
        try:
            bounds = reader.get_bounds(channel)
        except Exception as exc:
            checks.append({"channel": channel, "ok": False, "error": str(exc)})
            continue

        if bounds is None or len(bounds) < 2 or bounds[0] is None or bounds[1] is None:
            checks.append({"channel": channel, "ok": False, "bounds": None})
            continue

        end_sample = int(bounds[1])
        end_unix = end_sample / float(conf.sample_rate)
        age_s = now - end_unix
        newest_sample_unix = end_unix if newest_sample_unix is None else max(newest_sample_unix, end_unix)
        checks.append(
            {
                "channel": channel,
                "ok": age_s <= max_age_s,
                "bounds": [int(bounds[0]), end_sample],
                "newest_sample_unix": end_unix,
                "newest_age_s": age_s,
            }
        )

    if not checks:
        return None

    newest_age_s = None if newest_sample_unix is None else now - newest_sample_unix
    return {
        "newest_file": "DigitalRF:%s" % ",".join(channels),
        "newest_mtime": newest_sample_unix,
        "newest_age_s": newest_age_s,
        "digital_rf_channels": checks,
        "digital_rf_ok": all(check["ok"] for check in checks),
    }


def ringbuffer_status(conf, max_age_s: float, starting: bool = False) -> dict:
    now = time.time()
    digital_rf_status = digital_rf_ringbuffer_status(conf, max_age_s)
    if digital_rf_status is not None and "digital_rf_error" not in digital_rf_status:
        newest_path = digital_rf_status["newest_file"]
        newest_mtime = digital_rf_status["newest_mtime"]
        age_s = digital_rf_status["newest_age_s"]
    else:
        newest_path, newest_mtime = newest_matching_file(
            conf.data_dir,
            ("rf*.h5", "tmp*rf*.h5", "*.h5"),
        )
        age_s = None if newest_mtime is None else now - newest_mtime
    expected_sample_rate_hz = 25e6
    sample_rate_ok = abs(float(conf.sample_rate) - expected_sample_rate_hz) < 1.0
    fresh = age_s is not None and age_s <= max_age_s
    status = {
        "path": conf.data_dir,
        "ok": bool(os.path.isdir(conf.data_dir) and (fresh or starting) and sample_rate_ok),
        "starting": bool(starting and not fresh),
        "exists": os.path.isdir(conf.data_dir),
        "newest_file": newest_path,
        "newest_mtime": newest_mtime,
        "newest_age_s": age_s,
        "max_age_s": max_age_s,
        "sample_rate_hz": float(conf.sample_rate),
        "sample_rate_ok": sample_rate_ok,
    }
    if digital_rf_status is not None:
        status.update(digital_rf_status)
    return status


def output_status(conf, max_age_s: float) -> dict:
    now = time.time()
    newest_path, newest_mtime = newest_matching_file(
        conf.output_dir,
        ("chirp-*.h5", "cdetections-*.h5", "par-*.h5", "lfm_ionogram-*.h5", "*.png"),
    )
    age_s = None if newest_mtime is None else now - newest_mtime
    return {
        "path": conf.output_dir,
        "ok": age_s is not None and age_s <= max_age_s,
        "exists": os.path.isdir(conf.output_dir),
        "newest_file": newest_path,
        "newest_mtime": newest_mtime,
        "newest_age_s": age_s,
        "max_age_s": max_age_s,
    }


def build_status(
    conf,
    process_groups,
    ringbuffer_max_age_s,
    output_max_age_s,
    started_unix,
    startup_grace_s,
) -> dict:
    now = time.time()
    starting = (now - started_unix) < startup_grace_s
    processes = check_processes(process_groups)
    ringbuffer = ringbuffer_status(conf, ringbuffer_max_age_s, starting=starting)
    output = output_status(conf, output_max_age_s)
    disks = [
        disk_status(conf.data_dir, "RAM disk / ringbuffer"),
        disk_status(conf.output_dir, "Data disk"),
    ]
    checks_ok = (
        ringbuffer["ok"]
        and (output["ok"] or starting)
        and all(process["ok"] or starting for process in processes)
        and all(disk["ok"] for disk in disks)
    )
    return {
        "schema": "chirpsounder2.station_status.v1",
        "station": conf.station_name,
        **csversion.software_metadata(),
        "generated_unix": now,
        "generated_utc": utc_now_iso(now),
        "starting": starting,
        "started_unix": started_unix,
        "startup_grace_s": startup_grace_s,
        "ok": checks_ok,
        "ringbuffer": ringbuffer,
        "output": output,
        "processes": processes,
        "disks": disks,
    }


def write_json_atomic(path: str, payload: dict) -> None:
    directory = os.path.dirname(path) or "."
    os.makedirs(directory, exist_ok=True)
    fd, tmp_path = tempfile.mkstemp(prefix=".status-", suffix=".json", dir=directory)
    try:
        with os.fdopen(fd, "w") as handle:
            json.dump(payload, handle, indent=2, sort_keys=True)
            handle.write("\n")
        os.replace(tmp_path, path)
    finally:
        try:
            os.unlink(tmp_path)
        except FileNotFoundError:
            pass


def upload_json(path: str, upload_url: str, timeout: float) -> tuple[bool, str]:
    if requests is None:
        return False, "requests is not installed"
    try:
        with open(path, "rb") as handle:
            response = requests.post(upload_url, files={"file": handle}, timeout=timeout)
        body = response.text.strip()
        if response.ok:
            return True, body
        return False, "HTTP %d %s" % (response.status_code, body)
    except Exception as exc:
        return False, str(exc)


def default_output_path(status: dict) -> str:
    station = str(status["station"])
    generated_unix = float(status["generated_unix"])
    return "/tmp/station_status-%s-%.3f.json" % (station, generated_unix)


def main() -> int:
    parser = argparse.ArgumentParser(description="Write chirpsounder station health JSON.")
    parser.add_argument("--config", default="examples/marieluise/tgo.ini")
    parser.add_argument("--output", default=None)
    parser.add_argument("--upload-url", default=DEFAULT_UPLOAD_URL)
    parser.add_argument("--upload-timeout-s", type=float, default=30.0)
    parser.add_argument("--period-s", type=float, default=900.0)
    parser.add_argument("--once", action="store_true")
    parser.add_argument("--ringbuffer-max-age-s", type=float, default=120.0)
    parser.add_argument("--output-max-age-s", type=float, default=900.0)
    parser.add_argument("--startup-grace-s", type=float, default=300.0)
    parser.add_argument(
        "--process",
        action="append",
        default=None,
        help="Required process group as name=pattern|pattern. May be repeated.",
    )
    args = parser.parse_args()

    conf = cc.chirp_config(args.config)
    process_specs = args.process or DEFAULT_PROCESS_GROUPS
    process_groups = [parse_process_group(spec) for spec in process_specs]
    started_unix = time.time()

    while True:
        status = build_status(
            conf,
            process_groups,
            args.ringbuffer_max_age_s,
            args.output_max_age_s,
            started_unix,
            args.startup_grace_s,
        )
        output = args.output or default_output_path(status)
        write_json_atomic(output, status)
        print("%s wrote %s ok=%s" % (status["generated_utc"], output, status["ok"]))
        if args.upload_url:
            uploaded, message = upload_json(output, args.upload_url, args.upload_timeout_s)
            print(
                "%s upload %s: %s"
                % (status["generated_utc"], "ok" if uploaded else "failed", message)
            )
        if args.once:
            return 0 if status["ok"] else 1
        time.sleep(max(1.0, args.period_s))


if __name__ == "__main__":
    raise SystemExit(main())

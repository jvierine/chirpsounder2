import glob
import os
import re
import time
import chirp_config as cc
import ionowebsync

import argparse
parser = argparse.ArgumentParser(description="Upload latest dashboard plots to the web server.")
parser.add_argument(
    "--config",
    type=str,
    default="examples/marieluise/ramfjordmoen_digisonde.ini",
    help="Path to configuration file"
)
args = parser.parse_args()
conf=cc.chirp_config(args.config)
if conf.copy_to_server != True:
    print("Copy to server disabled in configuration. Exiting")
    exit(0)

posted_mtimes = {}


def should_upload(fname):
    base = os.path.basename(fname)
    match = re.match(r"^latest-lfm-unknown-(\d+)km-%s\.png$" % re.escape(conf.station_name), base)
    if match and not conf.serendipitous_range_start_allowed(float(match.group(1))):
        return False
    return True

while True:
    files = []
    for pattern in ("/tmp/latest*.png", "/tmp/yesterday*.png"):
        files.extend(glob.glob(pattern))
    for fname in sorted(set(files)):
        if not should_upload(fname):
            continue
        try:
            mtime = os.path.getmtime(fname)
        except FileNotFoundError:
            continue
        if posted_mtimes.get(fname) == mtime:
            continue
        response = ionowebsync.post_to_server(fname)
        if response is not None and response.ok:
            posted_mtimes[fname] = mtime
            print("posted %s" % fname)
        else:
            code = "no response" if response is None else "HTTP %d" % response.status_code
            print("failed to post %s: %s" % (fname, code))
    time.sleep(60)

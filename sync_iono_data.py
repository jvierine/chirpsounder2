import numpy as n
import os
import time
import chirp_config as cc

import argparse
parser = argparse.ArgumentParser(description="Sync data to jump host")
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
while True:
    os.system("rsync -av /tmp/latest*.png /tmp/roth*.png /tmp/yesterday*.png %s"%(conf.copy_destination))
    time.sleep(60)

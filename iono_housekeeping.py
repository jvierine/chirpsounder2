import chirp_config as cc
import os
import time

def housekeeping(conf):
    while True:
        if conf.ringbuffer_cleanup:
            print("cleaning files older than %d"%(conf.ringbuffer_max_age_min))
            print("run the following command")
            #find /dev/shm/hf25 -type f -name 'rf*h5' -mmin +5 -delete
            cmd="find %s -type f -mmin +%d -name 'rf*.h5' -delete"%(conf.data_dir,conf.ringbuffer_max_age_min)
            print(cmd)
            os.system(cmd)            
            cmd="find %s -type f -mmin +%d -name 'tmp*rf*.h5' -delete"%(conf.data_dir,conf.ringbuffer_max_age_min)
            print(cmd)            
            print("check the command and uncomment line starting with os.system to execute. disabled by default, as this is a dangerous command if you have the wrong data directory!")
            os.system(cmd)
            time.sleep(1)
        


if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser(description="Housekeeping program")
    parser.add_argument(
        "--config",
        type=str,
        default="examples/marieluise/tgo.ini",
        help="Path to configuration file"
    )
    args = parser.parse_args()
    conf=cc.chirp_config(args.config)
    housekeeping(conf)

import numpy as n
import digital_rf as drf
import os
import glob
import time
import sys
import chirp_config as cc
import chirp_det as cd
import h5py
import re
import psutil
import ionowebsync

p = psutil.Process()
# Set I/O priority to idle (lowest) to avoid interrupting realtime processes
p.ionice(psutil.IOPRIO_CLASS_IDLE)
p.nice(19)

            
def consolidate_files():
    import argparse
    parser = argparse.ArgumentParser(description="Plot range-time-frequency")
    parser.add_argument(
        "--config",
        type=str,
        default="examples/marieluise/ramfjordmoen_digisonde.ini",
        help="Path to configuration file"
    )
    args = parser.parse_args()
    conf = cc.chirp_config(args.config)

    data_dir = conf.output_dir
    fl=glob.glob("%s/*/chirp-*.h5"%(data_dir))
    fl.sort()
    chirptimes=[]
    # figure out time for each file
    for f in fl:
        secs=int(re.search(".*/chirp-.*-.*-([0-9]+)-([0-9]+).h5",f).group(1))/25e6
#        print(secs)
        chirptimes.append(secs)

    chirptimes=n.array(chirptimes)
    print("found ",len(chirptimes)," files")
    #channel                  Dataset {SCALAR}
    #chirp_rate               Dataset {SCALAR}
    #chirp_time               Dataset {SCALAR}
    #f0                       Dataset {SCALAR}
    #i0                       Dataset {SCALAR}
    #n_samples                Dataset {SCALAR}
    #sample_rate              Dataset {SCALAR}
    #snr                      Dataset {SCALAR}
    # conf.station_name

    # ignore ten last files, as they might be written in
#    print("gathering detections")

    # consolidate all detections in each minute to one file
    detections=[]
    files=[]

    # 15 minutes per file
    dt=60*15
    fidx=n.argsort(chirptimes)

    # we are still building this file. don't write it, as the detections into this file are still coming in
    current_file_idx=int(n.floor(time.time()/dt))
    prev_file_idx=-1
    
    for i in range(len(fl)-4):
        fname=fl[fidx[i]]
        try:
            h=h5py.File(fname,"r")
            chirp_rate=h["chirp_rate"][()]
            chirp_time=h["chirp_time"][()]
            f0=h["f0"][()]
            i0=h["i0"][()]
            snr=h["snr"][()]
            data_file_idx=int(n.floor(i0/25e6/dt))
            h.close()
        except:
            print("bad file %s"%(fname))
            continue

        # if we advance to next file, write everything out
        # don't write the current file yet as all the data might not be in yet
        if (data_file_idx != prev_file_idx) and data_file_idx != current_file_idx:
            m0=prev_file_idx*dt
            ofname="%s/%s/cdetections-%s-%d.h5"%(data_dir,cd.unix2dirname(m0),conf.station_name,m0)
            if len(detections) > 0:
                print("block %d writing %d detections %s"%(m0,len(detections),ofname))
                ho=h5py.File(ofname,"w")
                data = n.array(detections)
                ho["data"]=data
                ho.close()

                ionowebsync.post_to_server(ofname)
                
                # allocate new datastructures
                detections=[]
                # we have now consolidated everything into one file. delete individual files
                if data_file_idx != current_file_idx:
                    print("deleting files %d from previous block, which is now finished"%(len(files)))
                    for fi in range(len(files)):
                        # deleting file that is consolidated
                        os.system("rm %s"%(files[fi]))
                files=[]
        detections.append([chirp_time,i0/25e6,f0,chirp_rate,snr])
        files.append(fname)
        prev_file_idx=data_file_idx
    # write the tmp file
    m0=prev_file_idx*dt
    ofname="%s/%s/cdetections-%s-%d.h5"%(data_dir,cd.unix2dirname(m0),conf.station_name,m0)
    if len(detections) > 0:
        print("block %d writing %d detections %s"%(m0,len(detections),ofname))
        ho=h5py.File(ofname,"w")
        data = n.array(detections)
        ho["data"]=data
        ho.close()
        ionowebsync.post_to_server(ofname)        

    
if __name__ == "__main__":
    while True:
        consolidate_files()
        time.sleep(60)

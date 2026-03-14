import numpy as n
import digital_rf as drf
import os
import glob
import time
import sys
import chirp_config as cc
import chirp_det as cd
import h5py


def consolidate_files():
    if len(sys.argv) == 2:
        conf = cc.chirp_config(sys.argv[1])
    else:
        print('No config provided - Using defaults')
        conf = cc.chirp_config()

    data_dir = conf.output_dir
    fl=glob.glob("%s/*/chirp-*.h5"%(data_dir))
    fl.sort()

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
    print("gathering detections")
    current_minute=-1
    # consolidate all detections in each minute to one file
    detections=[]
    files=[]

    # 15 minutes per file
    dt=60*15
    for i in range(len(fl)-4):
        h=h5py.File(fl[i],"r")
        chirp_rate=h["chirp_rate"][()]
        chirp_time=h["chirp_time"][()]

        f0=h["f0"][()]
        i0=h["i0"][()]
        snr=h["snr"][()]
        data_minute=int(n.round(i0/25e6/dt))
        h.close()

        if (data_minute != current_minute):
            m0=current_minute*dt
            ofname="%s/%s/cdetections-%d.h5"%(data_dir,cd.unix2dirname(m0),m0)
            if len(detections) > 0:
                print("block %d writing %d detections %s"%(m0,len(detections),ofname))
                ho=h5py.File(ofname,"w")
                data = n.array(detections)
                ho["data"]=data

                ho.close()
                # allocate new datastructures
                detections=[]
                # if more than 1 hour old, delete old files
                tnow=time.time()
                if tnow-m0>(3600):
                    print("more than two hours old. deleting individual detection files")
                    for fi in range(len(files)):
                        # deleting file that is consolidated
                        os.system("rm %s"%(files[fi]))
                files=[]

        detections.append([chirp_time,i0/25e6,f0,chirp_rate,snr])
        files.append(fl[i])
        current_minute=data_minute
    
if __name__ == "__main__":
    while True:
        consolidate_files()
        time.sleep(60)

    
    

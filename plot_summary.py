#!/usr/bin/env python
import time
import os
import sys
import chirp_det as cd
import chirp_config as cc
import scipy.constants as c
import h5py
import glob
import matplotlib.pyplot as plt
import numpy as n
import matplotlib
matplotlib.use('Agg')


def summary_plots(conf, t0):

    fl = glob.glob("%s/%s/*.h5" % (conf.output_dir, cd.unix2dirname(t0)))
    fl.sort()
    n_ionograms = len(fl)
    h = h5py.File(fl[0], "r")
    freqs = n.copy(h["freqs"][()])
    ranges = h["ranges"][()]
    S = h["S"][()]

    ft0 = float(n.copy(h[("t0")]))
    dt = (ft0 - n.floor(ft0))
    dr = dt * c.c / 1e3
    # converted to one-way travel time
    range_gates = dr + ranges / 1e3

    SH = n.zeros([n_ionograms, S.shape[0]])
    SV = n.zeros([n_ionograms, S.shape[1]])

    dayno = n.floor(t0 / (24.0 * 3600.0))
    day_t0 = dayno * 24 * 3600.0

    cid = int(n.copy(h[("id")]))  # ionosonde id
    img_fname_r = "%s/%s/rstack-%03d-%1.2f.png" % (
        conf.output_dir, cd.unix2dirname(day_t0), cid, day_t0)
    img_fname_f = "%s/%s/fstack-%03d-%1.2f.png" % (
        conf.output_dir, cd.unix2dirname(day_t0), cid, day_t0)

    hours = n.zeros(n_ionograms)

    h.close()

    # tbd time of day
    for fi, f in enumerate(fl):
        h = h5py.File(f, "r")
        S = h["S"][()]
        filet0 = h["t0"][()]
        for i in range(S.shape[0]):
            noise = n.nanmedian(S[i, :])
            S[i, :] = (S[i, :] - noise) / noise
        S[S <= 0.0] = 1e-3
        SH[fi, :] = n.max(S, axis=1)
        SV[fi, :] = n.max(S, axis=0)
        hours[fi] = (filet0 - day_t0) / 3600.0
        h.close()
    dBH = 10.0 * n.log10(SH.T)
#    dBH=dBH-n.median(dBH)

    fig = plt.figure(figsize=(1.5 * 8, 1.5 * 6))
    nfloor = n.median(dBH)
    plt.pcolormesh(hours, freqs / 1e6, dBH, vmin=nfloor -
                   3, vmax=20 + nfloor, cmap="inferno")
    plt.colorbar()
    plt.title("Range stack\n%s %s" %
              (conf.station_name, cd.unix2datestr(day_t0)))
    plt.xlabel("Time (UTC hour of day)")
    plt.ylabel("Frequency (MHz)")
    plt.xlim([0, 24])

    plt.tight_layout()
    plt.savefig(img_fname_r)
    fig.clf()
    plt.clf()
    plt.close("all")
  #  if conf.copy_to_server:
   #     os.system("rsync -av %s %s/latest_rstack_%s.png"%(img_fname_r,conf.copy_destination,conf.station_name))

    dBV = 10.0 * n.log10(SV.T)
    nfloor = n.median(dBV)
    fig = plt.figure(figsize=(1.5 * 8, 1.5 * 6))
    plt.pcolormesh(hours, range_gates, dBV, vmin=nfloor -
                   3, vmax=20 + nfloor, cmap="inferno")
    plt.colorbar()
    plt.title("Frequency stack\n%s %s" %
              (conf.station_name, cd.unix2datestr(day_t0)))
    plt.xlabel("Time (UTC hour of day)")
    plt.ylabel("One-way virtual range (km)")
    plt.xlim([0, 24])

    plt.tight_layout()
    plt.savefig(img_fname_f)
    plt.clf()
    fig.clf()
    plt.close("all")
#    if conf.copy_to_server:
 #       os.system("rsync -av %s %s/latest_fstack_%s.png"%(img_fname_f,conf.copy_destination,conf.station_name))


def summary_page(conf, t0):
    rsfl = glob.glob("%s/2*/rstack*.png" % (conf.output_dir))
    rsfl.sort()
    fsfl = glob.glob("%s/2*/fstack*.png" % (conf.output_dir))
    fsfl.sort()

    print(fsfl)
    if conf.copy_to_server:
        try:
            os.system("rsync -av %s %s/latest_fstack_%s.png" %
                      (fsfl[len(fsfl) - 1], conf.copy_destination, conf.station_name))
        except:
            print("failed to copy latest fstack")
        try:
            os.system("rsync -av %s %s/previous_fstack_%s.png" %
                      (fsfl[len(fsfl) - 2], conf.copy_destination, conf.station_name))
        except:
            print("failed to copy previous fstack")
        try:
            os.system("rsync -av %s %s/latest_rstack_%s.png" %
                      (rsfl[len(rsfl) - 1], conf.copy_destination, conf.station_name))
        except:
            print("failed to copy latest rstack")
        try:
            os.system("rsync -av %s %s/previous_rstack_%s.png" %
                      (rsfl[len(rsfl) - 2], conf.copy_destination, conf.station_name))
        except:
            print("failed to copy previous rstack")


def summary(conf, t0):
    summary_plots(conf, t0)
    summary_page(conf, t0)


if __name__ == "__main__":
    if len(sys.argv) == 2:
        conf = cc.chirp_config(sys.argv[1])
    else:
        print("one argument with configuration file needed")
        exit(0)

    # 30 minutes delayed to ensure full day
    summary(conf, time.time() - 1800)

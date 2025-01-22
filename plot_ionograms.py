#!/usr/bin/env python
import re
import traceback
import time
import os
import os.path
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

def kill(conf):
    exists = os.path.isfile(conf.kill_path)
    return exists

def plot_ionogram(conf, fn, normalize_by_frequency=True):
    ho = h5py.File(fn, "r")
    t0 = float(n.copy(ho[("t0")]))
    ch = n.string_(ho[("ch")])
    ch = ch.decode('utf-8')
    if not "id" in ho.keys():
        print("id not in keys")
        return
    cid = int(n.copy(ho[("id")]))  # ionosonde id

    img_fname = "%s/%s/lfm_ionogram-%s-%03d-%1.2f.png" % (
        conf.output_dir, cd.unix2dirname(t0), ch, cid, t0)
    if os.path.exists(img_fname):
        print("Ionogram plot %s already exists. Skipping" % (img_fname))
        ho.close()
        return

    print("Plotting %s rate %1.2f (kHz/s) t0 %1.5f (unix)" %
          (fn, float(n.copy(ho[("rate")])) / 1e3, float(n.copy(ho[("t0")]))))
    # ionogram frequency-range
    S = n.copy(n.array(ho[("S")], dtype=n.float64))
    freqs = n.copy(ho[("freqs")])  # frequency bins
    ranges = n.copy(ho[("ranges")])  # range gates

    if normalize_by_frequency:
        for i in range(S.shape[0]):
            noise = n.nanmedian(S[i, :])
            S[i, :] = (S[i, :] - noise) / noise
        S[S <= 0.0] = 1e-3

    max_range_idx = n.argmax(n.max(S, axis=0))

    dB = n.transpose(10.0 * n.log10(S))
    if normalize_by_frequency == False:
        dB = dB - n.nanmedian(dB)

    dB[n.isnan(dB)] = 0.0
    dB[n.isfinite(dB) != True] = 0.0

    # assume that t0 is at the start of a standard unix second
    # therefore, the propagation time is anything added to a full second

    dt = (t0 - n.floor(t0))
    dr = dt * c.c / 1e3
    # converted to one-way travel time
    range_gates = dr + ranges / 1e3
    r0 = range_gates[max_range_idx]
    fig = plt.figure(figsize=(1.5 * 8, 1.5 * 6))
    plt.pcolormesh(freqs / 1e6, range_gates, dB,
                   vmin=-3, vmax=30.0, cmap="inferno")
    cb = plt.colorbar()
    cb.set_label("SNR (dB)")
    plt.title("%s Chirp-rate %1.2f kHz/s t0=%1.5f (unix s)\n%s %s (UTC)" % (
        ch, float(n.copy(ho[("rate")])) / 1e3, float(n.copy(ho[("t0")])), conf.station_name, cd.unix2datestr(float(n.copy(ho[("t0")])))))
    plt.xlabel("Frequency (MHz)")
    plt.ylabel("One-way range offset (km)")
    if conf.manual_range_extent:
        plt.ylim([conf.min_range / 1e3, conf.max_range / 1e3])
    else:
        plt.ylim([dr - conf.max_range_extent / 1e3,
                 dr + conf.max_range_extent / 1e3])

#    plt.ylim([dr-1000.0,dr+1000.0])
    if conf.manual_freq_extent:
        plt.xlim([conf.min_freq / 1e6, conf.max_freq / 1e6])
    else:
        plt.xlim([0, conf.maximum_analysis_frequency / 1e6])
    plt.tight_layout()
    plt.savefig(img_fname)
    fig.clf()
    plt.clf()
    plt.close("all")
    import gc
    gc.collect()
    ho.close()
    sys.stdout.flush()
    if conf.copy_to_server:
        os.system("rsync -av %s %s/latest_%s.png" %
                  (img_fname, conf.copy_destination, conf.station_name))


if __name__ == "__main__":
    if len(sys.argv) == 2:
        conf = cc.chirp_config(sys.argv[1])
    else:
        conf = cc.chirp_config()

    if conf.realtime:
        while True:
            if kill(conf):
                print("kill.txt found, stopping plot_ionograms.py")
                sys.exit(0)
            else:
                fl = glob.glob("%s/*/lfm*.h5" % (conf.output_dir))
                fl.sort()
                t_now = time.time()
                # avoid last file to make sure we don't read and write simultaneously
                for fn in fl[0:(len(fl) - 1)]:
                    try:
                        t_file = float(
                            re.search(".*-(1............).h5", fn).group(1))
                        # new enough file
                        if t_now - t_file < 48 * 3600.0:
                            plot_ionogram(conf, fn)

                    except:
                        print("error with %s" % (fn))
                        print(traceback.format_exc())
                time.sleep(10)
    else:
        fl = glob.glob("%s/*/lfm*.h5" % (conf.output_dir))
        for fn in fl:
            try:
                plot_ionogram(conf, fn)
            except:
                print("error with %s" % (fn))
                print(traceback.format_exc())
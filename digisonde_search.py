#!/usr/bin/env python
#
# Realtime search for digisonde complementary-code pairs.
#
import argparse
import contextlib
import datetime
import io
import os
import os.path
import sys
import time
import traceback

import digital_rf as drf
import h5py
import numpy as n
from mpi4py import MPI

import chirp_config as cc
import digisonde_stuff as ds


comm = MPI.COMM_WORLD
size = comm.Get_size()
rank = comm.Get_rank()


def kill(conf):
    return os.path.isfile(conf.kill_path)


def unix2date(x):
    return datetime.datetime.utcfromtimestamp(float(x))


def unix2datestr(x):
    return unix2date(x).strftime('%Y-%m-%d %H:%M:%S')


def unix2dirname(x):
    return unix2date(x).strftime('%Y-%m-%d')


def decimate_average(x, dec):
    trim_len = x.shape[-1] - (x.shape[-1] % dec)
    if trim_len != x.shape[-1]:
        x = x[..., :trim_len]
    return x.reshape((-1, dec)).mean(axis=-1)


def make_code_pair(search_sample_rate, ipp_s):
    code, codes, modes = ds.complementary_code(
        sr=search_sample_rate,
        ipp=ipp_s,
        mode=0,
        all_codes=False)
    pair_len = int(round(2.0 * ipp_s * search_sample_rate))
    pair = n.array(code[:pair_len], dtype=n.complex64)
    energy = n.sum(n.abs(pair)**2.0)
    if energy == 0:
        raise ValueError("empty complementary-code template")
    return pair, energy, codes, modes


class digisonde_code_search:
    def __init__(self, conf, args):
        self.conf = conf
        self.search_sample_rate = float(args.search_sample_rate)
        self.ipps = [float(v) * 1e-3 for v in args.ipps_ms]
        self.block_s = float(args.block_ms) * 1e-3
        self.block_samples = int(round(self.block_s * self.conf.sample_rate))
        self.max_pair_s = 2.0 * max(self.ipps)
        self.raw_len = int(round(self.max_pair_s * self.conf.sample_rate))
        self.threshold_snr = float(args.threshold_snr)
        self.store_all = args.store_all
        self.verbose = args.verbose
        self.max_lag_s = float(args.max_lag_s)
        self.dec = int(round(self.conf.sample_rate / self.search_sample_rate))
        self.raw_t = n.arange(self.raw_len, dtype=n.float64) / self.conf.sample_rate

        if abs(self.conf.sample_rate / self.dec - self.search_sample_rate) > 1e-6:
            raise ValueError("sample-rate/search-sample-rate must be an integer")

        freqs = n.arange(args.freq_start, args.freq_stop + 0.5 * args.freq_step, args.freq_step)
        if args.max_frequencies is not None:
            if args.max_frequencies < 25:
                raise ValueError("--max-frequencies must be at least 25")
            freqs = freqs[:args.max_frequencies]
        if len(freqs) < 25:
            raise ValueError("digisonde search requires at least 25 frequencies")
        self.freqs = n.array(freqs, dtype=n.float64)
        self.rank_freqs = self.freqs[n.arange(len(self.freqs)) % size == rank]

        self.templates = {}
        for ipp_s in self.ipps:
            self.templates[ipp_s] = make_code_pair(self.search_sample_rate, ipp_s)

        if self.verbose and rank == 0:
            print("digisonde search frequencies %1.3f-%1.3f MHz step %1.1f kHz (%d total)" %
                  (self.freqs[0] / 1e6, self.freqs[-1] / 1e6,
                   args.freq_step / 1e3, len(self.freqs)))
            print("MPI ranks %d, block %1.1f ms, max lag %1.1f s" %
                  (size, 1e3 * self.block_s, self.max_lag_s))
        if self.verbose:
            print("%d/%d searching %d frequencies" % (rank + 1, size, len(self.rank_freqs)))

    def next_realtime_block(self, d, ch, block_idx):
        bounds = d.get_bounds(ch)
        readable_end = bounds[1] - self.raw_len
        if readable_end <= bounds[0]:
            return None, bounds

        first_block = int(n.ceil(bounds[0] / self.block_samples))
        last_block = int(n.floor(readable_end / self.block_samples))
        if block_idx is None or block_idx < first_block:
            block_idx = first_block

        lag_s = (readable_end - block_idx * self.block_samples) / self.conf.sample_rate
        if lag_s > self.max_lag_s:
            skipped_to = int(n.floor((readable_end - self.max_lag_s * self.conf.sample_rate) /
                                     self.block_samples))
            skipped_to = max(first_block, skipped_to)
            if self.verbose and skipped_to > block_idx and rank == 0:
                print("realtime catch-up: skipping %d 10 ms blocks" % (skipped_to - block_idx))
            block_idx = skipped_to

        if block_idx > last_block:
            return None, bounds
        return block_idx, bounds

    def score_pair(self, z, ipp_s):
        template, energy, codes, modes = self.templates[ipp_s]
        pair_len = len(template)
        seg = z[:pair_len]
        noise_power = n.median(n.abs(seg)**2.0) / n.log(2.0) + 1e-20
        corr = n.vdot(template, seg)
        snr = (n.abs(corr)**2.0) / (energy * noise_power)
        return float(snr), corr, pair_len

    def store_detection(self, ch, i0, freq, ipp_s, snr, corr, z, pair_len):
        t0 = i0 / self.conf.sample_rate
        dname = "%s/%s" % (self.conf.output_dir, unix2dirname(t0))
        if not os.path.exists(dname):
            try:
                os.mkdir(dname)
            except OSError:
                pass

        ipp_ms = int(round(1e3 * ipp_s))
        ofname = "%s/digisonde-search-%s-%dms-%07dkHz-%d.h5" % (
            dname, ch, ipp_ms, int(round(freq / 1e3)), int(i0))
        if os.path.exists(ofname):
            return ofname

        with h5py.File(ofname, "w") as ho:
            ho["type"] = "digisonde_search"
            ho["channel"] = ch
            ho["i0"] = int(i0)
            ho["t0"] = t0
            ho["sample_rate"] = self.conf.sample_rate
            ho["search_sample_rate"] = self.search_sample_rate
            ho["center_freq"] = self.conf.center_freq
            ho["f0"] = float(freq)
            ho["ipp"] = float(ipp_s)
            ho["ipp_ms"] = ipp_ms
            ho["code_pair_index"] = 0
            ho["snr"] = float(snr)
            ho["corr_real"] = float(n.real(corr))
            ho["corr_imag"] = float(n.imag(corr))
            ho["n_samples"] = int(self.raw_len)
            ho["pair_samples"] = int(pair_len)
            ho["receiver"] = self.conf.station_name
            ho.create_dataset(
                "baseband",
                data=n.array(z[:pair_len], dtype=n.complex64),
                compression="gzip",
                compression_opts=3,
                shuffle=True)
        return ofname

    def print_detection(self, ch, i0, freq, ipp_s, snr, corr, ofname):
        print("%d/%d digisonde candidate %s ch=%s freq=%1.3f MHz ipp=%d ms snr=%1.2f corr=%1.3e%+1.3ej file=%s" %
              (rank + 1, size,
               unix2datestr(i0 / self.conf.sample_rate),
               ch, freq / 1e6, int(round(1e3 * ipp_s)), snr,
               n.real(corr), n.imag(corr), ofname))

    def search_block(self, d, ch, block_idx):
        i0 = block_idx * self.block_samples
        raw = d.read_vector_c81d(i0, self.raw_len, ch)
        detections = 0

        for freq in self.rank_freqs:
            osc = n.exp(-1j * 2.0 * n.pi * (freq - self.conf.center_freq) * self.raw_t)
            z = decimate_average(n.array(raw * osc, dtype=n.complex64), self.dec)

            for ipp_s in self.ipps:
                snr, corr, pair_len = self.score_pair(z, ipp_s)
                confident = snr >= self.threshold_snr
                if self.store_all or confident:
                    ofname = self.store_detection(ch, i0, freq, ipp_s, snr, corr, z, pair_len)
                if confident:
                    self.print_detection(ch, i0, freq, ipp_s, snr, corr, ofname)
                    detections += 1
        return detections


def scan(conf, searcher, block0=None):
    d = drf.DigitalRFReader(conf.data_dir)
    channelnames = d.get_channels()
    if len(channelnames) == 0:
        print("no channels")
        return block0
    ch = conf.channel[0] if len(conf.channel) > 0 else channelnames[0]
    if ch not in channelnames:
        if rank == 0:
            print("configured channel %s not found, using %s" % (ch, channelnames[0]))
        ch = channelnames[0]

    try:
        bounds = d.get_bounds(ch)
    except Exception:
        print("no data")
        return block0

    readable_end = bounds[1] - searcher.raw_len
    if readable_end <= bounds[0]:
        return block0

    if block0 is None:
        block0 = int(n.ceil(bounds[0] / searcher.block_samples))
    block1 = int(n.floor(readable_end / searcher.block_samples))

    for block_idx in range(block0, block1 + 1):
        cput0 = time.time()
        try:
            ndet = searcher.search_block(d, ch, block_idx)
            if searcher.verbose:
                cput1 = time.time()
                data_time = searcher.block_s
                speed = data_time / (cput1 - cput0)
                print("%d/%d %s block %d detections %d speed %1.3f * realtime" %
                      (rank + 1, size,
                       unix2datestr(block_idx * searcher.block_samples / conf.sample_rate),
                       block_idx, ndet, speed))
        except Exception:
            print("%d/%d skipping block %d" % (rank + 1, size, block_idx))
            traceback.print_exc(file=sys.stdout)
    return block1 + 1


def realtime_scan(conf, searcher):
    d = drf.DigitalRFReader(conf.data_dir)
    channelnames = d.get_channels()
    if len(channelnames) == 0:
        print("no channels")
        time.sleep(1)
        return None
    ch = conf.channel[0] if len(conf.channel) > 0 else channelnames[0]
    if ch not in channelnames:
        if rank == 0:
            print("configured channel %s not found, using %s" % (ch, channelnames[0]))
        ch = channelnames[0]
    block_idx = None

    while True:
        if kill(conf):
            print("kill.txt found, stopping digisonde_search.py")
            sys.exit(0)

        try:
            next_block, bounds = searcher.next_realtime_block(d, ch, block_idx)
            if next_block is None:
                time.sleep(0.05)
                continue

            cput0 = time.time()
            ndet = searcher.search_block(d, ch, next_block)
            cput1 = time.time()
            elapsed = cput1 - cput0
            skip_blocks = max(1, int(n.ceil(elapsed / searcher.block_s)))
            block_idx = next_block + skip_blocks
            if searcher.verbose:
                print("%d/%d %s block %d detections %d, elapsed %1.3f s, skip %d blocks" %
                      (rank + 1, size,
                       unix2datestr(next_block * searcher.block_samples / conf.sample_rate),
                       next_block, ndet, elapsed, skip_blocks - 1))
        except Exception:
            print("problem. retrying in a bit.")
            traceback.print_exc(file=sys.stdout)
            time.sleep(1)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Realtime MPI digisonde search")
    parser.add_argument(
        "--config",
        type=str,
        default="examples/marieluise/ramfjordmoen_digisonde.ini",
        help="Path to configuration file")
    parser.add_argument("--freq-start", type=float, default=2e6)
    parser.add_argument("--freq-stop", type=float, default=14e6)
    parser.add_argument("--freq-step", type=float, default=50e3)
    parser.add_argument(
        "--max-frequencies",
        type=int,
        default=None,
        help="Optional cap for quick tests; must be at least 25")
    parser.add_argument("--ipps-ms", type=float, nargs="+", default=[5.0, 10.0])
    parser.add_argument("--block-ms", type=float, default=10.0)
    parser.add_argument("--search-sample-rate", type=float, default=100e3)
    parser.add_argument("--threshold-snr", type=float, default=None)
    parser.add_argument("--max-lag-s", type=float, default=60.0)
    parser.add_argument("--store-all", action="store_true")
    parser.add_argument("--verbose", action="store_true")
    args = parser.parse_args()

    if args.verbose:
        conf = cc.chirp_config(args.config)
    else:
        with contextlib.redirect_stdout(io.StringIO()):
            conf = cc.chirp_config(args.config)
    if args.threshold_snr is None:
        args.threshold_snr = conf.threshold_snr

    searcher = digisonde_code_search(conf, args)
    if conf.realtime:
        realtime_scan(conf, searcher)
    else:
        try:
            scan(conf, searcher)
        except Exception:
            print("error. catching exception.")
            traceback.print_exc(file=sys.stdout)
            time.sleep(1)

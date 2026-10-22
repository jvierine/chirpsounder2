#!/usr/bin/env python3
"""Receiver-side recovery of the original plots, one bounded child per product.

The parent has no plotting imports. Images and ready-to-display scientific data
are computed from the receiver's local HDF5 files; the web server only receives
and serves them. Plotting children exit after each image to bound their memory.
"""
import argparse
import configparser
import datetime as dt
import glob
import json
import os
from pathlib import Path
import subprocess
import sys
import time
from types import SimpleNamespace


def configuration(path):
    c = configparser.ConfigParser(inline_comment_prefixes=('#', ';'))
    c.read(path)
    return c


def text(c, section, name, default):
    try:
        return json.loads(c.get(section, name))
    except Exception:
        return default


def dates(hours=48):
    today = dt.datetime.utcnow().date()
    return [(today - dt.timedelta(days=i)).isoformat() for i in range(int(hours // 24) + 2)]


def paths(base, pattern):
    result = []
    for day in dates():
        result.extend(glob.glob(str(Path(base) / day / pattern)))
    return result


def epoch(path):
    try:
        return float(Path(path).stem.rsplit('-', 1)[-1])
    except ValueError:
        return 0.


def post(path, endpoint):
    path = str(Path(path).resolve())
    result = subprocess.run(['curl', '--fail', '--silent', '--show-error', '--retry', '2',
                             '--max-time', '90', '-F', 'file=@' + path, endpoint],
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE, universal_newlines=True)
    if result.returncode:
        raise RuntimeError('upload failed for %s: %s' % (path, result.stderr[-400:]))
    print('published %s' % Path(path).name, flush=True)


def worker(args, c):
    # User-service cgroup memory controls are unavailable on some receivers.
    # Enforce a real child limit independently, without touching acquisition.
    import resource
    budget = 2 * 1024 ** 3
    resource.setrlimit(resource.RLIMIT_AS, (budget, budget))
    import numpy as np
    import h5py
    import matplotlib
    matplotlib.use('Agg')
    import matplotlib.pyplot as plt
    plt.rcParams.update({'font.size': 16, 'axes.labelsize': 18, 'axes.titlesize': 22,
                         'xtick.labelsize': 16, 'ytick.labelsize': 16, 'savefig.dpi': 110})
    station = text(c, 'config', 'receiver_station_name', 'TGO')
    base = text(c, 'config', 'output_dir', '/data0')
    png_url = args.server.rstrip('/') + '/upload.php'
    h5_url = args.server.rstrip('/') + '/upload_h5.php'
    if args.job == 'rti':
        import plot_rtf
        output = '/tmp/latest-rti-%s-%s.png' % (args.tx, station)
        before = Path(output).stat().st_mtime_ns if Path(output).exists() else None
        plot_rtf.get_recent_view(SimpleNamespace(output_dir=base, station_name=station), args.tx, station,
                                 n_hours=48, pfname=output, data_dir=base)
        if not Path(output).exists() or Path(output).stat().st_mtime_ns == before:
            raise RuntimeError('RTI generation produced no new image')
        post(output, png_url)
        derived = '/tmp/latest-rti-%s-%s.h5' % (args.tx, station)
        if Path(derived).exists():
            post(derived, h5_url)
        return
    if args.job == 'detections':
        import plot_detectionfiles as plots
        files = paths(base, 'cdetections-%s-*.h5' % station)
        files.extend(glob.glob(str(Path(base) / ('cdetections-%s-*.h5' % station))))
        start = time.time() - 48 * 3600
        rows = []
        for path in sorted(set(files), key=epoch):
            if epoch(path) < start - 900 or epoch(path) > time.time():
                continue
            try:
                with h5py.File(path, 'r') as h:
                    data = np.asarray(h['data'][()], dtype=np.float64)
                if data.ndim != 2 or data.shape[1] != 5:
                    continue
                data[:, 2] = plots.frequency_mhz(data[:, 2])
                rows.append(data)
            except Exception as exc:
                print('skipping %s: %s' % (path, exc), flush=True)
        if not rows:
            raise RuntimeError('no retained detection data in the last 48 hours')
        data = np.concatenate(rows)
        data = data[(data[:, 0] >= start) & (data[:, 0] <= time.time())]
        shared = configuration(str(Path(args.config).parent / 'server.ini'))
        stations = text(shared, 'stations', 'station_info', {})
        stations.update(text(c, 'stations', 'station_info', {}))
        output = '/tmp/latest-rothr_jorn-%s.png' % station
        plots.plot_propagation_range(data, start, n_hours=48, min_detections=5,
              pfname=output, station_name=station, station_info=stations,
              propagation_range_bands='auto', propagation_range_factor='auto',
              propagation_range_band_overrides={'JORN': {'paths': ['short', 'long']}})
        post(output, png_url)
        day = dt.datetime.utcnow().strftime('%Y-%m-%d')
        product = Path(base) / day / ('detection_summary-%s-%d.h5' % (station, int(time.time() // 86400) * 86400))
        product.parent.mkdir(parents=True, exist_ok=True)
        temporary = product.with_suffix('.partial.h5')
        with h5py.File(temporary, 'w') as h:
            h.attrs.update(schema='receiver-detection-summary-v1', receiver=station,
                           window_start_unix=start, window_end_unix=time.time(),
                           frequency_unit='MHz', minimum_cluster_detections=5)
            h.create_dataset('data', data=data, compression='gzip', compression_opts=3, shuffle=True)
        os.replace(str(temporary), str(product))
        post(product, h5_url)
        return
    if args.job == 'ionogram':
        import plot_rtf
        with h5py.File(args.source, 'r') as h:
            ranges, freqs, snr, timestamp = plot_rtf.read_ionogram(h)
        snr = np.asarray(snr, dtype=np.float64)
        with np.errstate(invalid='ignore', divide='ignore'):
            power = 10 * np.log10(np.where(snr > 0, snr, np.nan))
        kind = 'digisonde' if Path(args.source).name.startswith('digisonde') else 'lfm'
        output = '/tmp/latest-%s-%s-%s.png' % (kind, args.tx, station)
        fig, ax = plt.subplots(figsize=(12, 9))
        image = ax.pcolormesh(freqs / 1e6, ranges / 1000, power.T, shading='auto', cmap='gist_yarg', vmin=0, vmax=20)
        fig.colorbar(image, ax=ax).set_label('SNR (dB)')
        ax.set(xlabel='Frequency (MHz)', ylabel='One-way virtual range (km)',
               title='%s → %s\n%s UTC' % (args.tx, station, dt.datetime.utcfromtimestamp(timestamp).strftime('%Y-%m-%d %H:%M:%S')))
        fig.tight_layout()
        temporary = output + '.partial.png'
        fig.savefig(temporary)
        plt.close(fig)
        os.replace(temporary, output)
        post(output, png_url)
        return
    if args.job == 'spectrum':
        with h5py.File(args.source, 'r') as h:
            power = h['power'][()]
            times = h['time_unix'][()]
            frequencies = h['frequency_hz'][()]
        with np.errstate(invalid='ignore', divide='ignore'):
            relative = 10 * np.log10(power / np.nanmean(np.where(power > 0, power, np.nan), axis=0))
        finite = relative[np.isfinite(relative)]
        low, high = np.percentile(finite, [5, 95])
        if high <= low:
            low, high = low - .5, high + .5
        import matplotlib.dates as mdates
        t = [dt.datetime.utcfromtimestamp(value) for value in times]
        fig, ax = plt.subplots(figsize=(14, 8))
        mesh = ax.pcolormesh(mdates.date2num(t), frequencies / 1e6, relative.T,
                             shading='auto', cmap='plasma', vmin=low, vmax=high)
        ax.xaxis.set_major_formatter(mdates.DateFormatter('%H:%M:%S'))
        ax.set(xlabel='Time (UTC)', ylabel='Frequency (MHz)',
               title='%s full-band spectrum · %s UTC' % (station, t[0].strftime('%Y-%m-%d')))
        fig.colorbar(mesh, ax=ax).set_label('Relative power (dB)')
        fig.tight_layout()
        output = '/tmp/latest-rf-spectrum-%s.png' % station
        fig.savefig(output + '.partial.png')
        plt.close(fig)
        os.replace(output + '.partial.png', output)
        post(output, png_url)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--config', required=True)
    parser.add_argument('--server', default='https://juha.no')
    parser.add_argument('--job', choices=['rti', 'detections', 'ionogram', 'spectrum'])
    parser.add_argument('--tx')
    parser.add_argument('--source')
    parser.add_argument('--once', action='store_true')
    args = parser.parse_args()
    c = configuration(args.config)
    if args.job:
        worker(args, c)
        return
    base = text(c, 'config', 'output_dir', '/data0')
    station = text(c, 'config', 'receiver_station_name', 'TGO')
    links = text(c, 'rtf', 'links', [['SGO', station], ['Ramfjordmoen', station], ['Juliusruh', station]])
    last = {}
    while True:
        started = time.time()
        jobs = []
        for tx, rx in links:
            jobs.append(('rti:' + tx, 300, ['--job', 'rti', '--tx', tx]))
        jobs.append(('detections', 900, ['--job', 'detections']))
        for tx in [link[0] for link in links]:
            files = paths(base, '*_ionogram-%s-%s-*.h5' % (tx, station))
            if files:
                source = max(files, key=epoch)
                jobs.append(('ionogram:' + tx + ':' + source, 3600, ['--job', 'ionogram', '--tx', tx, '--source', source]))
        spectra = paths(base, 'rf_spectrum-%s-*.h5' % station)
        if spectra:
            source = max(spectra, key=epoch)
            jobs.append(('spectrum:' + source, 3600, ['--job', 'spectrum', '--source', source]))
        for key, period, extra in jobs:
            if time.time() - last.get(key, 0) < period:
                continue
            command = [sys.executable, '-u', str(Path(__file__).resolve()), '--config', args.config,
                       '--server', args.server] + extra
            print('starting %s' % key, flush=True)
            try:
                result = subprocess.run(command, timeout=300)
                if result.returncode == 0:
                    last[key] = time.time()
            except subprocess.TimeoutExpired:
                print('plot exceeded five-minute limit: %s' % key, flush=True)
        # Prune bookkeeping for superseded individual snapshots.
        keep = {job[0] for job in jobs}
        last = {key: value for key, value in last.items() if key in keep}
        if args.once:
            return
        time.sleep(max(5, 30 - (time.time() - started)))


if __name__ == '__main__':
    main()

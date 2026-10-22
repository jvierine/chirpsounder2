"""Receiver-side reduced RTF product; provenance stays in HDF5 metadata."""
import calendar
import datetime as dt
import os
from pathlib import Path
import h5py
import numpy as np


def save_rti_product(files, times, range_km, snr_linear, frequency_hz, tx, rx, start, end):
    def unix(value):
        return calendar.timegm(value.utctimetuple()) + value.microsecond / 1e6
    epochs = np.asarray([unix(value) for value in times], dtype=np.float64)
    end_unix = unix(end) if end is not None else float(epochs[-1])
    start_unix = unix(start) if start is not None else float(epochs[0])
    parent = Path(files[-1]).parent
    base = parent.parent if len(parent.name) == 10 and parent.name[4] == '-' else parent
    day = dt.datetime.fromtimestamp(end_unix, dt.timezone.utc).strftime('%Y-%m-%d')
    output = base / day / ('rti-%s-%s-%d.h5' % (tx, rx, int(end_unix // 86400) * 86400))
    output.parent.mkdir(parents=True, exist_ok=True)
    temporary = output.with_suffix('.partial.h5')
    with h5py.File(temporary, 'w') as h:
        h.attrs.update(schema='receiver-rtf-v1', transmitter=tx, receiver=rx,
                       window_start_unix=start_unix, window_end_unix=end_unix,
                       echo_threshold_linear=20.0, source_file_count=len(files),
                       producer='receiver_rti_product.save_rti_product')
        h.create_dataset('time_unix', data=epochs)
        h.create_dataset('range_km', data=np.asarray(range_km, dtype=np.float64))
        for key, values in [('snr_peak_linear', snr_linear), ('peak_frequency_hz', frequency_hz)]:
            h.create_dataset(key, data=np.asarray(values, dtype=np.float32), compression='gzip', compression_opts=3, shuffle=True)
        h.create_dataset('source_files', data=np.asarray(files, dtype=h5py.string_dtype()))
    os.replace(str(temporary), str(output))
    latest = Path('/tmp/latest-rti-%s-%s.h5' % (tx, rx))
    staged = latest.with_suffix('.link')
    try:
        staged.unlink()
    except FileNotFoundError:
        pass
    staged.symlink_to(output)
    os.replace(str(staged), str(latest))
    print('computed receiver RTF data: %s, %d soundings' % (output, len(files)), flush=True)
    return str(output)


def peak_by_range(snr, frequencies_hz):
    """Maximum linear SNR and its measured frequency, preserving empty gates."""
    snr = np.asarray(snr)
    frequencies_hz = np.asarray(frequencies_hz)
    valid = np.any(~np.isnan(snr), axis=0)
    maxima = np.full(snr.shape[1], np.nan)
    frequencies = np.full(snr.shape[1], np.nan)
    if np.any(valid):
        values = np.where(np.isnan(snr[:, valid]), -np.inf, snr[:, valid])
        indices = np.argmax(values, axis=0)
        maxima[valid] = values[indices, np.arange(values.shape[1])]
        frequencies[valid] = frequencies_hz[indices]
    return maxima, frequencies

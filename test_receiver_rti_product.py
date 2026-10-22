import datetime as dt
import tempfile
import unittest
from pathlib import Path

import h5py
import numpy as np
from receiver_rti_product import save_rti_product, peak_by_range


class ReceiverProductTest(unittest.TestCase):
    def test_peak_frequency_matches_maximum_snr_and_keeps_empty_gates(self):
        data = np.array([[2., np.nan, 7., np.nan], [9., np.nan, 7., 1.],
                         [4., np.nan, 2., 8.]])
        peaks, frequency = peak_by_range(data, [2e6, 5e6, 8e6])
        np.testing.assert_equal(peaks, [9., np.nan, 7., 8.])
        np.testing.assert_equal(frequency, [5e6, np.nan, 2e6, 8e6])
        rng = np.random.default_rng(42)
        measured = rng.uniform(0, 100, size=(80, 300))
        measured[rng.random(measured.shape) < .3] = np.nan
        measured[:, 0] = np.nan
        bins = np.linspace(1e6, 16e6, 80)
        actual, selected = peak_by_range(measured, bins)
        for gate in range(1, measured.shape[1]):
            self.assertEqual(actual[gate], np.nanmax(measured[:, gate]))
            self.assertEqual(selected[gate], bins[np.nanargmax(measured[:, gate])])

    def test_retained_window_preserves_gaps_units_and_replaces_atomically(self):
        start = dt.datetime(2026, 10, 20, 7)
        end = start + dt.timedelta(hours=48)
        times = [start, start + dt.timedelta(hours=1), end]
        signal = np.array([[2., 25.], [np.nan, np.nan], [3., 40.]])
        frequency = np.array([[2e6, 5e6], [np.nan, np.nan], [3e6, 7e6]])
        latest = Path('/tmp/latest-rti-TESTTX-TESTRX.h5')
        try:
            with tempfile.TemporaryDirectory() as folder:
                source = Path(folder) / '2026-10-22' / 'lfm_ionogram-TESTTX-TESTRX-1792652400.h5'
                source.parent.mkdir()
                output = save_rti_product([str(source)], times, [200., 500.], signal,
                                          frequency, 'TESTTX', 'TESTRX', start, end)
                with h5py.File(output) as h:
                    self.assertEqual(h.attrs['window_end_unix'] - h.attrs['window_start_unix'], 48*3600)
                    np.testing.assert_equal(h['snr_peak_linear'][()], signal)
                    np.testing.assert_equal(h['peak_frequency_hz'][()], frequency)
                    np.testing.assert_equal(h['range_km'][()], [200., 500.])
                    self.assertTrue(np.all(np.diff(h['time_unix'][()]) > 0))
                    self.assertEqual(h['snr_peak_linear'].compression, 'gzip')
                signal[-1, -1] = 80.
                replacement = save_rti_product([str(source)], times, [200., 500.], signal,
                                               frequency, 'TESTTX', 'TESTRX', start, end)
                self.assertEqual(output, replacement)
                self.assertEqual(latest.resolve(), Path(output).resolve())
                with h5py.File(latest) as h:
                    self.assertEqual(h['snr_peak_linear'][-1, -1], 80.)
                self.assertFalse(list(Path(folder).rglob('*.partial.h5')))
        finally:
            latest.unlink(missing_ok=True)


if __name__ == '__main__':
    unittest.main()

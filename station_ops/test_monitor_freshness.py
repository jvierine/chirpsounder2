"""Freshness checks must not traverse the historical station archive."""
import os
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import station_monitor as monitor


class FreshnessTests(unittest.TestCase):
    def test_daily_products_and_midnight(self):
        with tempfile.TemporaryDirectory() as root:
            today = monitor.datetime.now(monitor.timezone.utc).date()
            previous = Path(root) / (today - monitor.timedelta(days=1)).isoformat()
            previous.mkdir()
            product = previous / "prn_ionogram-test.h5"
            product.touch()
            result = monitor.output_status(SimpleNamespace(output_dir=root), 86400)
            self.assertTrue(result["ok"])
            self.assertEqual(result["newest_file"], str(product))

    def test_archive_is_not_visited_or_counted_as_fresh(self):
        with tempfile.TemporaryDirectory() as root:
            old = Path(root) / "2020-01-01"
            old.mkdir()
            (old / "lfm_ionogram-old.h5").touch()
            walk = os.walk

            def guarded(path, *args, **kwargs):
                self.assertNotIn("2020-01-01", str(path))
                return walk(path, *args, **kwargs)

            with patch.object(monitor.os, "walk", guarded):
                self.assertFalse(monitor.output_status(SimpleNamespace(output_dir=root), 3600)["ok"])

    def test_root_level_and_current_digisonde(self):
        with tempfile.TemporaryDirectory() as root:
            conf = SimpleNamespace(output_dir=root)
            product = Path(root) / "lfm_ionogram-root.h5"
            product.touch()
            self.assertTrue(monitor.output_status(conf, 3600)["ok"])
            product.unlink()
            today = Path(root) / monitor.datetime.now(monitor.timezone.utc).date().isoformat()
            today.mkdir()
            (today / "digisonde_ionogram-test.h5").touch()
            self.assertTrue(monitor.output_status(conf, 3600)["ok"])


if __name__ == "__main__":
    unittest.main()

"""Guard transmitter schedule conversion; configuration only, no live DSP."""
import json
from pathlib import Path
import tempfile
import unittest

from import_prn_config import convert


class FrequencyProgramTests(unittest.TestCase):
    def test_ingv_generated_schedule_does_not_use_obsolete_ini_table(self):
        values = dict(sample_rate=1000000, dec=10, code_type=["prn"],
                      freqs=[[1.0, 0], [1.1, 0]], nfreqs=300,
                      frequency_duration=1, station_id=10, code_len=10000,
                      pulse_length=[400], ipp=[400], bw=[100000], range_shift=5)
        with tempfile.TemporaryDirectory() as root:
            source = Path(root) / "italy.ini"
            source.write_text("[config]\n" + "\n".join(
                f"{key}={json.dumps(value)}" for key, value in values.items()))
            ingv = convert(source, "DOB", "Montelibretti",
                           frequency_program="ingv-generated")
            self.assertEqual(len(ingv["frequencies_hz"]), 300)
            self.assertEqual(ingv["frequencies_hz"][0], 1e6)
            self.assertEqual(ingv["frequencies_hz"][-1], 30.9e6)
            self.assertEqual(ingv["period_s"], 300)
            self.assertEqual(ingv["start_offset_s"], 0)
            self.assertEqual(ingv["codes"][0]["seed"], 10)
            self.assertEqual(ingv["codes"][0]["pulse_length"], 400)
            self.assertEqual(ingv["codes"][0]["ipp"], 400)
            received = convert(source, "DOB", "Montelibretti",
                               frequency_program="ingv-generated",
                               max_receive_frequency_mhz=24.9)
            self.assertEqual(len(received["frequencies_hz"]), 240)
            self.assertEqual(received["frequencies_hz"], ingv["frequencies_hz"][:240])
            self.assertEqual(received["period_s"], 300)
            # Swedish programs continue to use their exact supplied table.
            irf = convert(source, "DOB", "Kiruna")
            self.assertEqual(irf["frequencies_hz"], [1e6, 1.1e6])
            self.assertEqual(irf["period_s"], 60)


if __name__ == "__main__":
    unittest.main()

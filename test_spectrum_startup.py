"""Exercise the live loop with no recorder channel, then an incomplete minute."""
import ast
from pathlib import Path
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, MagicMock


class Finished(Exception):
    pass


class StartupRetryTests(unittest.TestCase):
    def environment(self, refresh):
        source = Path(__file__).with_name("plot_rf_spec.py").read_text()
        main = next(node for node in ast.parse(source).body if isinstance(node, ast.FunctionDef) and node.name == "main")
        args = SimpleNamespace(image_pixels=128, output=None, refresh_sec=refresh, no_store=False)
        conf = SimpleNamespace(station_name="KHO", data_dir="/missing")
        drf = SimpleNamespace(DigitalRFReader=Mock(side_effect=[ValueError("No channels found"), object(), object()]))
        clock = SimpleNamespace(time=lambda: 0, sleep=Mock())
        env = dict(parse_args=lambda: args, set_publication_style=lambda: None,
                   load_config=lambda _: conf, load_frequency_overlays=lambda _: [],
                   FIGURE_SIZE_INCHES=16, PLOT_DURATION_SECONDS=60, READINESS_DELAY_SECONDS=3.0, drf=drf,
                   plt=SimpleNamespace(figure=Mock()), GridSpec=MagicMock(), time=clock,
                   collect_plot_data=Mock(side_effect=[RuntimeError("not enough data"), {}]),
                   add_frequency_overlays=Mock(), add_lfm_overlays=Mock(),
                   save_power_hdf5=Mock(), draw_plot=Mock(), save_plot=Mock(side_effect=Finished))
        exec(compile(ast.Module(body=[main], type_ignores=[]), str(__file__), "exec"), env)
        return env

    def test_live_recovers_after_missing_channel_and_short_buffer(self):
        env = self.environment(60)
        with self.assertRaises(Finished):
            env["main"]()
        self.assertEqual(env["drf"].DigitalRFReader.call_count, 3)
        self.assertEqual([call.args[0] for call in env["time"].sleep.call_args_list], [5.0, 63.0])
        env["save_power_hdf5"].assert_called_once()

    def test_one_shot_reports_missing_channel(self):
        env = self.environment(0)
        with self.assertRaisesRegex(ValueError, "No channels"):
            env["main"]()
        env["time"].sleep.assert_not_called()


if __name__ == "__main__":
    unittest.main()

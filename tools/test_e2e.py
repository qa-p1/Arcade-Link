import importlib.util
import os
from pathlib import Path
import tempfile
import sys
import unittest
from unittest import mock
import e2e


class RunnerTests(unittest.TestCase):
    def test_runner_imports_without_posix_kill_signal(self):
        spec = importlib.util.spec_from_file_location("arcade_e2e_windows", e2e.LINK / "tools/e2e.py")
        module = importlib.util.module_from_spec(spec)
        signals = type("WindowsSignals", (), {"SIGTERM": e2e.signal.SIGTERM})()
        with mock.patch.dict("sys.modules", {"signal": signals}):
            spec.loader.exec_module(module)
        self.assertEqual(module.APP_IDS, e2e.APP_IDS)

    def test_app_count_tracks_the_rust_list_and_shelf_uses_cpp(self):
        self.assertEqual(set(e2e.APP_IDS), set(e2e.APPS))
        self.assertEqual(len(e2e.APP_IDS), 7)
        if not os.environ.get("ARCADE_SHELF_REPO"):
            self.assertEqual(e2e.APPS["arcade.shelf"]["dir"], e2e.CODING / "C++/Arcade-Shelf")

    def test_missing_and_stale_builds_explain_how_to_rebuild(self):
        with tempfile.TemporaryDirectory() as root:
            root = Path(root)
            spec = {"dir": root, "bin": "target/app", "build": ["cargo", "build"]}
            with self.assertRaisesRegex(AssertionError, "missing build.*cargo build"):
                e2e.require_current_build(spec)
            (root / "target").mkdir()
            binary = root / "target/app"
            binary.write_text("binary")
            binary.chmod(0o755)
            source = root / "app.rs"
            source.write_text("source")
            os.utime(binary, ns=(1, 1))
            with self.assertRaisesRegex(AssertionError, "cargo build.*stale build.*newer source"):
                e2e.require_current_build(spec)
            os.utime(binary, ns=(source.stat().st_mtime_ns + 1, source.stat().st_mtime_ns + 1))
            self.assertEqual(e2e.require_current_build(spec), binary)

    def test_tray_metric_does_not_replace_existing_regression_checks(self):
        path = e2e.LINK / "benchmarks/bench.py"
        spec = importlib.util.spec_from_file_location("arcade_bench", path)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        self.assertEqual(len(module.APPS), 7)
        base = {"apps": {"find": {"startup_ms_median": 20}}}
        self.assertTrue(module.compare({"apps": {"find": {"startup_ms_median": 50}}}, base, .05))

    def test_frame_probe_waits_for_the_instrumented_frame(self):
        spec = importlib.util.spec_from_file_location("arcade_bench_frames", e2e.LINK / "benchmarks/bench.py")
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        command = [sys.executable, "-u", "-c", "import sys,time\nprint('ready', flush=True)\nfor line in sys.stdin:\n time.sleep(.02)\n print('frame', flush=True)"]
        with tempfile.TemporaryDirectory() as root:
            env = module.isolated_env(Path(root))
            samples = module.frame_latency(command, env, 2)
            self.assertEqual(len(samples), 2)
            self.assertTrue(all(sample >= 15 for sample in samples), samples)
            with self.assertRaisesRegex(AssertionError, "expected 'frame'"):
                module.frame_latency([sys.executable, "-u", "-c", "import sys\nprint('ready',flush=True)\ninput()\nprint('wrong',flush=True)"], env, 1)

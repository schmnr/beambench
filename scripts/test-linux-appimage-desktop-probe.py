"""The desktop gate must reject early exits and a live but unloaded frontend."""
import importlib.util
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

spec = importlib.util.spec_from_file_location(
    'desktop_probe', Path(__file__).parent / 'linux-appimage-smoke/probe.py')
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)


class ReadinessTests(unittest.TestCase):
    def test_exit_and_timeout_are_failures_even_with_stale_ready_text(self):
        with tempfile.TemporaryDirectory() as temporary:
            log = Path(temporary) / 'desktop.log'
            log.write_text('Frontend ready\n')
            app = subprocess.Popen([sys.executable, '-c', 'pass'])
            app.wait()
            self.assertFalse(probe.wait_for_frontend(app, log, timeout=0.2))
            log.write_text('Native app started, but frontend never loaded\n')
            app = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(30)'],
                                   start_new_session=True)
            try:
                self.assertFalse(probe.wait_for_frontend(app, log, timeout=0.2))
            finally:
                probe.stop(app)

    def test_crash_after_ready_callback_is_a_failure(self):
        with tempfile.TemporaryDirectory() as temporary:
            log = Path(temporary) / 'desktop.log'
            log.write_text('Frontend ready\n')
            app = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(0.2)'],
                                   start_new_session=True)
            try:
                self.assertFalse(probe.wait_for_frontend(app, log))
            finally:
                probe.stop(app)


if __name__ == '__main__':
    unittest.main()

"""Boot the unmodified release AppImage in a disposable, isolated container."""
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time


def wait_for_frontend(app, log, timeout=40):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if app.poll() is not None:
            return False
        if 'Frontend ready' in log.read_text(errors='replace'):
            # A process that immediately crashes after the callback is a failure.
            time.sleep(2)
            return app.poll() is None
        time.sleep(0.1)
    return False


def stop(process):
    try:
        os.killpg(process.pid, signal.SIGTERM)
    except ProcessLookupError:
        pass
    try:
        process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        os.killpg(process.pid, signal.SIGKILL)
        process.wait()


def main():
    scenario = sys.argv[1]
    if scenario not in ['x11', 'plasma-wayland-env']:
        raise ValueError('Unknown desktop scenario')
    with tempfile.TemporaryDirectory(prefix='beambench-desktop-') as temporary:
        root = Path(temporary)
        runtime = root / 'runtime'
        runtime.mkdir(mode=0o700)
        env = dict(os.environ, DISPLAY=':99', XDG_RUNTIME_DIR=str(runtime),
                   XDG_CURRENT_DESKTOP='KDE', DESKTOP_SESSION='plasma',
                   APPIMAGE_EXTRACT_AND_RUN='1', LIBGL_ALWAYS_SOFTWARE='1',
                   RUST_BACKTRACE='1', NO_AT_BRIDGE='1',
                   # Docker blocks WebKit's nested sandbox. This override exists
                   # only in the networkless, unprivileged test container.
                   WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS='1')
        if scenario == 'plasma-wayland-env':
            env.update(WAYLAND_DISPLAY='wayland-0', XDG_SESSION_TYPE='wayland')
        started = time.monotonic()
        with (root / 'xvfb.log').open('w') as xlog:
            xvfb = subprocess.Popen(
                ['Xvfb', ':99', '-screen', '0', '1280x800x24', '-nolisten', 'tcp'],
                env=env, stdout=xlog, stderr=subprocess.STDOUT, start_new_session=True)
            try:
                for _ in range(50):
                    if xvfb.poll() is not None:
                        raise RuntimeError('Xvfb exited before the desktop test')
                    if Path('/tmp/.X11-unix/X99').exists():
                        break
                    time.sleep(0.1)
                else:
                    raise RuntimeError('Xvfb did not create its display socket')
                log = root / 'desktop.log'
                with log.open('w') as output:
                    app = subprocess.Popen(
                        ['dbus-run-session', '--', '/test.AppImage'], env=env,
                        stdout=output, stderr=subprocess.STDOUT, start_new_session=True)
                    try:
                        ready = wait_for_frontend(app, log)
                        exit_code = app.poll()
                    finally:
                        stop(app)
            finally:
                stop(xvfb)
        print(json.dumps({
            'scenario': scenario, 'os': Path('/etc/os-release').read_text(),
            'frontend_ready': ready, 'exit_before_cleanup': exit_code,
            'seconds': round(time.monotonic() - started, 2),
            'desktop_log': log.read_text(errors='replace')[-18000:],
            'xvfb_log': (root / 'xvfb.log').read_text(errors='replace')[-2000:],
        }), flush=True)
        return 0 if ready else 1


if __name__ == '__main__':
    sys.exit(main())

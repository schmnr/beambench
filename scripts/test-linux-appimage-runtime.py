#!/usr/bin/env python3
"""Exercise camera discovery, display/fonts, and actual AppImage repair.

Set BEAMBENCH_TEST_APPIMAGE to test the final package with real GStreamer.
"""
import hashlib
import json
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import sys
import tempfile
import unittest

SCRIPTS = Path(__file__).resolve().parent


def camera_environment():
    return {k: v for k, v in os.environ.items() if not (
        k.startswith('GST_') or k.startswith('BEAMBENCH_ORIGINAL_GST_')
    )}


class RuntimePolicyTests(unittest.TestCase):
    def test_relaunch_cleans_previous_mount_paths_and_preserves_user_overrides(self):
        with tempfile.TemporaryDirectory(prefix='camera restart ') as temporary:
            root = Path(temporary)
            # The mount prefix follows the AppImage filename, which users can rename.
            previous = '/tmp/.mount_RenamedLaser123'
            executable = root / 'beambench-tauri'
            shutil.copyfile(SCRIPTS / 'linux-appimage-exec.sh', executable)
            executable.chmod(0o755)
            binary = root / 'beambench-tauri.bin'
            keys = ['PATH', 'LD_LIBRARY_PATH', 'FONTCONFIG_FILE',
                    'GST_PLUGIN_SYSTEM_PATH', 'GST_PLUGIN_SYSTEM_PATH_1_0',
                    'GST_PLUGIN_PATH_1_0', 'BEAMBENCH_APPIMAGE_RUNTIME_DIR']
            binary.write_text(f'''#!/usr/bin/env python3
import json, os
print(json.dumps({{k: os.environ.get(k) for k in {keys!r}}}))
''')
            binary.chmod(0o755)
            source = shlex.quote(str(SCRIPTS / 'linux-appimage-runtime.sh'))
            cases = [
                # 0.2.30 inherited linuxdeploy's empty system plugin paths.
                ({'GST_PLUGIN_SYSTEM_PATH': previous + '/usr/lib/gstreamer:',
                  'GST_PLUGIN_SYSTEM_PATH_1_0': previous + '/usr/lib/gstreamer-1.0:'}, None, None),
                # 0.2.31 restored plugin discovery but retained the old font file.
                ({}, None, None),
                # Legacy launches with custom fonts still expose their mount in GST.
                ({'FONTCONFIG_FILE': '/custom fonts/fonts.conf',
                  'GST_PLUGIN_SYSTEM_PATH': previous + '/usr/lib/gstreamer:/custom gst',
                  'GST_PLUGIN_SYSTEM_PATH_1_0': previous + '/usr/lib/gstreamer-1.0:/custom gst'},
                 '/custom gst', '/custom gst'),
                # Fixed releases identify the previous mount, even with custom fonts.
                ({'BEAMBENCH_APPIMAGE_RUNTIME_DIR': previous,
                  'FONTCONFIG_FILE': '/custom fonts/fonts.conf',
                  'GST_PLUGIN_SYSTEM_PATH': '', 'GST_PLUGIN_SYSTEM_PATH_1_0': ''}, '', ''),
            ]
            for overrides, system, versioned in cases:
                with self.subTest(overrides=overrides):
                    env = camera_environment()
                    env.update(APPDIR=str(root),
                               BEAMBENCH_APPIMAGE_FONT_POLICY='1',
                               FONTCONFIG_FILE=previous + '/usr/share/beambench/fonts.conf',
                               PATH=previous + '/usr/bin:/usr/bin:/bin:' + previous + '-other/bin',
                               LD_LIBRARY_PATH=previous + '/usr/lib:/custom libs::',
                               GST_PLUGIN_PATH_1_0=previous + '/plugins:/custom plugins')
                    env.update(overrides)
                    result = subprocess.check_output(['bash', '-c', f'''source {source}
export GST_PLUGIN_SYSTEM_PATH="$APPDIR/usr/lib/gstreamer:${{GST_PLUGIN_SYSTEM_PATH-}}"
export GST_PLUGIN_SYSTEM_PATH_1_0="$APPDIR/usr/lib/gstreamer-1.0:${{GST_PLUGIN_SYSTEM_PATH_1_0-}}"
exec {shlex.quote(str(executable))}
'''], env=env, text=True)
                    self.assertEqual(json.loads(result), {
                        'PATH': '/usr/bin:/bin:' + previous + '-other/bin',
                        'LD_LIBRARY_PATH': '/custom libs::',
                        'FONTCONFIG_FILE': overrides.get('FONTCONFIG_FILE', str(root / 'usr/share/beambench/fonts.conf')),
                        'GST_PLUGIN_SYSTEM_PATH': system,
                        'GST_PLUGIN_SYSTEM_PATH_1_0': versioned,
                        'GST_PLUGIN_PATH_1_0': '/custom plugins',
                        'BEAMBENCH_APPIMAGE_RUNTIME_DIR': str(root),
                    })

    def test_camera_paths_survive_the_generated_launcher_and_arguments_are_preserved(self):
        with tempfile.TemporaryDirectory(prefix='camera AppDir ') as temporary:
            root = Path(temporary)
            executable = root / 'beambench-tauri'
            shutil.copyfile(SCRIPTS / 'linux-appimage-exec.sh', executable)
            executable.chmod(0o755)
            binary = root / 'beambench-tauri.bin'
            binary.write_text('''#!/usr/bin/env python3
import json, os, sys
print(json.dumps({'paths': {k: v for k, v in os.environ.items() if k.startswith('GST_') or k.startswith('BEAMBENCH_ORIGINAL_GST_')}, 'args': sys.argv[1:]}))
''')
            binary.chmod(0o755)
            source = shlex.quote(str(SCRIPTS / 'linux-appimage-runtime.sh'))
            for paths in [{}, {'GST_PLUGIN_SYSTEM_PATH': ''},
                          {'GST_PLUGIN_SYSTEM_PATH_1_0': ''},
                          {'GST_PLUGIN_SYSTEM_PATH': '/legacy plugins',
                           'GST_PLUGIN_SYSTEM_PATH_1_0': '/custom plugins:/other',
                           'GST_PLUGIN_PATH_1_0': '/extra plugins'}]:
                with self.subTest(paths=paths):
                    env = camera_environment()
                    env.update(APPDIR=str(root), **paths)
                    result = subprocess.run(['bash', '-c', f'''source {source}
export GST_PLUGIN_SYSTEM_PATH="$APPDIR/usr/lib/gstreamer:${{GST_PLUGIN_SYSTEM_PATH-}}"
export GST_PLUGIN_SYSTEM_PATH_1_0="$APPDIR/usr/lib/gstreamer-1.0:${{GST_PLUGIN_SYSTEM_PATH_1_0-}}"
exec {shlex.quote(str(executable))} 'project with spaces.bb' '' '--flag'
'''], env=env, check=True, capture_output=True, text=True)
                    self.assertEqual(json.loads(result.stdout), {
                        'paths': paths, 'args': ['project with spaces.bb', '', '--flag'],
                    })

    def runtime(self, **overrides):
        env = {k: v for k, v in os.environ.items() if k not in {
            'GDK_BACKEND', 'DISPLAY', 'WAYLAND_DISPLAY', 'FONTCONFIG_FILE',
            'BEAMBENCH_APPIMAGE_FONT_POLICY',
        }}
        env.update(APPDIR='/fixture AppDir', **overrides)
        source = shlex.quote(str(SCRIPTS / 'linux-appimage-runtime.sh'))
        result = subprocess.run(['bash', '-c', f'''source {source}
printf '%s\\n%s\\n%s\\n' "${{GDK_BACKEND:-}}" "${{FONTCONFIG_FILE:-}}" "${{BEAMBENCH_APPIMAGE_FONT_POLICY:-}}"
'''], env=env, check=True, capture_output=True, text=True)
        return result.stdout.splitlines()

    def test_display_selection_and_explicit_overrides(self):
        for env, backend in [
            ({'DISPLAY': ':1'}, 'x11'),
            ({'WAYLAND_DISPLAY': 'wayland-0'}, 'wayland'),
            ({'DISPLAY': ':1', 'WAYLAND_DISPLAY': 'wayland-0'}, 'x11,wayland'),
            ({'DISPLAY': ':1', 'GDK_BACKEND': 'wayland'}, 'wayland'),
            ({'WAYLAND_DISPLAY': 'wayland-0', 'GDK_BACKEND': 'x11,wayland'}, 'x11,wayland'),
            ({}, ''),
        ]:
            with self.subTest(env=env):
                self.assertEqual(self.runtime(**env)[0], backend)

    def test_font_policy_is_scoped_and_explicit_font_override_is_preserved(self):
        self.assertEqual(self.runtime()[1:], ['/fixture AppDir/usr/share/beambench/fonts.conf', '1'])
        self.assertEqual(self.runtime(FONTCONFIG_FILE='/custom/fonts.conf')[1:], ['/custom/fonts.conf', ''])

    @unittest.skipUnless(sys.platform == 'linux' and shutil.which('fc-match') and shutil.which('fc-list'), 'Linux Fontconfig tools unavailable')
    def test_normal_fonts_are_preserved_and_color_fallbacks_are_excluded(self):
        env = dict(os.environ)
        env.pop('FONTCONFIG_FILE', None)
        before = subprocess.check_output(['fc-match', 'sans-serif:charset=41', '-f', '%{file}'], env=env)
        env['FONTCONFIG_FILE'] = str(SCRIPTS / 'linux-appimage-fonts.conf')
        after = subprocess.check_output(['fc-match', 'sans-serif:charset=41', '-f', '%{file}'], env=env)
        self.assertEqual(before, after)
        self.assertTrue(after)
        colors = subprocess.check_output(['fc-list', '-f', '%{color}\n'], env=env, text=True)
        self.assertNotIn('True', colors)
        for codepoint in ['1f680', '1faea']:
            match = subprocess.check_output(['fc-match', f'emoji:charset={codepoint}', '-f', '%{color}'], env=env, text=True)
            self.assertNotEqual(match, 'True')


@unittest.skipUnless(shutil.which('mksquashfs') and shutil.which('unsquashfs'), 'SquashFS tools unavailable')
class AppImageRepairTests(unittest.TestCase):
    def test_repair_is_audited_idempotent_and_invalidates_only_the_old_signature(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            payload = root / 'payload'
            (payload / 'usr/lib').mkdir(parents=True)
            (payload / 'usr/lib/libwayland-client.so.0').write_bytes(b'incompatible fixture')
            (payload / 'usr/lib/libordinary.so.1').write_bytes(b'keep this library')
            binary = payload / 'usr/bin/beambench-tauri'
            binary.parent.mkdir()
            binary.write_text('#!/bin/bash\nprintf "original binary\\n"\n')
            binary.chmod(0o755)
            hook = payload / 'apprun-hooks/linuxdeploy-plugin-gtk.sh'
            hook.parent.mkdir()
            hook.write_text('#!/bin/bash\nexport GDK_BACKEND=x11 # generated GTK hook\n')
            squashfs = root / 'payload.squashfs'
            subprocess.run(['mksquashfs', str(payload), str(squashfs), '-noappend', '-processors', '2', '-quiet', '-no-progress'], check=True, capture_output=True)
            image = root / 'test.AppImage'
            image.write_bytes(b'fixture-runtime\n' + squashfs.read_bytes())
            image.chmod(0o755)
            signature = Path(str(image) + '.sig')
            signature.write_text('old signature')
            script = str(SCRIPTS / 'harden-linux-appimage')
            before = subprocess.run(['bash', script, '--check', str(image)], capture_output=True)
            self.assertNotEqual(before.returncode, 0)
            subprocess.run(['bash', script, str(image)], check=True, capture_output=True)
            self.assertFalse(signature.exists())
            subprocess.run(['bash', script, '--check', str(image)], check=True, capture_output=True)
            repaired_hash = hashlib.sha256(image.read_bytes()).hexdigest()
            signature.write_text('new signature')
            subprocess.run(['bash', script, str(image)], check=True, capture_output=True)
            self.assertEqual(hashlib.sha256(image.read_bytes()).hexdigest(), repaired_hash)
            self.assertEqual(signature.read_text(), 'new signature')
            extracted = root / 'extracted'
            subprocess.run(['unsquashfs', '-q', '-no-progress', '-o', str(len(b'fixture-runtime\n')), '-d', str(extracted), str(image)], check=True, capture_output=True)
            self.assertFalse((extracted / 'usr/lib/libwayland-client.so.0').exists())
            self.assertEqual((extracted / 'usr/lib/libordinary.so.1').read_bytes(), b'keep this library')
            self.assertEqual((extracted / 'usr/bin/beambench-tauri.bin').read_bytes(), binary.read_bytes())
            self.assertEqual((extracted / 'usr/bin/beambench-tauri').read_bytes(), (SCRIPTS / 'linux-appimage-exec.sh').read_bytes())


@unittest.skipUnless(os.environ.get('BEAMBENCH_TEST_APPIMAGE'), 'No final AppImage supplied')
class PackagedCameraTests(unittest.TestCase):
    def test_final_launcher_loads_host_plugins_and_captures_synthetic_frames(self):
        image = Path(os.environ['BEAMBENCH_TEST_APPIMAGE']).resolve()
        for tool in ['unsquashfs', 'gst-inspect-1.0', 'gst-launch-1.0']:
            self.assertIsNotNone(shutil.which(tool), f'Required package test tool missing: {tool}')
        env = camera_environment()
        for element in ['appsink', 'v4l2src', 'videoconvert']:
            subprocess.run(['gst-inspect-1.0', element], env=env, check=True, capture_output=True)
        subprocess.run(['bash', str(SCRIPTS / 'harden-linux-appimage'), '--check', str(image)], check=True, capture_output=True)
        with tempfile.TemporaryDirectory(prefix='packaged camera ') as temporary:
            root = Path(temporary)
            payload = image.read_bytes()
            offset = -1
            while True:
                offset = payload.find(b'hsqs', offset + 1)
                self.assertNotEqual(offset, -1, 'No valid SquashFS payload')
                if subprocess.run(['unsquashfs', '-s', '-o', str(offset), str(image)], capture_output=True).returncode == 0:
                    break
            appdir = root / 'AppDir'
            subprocess.run(['unsquashfs', '-q', '-no-progress', '-o', str(offset), '-d', str(appdir), str(image)], check=True, capture_output=True)
            # Retain the published AppRun, ELF launcher and executable wrapper.
            # Replace only the desktop binary with a real media-runtime probe.
            binary = appdir / 'usr/bin/beambench-tauri.bin'
            binary.write_text('''#!/usr/bin/env bash
set -euo pipefail
for element in appsink v4l2src videoconvert; do
  gst-inspect-1.0 "$element" >/dev/null
done
gst-launch-1.0 -q videotestsrc num-buffers=3 ! videoconvert ! appsink sync=false wait-on-eos=false
test -f "$FONTCONFIG_FILE"
test -n "$(fc-match sans-serif -f '%{file}')"
printf 'packaged camera pipeline passed\\n'
''')
            binary.chmod(0o755)
            env['GST_REGISTRY_1_0'] = str(root / 'registry.bin')
            env['APPDIR'] = str(appdir)
            previous = str(root / '.mount_RenamedLaser123')
            for inherited in [
                {},
                {'FONTCONFIG_FILE': previous + '/usr/share/beambench/fonts.conf',
                 'BEAMBENCH_APPIMAGE_FONT_POLICY': '1',
                 'GST_PLUGIN_SYSTEM_PATH': previous + '/usr/lib/gstreamer:',
                 'GST_PLUGIN_SYSTEM_PATH_1_0': previous + '/usr/lib/gstreamer-1.0:'},
                {'FONTCONFIG_FILE': previous + '/usr/share/beambench/fonts.conf',
                 'BEAMBENCH_APPIMAGE_FONT_POLICY': '1'},
                {'FONTCONFIG_FILE': '/etc/fonts/fonts.conf',
                 'GST_PLUGIN_SYSTEM_PATH_1_0': previous + '/usr/lib/gstreamer-1.0:'},
                {'FONTCONFIG_FILE': previous + '/usr/share/beambench/fonts.conf',
                 'BEAMBENCH_APPIMAGE_FONT_POLICY': '1',
                 'BEAMBENCH_APPIMAGE_RUNTIME_DIR': previous},
            ]:
                with self.subTest(inherited=inherited):
                    launch_env = dict(env, **inherited)
                    launch_env['LD_LIBRARY_PATH'] = previous + '/usr/lib:'
                    launch_env['PATH'] = previous + '/usr/bin:' + env['PATH']
                    result = subprocess.run([str(appdir / 'AppRun')], env=launch_env, check=True, capture_output=True, text=True, timeout=60)
                    self.assertIn('packaged camera pipeline passed', result.stdout)


if __name__ == '__main__':
    unittest.main()

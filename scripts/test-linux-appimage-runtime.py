#!/usr/bin/env python3
"""Exercise display selection, Fontconfig behavior, and actual AppImage repair."""
import hashlib
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import tempfile
import unittest

SCRIPTS = Path(__file__).resolve().parent


class RuntimePolicyTests(unittest.TestCase):
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

    @unittest.skipUnless(shutil.which('fc-match') and shutil.which('fc-list'), 'Fontconfig tools unavailable')
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


if __name__ == '__main__':
    unittest.main()

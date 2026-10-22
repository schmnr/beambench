#!/usr/bin/env python3
"""Require the final AppImage's actual frontend on old and new Ubuntu hosts.

Requires Docker on Linux x86_64. Never substitutes the desktop executable.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import uuid


def digest(path):
    result = hashlib.sha256()
    with path.open('rb') as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b''):
            result.update(block)
    return result.hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('appimage', type=Path)
    parser.add_argument('--ubuntu', nargs='+', choices=['22.04', '24.04', '26.04'],
                        default=['22.04', '24.04', '26.04'])
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    image = args.appimage.resolve(strict=True)
    scripts = Path(__file__).resolve().parent / 'linux-appimage-smoke'
    original_digest = digest(image)
    results = []
    # Unique tags prevent concurrent validations from sharing mutable images.
    for version in args.ubuntu:
        tag = f'beambench-desktop-{uuid.uuid4().hex}:ubuntu{version}'
        try:
            subprocess.run(['docker', 'build', '--build-arg', f'UBUNTU_VERSION={version}',
                            '-t', tag, str(scripts)], check=True, timeout=600)
            for scenario in ['x11', 'plasma-wayland-env']:
                run = subprocess.run([
                    'docker', 'run', '--rm', '--network', 'none', '--cap-drop', 'ALL',
                    '--memory', '2g', '--cpus', '2', '--pids-limit', '256', '--read-only',
                    '--tmpfs', '/tmp:rw,exec,mode=1777,size=1g',
                    '--tmpfs', '/home/beamtest:rw,uid=1100,gid=1100,mode=700,size=128m',
                    '--mount', f'type=bind,source={image},target=/test.AppImage,readonly',
                    '--mount', f'type=bind,source={scripts},target=/probe,readonly',
                    tag, 'python3', '/probe/probe.py', scenario,
                ], capture_output=True, text=True, timeout=90)
                result = json.loads(run.stdout)
                result.update(ubuntu=version, returncode=run.returncode)
                results.append(result)
                print(json.dumps(result), flush=True)
                args.output.write_text(json.dumps({
                    'sha256': original_digest, 'cases': results,
                }, indent=2) + '\n')
                if run.returncode or not result['frontend_ready']:
                    raise RuntimeError(f'Desktop failed on Ubuntu {version}: {scenario}')
        finally:
            subprocess.run(['docker', 'image', 'rm', tag], check=False)
    if digest(image) != original_digest:
        raise RuntimeError('AppImage changed during validation')


if __name__ == '__main__':
    main()

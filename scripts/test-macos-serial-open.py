#!/usr/bin/env python3
"""Exercise real serial opening with a simulated macOS stale-baud driver."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

if sys.platform != "darwin":
    sys.exit("This regression reproduces a macOS serial-driver failure.")

root = Path(__file__).resolve().parent.parent
build = subprocess.run(
    ["cargo", "test", "-p", "beambench-serial", "--lib", "--no-run", "--message-format=json"],
    cwd=root, text=True, stdout=subprocess.PIPE, check=True,
)
executables = [
    item["executable"]
    for line in build.stdout.splitlines()
    if (item := json.loads(line)).get("reason") == "compiler-artifact"
    and item.get("executable")
    and item["target"]["name"] == "beambench_serial"
    and item["profile"]["test"]
]
assert len(executables) == 1, executables

with tempfile.TemporaryDirectory(prefix="beambench-serial-regression-") as directory:
    library = Path(directory) / "stale-speed.dylib"
    subprocess.run(
        ["clang", "-dynamiclib", "-Wall", "-Wextra", "-o", str(library),
         str(root / "scripts/testing/macos-serial-stale-speed.c")], check=True,
    )
    env = dict(os.environ, DYLD_INSERT_LIBRARIES=str(library))
    result = subprocess.run(
        [executables[0], "--exact",
         "real::tests::macos_reopens_virtual_serial_port_and_exchanges_bytes",
         "--nocapture", "--test-threads=1"],
        cwd=root, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
    )
    print(result.stdout, end="")
    if result.returncode:
        sys.exit(result.returncode)
    assert "stale-speed injection: injected=1 rejected=0" in result.stdout, (
        "Driver injection did not run as expected; this is not a valid regression result."
    )

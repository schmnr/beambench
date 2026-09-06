# Serial open failure review

Report [feedback #29](https://github.com/schmnr/beambench-feedback/issues/29)
was submitted on macOS arm64 with Beam Bench 0.2.18. The connection events show
repeated failures opening a CH340 USB serial port with `Invalid argument`.
No RX or TX traffic was captured. The final error nevertheless claimed that
the controller had returned an unsupported protocol.

## Corrections

- Auto-detection now propagates the original structured transport error.
  It tries another protocol only after a completed, inconclusive GRBL probe.
  Port-open and transport-read failures no longer trigger misleading protocol
  retries or a claim that all listed baud rates were tried.
- The macOS/iOS serial initialization now normalizes the inherited termios
  speed before its first settings write. Previously this write could reapply
  a non-POSIX speed returned by `tcgetattr`, causing `tcsetattr` to reject it
  with EINVAL before the requested baud was applied. The later configuration
  step already performed this normalization. The requested baud still uses
  the existing IOSSIOSPEED path; Linux/Windows, port locking and DTR handling
  keep their upstream behavior.

The dependency correction is carried as a small runtime patch to vendored
serialport 4.9.0. See its [patch note](../../vendor/serialport/BEAMBENCH-PATCH.md).
The MPL-2.0 license and upstream source are included. Regenerating the license
report produced no change to the existing notices.

## Reproduction and checks

`python3 scripts/test-macos-serial-open.py` runs the native serial transport
against a macOS pseudo terminal with a test-only driver shim. The shim returns
an inherited non-POSIX speed and rejects any attempt to write it back. It does
not communicate with hardware and is never loaded by the application.

A separate executable using unpatched crates.io serialport 4.9.0 failed in
`RealSerialTransport::open` with `ConnectionFailed("Invalid argument")`. The
shim confirmed one injected speed and one rejected settings write. The patched
regression requires the same injection, zero rejected writes, a successful
open, and byte exchange in both directions.

The service tests cover preservation of the report's exact error, structured
error details, a missing port through the public connection entry point,
continued protocol fallback after an inconclusive probe, and feedback that
preserves the opening failure without inventing received data.

The CI workflow now runs the serial library tests and the simulated stale-speed
regression on macOS for pull requests and main-branch changes.

Final local results:

| Check | Result |
| --- | --- |
| Service library suite | 765 passed |
| Serial library suite | 22 passed |
| Stale-speed injected-driver regression | Passed, one injection and zero rejected writes |
| Unpatched 4.9.0 comparison | Failed as expected with `Invalid argument`, one rejected write |
| `cargo check -p beambench-tauri` | Passed |
| `cargo clippy -p beambench-serial --all-targets` | Passed |
| Changed Rust formatting, Python syntax, workflow YAML, staged whitespace | Passed |

There are 787 unique Rust tests in these two suites. The driver regression
reruns one serial test under fault injection and is not added to that count.

## Evidence limits

The customer's report does not identify which system call returned EINVAL.
The initialization failure is reproduced in software and corrected, but that
does not establish its cause on the customer's particular hardware. The
error-classification defect is directly established by both the report and
the source. Additional physical-controller and Windows/Linux checks were
declined by the user and are not required for this update.

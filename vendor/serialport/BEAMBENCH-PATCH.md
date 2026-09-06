# Beam Bench serialport patch

Based on the crates.io source of serialport 4.9.0, checksum
`a4d91116f97173694f1642263b2ff837f80d933aa837e2314969f6728f661df3`.
The upstream MPL-2.0 license and notices are retained.

The only runtime change is in `src/posix/tty.rs`: on macOS/iOS, normalize the
initial termios input and output speeds to B9600 before the first `tcsetattr`.
The later settings write already uses this intermediate speed and applies the
requested baud with IOSSIOSPEED. Previously the initial write reused whatever
speed `tcgetattr` returned, including non-POSIX values left by IOSSIOSPEED, which
some drivers reject with EINVAL before the requested baud is applied.

This keeps the existing IOSSIOSPEED implementation, requested baud, exclusivity,
error checks, and DTR behavior. Other platforms retain the upstream code.

Run `python3 scripts/test-macos-serial-open.py` from the Beam Bench root for a
driver-fault regression using a real macOS pseudo terminal. The test-only dylib
injects a stale speed and rejects any attempt to reapply it. It is never loaded
by the app or packaged in the executable.

The customer report establishes an EINVAL failure during port opening, but
does not identify its exact failing system call. This patch fixes a reproduced
initialization failure with that symptom; it does not establish that every
macOS EINVAL or the reporting customer's hardware has been verified.

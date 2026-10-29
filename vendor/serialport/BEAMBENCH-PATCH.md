# Beam Bench serialport patch

Based on the crates.io source of serialport 4.9.0, checksum
`a4d91116f97173694f1642263b2ff837f80d933aa837e2314969f6728f661df3`.
The upstream MPL-2.0 license and notices are retained.

The macOS runtime change is in `src/posix/tty.rs`: on macOS/iOS, normalize the
initial termios input and output speeds to B9600 before the first `tcsetattr`.
The later settings write already uses this intermediate speed and applies the
requested baud with IOSSIOSPEED. Previously the initial write reused whatever
speed `tcgetattr` returned, including non-POSIX values left by IOSSIOSPEED, which
some drivers reject with EINVAL before the requested baud is applied.

This keeps the existing IOSSIOSPEED implementation, requested baud, exclusivity,
error checks, and macOS DTR behavior.

Run `python3 scripts/test-macos-serial-open.py` from the Beam Bench root for a
driver-fault regression using a real macOS pseudo terminal. The test-only dylib
injects a stale speed and rejects any attempt to reapply it. It is never loaded
by the app or packaged in the executable.

The customer report establishes an EINVAL failure during port opening, but
does not identify its exact failing system call. This patch fixes a reproduced
initialization failure with that symptom; it does not establish that every
macOS EINVAL or the reporting customer's hardware has been verified.

On Windows, `src/windows/dcb.rs` and `com.rs` apply an explicit
`dtr_on_open(true)` in the first DCB configuration, instead of forcing DTR low
before asserting it later. The default and explicit-low opening policies are
unchanged. This lets the native xTool D1 adapter match Studio's high opening
level without the intermediate low transition. The Windows CI job runs the
vendor library unit tests for the initial DCB flags as well as service tests.

Windows `src/windows/enumerate.rs` always merges unique COM port names from
`HARDWARE\DEVICEMAP\SERIALCOMM` into SetupAPI discovery, preserving SetupAPI's
USB metadata. Previously the merge ran only when the registry contained more
ports, omitting registry-only ports when the counts were equal or smaller.
The Windows vendor library tests cover both cases, duplicates and empty lists.
The same backend also enumerates present, active `GUID_DEVINTERFACE_COMPORT`
interfaces independently of setup classes. It retains associated USB metadata
and includes working interface/registry results if setup-class lookup fails.
When `CreateFileW` cannot find a COM alias, `com.rs` can open the live interface
registered to that exact COM name instead. It preserves requested settings and
rejects ambiguous mappings. Access errors do not trigger fallback. This fixes
discovery/opening for live interfaces without a working legacy registration; it
cannot restore a physically absent device or install a missing driver.

The before-and-after Windows regressions and their modeled-device limits are
described in `docs/testing/windows-serial-interface-recovery.md` in the repository
root. The existing CH340 initialization recovery remains required.

# Windows CH340 connection failure investigation

Investigated October 27, 2026 against Beam Bench 0.2.34, source
`6ecf691eedd5`. The diagnostic correction and automatic recovery for the
documented CH340 initialization fault below were released in 0.2.35.

## What the report establishes

The report records one detected CH340 adapter, one connection attempt at
115200 baud, and an `open_failed` event about 1.6 seconds later. It has no
serial TX/RX, controller banner, status response or identified firmware.
The localized Windows message means "A device attached to the system is
not functioning."

That message corresponds to Win32 `ERROR_GEN_FAILURE`, value 31, in
[Microsoft's system error table](https://learn.microsoft.com/en-us/windows/win32/debug/system-error-codes--0-499-).
The report does not record the numeric error itself. This is a match to the
message, not an independently captured error number. Win32 error 31 is also
distinct from Device Manager's separately defined problem code 31.

The failure occurs in native serial opening or initialization, before the
GRBL status query. Firmware selection, homing, workspace placement, S-value
scaling and job output are not reached. The port's `available: true` field
only means Beam Bench does not own it; enumeration does not test whether
Windows can open and configure the device. The absent USB-driver field does
not establish that no driver is installed.

## Independent comparisons

- [AVRDUDE issue 1328](https://github.com/avrdudes/avrdude/issues/1328)
  records CH340 driver 3.8 failures at `SetCommState` and successful operation
  with older drivers. Later comments report machine-dependent behavior and
  an unreliable serial-enumerator workaround.
- [Arduino's CH340 support article](https://support.arduino.cc/hc/en-us/articles/13148652511260-avrdude-ser-open-can-t-set-com-state-for-COMn)
  documents serial-configuration failure with particular CH340 batches and
  driver versions. It does not establish the driver or chip batch here.
- [jSerialComm issue 495](https://github.com/Fazecast/jSerialComm/issues/495)
  records the same Win32 error 31 at `SetCommState`. Another serial library
  reportedly worked, so this error alone cannot exonerate application-side
  initialization or establish broken hardware.
- [Arduino Cloud Agent issue 1000](https://github.com/arduino/arduino-create-agent/issues/1000)
  concerns a different CH340 failure after opening: immediate empty reads
  with driver 3.9.2024.9. Its read-timeout workaround does not explain an
  initialization failure before any serial traffic.

The leading explanation is rejection during Windows USB/serial driver
initialization. The original bundle cannot distinguish `CreateFileW`,
`GetCommState`, `SetCommState` or `SetCommTimeouts`: the vendored serial
library discarded both the operation and numeric Windows error.
These comparisons do not establish a specific driver version or physical
fault in the original report.

The [jSSC 2.8.0 Windows source](https://github.com/scream3r/java-simple-serial-connector/blob/2.8.0/src/cpp/windows/jssc.c%2B%2B)
also requires successful `GetCommState` and `SetCommState` calls. It uses
overlapped I/O and different control defaults, but supplies no documented
CH340 error-31 recovery that can be transplanted and validated here.

## Subsequent recovery evidence

Further investigation found a specific software workaround for the high-baud
opening failure, rather than a driver-replacement recommendation:

- [GasVosky's January 2025 firsthand report](https://yoko-hama.blogspot.com/2025/01/wch-ch340.html)
  describes CH340 driver 3.9.2024.9 on Windows 10 rejecting an initial baud
  above 9600. Applying 9600 with `SetCommState`, then applying the desired
  baud with another `SetCommState`, restores communication.
- [TheBackShed's April 2024 hardware investigation](https://thebackshed.com/forum/ViewTopic_mobile.php?P=4&TID=16761)
  independently reports CH340 Nano boards requiring a 9600-baud connection
  before they accept normal high-speed programming. Changing the board's
  bootloader did not alter the behavior. This supports an adapter
  initialization problem rather than a GRBL handshake change.
- [SerialInit 0.2](https://sourceforge.net/projects/serialinit/files/serialinit.dll_0.2.zip/)
  is a published CH340 initialization workaround, also referenced by users
  in the Arduino and PlatformIO forums. Read-only disassembly of its
  `_configure_serial_port` function confirms `CreateFileA`, `GetCommState`,
  a 9600-baud 8N1 `SetCommState`, and timeout configuration. Its main function
  then closes the handle. No downloaded binary was executed or added to
  Beam Bench. The inspected executable's SHA-256 is
  `6f6f45c940b9eaa366f12c50c5e558e9662e714b83c1b3554b8949c957659c2e`.

The application now attempts the requested settings normally. If and only
if opening configuration returns Win32 `ERROR_GEN_FAILURE`, the target baud
is above 9600, and Windows identifies that exact port as USB VID/PID
`1a86:7523`, it applies 9600 and then the requested settings on the same
handle. There is no baud-rate guessing or serial I/O at 9600. The original
data format, flow control and DTR/RTS settings are identical across attempts.

The recovered settings are read back and checked, including baud, data bits,
parity, stop bits and control flags. Recovery failure or mismatched readback
closes the handle through the existing ownership guard. A 9600-baud port is
never returned in place of the requested baud. Successful opens, other USB
chips, unknown identities, access-denied errors, invalid settings, device-open
errors and failed `GetCommState` calls retain their previous behavior. Runtime
baud changes do not enter this opening-only recovery.

## Confirmed application defects and correction

The native error converter now retains the numeric Windows code alongside
the localized message. Each of the four opening/configuration calls names
its operation. For example, a configuration failure can now say
`SetCommState failed: ... (Windows error 31)` rather than only the localized
OS message. Existing error kinds for access-denied and missing devices are
preserved, so localized port-access guidance still works.

Feedback previously assigned `serial_port_unavailable` to every unclassified
`open_failed` event. It now uses `serial_open_failed` unless the transport
supplied a specific error code. This keeps device/configuration errors
distinct from explicit port-access failures and from an opened but silent
or unrecognized controller. The original native detail remains in the bundle.

The earlier diagnostic changes alone did not alter connection behavior.
The follow-up changes Windows opening configuration as described above;
GRBL probes and machine commands remain unchanged. There is no driver
installation, system preference change or new user setting.

## Validation

The service regression exercises the original localized failure and errors
that identify `GetCommState` and `SetCommState`. It checks that the feedback
bundle retains the attempted port and exact error without classifying it
as port unavailability, missing controller response or unrecognized traffic.

Native Windows tests call the real opening/configuration APIs against a
missing device or invalid handle. They verify distinct API names and Windows
error codes. A separate test sets the thread's Windows last-error value to
31 and checks its preservation; this is an error-propagation test, not an
emulation of the CH340 driver. Existing DTR tests remain in the same suite.

The recovery regression also runs the production `COMPort::open` and native
read/write paths through a test-only Win32 boundary that reproduces the
documented 9600-first fault. Real Windows event handles make cleanup
observable. With recovery disabled, the original opening path must fail with
error 31. With recovery enabled, it must reach the requested 115200 baud,
send a read-only status query and receive a controller status response.
Tests cover all opening DTR policies and supported flow-control modes, and
reject failed bootstrap, failed target configuration, mismatched baud or
control flags, and failed readback. Negative cases verify that no bytes are
sent and the handle closes. This models the documented driver behavior; it
is not physical hardware validation.

Commands:

```sh
cargo test -p beambench-service --lib ops::feedback::tests
cargo test -p beambench-serial --lib
pwsh -File scripts/test-windows-ch340-recovery.ps1
cargo fmt --all -- --check
rustfmt --edition 2021 --check vendor/serialport/src/windows/{com,dcb,error}.rs
git diff --check
```

Local validation passed all 35 feedback tests and 33 serial-transport tests.
The new feedback regression also failed when the old classification was
temporarily restored, then passed with the correction. Formatting and diff
checks passed. The Windows library and tests passed cross-target compilation.

The vendored serial library is checked for `x86_64-pc-windows-msvc` from an
isolated copy. Windows CI requires the before-and-after recovery regression
as well as the existing service/transport suites. No physical CH340 or laser
is available in this environment. The recovery addresses a documented
initialization fault consistent with the original report; it cannot repair
all causes of Win32 error 31 or establish which driver the reporter used.

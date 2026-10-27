# Windows CH340 connection failure investigation

Investigated October 27, 2026 against Beam Bench 0.2.34, source
`6ecf691eedd5`. The diagnostic correction below is unreleased.

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
No specific driver version, physical fault or working compatibility
fallback is established by these comparisons.

The [jSSC Windows source](https://github.com/scream3r/java-simple-serial-connector/blob/master/src/cpp/windows/jssc.c%2B%2B)
also requires successful `GetCommState` and `SetCommState` calls. It uses
overlapped I/O and different control defaults, but supplies no documented
CH340 error-31 recovery that can be transplanted and validated here.

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

These are diagnostic corrections. They do not establish that the original
CH340 connection succeeds. Connection calls, settings, DTR behavior and
protocol probes are unchanged. No driver replacement or machine command is
added.

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

Commands:

```sh
cargo test -p beambench-service --lib ops::feedback::tests
cargo test -p beambench-serial --lib
cargo fmt --all -- --check
rustfmt --edition 2021 --check vendor/serialport/src/windows/{com,dcb,error}.rs
git diff --check
```

Local validation passed all 35 feedback tests and 33 serial-transport tests.
The new feedback regression also failed when the old classification was
temporarily restored, then passed with the correction. Formatting and diff
checks passed. The Windows library and tests passed cross-target compilation.

The vendored serial library is checked for `x86_64-pc-windows-msvc` from an
isolated copy, matching the existing Windows CI setup. Windows CI executes
the native tests as well as the service/transport suites. No physical CH340
or laser is available in this environment. An actual driver compatibility
fix requires an independent Windows/CH340 lab reproduction across driver
versions; the original report is not marked resolved by this change.

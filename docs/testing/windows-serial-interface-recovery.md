# Windows serial interface discovery and opening

Investigated October 28, 2026 after the 0.2.36 release. This correction is
included in 0.2.37.

## Evidence and limits

A Windows 0.2.35 report records repeated successful scans returning zero serial
ports. Opening the saved COM3 endpoint fails at `CreateFileW` with Windows error
2, both at 115200 and 9600 baud. No handle is obtained, so baud configuration,
CH340 initialization recovery and GRBL detection are never reached. No serial
bytes are transmitted or received.

Earlier reports with the same saved profile also have zero ports and missing
COM3 errors. Neither the new bundle nor those earlier bundles record the USB
device identity, PnP status, driver version or serial device interfaces. They do
not establish that this controller uses a CH340. They also cannot distinguish a
missing driver or disconnected device from a live interface with a broken or
unavailable legacy COM registration. The reporter's underlying OS/device state
remains unconfirmed.

## Connection behavior corrected

The vendored Windows backend previously enumerated the Ports and Modem setup
classes and `HARDWARE\DEVICEMAP\SERIALCOMM`. It opened only the requested legacy
COM name. A device exposed through a live serial interface could therefore be
missed, and a missing COM alias ended the attempt even if that device's interface
was still accessible.

[Microsoft recommends discovering and accessing COM ports through
`GUID_DEVINTERFACE_COMPORT`](https://learn.microsoft.com/en-us/windows-hardware/drivers/install/guid-devinterface-comport).
The backend now enumerates present, active interfaces independently of the
legacy setup classes. It obtains their associated device information and paths
using [SetupAPI](https://learn.microsoft.com/en-us/windows/win32/api/setupapi/nf-setupapi-setupdigetdeviceinterfacedetailw).
It keeps USB metadata and merges by port name. An error looking up legacy setup
classes no longer discards working interface or registry results.

If opening a COM alias returns `ERROR_FILE_NOT_FOUND` or `ERROR_PATH_NOT_FOUND`,
the backend looks for that exact COM name on a live interface and opens the
returned path. It then runs the normal configuration and transport path at the
requested baud. It does not try another COM port. Conflicting registrations do
not select a device. Access-denied and other opening errors do not trigger this
fallback. A failed interface open retains its own native error.

There is no driver installation, registry change, PnP reset or serial probing
during discovery. An interface without a legacy COM name can be listed using its
interface path. A nonexistent device with no live interface remains unavailable.

## Validation

`scripts/test-windows-ch340-recovery.ps1` runs before-and-after controls in an
isolated copy of the vendor crate. It disables interface merging to reproduce
empty discovery, then separately disables interface opening to reproduce Windows
error 2. Both controls must fail their regression before the complete corrected
suite passes.

The test-only driver supplies a live interface with no legacy registration and
models native COM-alias failure. Production discovery merging and `COMPort::open`
must discover the intended device, open its interface, retain the requested
115200 baud and exchange a read-only status query/response. Tests cover both
missing-name errors, DTR policies and flow-control modes. Real Windows event
handles make handle cleanup observable. Negative cases cover absent and
ambiguous identities, unrelated COM ports, access errors and interface-open
failure. Port-name matching covers case, the Win32 COM prefix and malformed
Unicode names.

A separate read-only scan invokes the actual SetupAPI enumeration. The injected
interface fixture does not exercise SetupAPI's device-detail marshalling. It is
a model of the supported Windows registration state, not a reproduction on the
reporter's machine or physical laser validation. No reporting machine, physical
USB adapter or laser is available here. Passing this regression establishes a
working recovery path for that state; it does not establish which missing-device
cause affected the report.

The existing CH340 9600-first recovery regression remains required alongside
these tests. Cross-target compilation and required Windows CI validate the
Windows-only code before merge.

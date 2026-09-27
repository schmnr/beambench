# Linux Mint BRLTTY serial disconnect investigation

The report is consistent with a documented operating-system USB ownership
conflict. It is not evidence of incorrect engraving settings. Stopping BRLTTY
and then completing jobs is strong supporting evidence, but the customer's
kernel log and package versions are needed to confirm this particular case.

## Evidence

The customer reports intermittent immediate disconnections on Linux Mint,
followed by reliable operation after stopping and disabling `brltty`. The photo
shows an outline-only result above a filled result. It cannot establish USB
ownership, command order, or the exact point at which communication failed.

Upstream evidence checked on 2026-09-19:

- [BRLTTY maintainer discussion of CH340/CH341 interference](https://brltty.app/pipermail/brltty/2022-September/019513.html)
  includes a kernel trace where BRLTTY reconfigures a device after `ch341` has
  attached it, and the serial port disappears. The report concerns Ubuntu
  22.04.1 and BRLTTY 6.4-4ubuntu3.
- [Arduino's Linux Mint report](https://github.com/arduino/arduino-ide/issues/1788)
  documents the same class of CH340 problem on Mint 21.
- [Arduino's FTDI reproduction](https://github.com/arduino/help-center-content/issues/155)
  shows `ftdi_sio` losing its interface when BRLTTY sets the USB configuration.

Some braille devices use generic USB-to-serial identifiers shared by unrelated
equipment. A matching BRLTTY auto-detection rule can therefore select a laser's
adapter. This happens below Beam Bench's serial connection. A cable can remain
plugged in while the OS removes the usable serial interface.

These reports establish the mechanism, not that every Mint release, BRLTTY
version, CH340, or FTDI adapter has the problem. The customer's Mint version,
BRLTTY package version, and adapter identity are currently unknown.

## The reported workaround

`systemctl stop brltty` stops that service now. `systemctl disable brltty`
removes its enablement links; it is not a blanket prohibition against future
activation. The [systemd manual source](https://github.com/systemd/systemd/blob/main/man/systemctl.xml)
distinguishes disabling from masking and warns about active triggering units.
BRLTTY's [upstream USB rules](https://github.com/brltty/brltty/blob/master/Autostart/Udev/usb-template.rules.in)
can request `brltty-device@.service` through udev. Distribution packaging varies,
so those two commands are not a universal permanent fix.

BRLTTY provides braille accessibility. Do not automatically stop, disable,
mask, or remove it in Beam Bench, or recommend that all Linux users do so.
If a user needs braille support, retain it and investigate a device-specific
configuration or rule correction. Current [BRLTTY device documentation](https://brltty.app/doc/Devices.html)
describes USB device selectors and generic-device filtering, but the installed
version and activation path must be checked before prescribing a configuration.
If braille support is unused and the conflict is confirmed, the user can choose
a distribution-appropriate workaround after reviewing its scope.

## Confirming an affected machine

Read-only information to collect on the affected Linux computer:

```sh
cat /etc/os-release
dpkg-query -W brltty
lsusb
systemctl list-units --all 'brltty*' --no-pager
journalctl -k -b --no-pager | grep -Ei 'brltty|ch341|ftdi_sio|ttyUSB|usbfs'
```

Look for a BRLTTY USB reconfiguration followed by the same adapter's serial
disconnect at the time of failure. A missing journal entry does not rule it
out if access is restricted, the failure was on an earlier boot, or logs have
rotated. The mere presence of the BRLTTY package or a running service is not
proof that it claimed this device. Recheck after reconnecting USB and rebooting
when validating persistence of a workaround.

## Beam Bench findings

- `crates/beambench-serial/src/real.rs` propagates input-buffer query, read,
  and write failures. It cannot prevent another OS service from reconfiguring USB.
- `crates/beambench-service/src/ops/machine.rs` clears a stale session after a
  fatal streaming error and retains job diagnostics. Idle polling retries a
  transient error before treating repeated failures as lost communication.
  Reset and laser-off writes during teardown are best-effort; a severed
  transport cannot confirm a hardware stop.
- At the start of the investigation, serial-error guidance covered permissions
  and unavailable ports but had no BRLTTY-specific check or guidance. A generic
  I/O error alone cannot establish this cause.

Existing regression tests passed on the current macOS checkout, including
fatal job read failure, failed-job session cleanup, and idle polling recovery
and repeated-error disconnection. The two filtered test runs passed 10 tests
in total, including other tests matched by the filters. These are simulated
transport tests, not reproduction of BRLTTY on Mint or physical laser tests.

## Implemented response

Connection Diagnostics now flags BRLTTY as a possible cause when a visible
BRLTTY process is running on Linux and the attempted USB serial port is missing
or failed to open. It suppresses this hint for healthy/connecting sessions,
network transports, ordinary UART paths, and explicit permission errors.
Existing `/dev/serial/` aliases are not considered missing solely because port
enumeration lists their underlying tty names.

The read-only collector includes the distribution release, visible BRLTTY
process status, and Debian-family package version when available. It reads
bounded OS/package metadata and process names, caches Linux results for five
seconds, and does not execute commands, collect kernel logs or process IDs, or
modify services. Unavailable probe results remain unknown. These optional
details are omitted from minimized successful-job compatibility reports.

Expandable troubleshooting in all 23 locales explains log confirmation, a
conditional temporary stop when braille support is unused, how to restore the
service, and distribution-specific activation caveats. Commands are displayed
for the user to review and run. See the
[Linux troubleshooting guide](../linux-serial-troubleshooting.md).

Validation on macOS passed 36 targeted Rust tests (4 common feedback, 30 service
feedback, 2 Linux filesystem fixture tests) and 191 frontend/localization tests.
These cover warning conditions and exclusions, report serialization and privacy
minimization, and the troubleshooting UI. TypeScript and focused ESLint checks
also passed. English and French layouts were checked in a 340 px browser panel
using simulated Linux diagnostics, with no browser console errors.
Mint runtime and physical USB/laser reproduction remain unverified; this change
adds diagnosis and guidance rather than changing Linux USB ownership or job
streaming behavior.

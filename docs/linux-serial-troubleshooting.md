# Linux USB serial disconnects and BRLTTY

If a USB laser disappears from the port list or disconnects immediately, open
**Connection Diagnostics** in Beam Bench. On Linux, the panel can flag a possible
BRLTTY conflict when BRLTTY is running and an attempted USB serial connection
has failed to open or its port has disappeared. A warning is a lead to check,
not confirmation of the cause.

BRLTTY supplies braille accessibility. Some versions and device rules can
mistake common CH340/CH341 or FTDI USB adapters for braille devices. This can
remove the serial interface while the USB cable remains plugged in. See the
[upstream CH341 report](https://brltty.app/pipermail/brltty/2022-September/019513.html)
and [FTDI reproduction](https://github.com/arduino/help-center-content/issues/155).
Other causes include USB power, cables, controller resets, and other serial apps.

## Confirm the cause

With the laser idle, inspect the kernel log on the affected Linux computer:

```sh
sudo journalctl -k -b --no-pager | grep -Ei 'brltty|ch341|ftdi_sio|ttyUSB|usbfs'
```

Look for BRLTTY claiming or reconfiguring the laser's USB device immediately
before its serial port disconnects. A running BRLTTY process alone does not
prove a conflict. Missing entries may mean the problem has another cause or
the relevant log is unavailable. `-b` selects the current boot.

Beam Bench records the Linux distribution name/version, visible BRLTTY process
status, and installed BRLTTY package version when available. Package-version
detection currently supports Debian-family systems such as Mint and Ubuntu.
Unavailable information remains unknown. The app reads these details without
running commands or collecting kernel logs, process IDs, or command lines.
Use **Save Diagnostics** or review **Send to Beam Bench** to share a report.

## Try a temporary workaround

Only if logs confirm the conflict and you do not depend on braille support,
you can temporarily stop the main service. Keep the laser idle while changing
services or reconnecting USB:

```sh
sudo systemctl stop brltty.service
```

Reconnect USB, refresh the port list, and reconnect in Beam Bench. Verify the
connection with laser-off operations before starting a new job. An interrupted
job is not automatically resumed.

To restore the service:

```sh
sudo systemctl start brltty.service
```

Service names and activation mechanisms vary. If this unit does not exist or
BRLTTY starts again when USB is reconnected, inspect the installed units:

```sh
systemctl list-units --all 'brltty*' --no-pager
```

Use your distribution's BRLTTY/device-rule guidance for a persistent correction.
Stopping a service is temporary. Disabling its startup links is also not a
guarantee against USB-triggered activation: upstream [BRLTTY udev rules](https://github.com/brltty/brltty/blob/master/Autostart/Udev/usb-template.rules.in)
can start a device-specific service. Avoid blindly removing packages or masking
every BRLTTY unit.

If you use a braille display, keep BRLTTY available and seek a configuration
that excludes the laser adapter while retaining the display. The appropriate
rule depends on the installed package, USB identifiers, and braille hardware.
Beam Bench does not alter system services or USB rules.

If the error is **permission denied**, check serial-device permissions and your
distribution's serial group separately. BRLTTY guidance is suppressed for this
error because stopping a service does not grant serial-device access.

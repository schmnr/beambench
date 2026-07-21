# Controller Compatibility

Beam Bench supports controllers by their control board and firmware, not by
the laser source. A CO2 laser may use Ruida, Lihuiyu, GRBL, or another
controller, so select the controller that is actually installed in the
machine.

`Experimental` means the adapter is available for normal beta use and has
passed its software, protocol, and virtual-controller tests, but its real-world
compatibility is still being built from user reports. The app labels these
choices once in the controller selector and does not require an additional
Experimental confirmation.

## Current Compatibility

| Controller choice | Connection | Status | Current scope |
| --- | --- | --- | --- |
| GRBL | Serial | Supported | Existing GRBL job, framing, homing, jogging, unlock, origin, pause/resume, and status workflow. |
| FluidNC | Serial or Network (TCP, normally port 23) | Experimental | Requires an exact FluidNC identity and uses the normal GRBL-family job and machine controls. |
| grblHAL | Serial or Network (TCP, normally port 23) | Experimental | Requires exact grblHAL firmware identity and uses the normal GRBL-family job and machine controls. |
| LaserPecker LX1 / LX1 Max, LP2 Plus, LP4 / LP4 Safeguard, LP5 | Serial (460800 baud) | Experimental | Explicit LaserPecker adapter with official-profile workspaces, power scales, top-left coordinates, regular-mode commands, dual-laser selection for LP4/LP5, and shared GRBL-family jobs and controls. |
| LaserPecker LX2 | Network (GRBL/TCP port 8888) | Experimental | Explicit LaserPecker adapter with the official 500 x 305 mm, S0-1000 profile and required `START_PRINT` job header. |
| Generic GRBL-compatible | Serial | Experimental | Explicit fallback for unidentified or rebranded GRBL-compatible firmware. Job, frame, jog, unlock, origin, and pause/resume are available; homing remains hidden. |
| Standard Marlin | Serial | Experimental | Requires a standard Marlin `M115` identity. Vector, raster, perforation, frame, custom G-code, air-assist, Z-offset, and finish-position output are supported. Manual motion and pause/resume are not yet exposed; cancel requires reconnecting. |
| Snapmaker 2.0 | Serial | Experimental | Requires an exact Snapmaker 2.0 firmware identity and uses its documented laser-power commands. Jobs and framing are supported; manual motion and pause/resume are not yet exposed, and cancel requires reconnecting. Artisan is not included in this row. |
| Smoothieware | Serial | Experimental | Requires an exact Smoothieware identity and an enabled laser configuration. Jobs and framing are supported; manual motion and pause/resume are not yet exposed, and cancel requires reconnecting. |
| Ruida RDC6442S | Network (UDP port 50200) | Experimental | Exact Ethernet controller row with native vector/raster jobs, zero-output framing, XY home, finite output-disabled XY jogging, configurable Z- or U-channel lift-table jogging, pause/resume, cancel, completion confirmation, and controller-file cleanup. |
| Lihuiyu M2/M3 Nano | USB (`1a86:5512`) | Experimental | Stock K40-class CH341 board in M2-compatible mode. Supports vector/raster jobs, perforation, zero-output framing, home, unlock, finite XY jogging, pause/resume, cancel, and completion status. |

Auto-detect is also available for Serial and Network connections. It activates a
named adapter only when the controller provides matching identity evidence. If
identity is inconclusive, Beam Bench asks the user to choose a controller
instead of silently treating the machine as GRBL.

## Connecting

1. Open the Laser panel or **Device Settings > Connection**.
2. Choose **Serial**, **Network**, or **USB** for the machine's actual
   connection.
3. Choose the controller. Experimental choices carry the label directly in
   this list.
4. Select the serial port, enter the network host and port, or select the USB
   device, then choose **Connect**.

For FluidNC and grblHAL network connections, TCP port 23 is the normal default.
For LaserPecker LX2, choose **LaserPecker (Experimental)** with the Network
connection; the form defaults to `192.168.253.1` and TCP port `8888`. For the
other listed LaserPecker models, choose Serial and apply the matching built-in
machine preset; it supplies the 460800 baud rate and model-specific job settings.
For Ruida RDC6442S, use the controller's IP address and UDP port 50200. For a
stock K40/Lihuiyu board, choose USB; Beam Bench lists only matching CH341
`1a86:5512` devices and validates the controller before entering Ready.

### CLI And Local API

The standalone CLI uses the running app's Local API. Enable **Settings >
General > Local API**, then use the explicit connection command for the
machine's transport:

```bash
beambench-cli machine connect-serial --port /dev/ttyUSB0 --controller marlin
beambench-cli machine connect-serial --port /dev/ttyUSB0 --baud 460800 --controller laser-pecker
beambench-cli machine connect-network --host 192.168.253.1 --controller laser-pecker
beambench-cli machine connect-network --host 192.168.1.100 --controller ruida
beambench-cli machine list-lihuiyu-usb
```

Serial controller values are `auto-detect`, `grbl`, `fluid-nc`, `grbl-hal`,
`laser-pecker`, `marlin`, `snapmaker`, `smoothieware`, and
`generic-grbl-compatible`. Network controller values are `auto-detect`,
`fluid-nc`, `grbl-hal`, `laser-pecker`, and `ruida`. LaserPecker defaults to TCP
port 8888, Ruida defaults to UDP port 50200, and the other Network choices
default to TCP port 23. `list-lihuiyu-usb` returns the bus, address, and physical port chain needed
by `connect-lihuiyu`.

The corresponding Local API routes are:

- `POST /api/v1/machine/connect/controller/serial`
- `POST /api/v1/machine/connect/controller/network`
- `POST /api/v1/machine/connect/controller/continue`
- `GET /api/v1/machine/usb/lihuiyu`
- `POST /api/v1/machine/connect/usb`

Serial and Network calls may return a backend-owned `challenge` when Auto-detect
needs a controller choice or when detected identity disagrees with the selected
adapter. Pass its `attempt_id`, the chosen `selection`, and any returned mismatch
decision to the continuation route. The CLI prints the equivalent
`continue-controller` command when this occurs.

### Stock K40/Lihuiyu Notes

The first Lihuiyu row intentionally uses the M2-compatible command set on both
M2 and M3 Nano boards. Laser power remains controlled by the physical machine
panel: Beam Bench converts software power to beam off or beam on and does not
send unverified M3-only power or hardware-parameter commands.

The USB backend uses IOKit on macOS, usbfs on Linux, and WinUSB on Windows. If
the controller is listed but cannot be opened, check Linux USB permissions or
the active Windows device driver, then reconnect the controller and refresh the
USB list.

### Ruida Notes

The first Ruida row is deliberately exact: RDC6442S over Ethernet using the
identified card and protocol variant. Other Ruida models are not silently
treated as compatible. For a motorized bed, set **Ruida lift table axis** to Z
or U in the active machine profile, matching the channel shown by the machine's
Ruida controller; Beam Bench does not guess the wiring. This enables finite
manual table steps only. It does not enable automated job Z offsets or Focus
Test motion. Absolute positioning, Start From Current Position, continuous
jog, work-origin changes, manual fire, rotary, dual-head, USB, and
controller-parameter writes are not part of this row.

### LaserPecker Notes

The built-in profiles cover LX1, LX1 Max, LX2, LP2 Plus, LP4 (including the LP4
Safeguard base engraver), and LP5. LP4 and LP5 have separate 450 nm and 1064 nm
presets so every job selects the intended laser source. The serial path avoids a
DTR reset. Both serial and network LaserPecker paths avoid controller-settings
queries; the official LX2 profile explicitly disables settings fetches, and the
published serial profiles do not require them. Connection readiness comes from
a fresh idle/output-off GRBL status response instead.

LaserPecker's current firmware requirements for third-party G-code control are
LX1 V7009 or later, LX2 V9008.1 or later, and LP5 V8.0.0 or later. The LP1
series and LP1 Plus are not included because LaserPecker does not publish a
compatible desktop GRBL/Lbrn control path for them. LP2 (including LP2
Safeguard) and LP3 are not included because LaserPecker currently documents
those controllers as incompatible with Lbrn. Slide and rotary accessory
modes are not built-in presets yet; their documented `M3031`/`M3032` commands
can be used through a custom profile. On LX2, software Pause and Stop may be
delayed while the current command finishes, so they are not a substitute for
the machine's physical stop or power control.

Profile and firmware facts come from LaserPecker's official Lbrn guides
for [LX1](https://support.laserpecker.net/hc/en-us/articles/7639480826895-Operating-LX1-with-Lbrn),
[LX2](https://support.laserpecker.net/hc/en-us/articles/14362951661455-Operating-LX2-with-Lbrn),
[LP2 Plus](https://support.laserpecker.net/hc/en-us/articles/14066572295695-Operating-LP2-Plus-with-Lbrn),
[LP4](https://support.laserpecker.net/hc/en-us/articles/7638927193231-Operating-LP4-with-Lbrn),
and [LP5](https://support.laserpecker.net/hc/en-us/articles/10710301115535-Operating-LP5-with-Lbrn),
plus its current [firmware compatibility table](https://support.laserpecker.net/hc/en-us/articles/9793175374735-Firmware-Compatibility).

## Reporting A Controller Problem

Use **Help > Report a Bug...** when a controller fails to connect or an action
behaves incorrectly. Include the machine and controller-board model, firmware
version if known, operating system, connection type, and what happened. Beam
Bench's diagnostic preview supplies the connection and controller details that
are available to the app.

## Not Included In This Release

Trocen, TopWisdom, and galvo controllers such as EZCAD/JCZ/BSL are deferred.
They are not presented as live controller choices in this release.

Detailed interoperability and licensing boundaries are recorded in the
[Ruida feasibility decision](ruida-feasibility.md) and
[Lihuiyu feasibility decision](lihuiyu-feasibility.md).

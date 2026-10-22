# xTool D1 and D1 Pro support

- Status: Experimental native USB and Wi-Fi adapters with eight stock-module presets in 0.2.32. Physical validation is pending.
- Sources checked: October 20, 2026.
- First scope: stock blue-diode modules, flat work, native Wi-Fi. USB is deferred.
- Physical testing: not run. No D1-family machine was connected.

## Wi-Fi setup

Native USB and Wi-Fi support is included in 0.2.32. Both adapters remain Experimental and require physical validation.
Choose **xTool D1 / D1 Pro USB / Wi-Fi (Experimental)** under **Network**, enter the
machine's LAN address and port **8080**, and allow status port **8081**.
An inserted SD card and both services are required. Set up the machine's Wi-Fi
with xTool's own software first, then close other controller applications.

Apply the stock Wi-Fi preset for D1 5W/10W or D1 Pro 5W/10W, 20W or 40W.
Use **Start From: Current Position**, a top-left workspace, and the **laser
spot**, not the crosshair, as the positioning reference. Presets do not change
a saved project's Start From mode. Manually position while idle, then run a
laser-off **Frame**. After **Ready to run**, press the machine's physical
button and wait for the frame to complete. Start then uploads the actual job
and waits for the physical button again. Repeat framing if the head, material,
profile or artwork changes. Keep the computer awake and supervise the machine.

The [native protocol evidence and limits](xtool-d1-network-protocol.md) document
identity, verified file readback, status events, stop recovery, coordinates and
tests. Physical validation is pending. IR, extension frames, rotary,
crosshair offsets, manual jog/home/fire, automatic air and custom G-code are
outside this first adapter.

## Native USB setup

Experimental USB support is included in 0.2.32 for the same stock
blue-diode modules as Wi-Fi. Physical
validation is pending.

Choose the matching **USB** machine preset, then select **Serial**, the
machine's COM/serial port, **230400 baud**, and **xTool D1 / D1 Pro USB / Wi-Fi
(Experimental)**. Close other controller applications. Use **Current
Position**, a top-left workspace and the actual laser spot as described above.
Complete a laser-off Frame before Start. Both operations wait for the machine's
physical button before streaming. After Pause, click Resume in Beam Bench and
press the machine's button to continue. Keep the computer awake and connected
throughout the job. USB does not require the Wi-Fi transport's SD-card upload.

USB uses its own native serial worker. It does not use GRBL initialization,
reset, jog, realtime polling or compatibility fallback. The
[USB protocol evidence](xtool-d1-usb-protocol.md) records public vendor
communication definitions, corroborating serial observations, stricter
acknowledgement gates, tests and the remaining hardware checks.

The first USB adapter shares Wi-Fi's flat-work limits: no IR, rotary,
extensions, crosshair offsets, manual jog/home/fire, automatic air or custom
G-code. A lost connection never resumes or retries work automatically. If Stop
cannot confirm idle, use the physical stop and reconnect before further work.

## Legacy USB research

xTool's older LightBurn GRBL profiles describe a compatibility layer. The
[current LightBurn setup guide](https://docs.lightburnsoftware.com/2.2/Reference/DeviceTypes/xTool/)
uses a dedicated xTool device type for D1/D1 Pro USB. Beam Bench independently
implements native serial communication from the public sources above.
The legacy facts below remain research records, not native USB defaults.

## Verified legacy profile facts

These are factual values extracted from the files linked by xTool's
[D1 instructions](https://support.xtool.com/article/1019) and
[D1 Pro instructions](https://support.xtool.com/article/509). They are research
inputs, not Beam Bench presets or verified native-protocol defaults. The files
do not establish coverage for every replacement module, firmware or frame.

| Setting | `xTool-D1-V2.lbdev` | `xTool-D1ProV3.lbdev` |
| --- | --- | --- |
| Transport and baud | Serial, 230400 | Serial, 230400 |
| Workspace X x Y | 432 x 406 mm | 430 x 400 mm |
| Axis mirroring | X false, Y true | X false, Y true |
| Power scale | S1000 | S1000 |
| DTR enabled | false | true |
| GRBL `$J` jogging enabled | false | false |
| Home on startup | false | false |
| Startup G-code | `M106 S0` | `M106 S0` |
| End G-code | Empty | Empty |
| Transfer mode | Buffered | Buffered |
| G0 overscan | false | false |
| Process offset | X -16 mm, Y 0 | X -16 mm, Y 0 |
| Scanning offset compensation | Enabled | Enabled |

Downloads inspected:

- [D1 profile](https://xtool.zendesk.com/hc/article_attachments/16524795662871),
  SHA-256 `9e9e5b363505d04e9d6e20b01661e7fc033c4ed7712dba22195c43d1661d440b`.
- [D1 Pro profile](https://storage-us.xtool.com/resource/xtool/support-attachment/xTool-D1ProV3%281%29%20%2810%29.lbdev),
  SHA-256 `d03e302114e6bfcd81d6d3a7c82958ff8d04e5d4ecc76424b9e4c58a7fad73f4`.

The original D1 scanning table has speed/line-shift pairs of 50/0.08 and
100/0.15. The Pro table has 50/0.06, 100/0.10, 200/0.08, 300/0.08 and 400/0.10.
These are the profile's own values. Before using them, verify speed units,
offset direction and whether the emitter expects a full or half bidirectional
line displacement. Do not paste these numbers into Beam Bench's mm/min table.

The D1 instructions start from Current Position and require buffered transfer.
They also call for a power cycle when switching applications. xTool's
[D1 precautions](https://support.xtool.com/article/404) document `M106 S0`
for disabling the positioning light and limitations on reading controller
settings. A failed `$$` query therefore cannot justify replacing the power
scale or probing with settings writes.

## Positioning and accessory boundaries

xTool's [positioning-offset reference](https://support.xtool.com/article/190)
distinguishes the crosshair from the working laser spot. It specifies X -16 mm
for 5W/10W/20W modules, but Y -21 mm for the Pro 40W module. The generic Pro
download's X offset must not be applied to the 40W module. Native processing
and framing need a shared, tested coordinate conversion so an offset is applied
once, with the correct sign, at the correct operation.

Leave rotary, extensions and infrared modules outside the first flat-work
implementation:

- The [rotary setup article](https://support.xtool.com/article/588) describes
  firmware changes in July 2022 and specifies Z for D1 and Y for D1 Pro. The
  [Pro operating guide](https://support.xtool.com/article/509) instead specifies
  Z. This conflict needs resolution against exact firmware and an owner trace.
  Beam Bench's generic axis substitution alone does not establish the required
  vendor mode switching, framing or recovery behavior.
- xTool's [Pro extension homing instructions](https://support.xtool.com/article/851)
  require `M205 X406 Y930 S1` in the documented case. An enlarged workspace alone
  does not implement that setup. Do not send this command to a stock machine.
- The [1064 nm module instructions](https://support.xtool.com/article/236)
  disable pointer and scanning offsets and prescribe different constant-power
  choices for vector and bitmap work. Treat the infrared module separately.

## Existing Beam Bench gaps

The GRBL serial path normally opens with DTR and can retry with Ctrl-X.
Its manual jog uses `$J`, jog cancellation uses byte `0x85`, and homing uses
`$H`. The downloaded profiles explicitly disable `$J` and disagree on DTR.
These commands and reset policies need D1-specific evidence before reuse.

The MIT-licensed `xtd1_toolkit` includes a
[public D1 serial capture](https://github.com/whodafloater/xtd1_toolkit/blob/508ac22ba1fc85cb87b6ef3b7238b7b24b5ac53a/serial/README.md)
from firmware 40.30.009.01 B7. It documents newline-terminated queries, including
`?`, a position-only reply without an Idle/Run field, and an `$I` identity reply
that can arrive before or after `ok`. Beam Bench's standard GRBL polling sends
an un-terminated realtime `?` byte. The captured position report cannot establish
idle state or completed motion. A serial connection and S1000 are insufficient
to make the existing sender usable.

The read-only reply examples are preserved in
`crates/beambench-grbl/tests/fixtures/xtool_d1_serial_v1.json`. An offline replay
checks that neither acknowledgement order establishes GRBL readiness or an
Idle state. This records a protocol gap; it does not claim a working D1 adapter.

Machine profiles store pointer and scanning offsets. The native Wi-Fi presets
explicitly clear inherited offset and accessory flags in both preview and
application, with regression coverage. Native USB uses the same laser-spot compiler and required frame as Wi-Fi.
Start From behavior also needs an explicit decision; changing a profile must
not silently rewrite a saved project's positioning mode.

Public [D1 Pro network interoperability research](https://github.com/1RandomDev/xTool-Connect/blob/master/XTOOL_PROTOCOL.md)
describes HTTP on port 8080 and status events on port 8081. This is useful
evidence for a separate network transport. It does not establish the native USB
handshake or make those ports raw GRBL/TCP endpoints. The M1 ZIP upload and
status contract must not be reused merely because it also uses HTTP port 8080.
No third-party code, vendor profile, firmware or binary is bundled by this
investigation.

## Physical validation still needed

The source-extracted native contract now has a separate implementation and
bounded emulator tests. Physical testing is still pending. Complete HW-039 in
[the hardware checklist](../testing/manual-test-cases-with-laser.md), recording
exact model, module, stock frame, firmware, OS and application version.

Owner traces should cover connection and reconnection; native queries and
acknowledgements; an asymmetric laser-off frame; small vector and raster jobs;
pause/button resume; cancellation and completion; and disconnect behavior.
Record actual endpoints and output-off behavior. Tests must not be reported as
hardware captures. Remove identifiers, credentials and unrelated project data
before sharing logs.

# xTool D1 and D1 Pro support investigation

- Status: Needs review. No D1 or D1 Pro adapter or preset is available in Beam Bench.
- Sources checked: October 20, 2026.
- Proposed first scope: stock blue-diode modules, flat work, USB serial.
- Physical testing: not run. No D1-family machine was connected.

## Protocol decision

D1 support needs a controller adapter. Adding dimensions to a GRBL preset
would leave important control differences unresolved. The original M1's HTTP
adapter also does not establish D1 compatibility.

xTool's older LightBurn profiles select GRBL, but this is a compatibility layer
over xTool firmware. LightBurn's developer
[recommends its dedicated xTool device type](https://forum.lightburnsoftware.com/t/continuous-framing-xtool-d1-pro/181211/5).
Its [June 2026 D1 support response](https://forum.lightburnsoftware.com/t/lightburn-2-0-upgrade-doesnt-leave-home-position/190735/3)
describes failures with the legacy path and again recommends the dedicated
protocol. The [current LightBurn setup guide](https://docs.lightburnsoftware.com/2.2/Reference/DeviceTypes/xTool/)
uses the xTool device type and explicitly supports D1/D1 Pro USB connections.
This establishes that an implementation exists elsewhere, not its wire contract
or permission to copy that implementation.

Implement native D1 communication from documented protocol facts or reviewed
interoperability traces. Keep the existing GRBL and M1 behavior unchanged.
Do not advertise D1 support through Generic GRBL-compatible or infer a D1
from a CH340 USB chip, model-name substring, or GRBL-shaped status alone.

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

Machine profiles already store pointer and scanning offsets, but the preset
application contract does not currently apply those fields. A D1 entry must
include them in both its preview diff and application, with regression coverage.
Start From behavior also needs an explicit decision; changing a profile must
not silently rewrite a saved project's positioning mode.

Public [D1 Pro network interoperability research](https://github.com/1RandomDev/xTool-Connect/blob/master/XTOOL_PROTOCOL.md)
describes HTTP on port 8080 and status events on port 8081. This is useful
evidence for a separate network transport. It does not establish the native USB
handshake or make those ports raw GRBL/TCP endpoints. The M1 ZIP upload and
status contract must not be reused merely because it also uses HTTP port 8080.
No third-party code, vendor profile, firmware or binary is bundled by this
investigation.

## Evidence needed to implement the adapter

Extend the public serial observations with a published native specification or
reviewed communication traces
from an owner using supported software. Record the exact D1/Pro model, module,
stock or modified frame, firmware, OS and software version with each trace.
An exported `.lbdev` file supplies configuration facts but cannot supply the
missing command/response sequence.

The minimum trace set must establish:

1. Connection, identity, idle position/status and reconnection, including baud,
   DTR, line endings and any entry/exit from vendor or compatibility mode.
2. Laser-off short XY moves and framing, with both crosshair and spot reference
   positions recorded. Include the expected endpoints of an asymmetric shape.
3. A small vector and raster job, covering power scale, startup/shutdown,
   coordinate orientation, scan compensation, buffering and acknowledgements.
4. Pause, resume, cancel and completion, including the machine's replies after
   the command acknowledgement. Document device-button and disconnect behavior.

Owners should capture operations they already perform with supported software.
Do not ask them to send speculative native commands. Review logs before sharing
and remove device identifiers, network credentials and unrelated project data.

Turn the reviewed sequences into bounded offline fixtures before enabling a
named USB adapter. Tests must reject unrelated models, busy/output-active
connections, unknown status, missing acknowledgements and ambiguous completion.
Exercise real planner/emitter output and the streaming lifecycle, including
interruptions, rather than treating a synthetic `ok` as hardware proof.

After that contract passes, add separate stock-module presets with preview/apply
tests, translated guidance and the normal owner-validation checklist. Physical
testing pending is an acceptable preset evidence level after protocol behavior
is established; it is not a substitute for an unexplained control protocol.

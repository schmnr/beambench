# D1 native USB protocol evidence

Status: Experimental on unreleased `main`, software/emulator tested. Physical
validation is pending for both D1 and D1 Pro. Nothing in this record claims a
Beam Bench test on an actual laser.

## Public sources

The primary source is xTool Studio 1.9.11 for Windows, linked from xTool's
[version history](https://support.xtool.com/article/2419), retrieved 2026-10-21:

- [Official installer](https://storage.atomm.com/efficacy/atomm-package/packages/109/afffecbb-38d7-40ed-8bba-7e7acee5c18c/xTool-Studio-x64-1.9.11.exe),
  SHA-256 `3a2436980a9a6cfea6b7c75ffb34b2f4f8b8ed1966880e8cfb0002573954ac41`.
- Its `resources/exts.zip`, SHA-256
  `8e0e043c87916bf8a715f89ccd12312164810d1f058ec53d099bd559128869a6`.
- `D1/index.js`, SHA-256
  `2746754cc4a892d1217a0432eb4da8007f3676c84418819859f94647b07db8a8`.
- `D1Pro/index.js`, SHA-256
  `11fd631a51a2882839ddd51a22ba2c50b4eec64a0f2528ddbf7f85db2187fe74`.
- `ext-runtime/16a7d4c988a3/xcs-ext-base.js`, SHA-256
  `96208b8f902fd08cace560c2e761031fceca883316a3fe78ef8b618d809efdd4`.
- `resources/app.asar`, SHA-256
  `12546824da2d1c7683394448c505bbb185b514663ca5be6e47766f88817d0e32`.
  Its `.vite/build/discovery-Dj-ah5DM.js` and `app-D_Q460Hg.js` define
  native serial discovery, newline framing and acknowledged line streaming.

Only protocol facts were extracted. Beam Bench independently implements the
adapter; no vendor executable, bundle, profile, firmware or implementation code
is redistributed. Downloads and extraction tools stay in ignored `target/`.

Corroborating primary observations:

- The MIT-licensed [original D1 serial capture](https://github.com/whodafloater/xtd1_toolkit/blob/508ac22ba1fc85cb87b6ef3b7238b7b24b5ac53a/serial/README.md)
  establishes 230400 baud and newline termination, including for `?`. Its
  position-only response does not establish idle or completed motion.
- A [D1 Pro owner's console transcript](https://forum.lightburnsoftware.com/t/software-recognises-xtool-1d-pro-but-returns-to-home-when-sent-to-run/137268/5)
  includes the blue 10W module reply `M116 X0 Y10`, acknowledgements,
  cancellation and program-end events. It also shows that `M28` can follow
  homing, so that line alone cannot prove this adapter's job completed.
- xTool's [USB troubleshooting reference](https://support.xtool.com/article/534)
  distinguishes D1's 40.30 firmware family from D1 Pro's 40.31 family.
- [Inciser](https://github.com/harakas/inciser) demonstrates D1 Pro USB motion,
  but uses legacy compatibility initialization and position polling. It is
  corroboration of the serial transport, not this adapter's native lifecycle.

## Native command and response contract

Both model bundles select V1 with `channelType: serial` for USB. The native
serial service opens at 230400 baud, writes ASCII lines ending in LF, and
matches GET replies by their command prefix. It does not pulse DTR low/high
as a GRBL reset, and does not use a bare realtime `?` or Ctrl-X handshake.
Studio's bundled Windows serial binding enables DTR once through its default
`hupcl` setting. Beam Bench applies that native Windows level in the first serial-port
configuration. Its vendored serialport patch prevents the library from first
forcing DTR low and enabling it afterward. On Unix it leaves the opening level to the OS, as that binding does.
The older LightBurn profile's DTR flags belong to its compatibility path.

| Operation | Command or event | Interpretation |
| --- | --- | --- |
| Firmware query | `M99`, reply `M99 V...` | Accept exact 40.30 / 40.31 firmware families only. Editable device names and USB chip IDs are not identity. |
| Module query | `M116`, reply `M116 X0 Y...` | Accept stock blue D1 5W/10W or Pro 5W/10W/20W/40W only. |
| Working-state query | `M96`, reply `M96 N0` | Zero means idle. Nonzero means non-idle; never guess detailed states from numeric values. |
| Leave compatibility mode | `M112 N0` | Native entry after identity and idle checks. |
| Disable output | `M9 S0 N0` | Explicit output-off on connect and Stop. |
| Arm physical-button job | `M8 N11`, mode echo `M8 N11` | Prepare native online processing. Send no program bytes until the start event. |
| Physical start | `start:workingOnline` | Start streaming the owned, armed program. |
| Program transfer | Newline program lines, `ok` or `ok512` | Acknowledged transfer, not completed execution. Beam Bench keeps one program line in flight. |
| Pause | `M22 S1`, then `ok:PAUSING` | Stop streaming before requesting pause; do not report Paused from an acknowledgement alone. |
| Resume | Physical button, then `ok:WORKING_ONLINE` | Studio's D1 processing base only prompts for the button. D1 USB does not define the S1 bundle's `M22 S0` resume API. |
| Cancel | `M108`; `WORK_STOPED`, `WORK_STOPPED` or `M314 N3` | Cancellation cannot become successful completion. |
| Leave job mode | `M8 N1`, mode echo `M8 N1` | After cancel/output-off, establish an ordered mode boundary before a fresh idle query. |
| Program end | `ok:IDLE`, `MSG:Pgm End`, `[MSG:Pgm End]`, `ok[MSG:Pgm End]` | Require an owned running job, the complete transmitted and acknowledged tail, and a fresh `M96 N0` before reporting Completed. |
| Alarms | `err:*`, `ok:ERROR`, temperature/key-lock errors | Retain failure and attempt Stop/output-off for owned work. Never continue automatically. |

The native header adds `M207 S1` for processing, or `M207 S0` for a laser-off
frame, after `M17 S1`. The existing D1 compiler supplies the remaining native
header, local `G92` anchor, inline power and `M18` tail. A frame uses an
ordinary, all-zero-power online program and the same physical-button flow.
It does not use an unverified automatic framing shortcut.

## Beam Bench's stricter acceptance rules

The vendor GET matcher accepts a named reply alone. Beam Bench additionally
requires an acknowledgement within five seconds, in either order, and requires
both the arming-mode echo and its acknowledgement. This is a conservative
compatibility gate, not a claim that every firmware emits those replies.
Firmware that omits them is rejected for investigation rather than streamed
using guessed readiness. An acknowledgement alone never proves idle.

Input is bounded to 4095 bytes per line and 64 KiB per read/partial buffer.
Fragmented lines survive reads; invalid text, oversized input, merged `okok`,
unknown state events, write/read failures and missing acknowledgements fail
closed. No program line is retried. When no program acknowledgement is pending,
named working-state queries check liveness once per second. Physical-button
waits may last indefinitely while those queries remain responsive.

One background worker owns all serial reads and writes. Stop uses a separate
atomic generation checked during queries, arming and before every program
write. A stale queued command cannot arm work after Stop. The worker sends
cancel, output-off and mode exit, then requires the fresh idle response.
An unplug never reopens the port or resumes a job. A failed stop remains an
error and tells the user to use the physical stop. New work requires a new
arm and, through the service, a new completed laser-off frame.

## Software checks and remaining hardware checks

`crates/beambench-xtool/tests/d1_serial.rs` exercises both firmware families,
module rejection, fragmented replies, acknowledgement order, button start,
framing, pause/button resume, cancellation, completion barriers, input limits,
transport failure and interrupt-before-write behavior. The existing compiler
tests cover vector/raster coordinates, power, bounds and zero-power frames.

`crates/beambench-service/src/xtool_d1_serial_tests.rs` exercises the actual
worker and service planner, frame-proof consumption, progress, pause/resume,
interrupted arming, failed stop, lost transport and Stop while the service
session lock is held. Existing Wi-Fi tests continue to cover that transport.

Before claiming physical validation, complete HW-039 in the
[hardware checklist](../testing/manual-test-cases-with-laser.md) on each model,
module and firmware claimed. In particular record native query/mode echoes
and acknowledgements, long-job throughput, physical-button resume, stop/output
behavior and unplug/reconnect behavior on the target operating systems.

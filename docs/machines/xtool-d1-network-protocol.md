# D1 native Wi-Fi protocol evidence

Checked October 20, 2026. This is an independently written implementation of
public interoperability facts. Downloaded source, firmware and vendor profiles
are not bundled. No physical D1 was used. Software replay and loopback coverage
must not be described as hardware validation.

## Sources and scope

- [xTool-Connect protocol reference](https://github.com/1RandomDev/xTool-Connect/blob/940df33985b3336472b74fec152976538793aaf5/XTOOL_PROTOCOL.md),
  commit `940df33985b3336472b74fec152976538793aaf5`, GPL-3.0.
- [ha-xtool D-series protocol reference](https://github.com/thecodingdad/ha-xtool/blob/0f45273ced1e058bc0b3c1d02fefc34885e9a47d/docs/PROTOCOL.md),
  commit `0f45273ced1e058bc0b3c1d02fefc34885e9a47d`, MIT. Derived from the vendor's
  application bundles; the author's live probes used an S1, not a D1.
- [xtd1_toolkit](https://github.com/whodafloater/xtd1_toolkit/blob/508ac22ba1fc85cb87b6ef3b7238b7b24b5ac53a/xtd1.py),
  commit `508ac22ba1fc85cb87b6ef3b7238b7b24b5ac53a`, MIT. Includes original D1
  native file generation, HTTP upload and physical-button processing.
- [Manufacturer D1 operating guide](https://support.xtool.com/article/516),
  [original D1 specifications](https://uk.xtool.com/blogs/buyer-guide/xtool-d1-review),
  and the [manufacturer's Pro comparison table](https://fr.xtool.com/collections/machines-de-la-serie-xtool-m/products/reconditionnee-xtool-m1-graveur-laser-et-decoupeur-de-vinyle-intelligent).

The first adapter covers stock blue-diode D1 5W/10W and D1 Pro 5W/10W/20W/40W
flat work. It requires an SD card, HTTP port 8080 and status WebSocket port 8081.
USB, IR, extension frames, rotary, crosshair positioning and firmware updates
are separate work. Close other controller applications before using Beam Bench.
The native protocol has no job identifier or exclusive ownership mechanism.

## Read-only identity and telemetry

| Request | Required evidence |
| --- | --- |
| `GET /ping` | Top-level `result: "ok"` |
| `GET /getmachinetype` | Top-level `result: "ok"`, exact `type: "xTool D1"`, `"xTool D1Pro"` or `"xTool D1 Pro"` |
| `GET /getlaserpowerinfo` | `type: 0` blue diode, supported integer `power` wattage |
| `GET /system?action=version` | Acknowledged firmware-version object |
| `GET /peripherystatus` | `sdCard: 1`; `status: "normal"` does not prove Idle |
| `GET /system?action=get_working_sta` | `working: "0"` Idle, `"1"` API-started or `"2"` button-started work |
| `GET /progress` | `progress` percent, `working` elapsed milliseconds, `line` counter |

Progress persists after the previous job. Neither 100 percent nor HTTP state
zero is sufficient evidence of completion. Missing model/module endpoints,
unknown states, malformed JSON or a failed status channel prevent operation.
There is no fallback from an unconfirmed D1 to GRBL or the M1 adapter.

## File upload and control

`POST /cnc/data?filetype=1` takes multipart field `file`, plain G-code named
`tmp.gcode`. It stores `/tmp.gcode`, then waits for the physical start button.
The upload endpoint can acknowledge a failed write. Beam Bench sends once and
requires exact bytes from the static `GET /tmp.gcode` route before Ready to run.
A firmware without this readback route fails closed; no automatic retry occurs.

**Never use `/read?file=...` for verification. It executes the file.**
One newer reference describes that endpoint as a read operation, but
xTool-Connect explicitly documents immediate execution. The adapter does not
call it. Static-file readback is a distinct route.

Pause, resume and stop use `GET /cnc/data?action=pause|resume|stop`.
A successful pause/resume acknowledgement does not confirm the resulting
motion state. Ordered status confirmation is required within five seconds.
Stop additionally sends `GET /cmd?cmd=M9+S0+N0` and checks fresh HTTP Idle.
Emergency Stop sends these requests independently of the session/upload lock.
Cleanup checks `/tmp.gcode` against the pending file's exact bytes before
`GET /delete?file=/tmp.gcode`; it never deletes another application's file.

Native `filetype=0` framing can retain a busy state on original D1 firmware.
Beam Bench instead uploads a normal `filetype=1` program with all laser power
zero, using the same button-start and completion contract as an actual job.
It returns to the starting laser spot. Framing still requires supervision and
clear travel space, since the firmware does not report bed-relative position.

## Ordered status events

The read-only WebSocket emits `ok:WORKING_ONLINE_READY`,
`ok:WORKING_ONLINE`, `ok:WORKING_OFFLINE`, `ok:PAUSING`, `ok:IDLE`,
`WORK_STOPPED`, `ok:WORKING_FRAMING`, `ok:WORKING_FRAME_READY`, `ok:ERROR`,
and `err:flameCheck`, `err:tiltCheck`, `err:movingCheck`, `err:limitCheck`.

An owned program completes only after a running event followed by Idle and
fresh HTTP Idle, with no intervening stop/alarm. A stopped job is cancelled.
Unknown events or loss of ordered evidence require recovery; later Idle cannot
silently restore completion. A new physical-button repeat is unowned work.

The transport reads continuously through uploads, bounds messages to 4 KiB and
the event queue to 64, sends heartbeat pings every two seconds, and fails after
seven seconds without a matching pong. Ping/pong barriers drain earlier
events before upload and before verified readiness. Motion during transfer is
rejected, and an interrupted or unverified upload attempts a confirmed stop and
removes only its own pending file. It never reconnects silently during an owned job.
A network loss does not itself stop a stored program. Use the physical stop if
Beam Bench cannot confirm its stop request.

## Native output and positioning

The file header uses `M17 S1`, `M106 S0`, `M205 X... Y...`, `M101`, `G92`,
`G90`, and initial feed/power values. Motion uses `G0` and inline `G1 S0-1000`.
The footer turns output off and emits `M18`. The planner supplies actual-job
finish movement; only a frame adds a mandatory return to its physical anchor.

`M205` sets the work area in this dialect, rather than Marlin motion limits.
The manufacturer's [extension instructions](https://support.xtool.com/article/851)
also show an explicit enlarged work area. Stock blue-module limits are
432 x 406 mm for D1, 430 x 400 mm for Pro 5W/10W, 430 x 390 mm for Pro 20W,
and 430 x 348 mm for Pro 40W. Connection reads the installed module and checks
the selected workspace against it.

Current Position uses a local anchor at the laser spot, not a claimed measured
machine position. Negative coordinates from center/right/bottom job origins
are translated into nonnegative file coordinates and the same translation is
applied to `G92`, preserving the physical path. Pointer offsets are disabled,
so the manufacturer's different 40W crosshair offset is not applied twice.
Presets clear inherited pointer/scanning offsets and unsupported accessory flags,
showing those changes in the preview diff. Saved project positioning is not
rewritten. Apply the matching top-left preset and use a top-left project.

Generic GRBL file export is unavailable for these profiles; Frame and Start
prepare the native program. Custom G-code, dwell/perforation, Z moves, rotary output and automatic air
commands fail before transfer. Raster scanning can use a user's separately
measured calibration; legacy LightBurn tables are not copied because their
units and displacement convention need validation.

## Validation still needed

The software tests cover exact identity, unsupported modules, compiler power
and coordinates, laser-off framing, false upload success, stale progress,
button start, stop versus completion, pause/resume confirmation, recovery,
cleanup ownership and bounded real HTTP/WebSocket transport. Owner validation
must still record the exact machine, installed module, firmware, frame,
workspace orientation and physical endpoints, including an asymmetric shape,
a small vector/raster job, pause/resume and physical/network interruption.
USB now has a separate [native serial adapter and evidence record](xtool-d1-usb-protocol.md). Neither transport has physical Beam Bench validation yet.

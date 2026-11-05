# Return to homed XY

The Move window's former **Go Machine Zero** control now reads **Return to
Homed XY**. It returns X/Y to the machine position reported when the current
connection completed homing. It leaves Z unchanged, including on profiles
that enable Z movement. It turns the laser off and uses the selected move
feed, capped by the profile's maximum speed.

Literal machine zero is not necessarily the homing endpoint. A stock GRBL
3018 with 300 × 180 mm travel, `$23=3` and `$27=1` can finish homing at
`MPos:-299,-179,-1`. Sending `G53 G0 X0 Y0 Z0` instead moves toward the
opposite limits and can move Z toward its switch.

The replacement command for that measured endpoint at 1500 mm/min is:

```gcode
G90 G21 G94 G53 G1 X-299.000 Y-179.000 F1500 M5
```

No work offset, job origin, homing direction or bed-size calculation chooses
this destination. No settings are changed. The general machine-coordinate
positioning API keeps its existing literal-coordinate behavior.

## Reference lifetime

Home the machine through Beam Bench before using the control. The reference
is never persisted across connections. Reset, a controller startup banner,
disconnect, alarm, another homing attempt, or a controller setting write
invalidates it. Unlocking an alarm does not restore the reference.

Capture requires a fresh position in the Idle report that completes homing,
after acknowledgement or an observed Home state. A work-position report
also needs its current work-coordinate offset in that report. Missing,
malformed or nonfinite coordinates leave the return unavailable. A later
position report cannot substitute for the missed homing endpoint. Inch
reports are converted to millimeters using `$13`.

The service rejects returns during a job, test fire, rotary mode or a busy
machine. It consumes pending controller responses before dispatch and checks
for an intervening emergency stop. The command immediately marks the machine
busy until the next status report.

## Regression coverage

- The reported 3018 settings and negative homing endpoint, followed by a
  different position and work offset, return to `-299,-179` without Z.
- Positive, zero and opposite-direction homing endpoints use their measured
  coordinates. Jogging and changing the work origin do not change the saved
  endpoint.
- Missing, stale, malformed, nonfinite and incomplete position reports never
  fall back to zero. Fresh WPos/WCO and inch reports produce the correct
  machine-space target.
- Alarms, restart, reconnect, settings changes and failed rehoming invalidate
  the target. Unlock does not resurrect it.
- Service guards reject active jobs, test fire, rotary mode, absent homing,
  busy states and invalid feeds without sending a return command.
- The Move window uses the dedicated return command with only a feed value,
  even when the profile permits Z movement. Backend refusal displays an
  error without a moving notification.

These automated checks use simulated transports. Physical 3018 switch
clearance, motion and firmware behavior still require hardware validation.

# RAY5 non-homed framing investigation

On November 3, 2026, the owner shared a Facebook report about a Longer
RAY5 10W with an MKS DLC32 V2.1 board, no switches and reported `$21=0`, `$22=0`.
The reporter described unexpected rapid movement toward the frame, a Box/Frame command
that did not move despite working jog controls, and problems using Current
Position with different job origins.

The message does not identify the Beam Bench version, OS, exact firmware build,
project, displayed error or commands preceding the movement. No physical
controller or report bundle was available. The unexpected movement and the
no-motion frame attempt must not be assumed to have the same cause.

## Confirmed gap and fix

Automatic Home already skips a controller that reports `$22=0`. Manual Home
could still send `$H` in that state. A regression on the previous source
confirms that Home succeeds and sends the request despite the disabled setting.

The shared Home operation now checks the controller setting after draining
pending responses and rejects the request before sending `$H` or changing
coordinate validity. The controller setting takes precedence over the
profile's Homing checkbox. Firmware that omits `$22` retains the existing
explicit Home behavior.

This is an application guard, not proof of the reported movement's cause. Standard GRBL
normally rejects `$H` when homing is disabled; the report does not establish
whether `$H` or an ordinary positioning command preceded the movement.

## Framing checks

Current Position framing is supported without homing. A new recording-transport
regression uses a 400 x 400 mm bottom-left workspace, homing and Home on connect
off, `$21=0`, `$22=0`, and a 20 x 10 mm shape away from the canvas origin.
All nine job origins, both frame modes and three work positions, including zero
and negative coordinates, run through the actual service and streamer.

The 54 cases retain every expected corner, send only the expected frame and
return-to-anchor XY points, keep laser output off, send no `$H` or `G53`, and
leave machine coordinates unreferenced. They do not establish physical travel
clearance on an unhomed machine. Software cannot infer the frame edges from
an arbitrary work zero.

The desktop Frame button also requires a second confirmation click. Existing
UI tests verify that confirmation starts and continues streaming the frame.
A controller that does not provide fresh work position/offset information can
also be refused before framing. Neither explanation is established for this report.

## Validation and follow-up

- Service unit suite: 909 passed, one existing native power-manager test ignored.
- Laser panel suite: 35 passed.
- Home-on-connect regressions: three passed.
- Targeted Clippy, workspace formatting and diff whitespace checks passed.
- The manual Home regression fails on the previous source and passes with the
  guard. The expanded frame cases pass without changing placement behavior.

Request the app version and OS, a diagnostic report ID and project, the exact
action preceding the rapid movement, and available Console TX/RX around it.
Read-only `$$`, `$I`, `$G`, `$#`, `$N` and `?` outputs identify settings, firmware,
modal state, offsets, startup blocks and current status. Inspect saved profile
headers/footers too; framing currently uses those custom commands.

Do not ask the reporter to reproduce the rapid movement, enable switches that
are absent, home the machine, or use Machine Zero as a framing workaround.
The original hardware behavior remains unresolved. No release or correspondence
is part of this source change.

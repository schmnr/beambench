# AlgoLaser job rejection investigation

Report [feedback #30](https://github.com/schmnr/beambench-feedback/issues/30),
`r-a0533762f5574d739968947e81195ab3`, records a job rejected by an AlgoLaser
DIY KIT MK2. The first rejected command is `M7`. The Wi-Fi connection had
already succeeded and the machine had jogged. Beam Bench then disconnected
as part of its failed-job cleanup.

The confirmed application defects are now corrected locally. GRBL streaming
waits for an air-assist command's acknowledgement before sending following
motion, and errors identify the rejected line. Diagnostics retain successful
connection evidence and distinguish network failures from serial failures.
This does not establish which replacement air-assist command this MK2 needs.

The report was submitted on September 8, 2026, from Windows x86_64,
Beam Bench 0.2.18, commit `65b2b10fda68`, using the Generic GRBL Diode preset
and buffered transfer. This review checks current 0.2.19 source at
`60819a74` against that release. The relevant air-assist defaults, G-code
generation and TCP line writing have not changed. The new streaming line
length check does not address this failure.

## Evidence

Times below are UTC.

| Time | Recorded event |
| --- | --- |
| 16:34:15.692 | TCP connection attempt began on port 23. |
| 16:34:16.177 | GRBL session reached Ready after status, identity and settings queries. |
| 16:34:44.747 onward | Jog commands were acknowledged; subsequent Jog/Idle status reports showed changing X coordinates. |
| 16:34:52.805 | The streamer sent the first nine job commands. |
| 16:34:53.037 | Three `ok` responses, then `[echo: M7]` and `error:20`. Later commands in the same batch also received `error:20`. |
| 16:34:53.038 | Retained job marked failed after approximately 0.232 seconds. |
| 16:35:36 | Report captured after the live session had been removed. |

The transmitted batch begins with `G90`, `G21`, `M5`, `M7`, followed by a
rapid move, `M4 S300`, and three cutting moves. The first three acknowledgements
and explicit `M7` echo identify line 4 as the first rejected command.

The nine lines occupy 119 bytes including newlines, within Beam Bench's
127-byte send window. Removing the first three acknowledged commands leaves
108 bytes in flight, exactly matching the report. Nine of 1,531 lines were
sent, three acknowledged and 1,522 remained queued. The later commands had
already been sent before the first error arrived; their presence does not
mean the application continued streaming after it handled the error.

## Findings before correction

1. **The generic air-assist command is the leading compatibility problem.**
   `profile_presets()` in
   [profiles.rs](../../crates/beambench-service/src/ops/profiles.rs) sets
   Generic GRBL Diode to `M7` on and `M9` off.
   `generate_gcode()` in
   [gcode.rs](../../crates/beambench-grbl/src/gcode.rs) inserts the configured
   on command before an air-enabled cut entry. A custom job header can also
   insert `M7`. The bundle omits both the per-entry air setting and the custom
   header, so it cannot establish which supplied this particular command.

2. **The disconnect follows the job error.**
   `tick_job()`, `disconnect_failed_running_job()` and
   `drop_stale_machine_session()` in
   [machine.rs](../../crates/beambench-service/src/ops/machine.rs) retain the
   failed-job evidence, attempt a soft reset and `M5`, then remove the session.
   The report's reason, `failed_while_session_running`, matches that path.
   No initial TCP connection failure is recorded.

3. **The no-response warning is wrong for this report.**
   `build_machine_diagnostics()` in
   [feedback.rs](../../crates/beambench-service/src/ops/feedback.rs) sets the
   firmware version to `None` when the live session is absent.
   `known_issues_for()` treats this as grounds for `no_grbl_response`, without
   checking the successful connection event or retained controller replies.
   The report consequently recommends serial-driver troubleshooting after
   successful TCP communication. Its static note about zero serial ports is
   also irrelevant to this attempt. That static triage text comes from
   `src/lib/feedback-github.ts` in the sibling `Beam Bench Site` repository.

4. **The job error omits the rejected line.**
   `StreamingEngine::handle_response()` in
   [engine.rs](../../crates/beambench-streamer/src/engine.rs) records only
   `GRBL error 20: Unsupported command`. The pending-command queue could
   supply the line number and command, making this failure much easier to
   diagnose without reading the complete bundle.

## Remaining uncertainty and next check

`M7` rejection is established. A complete machine-specific fix is not.
The controller also rejected the subsequent `G0`, `M4` and `G1` lines. The
bundle cannot distinguish firmware behavior after the first error from a
second command-format or streaming compatibility issue. It contains no
firmware identification, full project, or successful comparison run.

There is a concrete precedent for repeated errors without independently
invalid motion commands. The
[grblHAL protocol loop](https://github.com/grblHAL/core/blob/master/protocol.c)
at compatibility level 0 keeps reporting its previous parser error instead
of executing subsequent ordinary G-code. This is a possible explanation
for the recorded sequence, not identification of the customer's firmware.

AlgoLaser's [MK2 support page](https://algolaser.com/pages/algolaser-diy-kit-mk2-machine-support)
documents PC software over Wi-Fi. An
[earlier owner's direct tests](https://forum.lightburnsoftware.com/t/algolaser-air-assist-how-to-use-m16-m17-effectively/127992)
on a DIY 5W also reported `M7` rejection and different pump commands.
That is corroborating evidence from another model, not enough to prescribe
`M8`, `M16` or `M17` for this MK2 and its unknown firmware.

The next controlled check should remove automatic air-assist commands from
a duplicate test setup and inspect exported G-code to confirm `M7` is absent,
including any custom header. Any physical test must maintain the machine's
required airflow independently. Use synchronous transfer for the diagnostic
run so each command is answered before the next is sent. Capture the first
remaining failure and firmware identity. This would establish whether the
air-assist mismatch explains the whole failure before introducing an
AlgoLaser preset or changing generated motion commands.

## Systematic assessment and corrections

The no-response diagnostic is a systematic defect in shared code. It was
also present in [feedback #20](https://github.com/schmnr/beambench-feedback/issues/20),
from Beam Bench 0.2.6: its retained history contains successful handshakes,
but its disconnected snapshot has no firmware version and claims no GRBL
response. That report concerns emergency-stop recovery, a different failure
path. The older reports [#2](https://github.com/schmnr/beambench-feedback/issues/2)
and [#4](https://github.com/schmnr/beambench-feedback/issues/4) also include
the warning without a retained connection attempt. These records establish
that the warning was not specific to AlgoLaser. They do not establish the
causes of those reports' other failures.

A repository issue search found only this report containing `M7`. There is
insufficient field evidence to estimate how many machines reject it. The
source-level exposure is broader: optional GRBL coolant commands vary by
controller, while every GRBL-family adapter uses the shared streamer. Air
assist is off in new cut entries; the generic profile supplies `M7` only when
the project enables it or custom G-code inserts it. Profile commands are
already configurable. Existing hardware settings are not rewritten without
evidence that the replacements work on that model and firmware.

Corrections apply to shared behavior:

- The streamer drains earlier acknowledgements before sending standalone
  `M7`, `M8` or `M9`, then waits for that command's acknowledgement. If it is
  rejected, subsequent motion stays unsent. Once acknowledged, buffered
  motion continues. Lowercase, zero-padded forms and trailing comments are
  handled too. An acknowledgement means command acceptance, not completion
  of preceding motion.
- Every GRBL parser error with a pending command includes its original
  G-code line number and text. Error 20 on these coolant commands adds
  guidance to check the profile and custom G-code. Later replies cannot
  overwrite the first failure or alter its retained command counts. Commands
  are not skipped or automatically retried.
- The shared session-registration path retains controller model, transport
  and available firmware text in a connection event. Disconnected summaries
  show that the last handshake succeeded. Live machine state stays
  disconnected; historical data is not presented as a current position.
- No-response warnings require an actual attempt with no response evidence.
  Validated live sessions and successful current-attempt events suppress the
  warning even without a banner. A new attempt starts a new evidence window,
  so an earlier success cannot hide a later connection failure.
- TCP and UDP attempts retain their endpoint type and original failure.
  Network snapshots omit serial baud rates and avoid serial-driver advice.
- The website's shared report formatter now limits serial-port, baud-rate
  and USB-serial driver hints to serial connections. TCP/UDP failures show
  network guidance and the original error. Older TCP reports, including
  this one, are recognized by their recorded transport-opening event.
  A new connection attempt prevents an old failure or transport from being
  reused. These changes are local in the sibling `Beam Bench Site` repository.
  Connection summaries preserve USB identifiers from earlier events in the
  same attempt. Existing feedback JSON fields carry these changes; no schema
  migration is required.

## Validation

The original replay reproduced the nine-command batch and 108-byte remainder.
The updated regression uses the same recorded commands and error. It now
requires only four sent lines, three acknowledged lines, three bytes in flight
and 1,527 queued lines at rejection. No following `G0`, `M4` or `G1` is sent.
The unsent geometry is represented by placeholder lines because the project
was not attached. A service-level test checks retained errors and teardown.
These tests supply recorded replies; they do not emulate AlgoLaser firmware.

Additional regressions cover successful acknowledgement and resumed buffering,
command attribution after buffer refill and in synchronous mode, late replies,
missing banners, no attempted connection, later reconnect failures, TCP/UDP
errors, USB identifier retention, and local TCP fixtures for FluidNC and
grblHAL.

Validation completed:

- `cargo test -p beambench-streamer -p beambench-service --lib`: 825 passed
  (774 service, 51 streamer).
- `cargo fmt --all -- --check`: passed.
- `cargo clippy -p beambench-streamer -p beambench-service --lib --tests
  --no-deps`: completed successfully. Its 85 unique service warnings point
  to unchanged lines; none overlaps this patch. An earlier run with
  `-D warnings` stopped on 65 existing warnings in unchanged
  `beambench-core` files, so the strict workspace warning gate is not clean.
- In `Beam Bench Site`, `npm run test:feedback`: 18 passed, including legacy
  TCP reports, TCP/UDP failures, unrelated serial devices and later attempts.
- Website TypeScript checking (`npx tsc --noEmit --incremental false`) and
  ESLint for both changed source/test files passed.
- `git diff --check`: passed in both repositories.

All changes remain local. No release, website deployment, existing-issue
rewrite or physical machine test has been performed.

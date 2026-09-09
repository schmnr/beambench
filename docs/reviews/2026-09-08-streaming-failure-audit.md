# Streaming failure audit

This follow-up to [the AlgoLaser report investigation](2026-09-08-algolaser-job-rejection.md)
examines adjacent GRBL streaming, transport-error and failed-job cleanup paths.
The earlier changes remain in place. This pass adds local corrections for the
failure cases below.

| Failure case | Cause and correction |
| --- | --- |
| Healthy jobs fail after repeated Idle reports. | The stall counter accumulated even while commands were being acknowledged. Each accepted acknowledgement now resets the counter. Non-Idle reports also break the sequence, including reports followed by Idle in the same receive batch. |
| Air-assist startup delays can be mistaken for a stalled stream. | GRBL can remain Idle while G4 delays its acknowledgement. The detector now allows the pending block's specified dwell duration, measured from its first Idle report. It still detects a missing acknowledgement after the delay expires. Generated delays and compact, commented and numbered custom G-code are covered. |
| A controller restart can leave the job streaming. | The engine ignored startup banners during a job. A recognized banner now fails running and paused jobs before further output. The queued commands and coordinate assumptions cannot survive a controller restart. |
| Alarm status can leave the job streaming. | The engine handled `ALARM:n`, but ignored `<Alarm|...>` status reports. Either representation now stops the job. A later Idle report cannot hide the earlier alarm. |
| A late read error can erase earlier replies from job progress. | The session returned a whole response batch only after every read succeeded. A later I/O error discarded the batch. Jobs now read and apply one response at a time. Earlier acknowledgements remain counted, and an earlier parser rejection retains its command and line number. The existing aggregate polling API remains available for non-job callers. |
| A parser rejection while paused can strand the session. | Failed-job cleanup required the session to be Running. The job was removed while the session stayed Paused, leaving pause/resume/cancel without an active job. Cleanup now also handles Paused sessions and retains the failure before teardown. |
| A detected acknowledgement stall can leave the session Ready without stopping it. | The watchdog changed the local session state to Ready, causing service cleanup to skip it. It now preserves the active state until shared cleanup attempts reset and M5, invalidates machine coordinates and disconnects. |

The startup-banner and alarm interpretation follows the
[GRBL interface contract](https://github.com/gnea/grbl/wiki/Grbl-v1.1-Interface).
GRBL's [dwell implementation](https://github.com/gnea/grbl/blob/master/grbl/motion_control.c)
waits for prior motion and then delays the command; this is why repeated Idle
reports alone cannot establish that an acknowledgement was lost.

## Regression coverage

Six new tests were run against the previous implementation and failed on the
expected behavior: healthy Idle/ack traffic, restart banners, alarm status,
paused failure cleanup, and the two partial-read cases. Nine new tests cover
this pass overall, with multiple transport modes and input variants per test.
They live in the ordinary crate test suites and are included in the existing
workspace `cargo nextest` check in [CI](../../.github/workflows/ci.yml).

- Buffered and synchronous jobs that acknowledge every short command while
  repeatedly reporting Idle must complete.
- A ten-second generated air-assist dwell tolerates more than five Idle
  reports. Its acknowledgement restarts stall detection for the next block.
  An injected elapsed deadline and five additional Idle reports confirm that
  a truly missing acknowledgement still fails. Idle reports received during
  the allowed delay do not count toward that final response allowance.
- Stock and vendor startup banners abort running and paused jobs. Alarm-only
  status reports do the same. Later acknowledgements and Idle reports cannot
  resume output or change the retained first failure.
- Injected read errors after a parser rejection preserve that rejection.
  Injected read errors after acknowledgements preserve their progress counts.
- Paused-job errors and acknowledgement stalls exercise service teardown,
  retained diagnostics, reset, laser-off and session removal.
- Existing tests continue to cover real missing acknowledgements, stale Idle
  completion, receive-window limits, failed resume, cancellation failures and
  network handshake diagnostics.

## Validation

All 1,052 tests passed across the relevant library suites:

- GRBL parsing, generation and sessions: 196.
- Serial and TCP transport: 22.
- Streaming and preflight: 56.
- Service operations and recovery: 778.

The streamer and service suites were rerun after the final dwell response
allowance adjustment, with all 834 tests passing. Rust formatting and Git
whitespace checks passed.

Clippy completed successfully for GRBL, streamer and service libraries/tests
with `--no-deps`. It reported the same 85 unique service warnings seen before
this follow-up; none points to a changed line. The repository's existing
warning backlog remains outside this correction.

Reproduce the full coverage from the repository root:

```sh
cargo test -p beambench-grbl -p beambench-serial -p beambench-streamer -p beambench-service --lib
cargo fmt --all -- --check
```

## Scope and limits

This review targets the failure patterns adjacent to the original report. It
does not certify that the entire application is free of bugs. The fixes are in
the shared GRBL job path, which serves serial and TCP connections. Other
controller protocols were inspected only where they share service cleanup.
No physical controller was operated, and the AlgoLaser MK2's exact supported
air-assist command remains unverified. These corrections are prepared for Beam Bench 0.2.20.

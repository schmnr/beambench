# Framing diagnostics

Bug and crash reports now include a bounded framing history under the existing
`recent_logs` field. Entries have target `beambench::frame`; their messages
contain JSON with a `stage` and `context`. The report schema and controls are
unchanged.

The shared service records these stages for desktop, API and CLI requests:

- `requested`: a confirmed framing request reached the service. Includes frame
  mode, feed, laser override/policy, selection count, available positioning and
  homing state, and connection/document epochs.
- `started`: dispatch succeeded. Includes initial job progress and available
  job/session command history.
- `rejected`: dispatch returned an error. Includes the original structured
  error and available session history. A partial initial write also includes
  the job diagnostic captured before teardown.
- `terminal`: a frame completed, failed, was cancelled, or was stopped through
  Emergency Stop or Disconnect. Includes the retained job progress and
  command/response history. Stop diagnostics are collected after stop delivery.

Snapshots are observational. Reading positioning and session context uses
nonblocking locks, so unavailable fields can be omitted. No status query,
motion command, firmware setting or additional confirmation is introduced.
Existing framing eligibility, errors, placement and streaming are preserved.

The history lives in memory for the current app process and survives connection
changes, clearing the live job, and routine status polling. It retains at most
24 entries and 256 KiB of message content, with a 32 KiB maximum per message.
The combined limit leaves room for project attachments within the existing
submission cap. Oversized messages explicitly say
`truncated` and contain an excerpt. Job/session console snapshots retain the
existing limit of 120 entries per console. A new app process starts empty.

The existing report preview, saved report/ZIP and submission paths include this
history and apply the existing user-home path scrubbing. Successful-job
compatibility and machine-catalog reports continue to omit command/log content.

## Interpretation

This records confirmed service requests, not clicks on Box/Outline or the first
confirmation click. An absent entry does not establish user error: the service
may not have been invoked, the app may have restarted, or older entries may
have been evicted. Compare timestamps and epochs before associating a trace
with a report. The history does not prove physical controller execution.

## Regression checks

Tests exercise refusal before dispatch, invalid feeds, unhomed Current Position
framing, successful completion, partial-write teardown, Emergency Stop and
Disconnect. They check original error/command behavior, preservation after
polling and disconnect, bounded UTF-8 messages, report/ZIP inclusion, home-path
scrubbing, and exclusion from minimized compatibility reports.

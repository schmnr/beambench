# Facebook report: framing, sleep and stuck editing gestures

Investigated 28 September 2026 against the current working tree.
Source: [Csaba Kiraly's post](https://www.facebook.com/groups/beambench/permalink/1499315061938046/),
opened through the supplied share link. The post had no comments when read.
The post does not identify an OS, controller, exact application version, or
the pointer/key sequence used before the stuck gesture.

## Implemented corrections

All four open points now have changes in the working tree:

- Active jobs acquire an idle-sleep assertion before output starts. The backend
  retains it through transfer, running, pause and repeated framing, then releases
  it on terminal progress, cancellation, disconnect, fatal streaming failure or
  shutdown. A failed acquisition produces a visible warning and a diagnostic
  entry. A bounded startup handshake prevents an unavailable power manager from
  indefinitely blocking Start.
- Acquisition and release run on the same dedicated thread. This preserves
  Windows execution-state ownership even though service calls arrive on
  different threads. Native macOS and Windows assertions use
  [keepawake](https://github.com/segevfiner/keepawake-rs). Linux first uses the
  Cinnamon/GNOME session idle inhibitor, with logind for other environments.
  The Cinnamon call follows its [published D-Bus interface](https://github.com/linuxmint/cinnamon-session/blob/master/cinnamon-session/org.gnome.SessionManager.xml).
  This is idle-sleep protection, not a guarantee against explicit sleep, lid
  closure, shutdown, battery exhaustion or desktop power-manager overrides.
- Continuous framing keeps a prepared motion snapshot. After confirmed
  completion, the backend starts the next pass while holding the job/session
  locks. It does not recalculate placement from a moved head or edited project.
  Stop disables repetition before cancellation is attempted. Emergency Stop
  uses the same lock order as ticking, and shutdown prevents new starts.
- Canvas pointer cancellation, lost capture, a missed button release, blur and
  hidden-document events clear the gesture and pan state. The owning tool is
  cancelled/reset; Select restores its original geometry and Node discards
  uncommitted local changes. A node-loading generation check prevents a late
  asynchronous response from replaying mouse-down after release/reset.
- The Laser panel now has a translated Frame speed input next to the framing
  options. It shares the Move panel's setting and converts the selected length
  and time units. It is disabled during active motion or frame confirmation.

## Verification after implementation

- Full service library suite: 820 passed, one native desktop smoke test ignored
  in the ordinary run. Tests cover repeated frames after fresh controller Idle,
  frozen geometry, cancel/disconnect/emergency-stop/shutdown cleanup, single-pass
  completion, streaming failure, and paused-job sleep protection.
- The ignored native smoke test was run separately on macOS and passed.
  `pmset -g assertions` confirmed the assertion was added and then removed.
- The power module compiled and its two portable acquisition/lifetime tests
  passed in Linux. The native Cinnamon/logind smoke test was not run because the
  container has no desktop power manager. Windows power code and tests passed
  the MSVC cross-compilation check; native Windows execution remains unverified.
- All 200 focused frontend tests passed, covering Canvas interruptions, delayed Node loads, frame
  speed editing and unit conversion, sleep warnings, existing App behavior and
  locale parity. Production frontend build, TypeScript checking and targeted
  ESLint passed. The speed control was visually checked in a 360-pixel preview
  using the real Laser panel; temporary preview files were removed afterward.
- Clippy completed with existing warnings. Formatting and diff whitespace checks
  passed. No physical laser was operated and no release or Facebook reply was
  published.

After reducing the prepared-frame enum's storage size, all 34 focused framing
tests were rerun successfully and the final service Clippy run completed.

## Original investigation

### Continuous framing: confirmed implementation gap

The Device Settings toggle saves `frame_continuously` to the machine profile.
Every reference to that field is a definition, default, persistence/copy path,
UI binding or fixture. Neither `frame_job` nor job completion reads it.
The current framing path generates one traversal. Frontend completion handling
clears a completed job after three seconds and does not request another frame.
The reporter's expectation matches the label; this is not a missed setting.

Relevant code:
- `tauri-app/src/components/dialogs/DeviceSettingsDialog.tsx`, continuous toggle.
- `crates/beambench-service/src/ops/machine.rs`, `frame_job`.
- `tauri-app/src/hooks/useMachinePolling.ts`, terminal-state handling.

Implement repetition in the job lifecycle with explicit framing purpose,
controller support and cancellation. Stop, errors, disconnect and a new job must
clear repetition before another pass can start. Avoid a timer that blindly
resubmits motion from the UI. Test multiple completed passes and each stop path.

### Sleep during a job: confirmed missing protection

The desktop/service sources and dependencies have no sleep inhibitor or wake-lock
implementation. Starting a background job tick loop does not prevent OS sleep.
This is a missing reliability feature, not evidence of incorrect user settings.
The reported physical interruption was not reproduced with hardware.

Use an OS idle-sleep inhibitor owned by the active job lifecycle. Cover transfer,
running, framing and paused jobs, and release it on completion, cancellation,
failure, disconnect and shutdown. Report acquisition failure. It should not claim
to override explicit user sleep, lid closure, shutdown or power loss.

### Stuck line-editing/panning gesture: credible bug, exact trigger unconfirmed

`Canvas.tsx` captures pointers and handles down/move/up, but has no
`pointercancel`, `lostpointercapture` or window-blur cleanup. The Space-pan flag
is cleared only by keyup. Panning moves the viewport without checking the held
button state. NodeTool likewise keeps its drag state until mouse-up/reset and
continues responding to later moves. Its event type does not include `buttons`.

These are concrete ways an interrupted gesture could remain active, but the
post does not establish which path occurred. Treat this as a likely application
bug, not a proven reproduction of this user's sequence.

Add Canvas-level regressions for cancelled/lost pointers, blur during Space-pan,
button release outside the window, and editing around pan/zoom. Define whether
an interrupted edit commits or rolls back, then finish/reset the owning gesture
consistently and prevent further geometry changes after release.

### Framing speed: usability request

`machineStore.frameJob` already supplies
`useUiStore.getState().moveWindowJogFeedRateMmMin` to the backend. The backend
uses that feed rate in the frame plan, and existing Laser panel tests verify
the value reaches the service. The Laser panel has no adjacent framing-speed
input. Exposing the same setting there would address the request without adding
two conflicting sources of speed.

### LightBurn curves: user confirms earlier correction

The reporter says the update fixed curves importing as straight lines. There is
no new failure described for that feature.

## Initial investigation validation

Ran the existing NodeTool and LaserPanel suites: 87 tests passed across two files.
Those tests verify normal editing and framing controls, not interrupted Canvas
gestures, continuous repeat execution or OS sleep prevention. No controller was
operated, no Facebook reply was sent, and no application code was changed during
the initial read-only investigation. The implementation above followed the
user's subsequent instruction to correct all four points.

The initial recommendation was to prioritize sleep protection and gesture
cleanup, then continuous framing and the speed control.

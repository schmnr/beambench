# Beam Bench Manual Test Cases: With Laser Connected

Use these cases only with a real configured machine profile. Use scrap material only.

Shared fixtures:

- `Known-good machine profile`
- `Known-bad connection setup`: wrong baud, wrong port, unplugged device, or occupied port
- `Standard runtime project`: mixed vector and raster output on multiple layers
- `Alarm scenario`: device or simulated state that produces unlockable alarm behavior
- `Camera-enabled profile`
- `DSP-capable profile`
- `Galvo-capable profile`

## Profiles, Discovery, Connection, Console, Runtime

### HW-001 — Machine Profiles CRUD And Activation
- Source Ref: `FSW-018`; `LB 13`, `LB 20.*`
- Feature / Function: Create, edit, activate, deactivate, and delete machine profiles
- Hardware Requirement: Laser
- Prerequisites: At least one existing profile or ability to create one
- Setup / Fixture: Known-good machine profile plus a second test profile
- Steps: Create/edit profile data, switch active profile, deactivate it, and delete a non-active profile.
- Expected Result: Active profile state updates correctly and propagates to machine-related UI and preview assumptions.
- Edge / Negative Cases: Deleting the active profile must be blocked or handled safely.
- Persistence / Reopen Check: Relaunch and verify profile list and active selection.
- Undo / Redo Expectation: Profile library changes are not project-history operations.
- Status: Active

### HW-002 — Device Settings Tabs And Save Paths
- Source Ref: `FSW-018`; `LB 20.1` to `LB 20.5`
- Feature / Function: Device Settings dialog across connection, machine, GRBL, controller, discovery, and profiles tabs
- Hardware Requirement: Laser
- Prerequisites: Device Settings dialog opens
- Setup / Fixture: Known-good profile
- Steps: Visit every tab, edit representative fields, save them, reopen dialog, and verify persistence.
- Expected Result: Tab content is reachable, fields save to the correct profile/settings store, and reopening shows saved values.
- Edge / Negative Cases: Invalid values should reject or normalize safely without corrupting the profile.
- Persistence / Reopen Check: Relaunch and confirm settings persist.
- Undo / Redo Expectation: App-data mutation, not project undo history.
- Status: Active

### HW-003 — Known-Good Connection
- Source Ref: `FSW-018`, `FSW-019`; `LB 13`
- Feature / Function: Connect and disconnect through the standard connection flow
- Hardware Requirement: Laser
- Prerequisites: Reachable device and correct profile
- Setup / Fixture: Known-good machine profile
- Steps: Connect from the dialog or menu, inspect resulting state, then disconnect.
- Expected Result: Session transitions to ready state, correct port/profile are reflected, and disconnect returns to clean disconnected state.
- Edge / Negative Cases: Repeat connect/disconnect several times to catch stale session leakage.
- Persistence / Reopen Check: Relaunch and verify the app starts disconnected unless intentionally designed otherwise.
- Undo / Redo Expectation: Connection state is non-project runtime state.
- Status: Active

### HW-004 — Connection Failure Modes
- Source Ref: `FSW-018`, `FSW-019`
- Feature / Function: Wrong baud, wrong port, unplugged device, occupied port
- Hardware Requirement: Laser
- Prerequisites: Known-bad connection setup
- Setup / Fixture: Bad port/baud scenarios
- Steps: Attempt to connect under each failure mode.
- Expected Result: Failure is clearly reported, no false ready state appears, and the UI remains usable afterward.
- Edge / Negative Cases: A failed attempt must not poison later connection attempts with valid settings.
- Persistence / Reopen Check: Connect successfully after a failure and verify normal state.
- Undo / Redo Expectation: N/A
- Status: Active

### HW-005 — Discovery Scan / Cancel / Refresh
- Source Ref: `FSW-020`; `LB 13`
- Feature / Function: Discovery section lifecycle
- Hardware Requirement: Laser
- Prerequisites: Discovery-enabled build and reachable or simulated candidates
- Setup / Fixture: Machine network/serial environment
- Steps: Run scan, refresh discovery state, cancel an active scan, inspect counts and status text.
- Expected Result: Discovery phase, status text, and candidate counts update correctly.
- Edge / Negative Cases: Cancel must halt scanning cleanly without leaving loading state stuck.
- Persistence / Reopen Check: Reopen Device Settings and verify discovery can run again normally.
- Undo / Redo Expectation: Runtime-only.
- Status: Active

### HW-006 — Discovery Candidate Bootstrap And Connect
- Source Ref: `FSW-020`
- Feature / Function: Candidate bootstrap-profile and direct-connect actions
- Hardware Requirement: Laser
- Prerequisites: At least one discovery candidate
- Setup / Fixture: Candidate surfaced in Discovery tab
- Steps: Bootstrap a profile from candidate metadata, then connect directly from a candidate.
- Expected Result: New profile is created with sensible defaults and direct-connect uses the intended candidate.
- Edge / Negative Cases: Unsupported candidates must report limitations without crashing the dialog.
- Persistence / Reopen Check: Relaunch and verify bootstrapped profile remains available.
- Undo / Redo Expectation: App-data mutation for bootstrap; runtime for connect.
- Status: Active

### HW-007 — Disconnected-State Gating
- Source Ref: `FSW-019`; `LB 13`
- Feature / Function: Safe disabled state when no live machine session exists
- Hardware Requirement: Laser
- Prerequisites: Machine disconnected
- Setup / Fixture: Valid project and disconnected runtime
- Steps: Inspect Laser panel, Machine menu, Move panel live-machine actions, and console-related surfaces.
- Expected Result: Live machine actions are disabled or safely gated, while non-hardware shell surfaces remain usable.
- Edge / Negative Cases: No disconnected view should present stale ready/running status.
- Persistence / Reopen Check: Relaunch into disconnected state and verify gating remains correct.
- Undo / Redo Expectation: N/A
- Status: Active

### HW-008 — Status Polling And Readouts
- Source Ref: `FSW-019`; `LB 13`, `LB 14`
- Feature / Function: Live status, run state, and position readouts
- Hardware Requirement: Laser
- Prerequisites: Connected machine
- Setup / Fixture: Known-good machine profile
- Steps: Observe status while idle, during motion, and after commands.
- Expected Result: Session state, run state, and position readouts update in near real time and match machine behavior.
- Edge / Negative Cases: Transient states must settle correctly rather than sticking in connecting/validating.
- Persistence / Reopen Check: Reconnect after relaunch and verify status polling resumes correctly.
- Undo / Redo Expectation: Runtime-only.
- Status: Active

### HW-009 — Jog Directions And Step Sizes
- Source Ref: `FSW-019`; `LB 14`
- Feature / Function: Jog pad with all directions and configured step sizes
- Hardware Requirement: Laser
- Prerequisites: Connected idle machine
- Setup / Fixture: Clear machine workspace
- Steps: Jog in all eight directions at each available step size.
- Expected Result: Motion direction and distance match the control pressed and selected step size.
- Edge / Negative Cases: Jog must remain blocked when machine is not idle.
- Persistence / Reopen Check: Reconnect and verify default jog controls still function.
- Undo / Redo Expectation: Runtime-only.
- Status: Active

### HW-010 — Jog Feed Presets
- Source Ref: `FSW-019`; `LB 14`
- Feature / Function: Jog feed selection and application
- Hardware Requirement: Laser
- Prerequisites: Connected idle machine
- Setup / Fixture: Known-good profile
- Steps: Set multiple feed presets and jog after each change.
- Expected Result: Feed preset changes are reflected in actual machine movement behavior and UI state.
- Edge / Negative Cases: Out-of-range feed values must clamp or reject safely.
- Persistence / Reopen Check: Verify preset defaults after relaunch/reconnect if they are intended to persist.
- Undo / Redo Expectation: Runtime-only.
- Status: Active

### HW-011 — Home From Idle And Blocked States
- Source Ref: `FSW-019`; `LB 13`, `LB 14`
- Feature / Function: Home command behavior
- Hardware Requirement: Laser
- Prerequisites: Connected machine
- Setup / Fixture: Idle machine and one non-idle/blocked scenario
- Steps: Home from idle, then attempt home when it should be blocked.
- Expected Result: Idle home succeeds; invalid-state home is disabled or safely rejected.
- Edge / Negative Cases: After home, reported coordinates and status should normalize predictably.
- Persistence / Reopen Check: Reconnect and verify home is still available in valid state.
- Undo / Redo Expectation: Runtime-only.
- Status: Active

### HW-012 — Unlock Alarm State
- Source Ref: `FSW-019`; `LB 13`
- Feature / Function: Unlock flow from alarm state
- Hardware Requirement: Laser
- Prerequisites: Machine in unlockable alarm state
- Setup / Fixture: Alarm scenario
- Steps: Trigger alarm, verify alarm indication, invoke Unlock, then resume normal control.
- Expected Result: Unlock is enabled only in alarm state and clears the session back to a usable state when successful.
- Edge / Negative Cases: Unlock should not appear available in non-alarm state.
- Persistence / Reopen Check: Reconnect after alarm/clear if needed and verify clean state.
- Undo / Redo Expectation: Runtime-only.
- Status: Active

### HW-013 — Console Send, History, And Clear
- Source Ref: `FSW-017`, `FSW-019`; `LB 13`
- Feature / Function: Console command send and log handling
- Hardware Requirement: Laser
- Prerequisites: Connected machine
- Setup / Fixture: Safe diagnostic commands
- Steps: Send valid command, inspect response, use history up/down, refresh log, and clear log.
- Expected Result: Sent/received lines are timestamped correctly, history navigation works, and clear only clears the visible log state.
- Edge / Negative Cases: Error or alarm responses should highlight clearly.
- Persistence / Reopen Check: Reopen panel or relaunch and verify the console remains usable.
- Undo / Redo Expectation: Runtime-only.
- Status: Active

### HW-014 — Live Macro Execution
- Source Ref: `FSW-017`
- Feature / Function: Run macros against a connected machine
- Hardware Requirement: Laser
- Prerequisites: At least one safe macro configured
- Setup / Fixture: Macro that issues harmless motion or status commands
- Steps: Run single-line and multi-line macros from panel and toolbar where applicable.
- Expected Result: Macro commands execute in order and console/log state reflects them accurately.
- Edge / Negative Cases: Failing macro lines must surface a clear failure without leaving false ready state.
- Persistence / Reopen Check: Relaunch and verify macro definitions remain intact.
- Undo / Redo Expectation: Runtime-only.
- Status: Active

### HW-015 — Start From Modes
- Source Ref: `FSW-019`; `LB 9`
- Feature / Function: `Absolute Coords`, `Current Position`, and `User Origin`
- Hardware Requirement: Laser
- Prerequisites: Connected machine and preview-capable project
- Setup / Fixture: Standard runtime project
- Steps: Switch `Start From` mode and inspect downstream positioning behavior in preview/frame/run-related commands.
- Expected Result: Each mode changes coordinate interpretation consistently.
- Edge / Negative Cases: Missing user origin must be handled safely when `User Origin` is selected.
- Persistence / Reopen Check: Save project and reopen to verify project-level `Start From` persistence.
- Undo / Redo Expectation: Project setting mutation, typically undoable if tracked.
- Status: Active

### HW-016 — Job Origin Anchor
- Source Ref: `FSW-019`; `LB 9`
- Feature / Function: 3x3 job-origin anchor
- Hardware Requirement: Laser
- Prerequisites: Connected machine and project with visible bounds
- Setup / Fixture: Standard runtime project
- Steps: Switch among representative anchor points and verify effect on framing/preview/runtime placement.
- Expected Result: Anchor selection changes origin semantics consistently across surfaces.
- Edge / Negative Cases: Anchor changes must not silently alter object geometry.
- Persistence / Reopen Check: Save/reopen project and confirm anchor persists.
- Undo / Redo Expectation: Project setting mutation if historied.
- Status: Active

### HW-017 — Set Origin And Go To Origin
- Source Ref: `FSW-019`; `LB 9`, `LB 13`
- Feature / Function: User-origin establishment and motion back to origin
- Hardware Requirement: Laser
- Prerequisites: Connected idle machine
- Setup / Fixture: Standard runtime project
- Steps: Set origin, move away, then command go-to-origin.
- Expected Result: Machine returns to the stored origin and UI feedback matches the action.
- Edge / Negative Cases: Origin commands must be blocked or safe when no valid runtime state exists.
- Persistence / Reopen Check: Reconnect and verify expected origin behavior if designed to persist.
- Undo / Redo Expectation: Runtime-only.
- Status: Active

### HW-018 — Rectangular Frame
- Source Ref: `FSW-019`; `LB 9`
- Feature / Function: Standard frame behavior
- Hardware Requirement: Laser
- Prerequisites: Connected idle machine and preview-capable project
- Setup / Fixture: Standard runtime project
- Steps: Trigger frame once, then confirm the action if confirmation flow is required.
- Expected Result: Machine frames the project bounds using rectangular mode and UI reports framing state clearly.
- Edge / Negative Cases: Frame should not start while an active job is running.
- Persistence / Reopen Check: N/A
- Undo / Redo Expectation: Runtime-only.
- Status: Active

### HW-019 — Rubber-Band Frame And Selected-Only Frame
- Source Ref: `FSW-019`; `LB 9`
- Feature / Function: Alternate frame mode and selected-only framing
- Hardware Requirement: Laser
- Prerequisites: Connected idle machine with selected subset of objects
- Setup / Fixture: Standard runtime project
- Steps: Switch to rubber-band mode and exercise selected-only framing.
- Expected Result: Frame path matches selected-only bounds and the chosen frame mode.
- Edge / Negative Cases: Selected-only frame with no selection must fail safely or disable.
- Persistence / Reopen Check: Verify frame-selected-only UI state after reconnect if intended to persist.
- Undo / Redo Expectation: Runtime-only.
- Status: Active

### HW-020 — Move To Laser
- Source Ref: `FSW-019`; `LB 14`
- Feature / Function: Move selected artwork to current laser position
- Hardware Requirement: Laser
- Prerequisites: Connected machine with known current work position and selected objects
- Setup / Fixture: Standard runtime project
- Steps: Use `Move to Laser` from the Move panel.
- Expected Result: Selected objects reposition to the current laser coordinate without corrupting selection or layer assignments.
- Edge / Negative Cases: No selection or no work position must disable the action.
- Persistence / Reopen Check: Save/reopen and verify object placement.
- Undo / Redo Expectation: Project mutation should be undoable.
- Status: Active

### HW-021 — Laser To Selection With Offsets
- Source Ref: `FSW-019`; `LB 14`
- Feature / Function: Move laser to selected artwork using current `Start From` semantics
- Hardware Requirement: Laser
- Prerequisites: Connected machine with selected objects
- Setup / Fixture: Standard runtime project
- Steps: Use `Laser to Selection` under different `Start From` modes.
- Expected Result: Machine motion reflects the selected artwork center and configured offset mode.
- Edge / Negative Cases: No selection must gate the action safely.
- Persistence / Reopen Check: N/A
- Undo / Redo Expectation: Runtime-only.
- Status: Active

### HW-022 — Go To Position
- Source Ref: `FSW-019`; `LB 14`
- Feature / Function: Manual X/Y move from the Move panel
- Hardware Requirement: Laser
- Prerequisites: Connected idle machine
- Setup / Fixture: Safe coordinate targets
- Steps: Enter explicit X/Y coordinates and command movement.
- Expected Result: Machine moves to requested coordinates and UI surfaces clear feedback.
- Edge / Negative Cases: Out-of-bounds or invalid coordinates must reject or clamp safely.
- Persistence / Reopen Check: N/A
- Undo / Redo Expectation: Runtime-only.
- Status: Active

### HW-023 — Saved Positions
- Source Ref: `FSW-019`; `LB 14`
- Feature / Function: Save current machine position, recall it, and delete it
- Hardware Requirement: Laser
- Prerequisites: Connected machine with readable work position
- Setup / Fixture: Several safe positions
- Steps: Save current position, move away, recall saved position, then delete it.
- Expected Result: Saved positions list updates correctly and recalling a saved position moves the machine to the stored coordinates.
- Edge / Negative Cases: Deleting a saved position should immediately remove it from the UI without leaving a stale selection.
- Persistence / Reopen Check: Relaunch and confirm saved-position persistence behavior.
- Undo / Redo Expectation: Runtime/app-state only.
- Status: Active

### HW-024 — Save G-Code From The Laser Panel
- Source Ref: `FSW-014`, `FSW-019`; `LB 1.1`, `LB 9`
- Feature / Function: Save machine file from connected workflow surface
- Hardware Requirement: Laser
- Prerequisites: Preview-capable project
- Setup / Fixture: Standard runtime project
- Steps: Export/save G-code from the Laser panel and from menu entry for comparison.
- Expected Result: File-save flow completes successfully and outputs a machine file aligned with current project settings.
- Edge / Negative Cases: Canceling save must not surface as a hard runtime error.
- Persistence / Reopen Check: Verify file exists and can be regenerated consistently after reopen.
- Undo / Redo Expectation: Non-mutating export.
- Status: Active

### HW-025 — Preflight Pass / Warning / Fail
- Source Ref: `FSW-019`; `LB 9`, `LB 13`
- Feature / Function: Preflight report outcomes
- Hardware Requirement: Laser
- Prerequisites: Connected machine and three project/runtime states that trigger pass, warning, and fail
- Setup / Fixture: Runtime project variants
- Steps: Run preflight in each scenario and inspect report details.
- Expected Result: Outcome badge, check list, and dialog contents match the actual issue set and gate job start correctly.
- Edge / Negative Cases: Fail outcome must prevent automatic job start.
- Persistence / Reopen Check: Re-run preflight after relaunch/reconnect to verify consistency.
- Undo / Redo Expectation: Non-mutating runtime validation.
- Status: Active

### HW-026 — Stale Preview Auto-Generation And Start Gating
- Source Ref: `FSW-015`, `FSW-019`
- Feature / Function: Auto-generate preview before start when preview is stale
- Hardware Requirement: Laser
- Prerequisites: Connected ready machine and stale preview state
- Setup / Fixture: Modify project after last preview
- Steps: Attempt job start directly without manually regenerating preview.
- Expected Result: Preview is regenerated first; if generation or preflight fails, job start is blocked cleanly.
- Edge / Negative Cases: Start button must not race multiple preview generations.
- Persistence / Reopen Check: Regenerate after reopen and confirm consistent behavior.
- Undo / Redo Expectation: Underlying project edits remain undoable; preview generation is non-mutating.
- Status: Active

### HW-027 — Start Double-Trigger Protection
- Source Ref: `FSW-019`
- Feature / Function: Protection against repeated start clicks
- Hardware Requirement: Laser
- Prerequisites: Connected ready machine with runnable project
- Setup / Fixture: Standard runtime project
- Steps: Attempt to trigger Start repeatedly in quick succession.
- Expected Result: Only one job-start flow is processed and the UI reflects an in-flight state.
- Edge / Negative Cases: Rapid repeated clicks should not queue duplicate jobs or corrupt state.
- Persistence / Reopen Check: Verify the app is normal after job completion/cancel.
- Undo / Redo Expectation: Runtime-only.
- Status: Active

### HW-028 — Pause / Resume / Cancel Lifecycle
- Source Ref: `FSW-019`; `LB 13`
- Feature / Function: Mid-job control flow
- Hardware Requirement: Laser
- Prerequisites: Connected machine actively running a safe test job
- Setup / Fixture: Scrap material and standard runtime project
- Steps: Start job, pause it, resume it, then cancel it on a later run.
- Expected Result: Runtime state transitions are correct and controls enable/disable accordingly.
- Edge / Negative Cases: Resume must only appear from paused state.
- Persistence / Reopen Check: After cancellation or completion, reconnect if needed and verify clean idle state.
- Undo / Redo Expectation: Runtime-only.
- Status: Active

### HW-029 — Emergency Stop
- Source Ref: `FSW-019`; `LB 13`
- Feature / Function: E-stop UI and post-stop state
- Hardware Requirement: Laser
- Prerequisites: Connected machine, preferably during safe controlled motion
- Setup / Fixture: Safe runtime scenario
- Steps: Trigger Emergency Stop from menu or panel.
- Expected Result: Machine motion halts as expected and UI enters a clear stopped/alarm/error state that requires explicit recovery.
- Edge / Negative Cases: E-stop should remain available whenever a live connection exists.
- Persistence / Reopen Check: Recover or reconnect and verify the app can resume normal operation.
- Undo / Redo Expectation: Runtime-only.
- Status: Active

### HW-030 — Override Controls During Run
- Source Ref: `FSW-019`
- Feature / Function: Speed/power override controls
- Hardware Requirement: Laser
- Prerequisites: Active running job and hardware that reflects overrides
- Setup / Fixture: Safe scrap-material job
- Steps: Adjust override controls during a running job and observe machine/UI response.
- Expected Result: Override UI reflects changed values and the machine responds without destabilizing the session.
- Edge / Negative Cases: Overrides should be gated when no active job exists.
- Persistence / Reopen Check: Verify override state resets or persists according to intended behavior after job end.
- Undo / Redo Expectation: Runtime-only.
- Status: Active

### HW-031 — Progress, State, And Completion Reset
- Source Ref: `FSW-019`
- Feature / Function: Job progress reporting and return to idle after completion
- Hardware Requirement: Laser
- Prerequisites: Runnable job
- Setup / Fixture: Short job that can complete naturally
- Steps: Run job to completion and observe progress, state text, and end-state cleanup.
- Expected Result: Progress bar advances sensibly, state text is accurate, and completion returns controls to idle-ready configuration.
- Edge / Negative Cases: No stale paused/running state should remain after completion.
- Persistence / Reopen Check: Start a second job after completion to confirm clean reset.
- Undo / Redo Expectation: Runtime-only.
- Status: Active

### HW-032 — Disconnect While Idle And While Running
- Source Ref: `FSW-019`, `FSW-020`
- Feature / Function: Runtime handling of transport loss
- Hardware Requirement: Laser
- Prerequisites: Connected machine
- Setup / Fixture: One idle scenario and one safe running scenario
- Steps: Remove/disrupt connection while idle; repeat during a safe running test.
- Expected Result: UI reports disconnect cleanly, no false ready state persists, and controls re-gate correctly.
- Edge / Negative Cases: Running-job disconnect must not leave the app permanently stuck in running state.
- Persistence / Reopen Check: Reconnect after each disconnect and verify normal operation.
- Undo / Redo Expectation: Runtime-only.
- Status: Active

### HW-033 — Recover After Disconnect Or Alarm
- Source Ref: `FSW-019`
- Feature / Function: Return from faulted runtime to normal session
- Hardware Requirement: Laser
- Prerequisites: Prior disconnect or alarm event
- Setup / Fixture: Use output from `HW-012` or `HW-032`
- Steps: Clear the fault, reconnect or unlock as needed, and verify controls return to ready state.
- Expected Result: Session recovers without requiring app restart unless truly unavoidable.
- Edge / Negative Cases: Recovery should not leave stale progress or stale work-position data.
- Persistence / Reopen Check: Relaunch only if needed; verify post-recovery health.
- Undo / Redo Expectation: Runtime-only.
- Status: Active

### HW-034 — Invalid Live Commands And Failed Motion / Frame Safety
- Source Ref: `FSW-019`
- Feature / Function: Safety of failing console, frame, and move commands
- Hardware Requirement: Laser
- Prerequisites: Connected machine
- Setup / Fixture: Safe invalid command and intentionally invalid motion/frame request
- Steps: Send harmless invalid command; trigger a motion/frame request that should fail safely.
- Expected Result: Error feedback is clear, session remains usable, and no project data is corrupted.
- Edge / Negative Cases: Repeated failures must not poison later valid commands.
- Persistence / Reopen Check: Run a valid command afterward to confirm recovery.
- Undo / Redo Expectation: Runtime-only.
- Status: Active

## Camera

### CAM-001 — Camera Device Refresh, Select, And No-Profile State
- Source Ref: `FSW-022`; `LB 15`
- Feature / Function: Camera panel and machine-panel camera section in neutral and configured states
- Hardware Requirement: Laser+Camera
- Prerequisites: Camera-capable environment
- Setup / Fixture: Camera-enabled profile and a no-profile state
- Steps: Refresh device list, inspect no-profile messaging, then select a camera under an active profile.
- Expected Result: Device selector, status text, and gating behave correctly with and without an active profile.
- Edge / Negative Cases: No camera detected must remain a safe empty state.
- Persistence / Reopen Check: Relaunch and verify selected camera behavior if intended to persist.
- Undo / Redo Expectation: App/runtime state only.
- Status: Active

### CAM-002 — Capture Frame And Overlay Readiness
- Source Ref: `FSW-022`; `LB 15`
- Feature / Function: Capture flow and overlay state update
- Hardware Requirement: Laser+Camera
- Prerequisites: Selected camera and active profile
- Setup / Fixture: Camera-enabled profile
- Steps: Capture frame and inspect status, file path, frame dimensions, and overlay-ready state.
- Expected Result: New frame metadata appears and overlay state updates consistently with the capture.
- Edge / Negative Cases: Capture should be disabled cleanly when no camera is selected.
- Persistence / Reopen Check: Reopen camera surfaces and verify the last capture metadata remains coherent.
- Undo / Redo Expectation: Runtime-only.
- Status: Active

### CAM-003 — Refresh Overlay / Calibration / Alignment State
- Source Ref: `FSW-022`
- Feature / Function: Manual refresh paths for camera-derived state
- Hardware Requirement: Laser+Camera
- Prerequisites: Existing camera state
- Setup / Fixture: Captured frame plus any existing calibration/alignment data
- Steps: Trigger refresh actions for devices, overlay, calibration, and alignment.
- Expected Result: UI state refreshes without stale values and without requiring full app restart.
- Edge / Negative Cases: Refresh with missing prerequisites must fail clearly rather than silently.
- Persistence / Reopen Check: Reopen camera panel and verify refreshed state remains correct.
- Undo / Redo Expectation: Runtime-only.
- Status: Active

### CAM-004 — Lens Calibration Save And Cancel
- Source Ref: `FSW-022`; `LB 15`
- Feature / Function: Camera calibration workflow
- Hardware Requirement: Laser+Camera
- Prerequisites: Selected camera and calibration fixture/process ready
- Setup / Fixture: Camera calibration target
- Steps: Open calibration dialog, run solve once to save, then run again and cancel.
- Expected Result: Successful calibration stores quality data; cancel leaves prior state unchanged.
- Edge / Negative Cases: Failed solve must surface explicit feedback without corrupting prior calibration.
- Persistence / Reopen Check: Relaunch and verify saved calibration remains.
- Undo / Redo Expectation: Runtime/app-state only.
- Status: Active

### CAM-005 — Camera Alignment Save And Cancel
- Source Ref: `FSW-022`; `LB 15`
- Feature / Function: Camera-to-workspace alignment workflow
- Hardware Requirement: Laser+Camera
- Prerequisites: Selected camera and alignment workflow ready
- Setup / Fixture: Camera alignment fixture
- Steps: Open alignment dialog, run solve once to save, then run again and cancel.
- Expected Result: Successful alignment stores quality data; cancel preserves prior alignment.
- Edge / Negative Cases: Solve failure must not clear a previously good alignment.
- Persistence / Reopen Check: Relaunch and verify saved alignment remains.
- Undo / Redo Expectation: Runtime/app-state only.
- Status: Active

### CAM-006 — Reset Calibration And Reset Alignment
- Source Ref: `FSW-022`
- Feature / Function: Removing stored camera-derived state
- Hardware Requirement: Laser+Camera
- Prerequisites: Existing saved calibration and alignment
- Setup / Fixture: Camera with stored calibration/alignment
- Steps: Use reset calibration and reset alignment actions separately.
- Expected Result: Relevant state clears immediately and dependent UI updates accordingly.
- Edge / Negative Cases: Resetting one state should not silently wipe the other.
- Persistence / Reopen Check: Relaunch and verify reset state remains cleared.
- Undo / Redo Expectation: Runtime/app-state only.
- Status: Active

### CAM-007 — Missing Camera / Missing Frame / Stale Frame Failure Modes
- Source Ref: `FSW-022`
- Feature / Function: Camera workflow safety on invalid prerequisites
- Hardware Requirement: Laser+Camera
- Prerequisites: Scenarios with no device, stale frame, or incomplete prerequisites
- Setup / Fixture: Camera-disabled or partially configured state
- Steps: Attempt capture, calibration, alignment, and overlay operations without valid prerequisites.
- Expected Result: Commands are disabled or fail clearly without corrupting saved camera state.
- Edge / Negative Cases: Recover by restoring valid camera state and confirm functionality returns.
- Persistence / Reopen Check: Relaunch and verify no bad state was persisted.
- Undo / Redo Expectation: Runtime-only.
- Status: Active

## DSP Family

### DSP-001 — DSP Capability-Aware Profile Activation And UI
- Source Ref: `FSW-021`; `LB 12`, `LB 13`
- Feature / Function: DSP profile activation and capability-sensitive shell behavior
- Hardware Requirement: DSP
- Prerequisites: DSP-capable profile
- Setup / Fixture: DSP profile and, if possible, attached DSP controller
- Steps: Activate DSP profile and inspect relevant UI surfaces for capability-aware behavior.
- Expected Result: Profile activates cleanly and unsupported GRBL-specific affordances do not present misleading runtime behavior.
- Edge / Negative Cases: Switching back to a non-DSP profile must restore appropriate UI behavior.
- Persistence / Reopen Check: Relaunch and verify profile state.
- Undo / Redo Expectation: App/runtime state only.
- Status: Active

### DSP-002 — DSP Discovery / Connect
- Source Ref: `FSW-020`, `FSW-021`
- Feature / Function: Discovery and connection of DSP-family candidate/device
- Hardware Requirement: DSP
- Prerequisites: DSP candidate or known DSP connection path
- Setup / Fixture: DSP environment
- Steps: Discover or select DSP target and connect.
- Expected Result: Candidate metadata and connection flow remain coherent for DSP hardware.
- Edge / Negative Cases: Unsupported DSP candidate must fail clearly, not masquerade as GRBL.
- Persistence / Reopen Check: Reconnect after relaunch and confirm sane behavior.
- Undo / Redo Expectation: Runtime-only.
- Status: Active

### DSP-003 — DSP Runtime Job / Frame / Home / Origin Basics
- Source Ref: `FSW-021`
- Feature / Function: Minimum runtime validation on DSP controller
- Hardware Requirement: DSP
- Prerequisites: Connected DSP device
- Setup / Fixture: Safe DSP-capable job
- Steps: Exercise frame, home/origin behavior as applicable, and basic job lifecycle.
- Expected Result: Core runtime actions work or are safely gated according to DSP capability.
- Edge / Negative Cases: Unsupported action must be blocked or clearly reported.
- Persistence / Reopen Check: Verify DSP session can recover after completion/cancel.
- Undo / Redo Expectation: Runtime-only.
- Status: Active

### DSP-004 — DSP Device Settings, Preflight, And Save Machine File
- Source Ref: `FSW-018`, `FSW-021`
- Feature / Function: DSP-related configuration and non-live output flow
- Hardware Requirement: DSP
- Prerequisites: DSP profile
- Setup / Fixture: DSP profile and standard runtime project
- Steps: Review/save relevant device settings, run preflight, and save machine file with DSP profile active.
- Expected Result: DSP-related settings persist and output/preflight behavior remains coherent for the active controller family.
- Edge / Negative Cases: GRBL-specific settings should not silently drive invalid DSP assumptions.
- Persistence / Reopen Check: Relaunch and verify saved settings.
- Undo / Redo Expectation: Settings are app-data; preflight/export are non-mutating.
- Status: Active

### DSP-005 — Ruida Lift-Table Z/U Step Jog
- Source Ref: `FSW-019`, `FSW-021`; `LB 14`
- Feature / Function: Finite manual lift-table movement on a Ruida Z or U channel
- Hardware Requirement: Ruida RDC6442S with a motorized lift table
- Prerequisites: Connected idle Ruida machine; physical stop accessible; table channel known from the controller configuration
- Setup / Fixture: Clear the table's travel path, choose a 0.1 mm step, set **Ruida lift table axis** to the known Z or U channel, and use a conservative table feed
- Steps: Confirm the table controls appear in Move; press the negative button once, then the positive button once; repeat at 1 mm only if the first motions are correct; disable the profile setting and confirm the controls disappear.
- Expected Result: Each press produces one bounded move on the selected controller channel with no laser output; opposite signs move in opposite directions; the session returns to Ready after motion settles.
- Edge / Negative Cases: Do not guess between Z and U. If either direction approaches a travel limit, use the physical stop and report the profile, controller model/firmware, selected channel, sign, step, feed, and resulting app state.
- Persistence / Reopen Check: Save and relaunch, then verify the selected Z/U mapping and table feed persist.
- Undo / Redo Expectation: Runtime motion and app-profile settings, not project history.
- Status: Active

## Galvo Family

### GAL-001 — Galvo Capability-Aware Profile Activation And UI
- Source Ref: `FSW-021`, `FSW-032`; `LB 12`
- Feature / Function: Galvo profile activation and capability-sensitive UI
- Hardware Requirement: Galvo
- Prerequisites: Galvo-capable profile
- Setup / Fixture: Galvo profile
- Steps: Activate galvo profile and inspect the shell for capability-aware behavior and safe gating of unsupported gantry assumptions.
- Expected Result: UI reflects galvo-family expectations without falsely presenting unsupported gantry-only workflows.
- Edge / Negative Cases: Switching profiles must cleanly change capability assumptions.
- Persistence / Reopen Check: Relaunch and verify profile state.
- Undo / Redo Expectation: App/runtime only.
- Status: Active

### GAL-002 — Galvo Discovery / Connect
- Source Ref: `FSW-020`, `FSW-021`
- Feature / Function: Discovery or connection of galvo-family device
- Hardware Requirement: Galvo
- Prerequisites: Galvo candidate or connection path
- Setup / Fixture: Galvo environment
- Steps: Discover or connect galvo hardware/profile path.
- Expected Result: Connection flow remains coherent and does not force GRBL-only assumptions.
- Edge / Negative Cases: Unsupported candidate should fail clearly.
- Persistence / Reopen Check: Reconnect after relaunch to confirm repeatability.
- Undo / Redo Expectation: Runtime-only.
- Status: Active

### GAL-003 — Galvo Runtime Job Lifecycle And Telemetry
- Source Ref: `FSW-021`
- Feature / Function: Basic galvo job start/pause/cancel and telemetry readout
- Hardware Requirement: Galvo
- Prerequisites: Connected galvo device
- Setup / Fixture: Safe galvo-capable job
- Steps: Exercise basic runtime lifecycle and inspect status/telemetry surfaces.
- Expected Result: Runtime lifecycle stays coherent for galvo hardware or clearly gates unsupported controls.
- Edge / Negative Cases: Pause/resume differences vs gantry controllers must not produce misleading UI.
- Persistence / Reopen Check: Verify clean idle state after completion/cancel.
- Undo / Redo Expectation: Runtime-only.
- Status: Active

### GAL-004 — Cylinder / Rotary And Capability-Specific Safe Gating
- Source Ref: `FSW-032`; `LB 12`
- Feature / Function: Partial advanced-mode coverage for galvo-family or cylinder-related surfaces
- Hardware Requirement: Galvo
- Prerequisites: Galvo-capable profile
- Setup / Fixture: Galvo profile and any visible advanced-mode surface
- Steps: Inspect any visible rotary/cylinder/capability-specific controls and verify either working behavior or explicit safe gating.
- Expected Result: Partial features are clearly gated, labeled, or functional; no misleading “fully supported” UX remains.
- Edge / Negative Cases: If no executable workflow exists, confirm the absence is represented in the matrix via `GAP-012`.
- Persistence / Reopen Check: N/A
- Undo / Redo Expectation: Depends on whether a real setting is changed; otherwise N/A.
- Status: Active

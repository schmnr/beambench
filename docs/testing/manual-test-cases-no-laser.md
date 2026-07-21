# Beam Bench Manual Test Cases: No Laser Connected

Use these cases when no laser is connected. All cases in this document have `Hardware Requirement: None`.

Shared fixtures:

- `Empty project`
- `Standard vector fixture`: rectangle, ellipse, polygon, star, open path, closed path, grouped objects, locked object, hidden object
- `Text fixture`: plain text, path text candidate, barcode target layer, variable text object
- `Raster fixture`: transparent PNG, grayscale image, large image
- `Import fixture set`: SVG, DXF, PDF, AI/EPS, bitmap image, Lbrn `.lbrn2` and legacy `.lbrn`, invalid file

## Startup, Recovery, Autosave

### NL-001 — Clean Launch
- Source Ref: `FSW-002`, `FSW-024`; `LB 1.1`, `LB 17`
- Feature / Function: Launch on a clean profile
- Hardware Requirement: None
- Prerequisites: No saved layout, recent files, or recovery files
- Setup / Fixture: Fresh local app state
- Steps: Launch Beam Bench.
- Expected Result: Default project is created, shell renders, default panels appear, app remains responsive.
- Edge / Negative Cases: Missing optional persisted state must not block launch.
- Persistence / Reopen Check: Quit and relaunch; clean launch remains stable.
- Undo / Redo Expectation: N/A
- Status: Active

### NL-002 — Warm Launch With Persisted State
- Source Ref: `FSW-002`, `FSW-024`; `LB 17`
- Feature / Function: Restore recent files, layout, and persisted shell state
- Hardware Requirement: None
- Prerequisites: Existing saved project, modified layout, visible/hidden panels, recent file entries
- Setup / Fixture: Persist a custom shell arrangement, then quit
- Steps: Relaunch Beam Bench.
- Expected Result: Recent files, panel layout, side-panel visibility, and other persisted shell state restore without corruption.
- Edge / Negative Cases: One stale recent path must degrade gracefully while valid items still restore.
- Persistence / Reopen Check: Verify restored state survives a second relaunch.
- Undo / Redo Expectation: N/A
- Status: Active

### NL-003 — Recovery Detection And Restore
- Source Ref: `FSW-003`; `LB 1.1`, `LB 1.5`
- Feature / Function: Recovery prompt and restore workflow
- Hardware Requirement: None
- Prerequisites: Autosave/recovery artifact exists for a dirty project
- Setup / Fixture: Create edits, force-close the app, relaunch
- Steps: Choose restore from the recovery dialog.
- Expected Result: Recovery dialog appears, restored project opens with expected dirty state and geometry.
- Edge / Negative Cases: Multiple recovery entries must remain distinguishable.
- Persistence / Reopen Check: Save restored project, relaunch, and verify the restored content is now the normal project state.
- Undo / Redo Expectation: Undo history may reset; project state must remain correct.
- Status: Active

### NL-004 — Recovery Dismiss And Discard Paths
- Source Ref: `FSW-003`
- Feature / Function: Non-restore recovery actions
- Hardware Requirement: None
- Prerequisites: Recovery dialog present
- Setup / Fixture: Existing recovery files
- Steps: Dismiss recovery without restore; relaunch; then discard recovery and relaunch again.
- Expected Result: Dismiss hides the dialog for the session without deleting source recovery; discard removes recovery from future startup.
- Edge / Negative Cases: Closing via `X`, Escape, and backdrop must behave consistently with the intended dismiss path.
- Persistence / Reopen Check: Relaunch after each action to confirm correct persisted behavior.
- Undo / Redo Expectation: N/A
- Status: Active

### NL-005 — Unsaved Close Prompt
- Source Ref: `FSW-002`, `FSW-003`; `LB 1.1`
- Feature / Function: Save / discard / cancel when closing with unsaved edits
- Hardware Requirement: None
- Prerequisites: Dirty project
- Setup / Fixture: Modify any object, then close or replace project
- Steps: Exercise save, discard, and cancel paths.
- Expected Result: Save persists changes, discard drops changes, cancel leaves the project open and intact.
- Edge / Negative Cases: Canceling the OS file-save dialog must leave the close action aborted.
- Persistence / Reopen Check: Reopen project after save/discard and confirm the chosen path matches disk state.
- Undo / Redo Expectation: Cancel preserves undo stack; save/discard may reset as normal.
- Status: Active

### NL-006 — Broken Recent File / Settings Fallback
- Source Ref: `FSW-003`, `FSW-024`
- Feature / Function: Graceful startup with stale recent entries or broken persisted settings
- Hardware Requirement: None
- Prerequisites: One invalid recent file entry or malformed optional persisted state
- Setup / Fixture: Seed stale recent path or corrupt non-critical settings state
- Steps: Launch app and attempt to use recent file entry.
- Expected Result: App boots, invalid recent file fails with clear feedback, shell falls back to defaults where needed.
- Edge / Negative Cases: A single bad entry must not wipe unrelated valid entries.
- Persistence / Reopen Check: Relaunch after clearing the failing path and verify normal behavior.
- Undo / Redo Expectation: N/A
- Status: Active

### NL-007 — Autosave Cadence And Relaunch
- Source Ref: `FSW-003`
- Feature / Function: Autosave generation and stability over repeated edits
- Hardware Requirement: None
- Prerequisites: Autosave enabled
- Setup / Fixture: Dirty project with edits over several minutes
- Steps: Make changes, wait past autosave interval, quit unexpectedly, relaunch.
- Expected Result: Recovery reflects recent state within the configured interval and no duplicate or corrupt recovery batch appears.
- Edge / Negative Cases: Rapid edit bursts must not freeze the UI or create malformed recovery state.
- Persistence / Reopen Check: Restore recovered work, save it, and confirm recovery artifacts no longer reappear.
- Undo / Redo Expectation: Recovery content correctness matters more than preserving prior history.
- Status: Active

## Menus, Toolbars, Panels, Layout, Dialog Shell

### NL-008 — File Menu Inventory And Enablement
- Source Ref: `FSW-024`; `LB 1.1`
- Feature / Function: File menu actions and enable/disable state
- Hardware Requirement: None
- Prerequisites: App running with and without an open project
- Setup / Fixture: Empty project and saved project
- Steps: Open File menu; verify New, Open, Save, Save As, export items, Import, Notes, Preferences, and recent projects.
- Expected Result: Each item is present, correctly enabled/disabled, and launches the intended flow.
- Edge / Negative Cases: No-project state must disable only the actions that require a project.
- Persistence / Reopen Check: Use Save / Save As from the menu and confirm behavior persists after reopen.
- Undo / Redo Expectation: Menu open/close is non-mutating.
- Status: Active

### NL-009 — Edit Menu Inventory And Enablement
- Source Ref: `FSW-024`; `LB 1.2`, `LB 4`, `LB 5`
- Feature / Function: Edit menu action availability
- Hardware Requirement: None
- Prerequisites: Cases with no selection, single selection, multi-selection, and clipboard content
- Setup / Fixture: Standard vector fixture
- Steps: Open Edit menu in each selection state and verify undo/redo, selection, duplicate, path ops, boolean ops, grouping, align, and distribute entries.
- Expected Result: Menu reflects current selection capability accurately and launches the right mutation when invoked.
- Edge / Negative Cases: Locked or incompatible selections must disable invalid actions rather than fail late.
- Persistence / Reopen Check: Save after a representative mutation and reopen to verify mutation persisted.
- Undo / Redo Expectation: Each invoked edit action must be undoable unless documented otherwise.
- Status: Active

### NL-010 — View And Window Menus
- Source Ref: `FSW-024`; `LB 1.3`, `LB 17`
- Feature / Function: View toggles and window/panel visibility flows
- Hardware Requirement: None
- Prerequisites: App running
- Setup / Fixture: Default project
- Steps: Exercise zoom commands, Grid, Snap to Grid, Snap to Objects, Preview, Preview Window, Side Panels, panel toggles, view styles, and reset layout.
- Expected Result: View state updates immediately, checkmarks stay in sync, panel toggles match actual visibility, and reset restores defaults.
- Edge / Negative Cases: Floating panels must remain discoverable through the Window menu.
- Persistence / Reopen Check: Persist non-default layout and view state, relaunch, then reset and verify default restoration.
- Undo / Redo Expectation: View-only toggles are typically non-historied; layout-affecting changes should persist correctly.
- Status: Active

### NL-011 — Tools And Arrange Menus
- Source Ref: `FSW-024`; `LB 2`, `LB 4`, `LB 5`
- Feature / Function: Tool activation and arrange-command access from menus
- Hardware Requirement: None
- Prerequisites: Standard vector fixture with valid selections
- Setup / Fixture: Single selection, multi-selection, image selection, text/path pair
- Steps: Invoke menu actions for drawing tools, barcode, offset, bitmap/image tools, path/text/image applications, arrays, break apart, copy along path, rubber-band outline, lock/unlock, draw order, and arrange helpers.
- Expected Result: Each menu item routes to the correct tool or dialog and is enabled only for valid state.
- Edge / Negative Cases: Unsupported combinations must disable the command instead of opening a broken dialog.
- Persistence / Reopen Check: Save after representative tool-driven mutations and reopen to verify state.
- Undo / Redo Expectation: Mutation-driving actions must be undoable.
- Status: Active

### NL-012 — Machine, Laser Tools, And Help Menus With No Hardware
- Source Ref: `FSW-024`, `FSW-031`; `LB 11`, `LB 13`
- Feature / Function: Safe gating of machine-related menus when disconnected
- Hardware Requirement: None
- Prerequisites: No machine connected
- Setup / Fixture: App running without active connection
- Steps: Open Machine, Laser Tools, and Help menus; inspect enablement; open allowed dialogs/help flows.
- Expected Result: Hardware actions are disabled or safely gated; Help actions work; deferred quality-test dialogs open or communicate limitations without corrupting state.
- Edge / Negative Cases: No disconnected action should falsely imply a live machine state.
- Persistence / Reopen Check: Close and relaunch after opening these dialogs; no stray machine state should persist.
- Undo / Redo Expectation: Non-mutating.
- Status: Active

### NL-013 — Panel Visibility And Docking
- Source Ref: `FSW-024`; `LB 17`
- Feature / Function: Show/hide and re-dock panels
- Hardware Requirement: None
- Prerequisites: Panel system active
- Setup / Fixture: Toggle several panels from Window menu and tab context menus
- Steps: Hide panels, re-show them, drag them between supported zones, and re-dock floating panels.
- Expected Result: Panels remain usable, tabs stay selectable, and panel content survives movement.
- Edge / Negative Cases: Hidden camera or floating panels must still be recoverable.
- Persistence / Reopen Check: Relaunch and verify visibility/dock state persisted.
- Undo / Redo Expectation: Layout changes are not object-history actions.
- Status: Active

### NL-014 — Floating Panels, Z-Order, And Redock
- Source Ref: `FSW-024`
- Feature / Function: Floating panel behavior
- Hardware Requirement: None
- Prerequisites: At least two floating panels
- Setup / Fixture: Float multiple panels
- Steps: Bring panels to front, overlap them, resize them, and dock them back.
- Expected Result: Z-order is sensible, no panel becomes inaccessible, and size constraints are respected.
- Edge / Negative Cases: Panels should not drift irrecoverably off-screen.
- Persistence / Reopen Check: Relaunch and confirm floating geometry persists or safely normalizes.
- Undo / Redo Expectation: N/A
- Status: Active

### NL-015 — Layout Persistence And Reset
- Source Ref: `FSW-024`
- Feature / Function: Persist custom layout and restore defaults
- Hardware Requirement: None
- Prerequisites: Modified layout
- Setup / Fixture: Reorder zones, hide panels, float at least one panel
- Steps: Quit and relaunch; then use reset-to-default-layout.
- Expected Result: Custom layout restores first; reset returns known default panel arrangement and re-enables side panels if needed.
- Edge / Negative Cases: Default-only panels must not disappear permanently after reset.
- Persistence / Reopen Check: Relaunch after reset and confirm default layout remains.
- Undo / Redo Expectation: N/A
- Status: Active

### NL-016 — Preview Window And Modal Shell Behavior
- Source Ref: `FSW-015`, `FSW-024`; `LB 1.5`
- Feature / Function: Preview window open/close shell behavior
- Hardware Requirement: None
- Prerequisites: Project capable of preview generation
- Setup / Fixture: Standard vector fixture
- Steps: Open Preview Window from menu and View menu, close via button and Escape, reopen, and interact with background shell.
- Expected Result: Preview window opens consistently, closes cleanly, and does not leave stale modal focus traps.
- Edge / Negative Cases: Preview window should remain stable if preview data is stale or empty.
- Persistence / Reopen Check: Reopen the window after project reload and confirm it still works.
- Undo / Redo Expectation: N/A
- Status: Active

### NL-017 — Settings, About, Notes, And Support Dialog Shell
- Source Ref: `FSW-003`, `FSW-024`; `LB 16`
- Feature / Function: Non-machine dialog shell behavior
- Hardware Requirement: None
- Prerequisites: App running
- Setup / Fixture: Open dialogs from menus
- Steps: Open Settings, About, Notes, and Generate Support Data flows; close via buttons, Escape, and backdrop where supported.
- Expected Result: Dialogs render with correct titles, keyboard handling works, and close behavior is consistent.
- Edge / Negative Cases: Canceling support-data export must not surface as a hard failure.
- Persistence / Reopen Check: Reopen each dialog after closing to confirm state remains healthy.
- Undo / Redo Expectation: Non-mutating except Notes/Settings content edits.
- Status: Active

### NL-018 — Main Toolbar Command Inventory
- Source Ref: `FSW-024`; `LB 5`, `LB 17`
- Feature / Function: Main toolbar actions
- Hardware Requirement: None
- Prerequisites: Fixture states for empty, selected, and multi-selected objects
- Setup / Fixture: Standard vector fixture
- Steps: Exercise File, Undo/Redo, Clipboard, Zoom, Grid/Snap, Preview, Device Settings shell, Group/Ungroup, Flip, align/distribute, same-size, move-together, Dock, Resize Slots, and Center on Page.
- Expected Result: Buttons are labeled, visually reflect enabled state, and invoke the same behavior as their menu equivalents.
- Edge / Negative Cases: Locked selections must block mutation buttons clearly.
- Persistence / Reopen Check: Save after representative actions and reopen.
- Undo / Redo Expectation: Mutations must be undoable.
- Status: Active

### NL-019 — Creation Toolbar Command Inventory
- Source Ref: `FSW-024`; `LB 2`
- Feature / Function: Creation toolbar tool activation
- Hardware Requirement: None
- Prerequisites: Canvas available
- Setup / Fixture: Empty project
- Steps: Activate Select, Draw, Shape submenu variants, Text, Node Edit, Trim, Tabs, Laser Position, and Measure.
- Expected Result: Active tool state changes correctly and shape presets configure the expected underlying tool.
- Edge / Negative Cases: Repeated tool changes must not leave stuck submodes.
- Persistence / Reopen Check: Tool state need not persist; project state must remain clean if nothing was created.
- Undo / Redo Expectation: Tool selection alone is non-historied.
- Status: Active

### NL-020 — Modifiers Toolbar Inventory
- Source Ref: `FSW-024`; `LB 4`, `LB 5`
- Feature / Function: Modifiers toolbar actions and dialogs
- Hardware Requirement: None
- Prerequisites: Valid single and multi-object selections
- Setup / Fixture: Standard vector fixture
- Steps: Exercise Offset, Weld, boolean submenu, Grid Array, Circular Array, Set Start Point, and Radius Tool.
- Expected Result: Each command is enabled only in valid contexts and opens correct dialogs or modes.
- Edge / Negative Cases: Invalid selections must disable commands rather than produce broken output.
- Persistence / Reopen Check: Save after representative mutations and reopen.
- Undo / Redo Expectation: Mutations must be undoable.
- Status: Active

### NL-021 — Node Subtoolbar Inventory
- Source Ref: `FSW-024`; `LB 3.1`
- Feature / Function: Node subtoolbar modes and immediate actions
- Hardware Requirement: None
- Prerequisites: Active node-editable vector object
- Setup / Fixture: Open and closed paths
- Steps: Enter Node Edit; switch through all submodes and trigger midpoint, align, trim-to-intersection, and extend-to-intersection actions.
- Expected Result: Subtoolbar appears only in node mode, active state tracks selected submode, immediate actions fire the intended operation.
- Edge / Negative Cases: Exiting node mode removes the subtoolbar cleanly.
- Persistence / Reopen Check: Save after representative node edits and reopen.
- Undo / Redo Expectation: Each edit action is undoable.
- Status: Active

### NL-022 — Status Bar Live State
- Source Ref: `FSW-024`, `FSW-033`; `LB 17`
- Feature / Function: Status bar readouts and toggles
- Hardware Requirement: None
- Prerequisites: Project with selectable objects
- Setup / Fixture: Standard vector fixture
- Steps: Move pointer, change selection, zoom, toggle grid/snap, and inspect transform-lock toggles.
- Expected Result: Cursor, selection bounds, zoom, view toggles, and transform-lock state update live and remain accessible by keyboard.
- Edge / Negative Cases: Empty selection should degrade to neutral readouts, not stale data.
- Persistence / Reopen Check: Toggle persisted status preferences where applicable and relaunch.
- Undo / Redo Expectation: View toggles are non-historied.
- Status: Active

## Project Lifecycle, Import, Persistence

### NL-023 — New / Open / Save / Save As Lifecycle
- Source Ref: `FSW-002`; `LB 1.1`
- Feature / Function: Core project document lifecycle
- Hardware Requirement: None
- Prerequisites: At least one saved project path available
- Setup / Fixture: Empty project and standard fixture
- Steps: Create new project, save it, modify it, use Save As, then reopen both variants.
- Expected Result: Path, metadata, and contents track the correct file after each save flow.
- Edge / Negative Cases: Canceling file chooser must not mutate path or dirty state.
- Persistence / Reopen Check: Reopen saved files and verify exact content.
- Undo / Redo Expectation: Document replacement may reset history; current document edits remain correct.
- Status: Active

### NL-024 — Recent Projects Handling
- Source Ref: `FSW-002`; `LB 1.1`
- Feature / Function: Recent project list population and use
- Hardware Requirement: None
- Prerequisites: Multiple saved projects
- Setup / Fixture: Open several projects in sequence
- Steps: Open File menu and use Recent Projects entries.
- Expected Result: Recent list reflects actual project history, opens selected file, and handles stale entries with clear feedback.
- Edge / Negative Cases: Duplicate path entries should not accumulate incorrectly.
- Persistence / Reopen Check: Relaunch and verify recent list persists.
- Undo / Redo Expectation: Opening another project replaces document state as expected.
- Status: Active

### NL-025 — Import Picker Cancel And Mixed Invalid Files
- Source Ref: `FSW-012`, `FSW-013`; `LB 1.1`
- Feature / Function: Picker-based import error handling
- Hardware Requirement: None
- Prerequisites: Existing project open
- Setup / Fixture: Valid and invalid import files
- Steps: Open Import, cancel; then choose mixed valid and invalid files.
- Expected Result: Cancel causes no mutation; mixed import yields valid objects plus clear reporting for rejected files.
- Edge / Negative Cases: Completely unsupported selection must fail cleanly without partial corruption.
- Persistence / Reopen Check: Save after mixed import and reopen to verify only valid imported objects persisted.
- Undo / Redo Expectation: Import should be undoable as a coherent mutation.
- Status: Active

### NL-026 — Drag-And-Drop Import Routing
- Source Ref: `FSW-012`; `LB 1.1`
- Feature / Function: Drop-zone import for single and multi-file batches
- Hardware Requirement: None
- Prerequisites: App open with visible canvas
- Setup / Fixture: Mixed file batch containing vector, raster, and unsupported file
- Steps: Drag files onto the app in single-file and multi-file batches.
- Expected Result: Supported files import, auto-routing feedback appears, and unsupported files do not break the drop flow.
- Edge / Negative Cases: Duplicate dropped files should not create inconsistent state.
- Persistence / Reopen Check: Save and reopen after drag-drop import.
- Undo / Redo Expectation: Drop import should be undoable.
- Status: Active

### NL-027 — Project Replacement Resets Transient State
- Source Ref: `FSW-002`, `FSW-004`
- Feature / Function: Clearing transient state on open/replace
- Hardware Requirement: None
- Prerequisites: Project A with selection, clipboard state, preview, and modified layout; Project B available
- Setup / Fixture: Two saved projects
- Steps: Open Project B while Project A is active.
- Expected Result: Selection, transient clipboard assumptions, preview data, and project-specific state reset appropriately for Project B.
- Edge / Negative Cases: Panel layout should persist globally while document state resets.
- Persistence / Reopen Check: Reopen Project A and verify its saved content is still correct.
- Undo / Redo Expectation: Document replacement is not treated as an in-document undo step.
- Status: Active

### NL-028 — Save / Reopen Imported Project
- Source Ref: `FSW-002`, `FSW-012`, `FSW-013`
- Feature / Function: Imported document round trip
- Hardware Requirement: None
- Prerequisites: Project with mixed imported objects
- Setup / Fixture: Imported SVG, bitmap, and PDF/DXF/AI content
- Steps: Save imported project, close it, and reopen it.
- Expected Result: Layers, objects, text, raster settings, and object transforms reopen faithfully.
- Edge / Negative Cases: Large import batches should not reopen partially or lose routing metadata.
- Persistence / Reopen Check: This case is itself the reopen check.
- Undo / Redo Expectation: After reopen, new edits should create fresh history normally.
- Status: Active

## Canvas Navigation, View, Selection

### NL-029 — Zoom Controls
- Source Ref: `FSW-005`, `FSW-024`; `LB 1.3`
- Feature / Function: Zoom in, zoom out, fit page, fit selection, menu zoom
- Hardware Requirement: None
- Prerequisites: Project with objects spread across the bed
- Setup / Fixture: Standard vector fixture
- Steps: Use wheel zoom, menu zoom, toolbar zoom, Fit Page, and Fit Selection.
- Expected Result: Viewport zooms smoothly and centers correctly for each command.
- Edge / Negative Cases: Fit Selection with no selection must stay disabled or no-op safely.
- Persistence / Reopen Check: Zoom level need not persist; reopening should not corrupt view state.
- Undo / Redo Expectation: View-only action, non-historied.
- Status: Active

### NL-030 — Pan Controls
- Source Ref: `FSW-005`; `LB 1.3`
- Feature / Function: Spacebar pan and middle-mouse pan
- Hardware Requirement: None
- Prerequisites: Zoomed-in canvas
- Setup / Fixture: Standard vector fixture
- Steps: Pan with spacebar drag and middle mouse across several zoom levels.
- Expected Result: Canvas pans without accidental selection or object movement.
- Edge / Negative Cases: Starting pan while a creation tool is active must not leave phantom geometry.
- Persistence / Reopen Check: N/A
- Undo / Redo Expectation: Non-historied.
- Status: Active

### NL-031 — Grid, Snap, And View Style Toggles
- Source Ref: `FSW-005`, `FSW-024`; `LB 1.4`, `LB 19.2`, `LB 19.3`
- Feature / Function: Grid visibility, snap modes, and view styles
- Hardware Requirement: None
- Prerequisites: Objects positioned away from integer grid intersections
- Setup / Fixture: Standard vector fixture
- Steps: Toggle Grid, Snap to Grid, Snap to Objects, and each view style from menu/toolbar/window surfaces.
- Expected Result: Visual state and snapping behavior update immediately; all surfaces stay synchronized.
- Edge / Negative Cases: Changing view style must not clear selection or break preview invalidation.
- Persistence / Reopen Check: Persist settings where supported and relaunch.
- Undo / Redo Expectation: View settings are non-historied.
- Status: Active

### NL-032 — Ruler, Cursor, And Selection Readouts
- Source Ref: `FSW-024`; `LB 17`, `LB 22`
- Feature / Function: Live positional readouts
- Hardware Requirement: None
- Prerequisites: Project with selectable objects
- Setup / Fixture: Standard vector fixture
- Steps: Move pointer across canvas, select objects, transform them, and watch ruler/status feedback.
- Expected Result: Cursor location, selection bounds, and numeric readouts reflect the current scene accurately.
- Edge / Negative Cases: Off-bed objects and fractional positions must still render meaningful numbers.
- Persistence / Reopen Check: N/A
- Undo / Redo Expectation: Readout-only.
- Status: Active

### NL-033 — Basic Selection Gestures
- Source Ref: `FSW-005`; `LB 1.2`
- Feature / Function: Click, shift-click, ctrl/cmd-click, drag-enclose, drag-crossing, deselect
- Hardware Requirement: None
- Prerequisites: Multiple nearby objects
- Setup / Fixture: Standard vector fixture
- Steps: Exercise each standard selection gesture and keyboard deselect path.
- Expected Result: Selection contents match gesture semantics and update the rest of the UI immediately.
- Edge / Negative Cases: Drag-start on empty space should not unexpectedly move objects.
- Persistence / Reopen Check: Save/reopen not required for selection-only behavior.
- Undo / Redo Expectation: Pure selection changes are non-historied unless the app intentionally tracks them.
- Status: Active

### NL-034 — Overlapping, Grouped, Locked, And Hidden Selection
- Source Ref: `FSW-004`, `FSW-005`; `LB 1.2`, `LB 18`
- Feature / Function: Selection behavior on special object states
- Hardware Requirement: None
- Prerequisites: Overlapping objects, group, locked object, hidden object
- Setup / Fixture: Standard vector fixture
- Steps: Attempt direct selection, additive selection, and layer-based selection across each state.
- Expected Result: Group selection, lock gating, and hidden-object exclusion behave predictably.
- Edge / Negative Cases: Locked objects may be selectable but must reject invalid mutations cleanly.
- Persistence / Reopen Check: Save/reopen grouped and locked states to confirm persistence.
- Undo / Redo Expectation: State-changing actions remain undoable.
- Status: Active

### NL-035 — Selection-Dependent Enablement
- Source Ref: `FSW-024`
- Feature / Function: Menu, toolbar, and panel enablement based on selection context
- Hardware Requirement: None
- Prerequisites: No selection, single vector, single raster, text object, compatible pair, incompatible pair, multi-selection
- Setup / Fixture: Mixed object project
- Steps: Compare enabled actions across all selection states.
- Expected Result: Only valid commands are enabled; no disabled command becomes reachable through an alternate surface.
- Edge / Negative Cases: Mixed selections must not falsely expose boolean or path-only actions.
- Persistence / Reopen Check: N/A
- Undo / Redo Expectation: N/A
- Status: Active

## Creation Tools

### NL-036 — Line And Bezier Creation
- Source Ref: `FSW-005`; `LB 2.1`
- Feature / Function: Pen tool for corner and bezier creation
- Hardware Requirement: None
- Prerequisites: Empty or scratch layer
- Setup / Fixture: Empty project
- Steps: Create corner segments, click-drag bezier segments, finish with right-click/Escape, close on start point.
- Expected Result: Intended geometry is created with correct bounds and selectable afterward.
- Edge / Negative Cases: Cancel during in-progress draw must not leave orphan geometry.
- Persistence / Reopen Check: Save/reopen and verify created path remains correct.
- Undo / Redo Expectation: Creation is undoable; redo recreates the same geometry.
- Status: Active

### NL-037 — Shape Creation Presets
- Source Ref: `FSW-005`; `LB 2.2`
- Feature / Function: Rectangle, ellipse, polygon presets, star, dual star
- Hardware Requirement: None
- Prerequisites: Empty or scratch layer
- Setup / Fixture: Empty project
- Steps: Create each primary shape via toolbar or menu presets.
- Expected Result: Shape type, polygon side count, and star dual-radius mode match the selected preset.
- Edge / Negative Cases: Rapidly switching presets must not reuse the wrong underlying tool configuration.
- Persistence / Reopen Check: Save/reopen and confirm shapes remain typed correctly.
- Undo / Redo Expectation: Each creation is undoable.
- Status: Active

### NL-038 — Text Creation And Direct Edit
- Source Ref: `FSW-008`; `LB 2.3`
- Feature / Function: Create text object and edit content
- Hardware Requirement: None
- Prerequisites: Text tool available
- Setup / Fixture: Empty project
- Steps: Create text, enter content, reselect it, and edit content again.
- Expected Result: Text object appears on the canvas, content updates, and selection/properties sync correctly.
- Edge / Negative Cases: Empty text or canceled edit must not create corrupt text objects.
- Persistence / Reopen Check: Save/reopen and verify text content and bounds.
- Undo / Redo Expectation: Text creation and content edits are undoable.
- Status: Active

### NL-039 — Tool Switch / Cancel Safety
- Source Ref: `FSW-024`; `LB 2`, `LB 17`
- Feature / Function: Safe interruption of creation and edit tools
- Hardware Requirement: None
- Prerequisites: A tool with in-progress state
- Setup / Fixture: Begin drawing or editing, then switch tools
- Steps: Switch tools mid-action, press Escape, click empty canvas, and return to Select.
- Expected Result: No stuck handles, phantom objects, or broken cursor/tool state remain.
- Edge / Negative Cases: Switching from node mode or trim mode should fully clear transient overlays.
- Persistence / Reopen Check: N/A
- Undo / Redo Expectation: Only committed mutations should appear in history.
- Status: Active

### NL-040 — Measure And Laser-Position Tool Safety
- Source Ref: `FSW-024`; `LB 2.6`
- Feature / Function: Non-edit tools that should not mutate geometry
- Hardware Requirement: None
- Prerequisites: Existing objects on canvas
- Setup / Fixture: Standard vector fixture
- Steps: Activate Measure and Laser Position tools, interact with canvas, cancel, and switch away.
- Expected Result: Tools provide feedback without changing project geometry or selection unexpectedly.
- Edge / Negative Cases: These tools must remain safe even with no hardware connected.
- Persistence / Reopen Check: N/A
- Undo / Redo Expectation: No history entries for inspection-only interactions.
- Status: Active

## Layers, Properties, Text, And Path Editing

### NL-041 — Layer CRUD And Family Naming
- Source Ref: `FSW-004`, `FSW-016`; `LB 8.1`
- Feature / Function: Add, rename, recolor, duplicate, delete, and reorder layers
- Hardware Requirement: None
- Prerequisites: Project with multiple layers
- Setup / Fixture: Standard vector fixture
- Steps: Create layers of different operation types, rename them, recolor them, reorder them, and remove one.
- Expected Result: Layer names, family labels, and order update consistently in the panel and on canvas.
- Edge / Negative Cases: Auto-generated family labels should remain stable when custom names are not set.
- Persistence / Reopen Check: Save/reopen and verify layer order and naming.
- Undo / Redo Expectation: Layer mutations are undoable.
- Status: Active

### NL-042 — Layer Toggles And Global Actions
- Source Ref: `FSW-004`, `FSW-016`; `LB 8.1`
- Feature / Function: Visible/output/air-assist toggles plus enable/show-all and sort helpers
- Hardware Requirement: None
- Prerequisites: Project with several layers
- Setup / Fixture: Mixed-operation layer stack
- Steps: Toggle visibility/output/air assist; use global actions such as enable all, show all, sort cut-last.
- Expected Result: Layer toggles affect canvas and preview state immediately and global actions apply consistently.
- Edge / Negative Cases: Hidden layers must not continue contributing visible geometry unexpectedly.
- Persistence / Reopen Check: Save/reopen and verify toggle state.
- Undo / Redo Expectation: Layer-state changes are undoable if they mutate project data.
- Status: Active

### NL-043 — Cut Settings Per Operation
- Source Ref: `FSW-014`, `FSW-016`; `LB 8.2`, `LB 21.*`
- Feature / Function: Cut settings editor across line, fill, offset fill, image, cut, and score layers
- Hardware Requirement: None
- Prerequisites: One layer of each operation type
- Setup / Fixture: Mixed-operation project
- Steps: Switch through layers and edit speed, power, passes, raster settings, overlap, direction, prefixes/suffixes, and operation-specific fields.
- Expected Result: Only applicable fields appear, edits persist to the correct layer, and preview invalidates when relevant.
- Edge / Negative Cases: Invalid/extreme inputs should clamp, reject, or normalize safely.
- Persistence / Reopen Check: Save/reopen and confirm settings round trip.
- Undo / Redo Expectation: Edits are undoable as layer-setting mutations.
- Status: Active

### NL-044 — Layer Settings Copy / Paste / Sort / Reassign
- Source Ref: `FSW-004`, `FSW-016`
- Feature / Function: Layer settings clipboard and moving objects between layers
- Hardware Requirement: None
- Prerequisites: At least two layers with distinct settings
- Setup / Fixture: Mixed-operation project
- Steps: Copy settings from one layer, paste to another, sort layers, and reassign selected objects to another layer.
- Expected Result: Settings transfer cleanly and reassigned objects update rendering and properties immediately.
- Edge / Negative Cases: Pasting incompatible settings should normalize rather than corrupt the target layer.
- Persistence / Reopen Check: Save/reopen and verify reassignment and settings transfer.
- Undo / Redo Expectation: Copy is non-mutating; paste/sort/reassign are undoable.
- Status: Active

### NL-045 — Transform Interactions And Locks
- Source Ref: `FSW-005`; `LB 1.4`
- Feature / Function: Move, resize, rotate, shear, and transform-lock gating
- Hardware Requirement: None
- Prerequisites: Unlocked and locked objects
- Setup / Fixture: Standard vector fixture
- Steps: Manipulate selection handles, then toggle move/size/rotate/shear locks and retry.
- Expected Result: Allowed transforms behave normally; locked transform types are blocked with clear feedback.
- Edge / Negative Cases: Locking one transform type must not unintentionally block others.
- Persistence / Reopen Check: Save/reopen and verify transform-locked project state.
- Undo / Redo Expectation: Successful transforms are undoable; blocked attempts create no history.
- Status: Active

### NL-046 — Numeric Arrange / Move / Align / Distribute
- Source Ref: `FSW-004`, `FSW-005`; `LB 5`, `LB 22`
- Feature / Function: Numeric placement, align/distribute, same-size, move-together, center-on-page
- Hardware Requirement: None
- Prerequisites: Multi-selection with varying sizes
- Setup / Fixture: Standard vector fixture
- Steps: Use toolbar/menu arrange controls and numeric move/property entry points.
- Expected Result: Objects move to the expected absolute/relative positions and arrangement results are visually correct.
- Edge / Negative Cases: Too-few selections must disable distribute and other multi-object-only actions.
- Persistence / Reopen Check: Save/reopen and verify final arrangement.
- Undo / Redo Expectation: Each arrange mutation is undoable.
- Status: Active

### NL-047 — Grouping, Draw Order, Lock, And Visibility
- Source Ref: `FSW-004`; `LB 5`, `LB 18`
- Feature / Function: Group/ungroup, lock/unlock, push draw order, object visibility
- Hardware Requirement: None
- Prerequisites: Several overlapping objects
- Setup / Fixture: Standard vector fixture
- Steps: Group and ungroup, lock and unlock, toggle object visibility, push objects forward/back/front/back.
- Expected Result: Object relationships and draw order update correctly in both canvas and properties/panels.
- Edge / Negative Cases: Locked groups must still be recoverable and unlockable.
- Persistence / Reopen Check: Save/reopen and verify object state.
- Undo / Redo Expectation: Each mutation is undoable.
- Status: Active

### NL-048 — Single-Object Properties
- Source Ref: `FSW-004`; `LB 22`
- Feature / Function: Name, layer, power scale, cut priority, visible, locked, shape-specific fields
- Hardware Requirement: None
- Prerequisites: Single object selected
- Setup / Fixture: Rectangle, ellipse, polygon, star, and raster/text objects
- Steps: Edit properties for each object type from the properties panel.
- Expected Result: Property edits update the selected object only and remain in sync with canvas state.
- Edge / Negative Cases: Unsupported fields should not appear for the wrong object type.
- Persistence / Reopen Check: Save/reopen and verify property values.
- Undo / Redo Expectation: Property edits are undoable.
- Status: Active

### NL-049 — Batch Properties
- Source Ref: `FSW-004`
- Feature / Function: Multi-selection batch layer/visibility/lock editing
- Hardware Requirement: None
- Prerequisites: Multi-selection with mixed values
- Setup / Fixture: Mixed object project
- Steps: Use batch properties to reassign layer, toggle visibility, and lock state.
- Expected Result: Mixed state is represented correctly and batch edits apply to the whole selection.
- Edge / Negative Cases: Indeterminate checkbox states must resolve predictably after the change.
- Persistence / Reopen Check: Save/reopen and verify batch edit results.
- Undo / Redo Expectation: Batch property mutations are undoable.
- Status: Active

### NL-050 — Text Formatting Toolbar / Properties Sync
- Source Ref: `FSW-008`; `LB 2.3`, `LB 22`
- Feature / Function: Font, size, style, alignment, spacing, text defaults
- Hardware Requirement: None
- Prerequisites: Selected text object and active Text tool
- Setup / Fixture: Text fixture
- Steps: Edit font family, size, bold, italic, uppercase, alignments, spacing, and verify toolbar vs properties synchronization.
- Expected Result: Text object updates immediately and tool defaults behave correctly when no text object is selected.
- Edge / Negative Cases: Missing system fonts must fall back gracefully.
- Persistence / Reopen Check: Save/reopen and confirm text formatting.
- Undo / Redo Expectation: Text-format changes are undoable.
- Status: Active

### NL-051 — Text On Path
- Source Ref: `FSW-009`; `LB 2.3`
- Feature / Function: Apply path to text and revert/remove path text
- Hardware Requirement: None
- Prerequisites: One text object and one compatible path
- Setup / Fixture: Text fixture plus curve path
- Steps: Apply path to text, inspect layout, then remove or change the path.
- Expected Result: Text follows the path correctly and transitions cleanly back to non-path layout when removed.
- Edge / Negative Cases: Invalid text/path selection should be blocked rather than partially applied.
- Persistence / Reopen Check: Save/reopen and verify path-text geometry/state.
- Undo / Redo Expectation: Apply/remove path text are undoable.
- Status: Active

### NL-052 — Barcode Dialog
- Source Ref: `FSW-010`; `LB 2.5`
- Feature / Function: Barcode creation across supported symbologies
- Hardware Requirement: None
- Prerequisites: Existing target layer
- Setup / Fixture: Empty project or dedicated barcode layer
- Steps: Open Create Barcode, try representative 1D and 2D options, and insert the result.
- Expected Result: Barcode object is created on the intended layer with valid geometry and selectable bounds.
- Edge / Negative Cases: Invalid or incomplete barcode input should block creation with clear feedback.
- Persistence / Reopen Check: Save/reopen and verify barcode object remains valid.
- Undo / Redo Expectation: Barcode insertion is undoable.
- Status: Active

### NL-053 — Variable Text Normal / Serial / Date
- Source Ref: `FSW-008`, `FSW-009`; `LB 2.4`
- Feature / Function: Variable text mode switching without CSV
- Hardware Requirement: None
- Prerequisites: Selected text object
- Setup / Fixture: Variable text-ready text object
- Steps: Switch among normal, serial, and date/time modes; edit template, offset, start/end/current/advance.
- Expected Result: Preview text updates and configuration persists to the object.
- Edge / Negative Cases: Zero/negative/invalid numeric inputs should not corrupt the variable-text state.
- Persistence / Reopen Check: Save/reopen and verify the variable text config persists.
- Undo / Redo Expectation: Config edits are undoable.
- Status: Active

### NL-054 — Variable Text CSV / Bake / Warnings
- Source Ref: `FSW-008`, `FSW-009`; `LB 2.4`
- Feature / Function: CSV load, merge preview, bake, and warning paths
- Hardware Requirement: None
- Prerequisites: Selected variable-text object
- Setup / Fixture: Valid CSV and malformed CSV fixture
- Steps: Load CSV, inspect merge fields, preview row output, use previous/next/reset, then bake resolved text; repeat with malformed/missing-field cases.
- Expected Result: Valid CSV populates merge metadata and preview; bake resolves text into the object; warnings appear for bad templates or missing fields.
- Edge / Negative Cases: Clearing CSV must remove source linkage without breaking the text object.
- Persistence / Reopen Check: Save/reopen and verify post-bake or CSV-linked state.
- Undo / Redo Expectation: Load/clear/bake/config changes are undoable.
- Status: Active

### NL-055 — Convert / Close / Join / Optimize / Delete Duplicates
- Source Ref: `FSW-006`; `LB 3.3`, `LB 4`
- Feature / Function: Core path operations
- Hardware Requirement: None
- Prerequisites: Convertible text/shape objects and open paths
- Setup / Fixture: Mixed vector project
- Steps: Convert to path, close path, close & join, auto-join, optimize, and delete duplicates.
- Expected Result: Geometry updates correctly and the resulting objects remain selectable and valid.
- Edge / Negative Cases: Invalid selections should disable the command rather than mutate partially.
- Persistence / Reopen Check: Save/reopen and verify converted/path-edited objects.
- Undo / Redo Expectation: Each operation is undoable.
- Status: Active

### NL-056 — Boolean Ops And Weld
- Source Ref: `FSW-006`; `LB 4`
- Feature / Function: Union, subtract, intersection, exclude, weld
- Hardware Requirement: None
- Prerequisites: Compatible overlapping vector shapes
- Setup / Fixture: Two-shape boolean fixture and incompatible selection fixture
- Steps: Run each boolean operation and weld from menu and toolbar surfaces.
- Expected Result: Resulting geometry matches the chosen operation and invalid selections remain blocked.
- Edge / Negative Cases: Self-intersections or incompatible object types must fail safely.
- Persistence / Reopen Check: Save/reopen and verify resulting boolean geometry.
- Undo / Redo Expectation: Each boolean mutation is undoable.
- Status: Active

### NL-057 — Node Edit Submodes
- Source Ref: `FSW-006`; `LB 3.1`
- Feature / Function: Insert, delete, break, delete segment, line/smooth/corner, close/open, auto-join
- Hardware Requirement: None
- Prerequisites: Editable path
- Setup / Fixture: Open and closed path fixture
- Steps: Exercise each node submode on appropriate geometry.
- Expected Result: Node topology and segment behavior match the intended submode.
- Edge / Negative Cases: Unsupported path state should block the specific submode without breaking node mode itself.
- Persistence / Reopen Check: Save/reopen and verify topology changes.
- Undo / Redo Expectation: Each node edit is undoable.
- Status: Active

### NL-058 — Trim, Tabs, Offset, Arrays, Radius, Start Point
- Source Ref: `FSW-007`; `LB 3.2`, `LB 4`, `LB 5`
- Feature / Function: Manufacturing-oriented edit flows
- Hardware Requirement: None
- Prerequisites: Valid vector shapes and selections
- Setup / Fixture: Path-intersection fixture and closed-path fixture
- Steps: Use Trim tool, Tabs tool, Offset dialog, Grid Array, Circular Array, Radius tool, and Set Start Point.
- Expected Result: Each tool produces the intended geometry or metadata change and returns to a sane selection state.
- Edge / Negative Cases: Tiny geometry, open paths, or invalid selections must fail safely.
- Persistence / Reopen Check: Save/reopen and verify the resulting geometry/start-point metadata.
- Undo / Redo Expectation: Each mutation is undoable.
- Status: Active

### NL-059 — Copy Along Path, Rubber-Band Outline, Dock, Resize Slots, Mirror Across Line
- Source Ref: `FSW-007`, `FSW-006`; `LB 3.3`, `LB 5`
- Feature / Function: Advanced arrange/path workflows
- Hardware Requirement: None
- Prerequisites: Valid source object/path combinations
- Setup / Fixture: Mixed vector project with slots and mirror line candidate
- Steps: Open each dialog or command and complete a representative valid workflow.
- Expected Result: Output geometry matches the requested arrangement and no intermediate dialog leaves partial state on cancel.
- Edge / Negative Cases: Invalid source/path pairings must block or warn before mutation.
- Persistence / Reopen Check: Save/reopen and verify output geometry.
- Undo / Redo Expectation: Mutations are undoable; canceled dialogs create no history.
- Status: Active

## Raster, Import/Export, Preview, Libraries, Settings

### NL-060 — Raster Import, Replace, Convert, And Mask
- Source Ref: `FSW-011`; `LB 6`, `LB 7`
- Feature / Function: Raster-image object lifecycle
- Hardware Requirement: None
- Prerequisites: Bitmap files and image layer or compatible selection
- Setup / Fixture: Raster fixture
- Steps: Import bitmap, replace image, convert vector/selection to bitmap where supported, and apply mask to image.
- Expected Result: Raster objects render correctly, route to valid layers, and preserve expected dimensions/settings.
- Edge / Negative Cases: Transparent or oversized images must not corrupt project state.
- Persistence / Reopen Check: Save/reopen and verify raster object fidelity.
- Undo / Redo Expectation: Each mutation is undoable.
- Status: Active

### NL-061 — Trace Image And Adjust Image
- Source Ref: `FSW-011`; `LB 6`
- Feature / Function: Trace and image adjustment dialogs
- Hardware Requirement: None
- Prerequisites: Selected raster image
- Setup / Fixture: Raster fixture
- Steps: Open Trace Image and Adjust Image, exercise representative controls, cancel once, then confirm once.
- Expected Result: Cancel leaves source unchanged; confirm produces the expected traced or adjusted result.
- Edge / Negative Cases: Extreme brightness/contrast/gamma/saturation values must not crash or produce invalid state.
- Persistence / Reopen Check: Save/reopen and verify confirmed changes.
- Undo / Redo Expectation: Confirmed changes are undoable; canceled dialogs create no history.
- Status: Active

### NL-062 — Import / Export By File Type
- Source Ref: `FSW-012`, `FSW-013`, `FSW-014`; `LB 1.1`
- Feature / Function: Supported import/export formats
- Hardware Requirement: None
- Prerequisites: Existing project and import fixture set
- Setup / Fixture: SVG, DXF, a three-color vector PDF and PDF-compatible AI file, EPS, image, Lbrn `.lbrn2` and legacy `.lbrn`; export destination path
- Steps: Import each supported file type and export each supported project format from menu flows. Import the colored PDF once through **File > Import** and once by dragging it onto the workspace; preconfigure one matching color layer with a non-default operation before one import. Import Lbrn projects containing paths, primitive shapes, editable text, groups, multiple cut layers, and an embedded bitmap.
- Expected Result: Supported formats complete successfully or cancel cleanly; exported files are created and reopenable where applicable. PDF/AI paint colors remain separated, an existing matching color layer retains its configured operation, new colors create ordinary Line layers, and File Import and drag/drop produce equivalent results. Lbrn artwork keeps its size and position, groups remain selectable, text remains editable, embedded bitmaps render, and Lbrn layer colors plus speed/power settings are recreated.
- Edge / Negative Cases: Unsupported formats are tracked in `GAP-010`, not treated as executable passes here.
- Persistence / Reopen Check: Reopen exported/imported project artifacts where meaningful.
- Undo / Redo Expectation: Import is undoable; export is non-mutating.
- Status: Active

### NL-063 — Preview Generation And Invalidation
- Source Ref: `FSW-015`; `LB 1.5`, `LB 9`
- Feature / Function: Preview generation after project changes
- Hardware Requirement: None
- Prerequisites: Project with visible output-bearing objects
- Setup / Fixture: Mixed-operation project
- Steps: Generate preview, then mutate geometry, layer settings, visibility, and ordering; regenerate or trigger preview refresh.
- Expected Result: Preview state reflects the current project and invalidates when machine-relevant inputs change.
- Edge / Negative Cases: Preview generation with stale layer settings must not silently use old data.
- Persistence / Reopen Check: Save/reopen and regenerate preview to confirm consistency.
- Undo / Redo Expectation: Preview itself is non-mutating; underlying changes remain undoable.
- Status: Active

### NL-064 — Preview Edge Cases And Persistence
- Source Ref: `FSW-015`; `LB 1.5`, `LB 9`
- Feature / Function: Preview with empty, hidden, disabled, selected-only, and complex projects
- Hardware Requirement: None
- Prerequisites: Empty project and mixed project variants
- Setup / Fixture: Empty project, all-hidden project, selected-only candidate, large project
- Steps: Generate preview in each scenario and open Preview Window where relevant.
- Expected Result: Empty/hidden cases report gracefully, large cases remain responsive, and preview survives save/reopen/project reload without stale state.
- Edge / Negative Cases: All-disabled layers must not present misleading output.
- Persistence / Reopen Check: Reopen project and verify preview correctness after regeneration.
- Undo / Redo Expectation: Non-mutating.
- Status: Active

### NL-065 — Material Library CRUD / Import / Export
- Source Ref: `FSW-016`; `LB 23`
- Feature / Function: Material preset management
- Hardware Requirement: None
- Prerequisites: Material panel available
- Setup / Fixture: At least one layer and one preset file
- Steps: Add, edit, duplicate, delete, filter, search, import, and export presets.
- Expected Result: Presets persist correctly and filters/search reflect the stored data accurately.
- Edge / Negative Cases: Importing malformed preset data must fail clearly without damaging existing presets.
- Persistence / Reopen Check: Relaunch and verify material library state.
- Undo / Redo Expectation: Library persistence actions are app-data mutations, not project object history.
- Status: Active

### NL-066 — Material Preset Apply And Create-From-Layer
- Source Ref: `FSW-016`; `LB 8.2`, `LB 23`
- Feature / Function: Applying presets to layers and generating presets from current layer
- Hardware Requirement: None
- Prerequisites: Layer with editable cut settings
- Setup / Fixture: Mixed-operation layer fixture
- Steps: Apply preset to layer, then create a preset from layer settings.
- Expected Result: Layer fields update correctly on apply and generated preset reflects the layer’s effective settings.
- Edge / Negative Cases: Image-layer raster settings must serialize coherently when creating the preset.
- Persistence / Reopen Check: Save project and relaunch app to verify both layer state and preset library state.
- Undo / Redo Expectation: Applying preset is undoable at project level; library creation is app-data mutation.
- Status: Active

### NL-067 — Art Library File Lifecycle / Preview / Organization
- Source Ref: `FSW-024`; `LB 24`
- Feature / Function: User-managed `.bbart` libraries, thumbnails, search, category filter, rename, load, unload, and save-as flows
- Hardware Requirement: None
- Prerequisites: Art Library panel visible
- Setup / Fixture: A small set of vector and raster art assets plus at least one existing `.bbart` file
- Steps: Create a new library via `New...`, choose a save path, add file-based art, rename the library, rename an item, search items, filter by category, use `Save As...`, `Load...`, and `Unload`, then relaunch and reopen the same libraries.
- Expected Result: The panel uses a sidebar + item grid layout; `.bbart` libraries load from chosen paths, previews render for normal items, library/item rename persists, and search/filter results update predictably.
- Edge / Negative Cases: Failed thumbnail generation must fall back to placeholder glyphs; missing-thumbnails from legacy libraries should repair after load; save failures must surface a visible save-error banner and disable destructive unload/delete actions until resolved.
- Persistence / Reopen Check: Relaunch and verify the loaded-library set, file paths, previews, names, and item metadata persist.
- Undo / Redo Expectation: Library data changes are app-data mutations, not project object history.
- Status: Active

### NL-068 — Art Library Selection Capture / Insert / Drag Flows
- Source Ref: `FSW-024`; `LB 24`
- Feature / Function: Add-selection snapshots, insert-to-project, context actions, drag-to-canvas, and cross-library copy/move semantics
- Hardware Requirement: None
- Prerequisites: Existing art library with at least one item
- Setup / Fixture: Vector item, raster item, a three-color vector PDF item, and an active project selection that includes mixed content where possible
- Steps: Add the current project selection into a library, verify the snapshot gets a thumbnail, insert items by double-click and by item context menu, insert the colored PDF item and verify its color layers, drag a library item to the canvas to place it, drag an item to another loaded library, then repeat with `Shift` held to switch from copy to move.
- Expected Result: Selection capture creates a reusable Beam Bench snapshot, insertions reconstruct the artwork correctly, PDF paint colors route to matching or newly created safe Line layers, canvas drop places artwork at the drop location, cross-library drag copies by default, and `Shift+drag` moves instead of copying.
- Edge / Negative Cases: Dropping onto the source library should be a no-op; trying `Add Selection` with nothing selected should show explicit feedback; deleting or renaming a selected item must not leave stale UI selection state behind.
- Persistence / Reopen Check: Save/reopen after insertion and verify inserted project objects remain; relaunch and verify copied/moved library items persist in the expected destination libraries.
- Undo / Redo Expectation: Project insertions are undoable; library mutations remain app-data operations outside project undo history.
- Status: Active

### NL-069 — Macro CRUD And Hotkey Conflict Handling
- Source Ref: `FSW-017`
- Feature / Function: Macro editor behavior and shortcut conflict validation
- Hardware Requirement: None
- Prerequisites: Macros panel visible
- Setup / Fixture: New macro and existing macro
- Steps: Add, edit, delete, and mark macros for toolbar display; assign valid and conflicting hotkeys.
- Expected Result: Macros save correctly and conflicting hotkeys are rejected with explicit feedback.
- Edge / Negative Cases: Unsaved edit changes should warn before switching away.
- Persistence / Reopen Check: Relaunch and verify macros persist.
- Undo / Redo Expectation: Macro library edits are app-data mutations.
- Status: Active

### NL-070 — Macro Import / Export Persistence
- Source Ref: `FSW-017`
- Feature / Function: Macro import/export round trip
- Hardware Requirement: None
- Prerequisites: Existing macro set
- Setup / Fixture: Macro JSON file
- Steps: Export macros, clear or alter the macro list, import back, and verify round trip.
- Expected Result: Export file is created and import restores the expected macro set.
- Edge / Negative Cases: Malformed import file must fail without destroying existing macros.
- Persistence / Reopen Check: Relaunch and confirm imported macros remain.
- Undo / Redo Expectation: App-data mutation.
- Status: Active

### NL-071 — Notes And Settings Persistence
- Source Ref: `FSW-003`, `FSW-024`; `LB 16`, `LB 19.*`
- Feature / Function: Project notes and application settings persistence
- Hardware Requirement: None
- Prerequisites: Settings dialog and Notes dialog accessible
- Setup / Fixture: Existing project
- Steps: In the native macOS Tauri app, run all six combinations of System, Light, and Dark app appearance with light and dark workspace backgrounds. Sweep the shell, Settings, menus, one representative dialog, forms, portals, scrollbars, warning/error states, and a newly opened window in explicit Light and Dark. Perform representative checks for the other four combinations, then import preferences and reset preferences once. Browser-only localhost testing does not count.
- Expected Result: Notes persist with the project; settings persist at app level; appearance changes only after Save; every open window synchronizes; app appearance does not change the independently selected workspace background; and canvas, preview, export, and print colors remain unchanged.
- Edge / Negative Cases: Cancel and failed saves must retain the active appearance. Settings conflicts, invalid values, corrupt theme cache data, or Linux native-chrome differences must not leave partial save state or prevent the web interface from using the selected theme. Treat Linux titlebar and GTK menu differences as expected best-effort behavior when the web interface is correct.
- Persistence / Reopen Check: Relaunch in Light, Dark, and System modes. Verify the cached startup appearance is immediate, backend hydration remains authoritative, imported preferences restore their appearance, and reset returns to Dark. On packaged Windows and Linux release-candidate builds, smoke-test cold-launch background, explicit Light/Dark, one System change, restart persistence, a new window, one form/dialog, and workspace-background independence.
- Undo / Redo Expectation: Notes edits may be undoable if tracked; app settings are not object-history actions.
- Status: Active

### NL-072 — Diagnostics / Support Export
- Source Ref: `FSW-003`; `LB 16`
- Feature / Function: Generate support-data/diagnostics export
- Hardware Requirement: None
- Prerequisites: Help menu available
- Setup / Fixture: Project with some non-default state
- Steps: Trigger Generate Support Data, complete export once, and cancel once.
- Expected Result: Successful export creates output; cancel exits cleanly; failures surface clearly.
- Edge / Negative Cases: Export must not mutate project content or block the app afterward.
- Persistence / Reopen Check: Relaunch after export and verify normal startup.
- Undo / Redo Expectation: Non-mutating.
- Status: Active

### NL-073 — Shortcut Gating And Clipboard
- Source Ref: `FSW-024`
- Feature / Function: Shortcut behavior outside vs inside text inputs plus clipboard flows
- Hardware Requirement: None
- Prerequisites: Editable text field and selected objects
- Setup / Fixture: Standard vector fixture and notes/settings input field
- Steps: Use cut/copy/paste/duplicate/paste-in-place/select-all while focus is on canvas, then repeat while focus is inside text inputs.
- Expected Result: Shortcuts work on canvas but do not fire destructive canvas commands while typing in inputs; clipboard actions behave correctly on objects.
- Edge / Negative Cases: Clipboard with incompatible selection or empty clipboard must gate safely.
- Persistence / Reopen Check: Save/reopen after paste/duplicate to verify object state.
- Undo / Redo Expectation: Clipboard mutations are undoable.
- Status: Active

### NL-074 — Accessibility, Keyboard, And Dialog Focus
- Source Ref: `FSW-033`; `LB 17`
- Feature / Function: Basic keyboard accessibility and modal focus behavior
- Hardware Requirement: None
- Prerequisites: Several dialogs and icon-button surfaces available
- Setup / Fixture: Open dialogs from menus and panels
- Steps: In both Light and Dark appearance, traverse controls by keyboard, verify visible labels and focus outlines, operate the Appearance selector and switches by name, use Escape to close dialogs, and confirm focus does not get lost behind active modal content. At the Settings dialog's minimum size, visually check German, one CJK locale, and the generated en-XA pseudo-locale.
- Expected Result: Major interactive surfaces remain keyboard-reachable, dialogs and switches expose usable names, text and status colors remain readable, control boundaries and focus indicators remain visible, and focus returns sensibly on close.
- Edge / Negative Cases: Floating panels, nested dialog openings, System-theme changes while a dialog is open, and 200% zoom must not hide controls, trap focus irrecoverably, or produce mixed-theme portal content.
- Persistence / Reopen Check: N/A
- Undo / Redo Expectation: Non-mutating.
- Status: Active

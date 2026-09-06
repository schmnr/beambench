# Beam Bench project review

Reviewed commit `65b2b10f` and its local working tree on September 4, 2026. Production source was clean at the start. This is a broad source and test review, with deeper tracing of machine control, planning, persistence, IPC, and geometry export. It is not a claim that every line or every supported controller has been validated. No machine connection or output was initiated during the review.

The project has useful separation between UI, shared services, planning, and controller protocols, plus substantial automated coverage. However, the issues below warrant fixes before relying on its shutdown behavior, unsaved-change protection, selected-only engraving, and geometry exports. A rewrite would be premature; these failures have identifiable boundaries that can be repaired and tested.

P1 means fix before relying on the affected workflow. P2 means a concrete correctness or reliability defect to address next.

## Fixes applied after review

The findings below describe the original reviewed commit. The working tree now addresses all ten:

1. Window close waits for controller-specific stop and disconnect. A failed stop keeps the window and connection available; late connections are rejected during shutdown. GRBL status that still reports motion cannot confirm a stop.
2. Selected-only planning retains hidden dependency objects. Missing image masks produce a diagnostic and blank output instead of exposing the original raster.
3. IPC includes `dirty`; project archives and plan hashes exclude runtime dirtiness.
4. Undo and redo conservatively mark restored documents dirty. Recovery restoration is also dirty until saved.
5. Save, path assignment, and dirty reset share the document lock. Document replacement and autosave use compatible locking so a concurrent edit cannot be marked saved.
6. DXF exports flatten curves at the existing 0.05 mm tolerance and include closing edges.
7. GRBL job preparation rejects commands longer than 126 bytes before output; the streaming engine also fails explicitly if given an unsendable command.
8. PDF exports resolve virtual clones and preserve quadratic curves through exact cubic conversion. EPS uses the same conversion. PDF cross-reference offsets now use actual byte positions.
9. Project loading bounds expanded JSON (64 MiB), individual assets (256 MiB), total archive content (512 MiB), objects (250,000), and assets (10,000). Reads enforce limits independently of ZIP metadata. Saving refuses archives that exceed these limits or lack asset bytes.
10. Project snapshots share immutable asset bytes through `Arc<Vec<u8>>`. Replacing an asset preserves the older bytes in undo history.

Validation after fixes: 1,974 targeted Rust tests passed (core 916, planner 329, project persistence 23, service 661, streamer 45). Frontend validation passed all 2,577 tests, production build, and ESLint. Rust formatting and whitespace checks passed. `CARGO_INCREMENTAL=0 cargo check -j4 -p beambench-tauri` passed, including the desktop close handler and its dependencies. The production frontend build still reports its existing large-chunk advisory.

Workspace Clippy with all targets reported warnings in existing code, with no errors reported, but was interrupted after the Tauri build-script compiler stopped making measurable progress for over ten minutes. This is not recorded as a completed Clippy pass. A subsequent focused desktop check with incremental compilation disabled completed successfully in 6m 09s. No build-cache configuration was changed in the repository.

Regression tests are part of the normal Rust suites. The separate reproduction-probes file is a historical record of the original failure assertions, not an executable acceptance suite. Hardware validation remains outstanding; tests use mock transports.

## Findings

### 1. P1: Window close does not stop an active machine job

[main.rs:208](</Users/schmnr/Documents/GitHub/Beam Bench/tauri-app/src-tauri/src/main.rs:208>), [context.rs:1288](</Users/schmnr/Documents/GitHub/Beam Bench/crates/beambench-service/src/context.rs:1288>)

The close handler checks project dirtiness, removes recovery files, and cleans camera files. It never checks the active job or calls the existing machine shutdown operations. The Quit command and keyboard shortcut both route to this handler. `ServiceContext::drop` only cleans camera files, and the job thread retains an `Arc` anyway.

A user can save a project, start streaming it, and close the window without any machine-stop step. Controller-buffered motion can continue after the sender exits; uploaded controller jobs can also outlive the app. Closing a serial handle is not a verified laser-off operation.

Keep the window open while active output is stopped through the controller-specific service path. Surface an unconfirmed stop and retain the controls if shutdown fails. If autonomous uploaded jobs should intentionally continue, make that a separate, explicit product decision. Add a close-lifecycle test with a mock controller that verifies stop ordering before exit.

### 2. P1: Selected-only planning removes image masks and clone dependencies

[planning.rs:145](</Users/schmnr/Documents/GitHub/Beam Bench/crates/beambench-service/src/ops/planning.rs:145>), [builder.rs:281](</Users/schmnr/Documents/GitHub/Beam Bench/crates/beambench-planner/src/builder.rs:281>)

`apply_session_job_options` deletes every unselected object from the copied project before building the plan. Objects can depend on other objects that are not selected.

For a masked image, the missing mask lookup is silently skipped, and an empty mask set returns the original raster. Selecting only that image therefore allows engraving pixels that its mask excluded. For a virtual clone, removing its unselected source makes clone resolution fail, so geometry disappears or the plan becomes empty.

Preserve dependency objects while filtering which objects emit output, or resolve dependencies against the complete project before filtering. Missing mask dependencies must fail visibly. Test image-only selection with both mask polarities, clone-only selection, and a selection containing a clone plus ordinary geometry.

### 3. P1: The IPC project omits the dirty flag that the frontend depends on

[project.rs:72](</Users/schmnr/Documents/GitHub/Beam Bench/crates/beambench-core/src/project.rs:72>), [unsavedGuardStore.ts:26](</Users/schmnr/Documents/GitHub/Beam Bench/tauri-app/src/stores/unsavedGuardStore.ts:26>), [useAutosave.ts:19](</Users/schmnr/Documents/GitHub/Beam Bench/tauri-app/src/hooks/useAutosave.ts:19>)

`Project.dirty` has `#[serde(skip)]`. Tauri commands return `Project` directly, so their responses omit the field even when Rust considers the document dirty. Frontend project decoration does not restore it. Both the New/Open guard and autosave interpret the missing value as clean.

For example, Replace Image calls `loadProject`, which replaces frontend state with the serialized project. Undo does the same. A subsequent New/Open can bypass the unsaved-change prompt, and autosave stops until another frontend operation explicitly sets `dirty` again. The native window-close handler reads Rust state separately, so it does not repair New/Open behavior.

Introduce an IPC document response that includes authoritative saved-state information while keeping runtime fields out of the persisted archive and plan hash. Test actual Rust serialization against frontend consumers, rather than supplying mocks with fields that production omits.

### 4. P1: Undo can mark a document clean even though it differs from disk

[project.rs:1298](</Users/schmnr/Documents/GitHub/Beam Bench/crates/beambench-service/src/ops/project.rs:1298>), [history.rs:38](</Users/schmnr/Documents/GitHub/Beam Bench/crates/beambench-service/src/history.rs:38>)

History stores whole `Project` snapshots including their historical `dirty` value. Undo restores that value unchanged. Reproduce by starting from saved state A, editing to B, saving B, then undoing to A. The restored snapshot is clean even though the saved file contains B. The native close guard can now allow closing without saving the undo result.

Saved-state tracking must compare the current revision with the last successfully saved revision. At minimum, conservatively mark undo/redo results dirty until that comparison exists. Test edit/save/undo/close and undo/save/redo/close sequences. This defect remains even after the IPC serialization issue is fixed.

### 5. P1: Save completion can clear a concurrent edit's dirty flag

[persistence.rs:294](</Users/schmnr/Documents/GitHub/Beam Bench/crates/beambench-service/src/ops/persistence.rs:294>)

`save_project_to_path` writes the archive under the project lock, releases that lock, updates a separately locked path, then reacquires the project lock and unconditionally clears `dirty`. An API request or other concurrent operation can edit or replace the project between those phases. Save completion then marks contents that were never written clean; a replacement project can also receive the prior save's name/path.

The supplied, unexecuted reproduction holds the path lock, waits for the archive to appear, edits the in-memory project, and then releases the path lock. It asserts the source-predicted outcome: disk and memory differ, but the final in-memory dirty flag is false.

Commit save completion against the exact project ID and revision that was written. Coordinate document replacement and path changes with the same transaction boundary. Delete recovery data only when it belongs to that successfully saved state.

### 6. P2: DXF export changes curves and drops closing edges

[export_dxf.rs:74](</Users/schmnr/Documents/GitHub/Beam Bench/crates/beambench-core/src/export_dxf.rs:74>)

The curve branch emits one straight line to each quadratic or cubic endpoint. The `Close` branch emits nothing. A closed triangular SVG path becomes two DXF lines, and a cubic arch becomes a single chord. Curved outlines, circles, and closed parts therefore export with different geometry despite a successful response.

Use the existing path-flattening implementation with a stated tolerance, including the closing edge, or emit appropriate DXF curve entities. Validate exported geometry by reimporting and comparing closure and maximum geometric deviation. Checking that a DXF header or a LINE entity exists is insufficient.

### 7. P2: An overlong custom G-code line stalls GRBL streaming forever

[engine.rs:120](</Users/schmnr/Documents/GitHub/Beam Bench/crates/beambench-streamer/src/engine.rs:120>), [gcode.rs:280](</Users/schmnr/Documents/GitHub/Beam Bench/crates/beambench-grbl/src/gcode.rs:280>)

The sender breaks whenever the next line plus its newline exceeds the 127-byte receive window. A line that cannot fit even into an empty window is never sent, rejected, or advanced. Custom header/footer lines are passed through without this validation; even a long comment can trigger it. The existing idle/ack desynchronization check requires bytes in flight, so it does not catch this zero-byte stall.

Validate all generated lines before starting the job and return a useful error for lines above the supported size. Handle comments consistently before byte accounting. Do not truncate executable G-code to make it fit. Add a preparation test covering an overlong header, footer, and comment.

### 8. P2: PDF export silently omits virtual clones

[export_pdf.rs:52](</Users/schmnr/Documents/GitHub/Beam Bench/crates/beambench-core/src/export_pdf.rs:52>)

PDF export iterates raw project objects and calls `object_to_world_vecpath`. That converter returns `None` for `VirtualClone`. SVG, DXF, and EPS explicitly resolve clones first; PDF does not. Selecting a clone and exporting it to PDF produces a file without that clone's drawing commands.

Resolve clones against the complete project before applying the output selection. Add equivalent output fixtures across formats. PDF and EPS also replace quadratic curves with endpoint lines; cover those with the same geometry-fidelity tests used for DXF.

### 9. P2: Project archives have no decompressed-size budget

[persistence.rs:90](</Users/schmnr/Documents/GitHub/Beam Bench/crates/beambench-project/src/persistence.rs:90>)

The loader reads `project.json` and each asset into growing buffers with no per-entry or cumulative decompressed limit. A small compressed project can demand a very large allocation and freeze or terminate the app. Recovery discovery calls the same loader for each recovery file, so startup recovery scanning also encounters this exposure.

Enforce limits on expanded JSON, individual assets, total expanded bytes, and object counts. Check actual bytes read as well as ZIP metadata, and report an actionable error. Test with a small generated compressed fixture and a deliberately small test budget; do not allocate a real denial-of-service payload.

### 10. P2: Every undo snapshot duplicates all asset bytes

[history.rs:30](</Users/schmnr/Documents/GitHub/Beam Bench/crates/beambench-service/src/history.rs:30>), [project.rs:71](</Users/schmnr/Documents/GitHub/Beam Bench/crates/beambench-core/src/project.rs:71>)

History clones the entire project into up to 50 snapshots. `asset_data` is a `HashMap<AssetId, Vec<u8>>`, so Rust cloning duplicates the binary payloads as well as document state. Fifty ordinary edits to a project with 20 MB of embedded assets can retain approximately 1 GB of asset copies, before decoded image caches or other project data. This is a calculation from the storage model, not a measured application benchmark.

Share immutable asset bytes across revisions, or retain assets in a version-aware store outside document snapshots. Preserve undo for image replacement and deletion. Add a memory-oriented workload using repeated geometry edits around unchanged assets.

## Verification and limits

- Frontend: all 200 test files and 2,577 tests passed.
- Frontend production build passed. Vite reported its large-chunk warning.
- Frontend lint passed.
- `npm audit --omit=dev` reported zero known vulnerabilities at review time. This does not cover Rust or prove application security.
- `cargo test --workspace`: interrupted after approximately 13 minutes when compilation stopped making visible progress; no test results were produced.
- A second, smaller run, `cargo test -j 2 -p beambench-service --test review_probes`, also stalled during compilation and was stopped. The reproduction probes were **not executed or compiler-validated**. These Rust findings are source-reviewed, not test-confirmed.
- Rust clippy, formatting checks, dependency-advisory scanning, and packaged desktop smoke tests were not completed.

Production source remains unchanged. The seven drafted probes are retained in [2026-09-04-reproduction-probes.rs](</Users/schmnr/Documents/GitHub/Beam Bench/docs/reviews/2026-09-04-reproduction-probes.rs>), outside the normal test suite. They assert the suspected current defects, not the behavior that fixes should preserve. To investigate further, place a temporary copy at `crates/beambench-service/tests/review_probes.rs` and run the targeted command with `BEAMBENCH_CONFIG_DIR` and `BEAMBENCH_DATA_DIR` pointing to disposable directories. Remove that temporary copy afterward. The save probes require an isolated config directory explicitly.

All Rust findings above are based on traced source paths. No physical controller, packaged installer, or Windows/Linux runtime was tested. This review does not establish hardware compatibility or certify operational safety.

## Repair order

First address shutdown, selected-only dependency handling, and the three saved-state failures. Then fix export fidelity and GRBL line validation. Follow with bounded input loading and asset sharing. Keep the repairs small and add tests at the boundaries where these failures occur.

The existing architecture is worth retaining. Shared services reduce CLI/API/desktop drift, and the GRBL completion gate correctly requires fresh Idle status after acknowledgements. The main maintenance concern is concentration of behavior: machine operations, project operations, and the planner builder each occupy roughly 8,000–10,000 lines including tests. Extract coherent responsibilities as fixes expose them, rather than introducing another generic service layer.

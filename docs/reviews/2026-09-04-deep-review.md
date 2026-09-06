# Beam Bench deeper review, 4 September 2026

**Historical findings:** Corrections and current verification are recorded in [the fix report](2026-09-04-deep-review-fixes.md). The findings and diagnostic probes below describe the pre-fix working tree.

The earlier fixes address specific defects. They do not establish that Beam Bench is ready for unrestricted machine use. This pass found additional problems in failure handling, exported output, restart behavior, import fidelity, and concurrent job lifecycle operations.

This review covers Beam Bench 0.2.18 at HEAD `65b2b10fda6892e68f3c8196318d4480ed90c0b9`, plus the earlier uncommitted fixes. Source locations refer to that working tree. It adds review evidence, not production fixes. Prior findings and fixes remain in `2026-09-04-project-review.md`.

## Findings in priority order

### 1. P1: cancellation reports success after the controller stop fails

Location: `crates/beambench-service/src/ops/machine.rs:4725–4746`; underlying GRBL operation at `crates/beambench-streamer/src/job.rs:176–181`.

The service catches an error from `job.cancel(session)`, logs it, deletes the active job, clears remembered progress, emits `job.cancelled`, and returns success. GRBL cancellation can fail while writing the soft reset, before it sends M5. In that case the application has not established that the controller stopped, but has removed its active-job state. Buffered controller motion may continue. A mock-transport Rust probe injected a reset-write failure after starting a job and confirmed that cancellation returned success and removed the active job. No physical machine was involved.

Return the stop failure, preserve a recoverable machine/job state, and expose the failure to the operator. Do not emit successful cancellation until the controller-specific stop contract is met. Test failed reset writes and failed M5 writes independently.

### 2. P1: G-code export accepts plans that job start rejects

Location: `crates/beambench-service/src/ops/planning.rs:517–554`; mask fallback at `crates/beambench-planner/src/builder.rs:298–330`; start rejection at `crates/beambench-service/src/ops/machine.rs:4215`.

Export rejects empty plans and validates rotary feed limits, but does not reject `failed_entries`. The image-mask code records a failed entry for a non-vector or zero-area mask, then returns the original image if no usable masks remain. A plan can therefore contain unmasked raster output and a mask failure. Job start blocks that plan; export can write the laser commands to disk. Other omitted operations can also produce an incomplete exported job. A Rust probe confirmed that a plan with one failed mask entry exported successfully and wrote 7,639 bytes of G-code.

Reject failed entries before any output file is written. Invalid masks should also produce blank output consistently, as missing masks now do. Share the applicable geometric checks with start, including raster travel beyond the bed; exporting to another sender should not silently bypass those checks. Connection-dependent checks need an explicit export policy.

### 3. P1: installing an update bypasses the unsaved-project guard

Location: `tauri-app/src/services/updateService.ts:123–174`; direct installation entry in `tauri-app/src/components/settings/UpdateDialog.tsx`.

The updater checks machine state before download, installation, and relaunch, but never checks project dirtiness or invokes the normal save/discard/cancel flow. An isolated Vitest probe put a dirty project in the project store and confirmed that both mocked install and mocked relaunch were called. No real update was installed.

Run the unsaved-project guard before installation can restart or replace the application. Recheck after download because the user can edit while it runs. Keep cancellation and save failure from reaching install/relaunch. Preserve the existing machine-state checks.

### 4. P1: a new job can miss its background tick loop

Location: `crates/beambench-service/src/ops/machine.rs:4582–4605` and `4650–4678`.

A terminal tick removes the old job, releases its locks, and publishes the completion event. The old worker clears `job_tick_loop_running` only after that tick returns. Another request can start a new job in this interval. Its attempt to spawn a worker sees the old flag and returns, after which the old worker exits and clears the flag. The new job then has no background worker to send commands or advance status.

This is a source-confirmed interleaving, not a reproduced scheduling trace. Completion-driven API clients are a plausible trigger. Tie worker ownership and job publication together, or keep a persistent worker that cannot exit while a replacement job exists. Add a deterministic barrier-based completion/restart test.

### 5. P1: preflight and G-code generation can use different settings

Location: `crates/beambench-service/src/ops/machine.rs:4314–4334`; profile mutation at `crates/beambench-service/src/ops/profiles.rs:1889–1903`.

Start retains the preflight plan, then independently fetches the project and active profile again to construct output. Profile activation can update settings during start without acquiring the job lock. Concurrent project changes are also not frozen by that lock. A request can therefore validate one snapshot and emit with another snapshot's power scaling, finish settings, or coordinate assumptions.

The lock and reread mismatch is confirmed in source. Its scheduling and controller effects were not reproduced. Carry an immutable project/profile/output configuration through preflight and emission, or verify a revision token before committing the job. Revalidating only rotary feed does not establish equivalence.

### 6. P2: G-code import silently loses valid commands and coordinates

Location: `crates/beambench-core/src/import_gcode.rs:161–179`.

The parser tokenizes by whitespace and retains only one G/M command per line. `G1X10Y20` becomes a command string with no coordinates. `G91 G1 X10` loses the relative-mode command. Lowercase command letters also miss the command branch. A file mixing these forms with supported lines can import only part of its geometry or place it incorrectly, rather than report the unsupported input. A Rust probe confirmed both missing compact coordinates and loss of G91 from a multi-command block.

Tokenize letter-number words independently of whitespace and retain compatible modal commands in each block. Until the parser supports a form, return an actionable warning or error instead of silently treating partial geometry as a successful import. Arc omission also needs explicit user-visible handling.

### 7. P2: PDF import ignores the page/object graph

Location: `crates/beambench-core/src/import_pdf.rs:188–239`.

The importer scans every raw stream and treats its operators as page drawing. It does not follow page resources or Form XObject invocations. An unused form can become imported geometry; a form drawn with a transform or drawn multiple times loses that placement and multiplicity. Clipping is also not represented by this stream-scanning approach. These are ordinary PDF structures, not only exotic compression filters. A small PDF-fragment probe with empty page content and an unused painted form confirmed that the importer extracted the form. That diagnostic fixture does not include a complete cross-reference table; the source independently shows that references are never consulted.

Use a parser that resolves page resources, graphics state, forms, and clipping, or explicitly reject unsupported documents. A successful import should not silently change the physical design.

### 8. P2: PDF decompression has no expanded-size budget

Location: `crates/beambench-core/src/import_pdf.rs:179–184`.

FlateDecode reads into a growing vector until EOF. There is no per-stream or aggregate expanded-byte limit. The new project-archive limits do not cover this separate import path. A small compressed input can exhaust memory during import and terminate the app, including any unsaved work. This is confirmed by the unbounded allocation path; no destructive memory-exhaustion test was run.

Bound expanded bytes per stream and document and reject excess before allocation grows further. Apply corresponding pixel/allocation budgets to raster processing, whose target dimensions are derived from physical size and DPI. The raster budget needs separate end-to-end verification before assigning an additional defect severity.

### 9. P2: numeric step buttons lack accessible names and activation behavior

Location: `tauri-app/src/components/shared/NumberStepper.tsx:106–128`.

The mock-backed UI accessibility tree exposes eight unnamed buttons in the default layer settings. These are the increment/decrement controls. The component renders only hidden SVG icons, removes the buttons from tab order, and handles pointer events without a click handler. Screen-reader button activation cannot rely on that pointer-only path. The labeled number inputs remain available, so numeric editing itself is not completely blocked.

Provide field-specific accessible names and normal activation support. Retain keyboard stepping on the number input and avoid double increments when adding click behavior.

### 10. P2: supported minimum window width overflows the toolbar

The desktop configuration allows 900×600 windows at `tauri-app/src-tauri/tauri.conf.json:18–19`. At that viewport the mock-backed application had a document scroll width of 940 pixels; the connection control extended beyond the right edge. A horizontal scrollbar appeared. The layer panel's vertical scroll is intentional and is not counted as a defect.

Make the toolbar collapse, wrap, or use an overflow menu at the supported minimum width, or align the minimum width with the layout requirement. Verify the native webview as well as the browser approximation.

## Dependencies and security boundaries

The npm audit found three affected development packages: `@humanfs/node`, `browserslist`, and `postcss-selector-parser`. The separate `npm audit --omit=dev --json` run reported zero vulnerabilities. Development dependencies can still affect CI/builds; this result is not evidence of a remotely exploitable installed app. Saved audit JSON contains the exact advisory URLs and affected ranges.

An OSV batch checked 756 registry packages from Cargo.lock. Matches covered 22 packages, including maintenance notices and duplicate aliases. Three distinct unsoundness issues deserve dependency-owner follow-up:

- `event-listener 5.4.1`, patched in 5.4.2. [RUSTSEC-2026-0221](https://rustsec.org/advisories/RUSTSEC-2026-0221.html).
- `i_tree 0.8.3`, patched in 0.10.0. [RUSTSEC-2025-0165](https://rustsec.org/advisories/RUSTSEC-2025-0165.html).
- `glib 0.18.5`, patched in 0.20.0. This is relevant to the Linux GTK dependency chain. [RUSTSEC-2024-0429](https://rustsec.org/advisories/RUSTSEC-2024-0429.html).

These are lockfile matches, not demonstrated application exploits. Patch versions and transitive upgrades need compatibility checks. GTK3 and other unmaintained dependencies are maintenance risks, not additional proven vulnerabilities.

The optional network API is unauthenticated and can expose powerful file and machine operations. The settings UI says to enable it only on trusted networks. Treat this as an explicit trust boundary, not internet-safe access. Origin rejection is useful but does not authenticate other network clients. Tauri's CSP is disabled; no executable injection chain was established in this review. Print HTML construction was checked and includes escaping/color validation, so the presence of `innerHTML` alone was not reported as an XSS defect.

## Review coverage and limits

| Area | Work performed | Remaining limit |
| --- | --- | --- |
| Persistence and history | Reviewed dirty state, save locking, recovery, archive limits, and earlier fixes | No crash/power-loss filesystem campaign |
| Planning and output | Traced selected objects, masks, failed entries, preflight, coordinate conversion, raster travel, G-code export, vector exports | No dimensional comparison of physical burns |
| Machine lifecycle | Traced start, tick ownership, completion, pause/resume, cancellation, fatal errors, and close | No real serial/USB/network controller tests |
| Controller backends | Reviewed shared runtime dispatch and stop/disconnect behavior across GRBL and non-GRBL paths | Protocol-specific hardware behavior remains unverified |
| Import and geometry | Inspected G-code, PDF, raster processing, and import/export boundaries | Not a complete corpus for every supported file format |
| Frontend | Existing test/build/lint results, isolated updater probe, mock-backed workspace inspection at default and minimum dimensions | Not a native end-to-end session; camera, dialogs, assistive technology, every theme not exhaustively tested |
| API and desktop | Reviewed routes, origin restriction, bind configuration, Tauri permissions/CSP, updater, shutdown, CI | No penetration test or Windows/Linux runtime run |
| Dependencies | npm full/production audit and OSV Cargo.lock batch | Advisory reachability and transitive upgrade compatibility not established |

The repository is too large for a claim that every line and every state combination has been proved correct. This pass reviewed subsystem boundaries and failure paths in depth and used tests as supporting evidence. It does not certify hardware safety or absence of further defects.

The UI audit uses a provisional 0–4 scale, where 4 requires broad verification: accessibility 2, performance 2, minimum-window behavior 2, theming 2, interface consistency 3. These are evidence-limited review scores, not WCAG certification. Positive findings include named main toolbar actions, explicit disabled states, a clear separation of layer visibility and output, shared theme tokens, and scrollable panels. Missing theme/assistive-technology coverage limits the scores; it is not itself a confirmed defect.

## Validation evidence

Prior fix verification passed 1,974 Rust library tests across core, planner, project, service, and streamer; 2,577 frontend tests in 200 files; frontend build/lint; formatting; and desktop `cargo check`. Those results precede this deeper pass and are not presented as a fresh full-workspace run.

This pass freshly passed all 2,577 frontend tests in 200 files, the canvas performance budget test, the three-test updater diagnostic suite including dirty-project relaunch, and the production npm audit. `cargo fmt --all -- --check` and `git diff --check` passed. The updater probe asserts the observed defect, not desired acceptance behavior. It is preserved outside the production test suite in `2026-09-04-updater-probe.ts.txt`.

The full workspace Rust test attempt did not finish. It reached the Tauri test compilation and stopped making visible progress; it was interrupted. The four standalone Rust diagnostic probes passed, reproducing cancellation error suppression, invalid-mask G-code export, compact/multi-command G-code parsing loss, and unused PDF-form extraction. The backend-only retry is recorded below with its final status. Earlier full Clippy verification was also incomplete. None of these incomplete checks should be represented as passing.

## Recommended order

Address cancellation and export rejection first. Then close the updater data-loss path and make job start/worker ownership atomic. Add deterministic failure and concurrency tests before changing those state machines. Improve import fidelity and resource budgets next, followed by dependency upgrades and UI accessibility/layout fixes.

Keep the existing architecture where possible. The main problems found here are inconsistent contracts between adjacent operations, not evidence that the whole application needs a rewrite.

The Rust diagnostic source is `2026-09-04-deep-review-probes.rs`. To rerun using Cargo, temporarily place it at `crates/beambench-service/tests/deep_review_probes.rs`, run `cargo test -p beambench-service --test deep_review_probes -- --nocapture`, then remove that temporary copy. These tests intentionally assert current defective behavior and must not become permanent acceptance tests. The updater probe can similarly be copied to `tauri-app/src/services/deepReviewUpdater.test.ts` and run with Vitest from `tauri-app`.

Final backend retry status: `CARGO_INCREMENTAL=0 cargo test -j4 --workspace --exclude pdf417 --exclude beambench-tauri` completed test compilation in 12m 38s and passed the API suite's 91 tests. The build-info target had zero tests. Native test executables then spent roughly a minute at startup before producing output, including the zero-test target. The retry was interrupted while the camera test executable was starting. No Rust test assertion had failed, but the broader suite remains incomplete. This is a verification limitation, not a product defect or a passing result. No physical-controller validation or complete Clippy run was performed in this pass.

Temporary browser/IPC fixtures and temporary Vitest files were removed. The local review web server and test processes were stopped. Only review documents and diagnostic evidence were added by this pass; the earlier production fixes remain in the working tree.

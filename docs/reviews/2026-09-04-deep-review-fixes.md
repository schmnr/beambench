# Deeper review fixes, 4 September 2026

This change addresses the ten numbered findings in [the deeper review](2026-09-04-deep-review.md). Earlier review fixes remain in the working tree. The original review and diagnostic probes describe the defects before these changes; the probes intentionally assert old behavior and are not acceptance tests.

| Finding | Correction | Regression evidence |
| --- | --- | --- |
| 1. Cancellation hides stop errors | Propagate controller stop errors and preserve the active job for recovery/retry. Emit successful cancellation only after the stop operation succeeds. | Mock GRBL reset-write and M5-write failures both retain the job; retry succeeds after clearing the transport failure. |
| 2. Export accepts incomplete plans | Reject failed planning entries, invalid raster travel, and paths outside the configured workspace before writing the destination. Invalid masks blank raster output instead of exposing the source image. | Invalid-mask export rejection and an undersized workspace leave the destination unchanged. |
| 3. Updates bypass unsaved work | Use the existing save/discard/cancel guard before download, installation, and relaunch; edits made during download require renewed approval. | Cancellation prevents installation/relaunch; edits during download prompt again; a save that leaves the project dirty cannot authorize the pending action. |
| 4. Worker handoff loses new jobs | Release worker ownership before checking for a replacement job and attempt to acquire ownership for that job. Concurrent starters still use the atomic ownership flag. | Deterministic test simulates a replacement job arriving while the old worker still owns the flag and verifies that it is processed. |
| 5. Preflight and emission differ | Carry the same plan, project, and machine profile snapshots through preflight and output generation. Fresh planning uses the captured profile, including calibration. | A test pauses start after preflight, replaces the profile/header and project, then verifies successful emission from the validated snapshot. |
| 6. G-code import loses commands | Parse compact and lowercase letter-number words and retain multiple modal commands. Reject malformed words and unsupported commands, including arcs, instead of importing partial geometry. Stop at program end and do not interpret dwell parameters as motion. | Compact/multiple-modal input matches equivalent expanded input; malformed and unsupported input rejects through the service. |
| 7. PDF stream scanning changes geometry | Resolve the page and its referenced content streams with lopdf. Unreferenced forms do not become geometry. Reject unsupported drawing features explicitly. | Complete PDF fixtures cover unused forms, invoked forms, and clipping. |
| 8. Import allocations are unbounded | Limit PDF input to 64 MiB and aggregate expanded page content to 32 MiB; reject encodings that trigger eager metadata expansion. Bound decoded raster pixels and target pixels to 67,108,864 each, and reject nonfinite dimensions. | Compressed oversized PDF content, escaped PDF metadata names, and invalid/oversized raster targets reject. |
| 9. Step buttons are inaccessible | Add field-specific names and normal keyboard/assistive activation while retaining pointer repetition and avoiding a double pointer increment. | Component activation tests and a browser accessibility-tree check found no unnamed step buttons. |
| 10. Minimum window overflows | Allow the main toolbar to wrap within its container. | Mock-backed browser check at 900×600 reports a 900-pixel document width, with the connection control fully visible; also checked at 1280×800. |

## Behavior and compatibility

PDF import deliberately supports a restricted vector subset. Multiple pages, invoked forms, clipping, text, unsupported graphics state/color operations, encrypted PDFs, compressed object streams, and unsupported page transforms produce an error. Convert text to paths and flatten unsupported features, or export a supported vector format. This prevents a successful import from silently changing the design; it does not add a full PDF renderer. Escaped reserved PDF names are checked conservatively before parsing.

G-code export uses the active machine profile's workspace dimensions. Without a configured profile, the existing default machine profile applies. Configure the intended machine dimensions before offline export. Connection-dependent start checks remain specific to starting a job; rotary export retains its connected-GRBL/current-position requirement.

## Dependencies

The npm lockfile updates remove all advisories reported by the full npm audit, including development dependencies. `event-listener` is updated to 5.4.2. The third-party license inventory is regenerated for the resolved dependencies, including lopdf.

Update: the [geometry dependency upgrade](2026-09-04-geometry-upgrade.md) removes the i_tree advisory. The glib advisory remains.

At completion of the ten-finding fix pass, two transitive Rust advisory matches remained: `i_tree 0.8.3` through the geometry libraries, and `glib 0.18.5` through the Linux GTK stack. Their fixed releases require dependency-generation upgrades, rather than a compatible patch update. At that point both were unresolved. The later geometry upgrade resolves i_tree; glib still requires follow-up. Neither review establishes that the remaining glib issue is unreachable or safe. See the advisory links in the original review.

## Validation

- Rust library suites: 2,124 passing tests across core (922), planner (329), raster (160), service (668), and streamer (45).
- Frontend: 2,581 passing tests across 202 files; TypeScript/production build and ESLint pass. Vite retains its existing large-chunk warning.
- Full npm audit: zero reported vulnerabilities, including development dependencies.
- Rust formatting and Git whitespace checks pass.
- Desktop `cargo check -p beambench-tauri` passes.
- Clippy completes successfully for core, planner, raster, service, and streamer libraries, with warnings (including a collapsible conditional in the new PDF name guard). This was not a warning-free or warnings-as-errors run.

No physical laser, controller, real updater installation, native minimum-window session, or Windows/Linux runtime was exercised. Browser checks used mocked IPC. These fixes and tests do not establish that the app is defect-free or certify machine safety.

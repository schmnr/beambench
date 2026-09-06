# Proactive bug review — 5 September 2026

Nine additional defects were found in framing, vector output, resume recovery, selected output, design transactions, DXF import, and image loading. Local fixes and regression tests accompany this review. The earlier LightBurn shared-path and transform-anchor fixes remain separate from these findings.

## Findings

| Priority | Defect and trigger | Correction and regression coverage |
| --- | --- | --- |
| P1 | **Rubber-band framing visits the origin.** The hull ends with a `Close` command. The service converted that command to `(0,0)`, adding an unintended motion segment; framing power could be enabled for that segment. | Ignore commands without coordinates. A recorded-transport test frames artwork at `(100,100)` and checks its corners and the absence of an origin move. |
| P1 | **Framing does not enclose rotated artwork.** Rectangular framing used nominal object bounds before rotation. The fallback rubber-band outline also ignored raster transforms, and clones supplied placeholder bounds. | Use world-space vector bounds, transformed raster corners, and resolved clone geometry. Tests cover a rotated rectangle, rotated raster, a selected group containing a rotated clone, and 36 frame-versus-cut comparisons across six angles, two workspace origins, and three positioning modes. |
| P1 | **Cutting distorts transformed vectors.** The new frame-versus-cut test exposed a separate defect: normalization applied the object transform and then fitted the transformed result back to its nominal, untransformed bounds. This erased translation and scale and distorted rotated geometry. | Fit the source geometry to nominal bounds first, then apply the transform around the object's world-space center. Explicit expected-bound tests cover rotation, translation, and nonuniform scale; the 36-case motion test compares the resulting cut and frame extents, excluding the optional laser-off parking move. |
| P1 | **A failed resume can restart the sender.** The stream engine resumed before sending GRBL's resume byte. If that write failed, the operation returned an error while the sender was already unpaused. | Resume the engine only after the controller and session resume operations succeed. An injected write failure followed by an acknowledgement and tick must leave the job paused without sending another line. |
| P1 | **A design transaction can overwrite newer work.** Transactions calculate outside the project lock. Their final commit previously replaced whatever project was current, including intervening edits or a different project, and could resurrect a closed project. | Compare the complete captured project with the current project while holding the commit lock. Reject stale transactions before changing undo history. Tests cover intervening edits, project replacement, project closure, and changed image bytes. The API exposes `stale_revision` for retry. |
| P1 | **Some DXF units silently change physical size.** Only inches, feet, centimeters, and meters had conversion factors. Other explicit units fell back to millimeters; for example, 1,000 microns became 1,000 mm instead of 1 mm. | Convert engineering and US survey units. Reject malformed and unsupported explicit units. Equivalent-line tests cover microinches, mils, yards, microns, decimeters, and survey feet. Unitless files retain the existing millimeter default. |
| P2 | **Selected-only output drops grouped artwork.** The UI selects a group by its parent ID, but planning retained only exact ID matches. Groups themselves emit no cut geometry, so their children disappeared from output. | Expand group selections recursively while retaining dependency objects for resolution. Tests cover nested groups, overlapping parent/child selections, hidden subtrees, and cyclic references. Framing uses the same expansion. |
| P2 | **Concurrent image loads erase each other's cache entries.** Each request rebuilt the cache from its snapshot taken before awaiting the backend. The last request to finish discarded earlier entries and left their blob URLs untracked. | Merge results into the current cache. A reversed-completion test requires both images to remain cached and both URLs to be revoked on close. Duplicate consumers of one asset reuse the same URL. |
| P2 | **Late image loads repopulate a closed project's cache.** A request could finish after closing or switching projects and insert stale image data into current UI state. | Check the project ID after the response and before creating a blob URL or recording an error. A deferred-response test closes the project first and verifies empty caches and no URL allocation. |

## Validation

- Frontend: 2,585 tests pass across 202 files. TypeScript and ESLint for the changed store and tests pass.
- Backend: 2,164 tests pass across core (931), planner (329), project persistence (23), raster (160), service (675), and streamer (46). The final run includes the normalization fix and all new regressions.
- Rust formatting and Git whitespace checks pass for the changed code.
- Fifteen new regression tests were added in this pass; the frame comparison test exercises 36 coordinate combinations. That comparison exposed the vector-normalization defect after the framing fixes, demonstrating why checks across features matter.
- Changes remain local and uncommitted.

Reproduce the backend coverage from the repository root:

```sh
cargo test -p beambench-service -p beambench-streamer -p beambench-core -p beambench-planner -p beambench-project -p beambench-raster --lib
```

Run `npm test` from the frontend directory for the UI suite.

The DXF unit codes follow Autodesk's [DXF HEADER reference](https://help.autodesk.com/cloudhelp/2024/ENU/AutoCAD-DXF/files/GUID-A85E8E67-27CD-4C59-BE61-4DC9FADBE74A.htm) and [INSUNITS documentation](https://help.autodesk.com/cloudhelp/2022/ENU/AutoCAD-Core/files/GUID-A58A87BB-482B-4042-A00A-EEF55A2B4FD8.htm). Astronomical-unit codes 18–20 produce an explicit unsupported-unit error; they are not imported at an invented scale.

## Improving bug discovery

The existing CI already runs broad Rust and frontend suites, with a Windows source test job. These regressions live in those ordinary suites, so future pull requests run them automatically. The gap was missing combinations and failure paths, despite a large passing test count.

Use the following checks for changes in each area:

1. **Geometry and import:** encode the same physical artwork in different representations and compare imported contours, closure, dimensions, and output. Include shared versus inline LightBurn paths, groups versus leaves, clones versus concrete objects, and equivalent DXF units. Retain minimized customer files as fixtures when permission and source files are available.
2. **Motion:** compare preview, frame, and emitted commands using the same artwork across rotation, coordinate origin, positioning mode, and selection state. Inject failures at pause, resume, cancellation, and disconnection boundaries. Assert the next permitted command and retained recovery state.
3. **Project state:** deliberately finish asynchronous operations in the opposite order. Close or replace the project while work is pending. Assert that current content, image caches, and undo history survive unchanged when stale work is rejected.
4. **Release testing:** run a short native Windows/macOS/Linux workflow covering import, group selection, save/reopen, undo/redo, and frame preview. Exercise actual controller disconnect/reconnect and motion behavior on designated hardware before release. Mock transport tests cannot establish firmware or physical-machine behavior.

## Limits and follow-up areas

This was a source and automated-test review of high-impact paths, not an exhaustive review of every controller or every UI workflow. No physical laser was operated, and no Windows/Linux runtime session was exercised. Backend validation covers the six library suites listed above; the full desktop integration suite was not completed.

Further review should cover late IPC responses in other project-store actions, rapid close/reopen of the same project, malformed importer coordinate systems beyond units, and controller-specific recovery behavior. These are coverage gaps, not additional confirmed findings from this pass. The previously documented Linux GTK dependency advisory was not re-audited here.

The exact medal file from the Facebook report was not supplied. Its final artwork comparison remains pending that file; the earlier LightBurn fix has synthetic and public-file equivalence coverage.


## Finding during the documentation audit

The native screenshot pass exposed a coordinate-display mismatch in Measure. On a 400 mm-high bottom-left workspace, a point at canvas Y=140 appeared as Y=260 in Properties but Y=140 in Measure. The copied measurement value was also wrong for the displayed workspace.

Measurement endpoint, midpoint, and circle/ellipse-center readouts now use the same canvas-to-workspace conversion as Properties. Distances and the stored measurement geometry are unchanged. The readout updates when the workspace changes and converts to inches after resolving the origin. Regression coverage checks bottom-left coordinates, clipboard output, switching to top-left, and circle/ellipse centers in inches.

This is a display correction. Physical laser behavior was not exercised for it. See the site documentation audit for native capture and final validation results.

The same Mac session exposed dropped characters when renaming an object in Properties. The controlled field used the last backend response as its value and sent each keystroke through asynchronous IPC. Object and layer Properties name fields now keep a local draft and commit the complete name on blur or Enter. The input is keyed to the selected object or layer, and Enter does not commit during IME composition. Ordinary immediate-update fields keep their existing behavior.

The rename change passed 51 tests across shared input, Properties, and layer Properties, plus TypeScript and changed-file ESLint. These targeted checks supplement the earlier full-suite results above; the full suite was not rerun for the documentation changes.

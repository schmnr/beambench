# Release 0.2.27 smoke record

## Build

- Version / tag: 0.2.27 / v0.2.27
- Commit: the release source commit containing this record; resolved by the tag.
- Date: 2026-10-11
- Tester: Codex, with user review and approval of the Layers layout.
- Platforms exercised: native macOS review app with current frontend source; Linux runtime reproduction in Ubuntu containers. Windows source checks run in CI.
- Hardware exercised: no physical laser available. All controller checks use simulated transports.

## Results

| Area | Check | Result | Notes / evidence |
| --- | --- | --- | --- |
| Launch | Launch and create a project | Pass | Native macOS review app starts and creates an empty C00 project. Signed 0.2.27 package checks follow packaging. |
| Project safety | New, Open, Close and unsaved prompts | Pass | Untouched new project stays clean; changes prompt on quit. Automated project safety regressions pass. |
| Canvas safety | Clear, recovery and import | Pass | Full frontend suite includes state/reset/import regressions. No additional manual raster cycle in this review. |
| Undo | Undo representative edits | Pass | Native Air Assist toggle and undo exercised; full undo regressions pass. |
| Raster | Import, adjust and repeated preview | Not run | No additional native image workflow in this review. Existing automated raster/import regressions pass. |
| Raster output | Compare processed image and output | Not run | No physical laser. Automated planning and raster output regressions pass. |
| Preview | Vector/raster twice and cancellation | Pass | Automated preview/cancellation suite passes. No separate native repeated raster smoke. |
| Placement | Origins, Frame and generated bounds | Pass | Automated placement and framing regressions pass. Physical motion not run. |
| Start From | Compare coordinate modes | Pass | Automated planner/service coordinate-mode regressions pass. Physical motion not run. |
| Material Test | Frame and Start anchors | Pass | Automated material-test placement coverage passes. Physical tests not run. |
| Machine state | Home, origin, jog, reconnect | Not run | No physical hardware. Simulated GRBL, serial, TCP and service regressions pass. |
| Job lifecycle | Pause, cancel, completion, double-start | Not run | No physical hardware. Automated lifecycle and stop safety regressions pass. |
| Layers | Name/color, Show/Output/Air Assist, stacked cuts | Pass | Native view confirms requested order and spacing. Single-entry Air Assist changes only that cut entry; stacked settings remain independent in regression tests. |
| Localization | Native menus, script rendering and persistence | Pass | English, German, Japanese, Simplified Chinese, Korean, Russian and Greek inspected. German settings tabs fit. Italian persists across restart, then English restored. Native language menu contains 23 entries. macOS native menu has no pseudo-locale entry; automated pseudo-locale coverage passes. |
| Linux startup | Color font crash and display selection | Pass | Before/after COLRv1 reproduction, native Wayland and invalid-X11 fallback documented in feedback-fix evidence. Packaging runtime test audits real SquashFS and Fontconfig. |
| Diagnostics | New report payload accepted by intake | Pass | Site validation accepts startup logs and failed-command context; full retained error history plus both startup logs stays below 200 entries. |
| Updates | Version, download and signature | Not run | Verify after signed packages are built and before stable manifest publication. |
| Packages | Signed installers and corresponding source | Not run | Verify after exact-tag builds. Final publication evidence belongs in the companion site release record. |

## Release decision

- Automated checks: frontend 2774 tests pass; production build and lint pass with existing warnings. Critical backend suites pass; final source CI must pass before tagging/building. Feedback regressions and desktop Rust checks cover added diagnostics.
- Known failures: none in the implemented fixes. Encrypted PDFs and raster complexity limits remain documented limitations.
- Checks not run: physical hardware, native Windows/Intel macOS installation, additional native raster workflows, and packages before their builds exist.
- Decision: source candidate approved by the user. Publish only after exact-tag platform build gates, signing, notarization, checksum/manifest validation and isolated upgrade smoke succeed.

See [feedback fix evidence](feedback-fixes-2026-10-11.md) and [layer control review](../feedback-layer-controls-2026-10-11.md) for reproduction details and limits. New presets use manufacturer documentation and have not been physically tested.

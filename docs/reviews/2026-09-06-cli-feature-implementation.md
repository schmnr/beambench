# CLI feature implementation follow-up

This implements the gaps identified in [the coverage check](2026-09-06-cli-feature-coverage.md). The app remains version 0.2.18 in the working tree; these changes have not been published or released.

## Implemented coverage

| Area | CLI access |
| --- | --- |
| Current connection, including network and USB | `job check`, `job start`, selection options, and explicit `--confirm-advisories`; existing progress/pause/resume/cancel commands |
| Z and runtime overrides | `machine jog --z-mm`, `feed-override`, `power-override`, `reset-overrides` |
| Live preview and output | `preview current`, `preview current-stats`, `export gcode-current` |
| Rotary, Z capability/feed, quality-test preferences and other profile fields | Sparse `profile configure PROFILE FILE` updates through the running app |
| App settings | `settings show`, `settings update FILE` |
| Project configuration and specialized editing | `project edit`, including placement, origin, notes, material height, optimization, slot resizing, and Outliner moves |
| Advanced vector and image editing | `vector edit`, including node clipboard/topology, mesh deformation, crop, masks, bitmap conversion, arrays, fillets, tabs and path operations |
| Nesting | `design nest FILE` |
| Material/focus/interval tests | `quality-test` preview/export/frame/start/create-on-canvas and recipe import/export |
| Variable text and CSV | `variable-text` parse/load-csv/resolve/preview/batch |
| Art libraries | `art-library` file and selection snapshots, organization, thumbnails, persistence and insertion |
| Material and macro libraries | Material apply/import/export, macro import/export |

The native catalog contains 173 commands: 75 project, 72 vector/image, 14 art-library, 5 variable-text and 7 quality-test commands. Some overlap existing API operations; this count is not a claim of 173 distinct new product features. Existing design transactions continue to expose their 50 operation types.

## Shared implementation and regression protection

Desktop project/vector/library/variable-text/quality-test command bodies and their existing tests now live under `beambench-service::ops::workflows`. Tauri retains adapters with the same signatures. A single definition supplies each native API command's typed deserialization, dispatch and field catalog; the CLI builds its subcommands from that catalog.

Coverage tests compare desktop command names and argument names against the public definitions, and parse every catalog operation as an actual CLI command. Existing create/close routes and internal whole-project replacement are explicit exceptions to duplicate exposure. Structured native operations do not use whole-project replacement as an editing shortcut.

Native API calls serialize with design transactions, preserve the underlying undo behavior, invalidate planning and refresh the desktop when project contents change. Read operations do not invalidate the current preview. Art-library changes also refresh the library UI. Quality-test frame/start operations enforce hardware confirmations before device access and start the normal job progress loop. Native operations wait for completion rather than timing out while an unlimited nesting operation continues to mutate state.

New workflow tests exercise the actual API router and shared service for project configuration, undo, node conversion, image conversion/masks/crop, CSV and variable-text batches, library insertion, quality-test preview/export/materialization, nesting, sparse Z/rotary updates and busy/confirmation failures. CLI integration tests cover current-session starts without reconnection, selection/advisory forwarding, Z/overrides, live output, path resolution and input-file confirmation rejection. One test runs the shipped CLI against a real private API server through edit → preview → G-code export → undo.

The implementation also rejects invalid material heights and prevents bitmap conversion from overflowing dimensions or allocating more than 64 million pixels. Failed bitmap validation leaves the document and undo stack unchanged; successful conversion marks the project dirty.

See [Native CLI workflows](../cli-workflows.md) for commands and request examples. The agent guide and capability registry have been updated.

## Validation

Final command results are recorded alongside this review in `2026-09-06-cli-feature-validation.json`.

Physical controller operation, platform-specific Windows/Linux behavior, physical focusing/material placement, device-button presses, and operating-system UI interactions have not been established by these software checks. Camera capture and overlay rendering still depend on the frontend bridge. The native field catalog is a typed field reference, not a JSON Schema document.

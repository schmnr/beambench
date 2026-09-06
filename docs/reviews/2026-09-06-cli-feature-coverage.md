# CLI feature coverage check

This is the pre-implementation coverage snapshot. The identified gaps are addressed in the [implementation follow-up](2026-09-06-cli-feature-implementation.md).

At the time of this check, the CLI did not give an AI agent full access to the app's capabilities. The preceding CLI bug audit checked and corrected existing commands against current behavior. It did not establish feature parity with every desktop workflow.

This check compares the current CLI command definitions, HTTP routes and request fields, design transaction operations, desktop command handlers, and recent changelog. It distinguishes a missing CLI wrapper from an operation missing from both public interfaces. It is a source comparison, not an end-to-end test of every feature or controller.

## Existing agent access

The design transaction schema declares 50 operation types. These include shape, text, barcode and image creation; layer and cut-entry management; transforms; boolean operations; grouping; offsets; arrays; path text; tabs; trim; alignment; and distribution. Agents use `design schema`, `design plan`, and `design apply` to access them. Looking only at the top-level CLI subcommands would miss much of this functionality.

Agents can also inspect state, import supported files, open/save projects, undo/redo, render designs for inspection, manage common profile fields, operate camera capture/alignment, connect supported controllers, frame, and control jobs within the exposed options. Most commands require the desktop app's local API. Camera capture and overlay rendering also need its frontend bridge.

An agent can address explicit object IDs without changing the GUI selection. Functional equivalence therefore does not require simulating every mouse gesture, dialog, or keyboard shortcut.

## Confirmed coverage and gaps

| Workflow | Packaged CLI | HTTP API / remaining gap | Source evidence |
| --- | --- | --- | --- |
| Shapes, text, barcodes, layers, cut entries, common transforms and vector operations | Broad access through 50 design operation types | Same transaction implementation is exposed by API; declaration alone does not prove every option matches the desktop | `crates/beambench-service/src/ops/design.rs`, `operation_schemas` and `execute_op` |
| Start a job using the current connection, including network and USB controllers | Missing a current-session start command. `job run` requires a serial port and attempts a new connection | `POST /api/v1/jobs/start` already starts through the current session. CLI can connect to network/USB controllers but cannot complete that workflow through `job run` | CLI `JobCmd` and `handle_job`; API `routes/jobs.rs` |
| Z jog added in 0.2.17 | Missing; `machine jog` accepts X, Y and feed only | `POST /api/v1/machine/jog` accepts `z_mm` | CLI `MachineCmd::Jog`; API `routes/machine.rs`, `JogBody`; changelog 0.2.17 |
| Feed and power overrides | Missing command wrappers | API has `/machine/overrides/feed`, `/spindle`, and `/reset` | API `routes/machine.rs` |
| Preflight advisories | `job run` stops on warnings; no explicit advisory acceptance option | Job-start API accepts `confirm_advisories` | CLI `handle_job`; API `StartJobBody` |
| Current project preview and G-code export using live position | CLI preview/export commands load a saved file offline | API has `/preview/generate`, `/preview/stats`, and `/preview/export/gcode` for the current session. CLI lacks wrappers for that mode | CLI `PreviewCmd`, `ExportCmd`; API `routes/preview.rs` |
| Rotary profile configuration | Missing profile flags | Profile API accepts rotary fields | CLI `ProfileCmd`; API `SaveProfileRequest` |
| Enable Z movement and set its feed rate | Missing profile flags | Profile API preserves existing `supports_z_moves` and `z_move_feed_mm_min` but does not accept changes to them | API `SaveProfileRequest` and update mapping; desktop `commands/machine.rs` |
| Material preset apply and library import/export; macro library import/export | Basic CRUD and macro run exist, but these wrappers are missing | Corresponding material/macro HTTP routes exist | CLI `MaterialCmd`, `MacroCmd`; API `routes/materials.rs`, `routes/macros.rs` |
| Application settings | No settings-editing namespace | API exposes a typed subset of settings through `/app/settings` | CLI `Commands`; API `routes/app.rs`; service `UpdateAppSettingsInput` |
| Nesting | Missing | No native nesting operation in the API routes or design transaction schema; desktop calls shared nesting service | Desktop `commands/export.rs::nest_selected`; service `ops/nesting.rs` |
| Material, focus and interval tests | Missing | No quality-test API routes or design transaction operations. Preview, frame, start and recipe handling exist as desktop commands | Desktop `commands/quality_test.rs`; API router inventory |
| CSV and variable-text batch generation | Missing native workflow | No equivalent batch API or design operation. An agent can create ordinary text itself, but that is not access to the app's batch generator | Desktop `commands/variable_text.rs`; design `create_text` initializes `variable_text` to none |
| Art libraries | Missing | No art-library API routes; desktop can load, save, organize and insert library items | Desktop `commands/art_library.rs`; capability registry `art_library.manage` |
| Advanced editing | Partial | Mesh deformation, slot resizing, several node-topology operations, and Outliner reparenting have desktop handlers without matching public operations | Desktop `commands/vector.rs` and `commands/project.rs`; API vector/project routes and design dispatch |
| Image editing | Partial; `update_object` can replace typed object data, including image settings | Native crop, mask-assignment and bitmap-conversion workflows lack dedicated public operations. Editing serialized data is not equivalent coverage of their behavior | Design `update_object`; desktop `crop_image`, `assign_image_mask`, `convert_to_bitmap` |
| Start From, Job Origin, optimization, project notes and material height | No dedicated configuration commands | Desktop has typed setters; public project routes and design operations lack equivalent setters. The internal whole-project replacement route is not a substitute for a supported edit contract | Desktop `commands/project.rs`; API `routes/projects.rs` |
| Selection, clipboard, canvas tools and panel arrangement | Mostly frontend-owned; last-known selection can be inspected | Many task outcomes can use explicit object IDs and design operations. Literal UI control remains incomplete | Capability registry and desktop command registry |

“Missing” means no supported native command or equivalent public operation was found for that workflow. It does not mean an agent could never reproduce a result by writing geometry, editing whole documents, invoking raw controller commands, or using computer control. Those workarounds do not establish CLI feature parity.

## What the previous audit did account for

The fixes use current machine profiles, controller connections, placement validation, legacy layer migration, macro confirmation, project state, and error contracts. The regression results remain valid for that scope. New Z controls, non-serial job-start workflows, nesting, and other unexposed features need a separate coverage implementation pass.

The capability registry currently uses broad entries such as `machine.jog`, `job.start`, and `profiles.manage`. A `supported` label means the advertised command exists. It does not mean every corresponding desktop field or controller workflow is available. The registry should state these limits explicitly and include newly exposed operations.

## Implementation priorities

1. Complete machine workflows first: current-session job start, Z jog and profile fields, runtime overrides, live preview/export, and deliberate preflight-advisory acceptance. Preserve the same service validation and confirmation requirements as the app.
2. Add shared commands for current project configuration, nesting, quality tests, variable-text batches, and art libraries.
3. Close editing gaps using the desktop's existing operations, including node topology, image operations, Outliner structure, and specialized geometry tools. Avoid maintaining a second implementation in the CLI.
4. Update agent discovery and add tests that compare available desktop operations with CLI/API coverage or a documented exception. Verify complete agent workflows, not only successful command parsing.

The intended standard is that an agent can inspect and perform every meaningful project and machine action through documented structured commands, with equivalent validation, undo behavior, and results. UI presentation details can remain optional. Physical material handling, focusing hardware, required device-button presses, and other physical steps still require a person.

No application behavior was changed during this coverage check. No additional hardware tests were performed.

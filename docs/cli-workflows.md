# Native CLI workflows

These commands extend the original CLI to use the same project and editing implementations as the desktop. They require the matching version of Beam Bench with its local API enabled. Run `beambench-cli agent workflows --json` for the full command and field catalog, including serialized examples for quality tests, variable text, nesting, and slot resizing.

## Work with the current machine connection

`job start` uses the current serial, network, or USB connection. It preserves the open project and its placement settings. Start with `job check` to inspect preflight. Review warnings before using `--confirm-advisories`; failures remain blocking.

```sh
beambench-cli job check --json
beambench-cli preview current --json
beambench-cli preview current-stats --json
beambench-cli export gcode-current output.gcode --json
beambench-cli job start --confirm-motion --confirm-laser-on --json
beambench-cli job progress --json
```

Starting returns the initial job result. Use `job progress`, `job pause`, `job resume`, and `job cancel` to follow or control it. The existing `job run FILE --port PORT` command retains its blocking serial-connect workflow.

`job check`, `job start`, `preview current`, and `export gcode-current` accept `--selected-ids ID1,ID2` and `--use-selection-origin`. Explicit IDs avoid dependence on the GUI selection. The latter option requires a selection. Saved-file `preview generate`, `preview stats`, and `export gcode` remain offline operations.

```sh
beambench-cli machine jog 0 0 --z-mm -1 --feed 300 --confirm-motion --json
beambench-cli machine feed-override increase10 --json
beambench-cli machine power-override decrease1 --json
beambench-cli machine reset-overrides --json
```

Override actions are `reset`, `increase10`, `decrease10`, `increase1`, and `decrease1`. Controller support and Z capability are checked by the service.

## Configure profiles, projects, and settings

Use `profile configure PROFILE patch.json` for a sparse profile update. Omitted fields retain their current values. This supports rotary settings, Z movement, quality-test preferences, camera settings, and the other profile API fields. For example:

```json
{
  "supports_z_moves": true,
  "z_move_feed_mm_min": 300,
  "rotary_enabled": true,
  "rotary_mm_per_rotation": 360,
  "rotary_object_diameter_mm": 80
}
```

```sh
beambench-cli profile configure "My laser" patch.json --json
beambench-cli settings show --json
beambench-cli settings update settings-patch.json --json
beambench-cli project edit schema --json
beambench-cli project edit set-material-height --input height.json --json
beambench-cli project undo --json
```

`height.json` contains `{"value":3}`; `{"value":null}` clears the height. Native project edits also include `set-start-from`, `set-job-origin`, `set-user-origin`, `set-optimization`, `update-project-notes`, `bind-machine-profile`, `resize-slots`, and `move-objects-in-outliner`. Their help and schema list their input fields. For instance, `set-start-from` accepts `{"mode":"current_position"}`, and `update-project-notes` accepts `{"notes":"3 mm plywood"}`.

## Native editing

Each native command accepts `--input FILE`, containing a JSON object with its arguments. Commands without arguments can omit the file. Fields use the names listed in the catalog; nested objects keep their native serialization, which may use camelCase. `agent workflows` includes ready-to-edit examples of the larger settings objects.

```sh
beambench-cli vector edit schema --json
beambench-cli vector edit convert-segment-to-curve --input segment.json --json
beambench-cli vector edit mesh-deform-selection --input warp.json --json
beambench-cli vector edit assign-image-mask --input mask.json --json
beambench-cli vector edit crop-image --input crop.json --json
beambench-cli design nest nesting.json --json
```

Example `segment.json`:

```json
{"object_id":"OBJECT_UUID","subpath_idx":0,"command_idx":1}
```

Example `mask.json`:

```json
{"image_object_id":"IMAGE_UUID","mask_object_ids":["MASK_UUID"],"polarity":"keep_inside"}
```

Example `nesting.json`:

```json
{
  "selected_ids": ["CONTAINER_UUID", "PART_UUID"],
  "options": {
    "paddingMm": 1,
    "allowRotation": true,
    "allowMirror": false,
    "lockInnerObjects": true,
    "timeLimitMs": 15000,
    "rotationStepDeg": 15
  }
}
```

The native vector commands cover node clipboard and topology, segment conversion, mesh deformation, multi-object booleans, path closure, arrays, fillets, tabs, image crop/masks and bitmap conversion. Project commands cover specialized arrangements, slot resizing, text guides, image adjustment, Outliner moves and selection queries. Existing `design plan` and `design apply` remain the interface for composing supported design operations into a single transaction.

## Libraries and variable text

```sh
beambench-cli material apply PRESET_UUID LAYER_UUID --json
beambench-cli material import materials.json --json
beambench-cli material export materials.json --json
beambench-cli macro import macros.json --json
beambench-cli macro export macros.json --json
beambench-cli art-library schema --json
beambench-cli art-library create --input library.json --json
beambench-cli art-library add-selection --input selection.json --json
beambench-cli art-library insert --input insertion.json --json
beambench-cli variable-text load-csv --input csv.json --json
beambench-cli variable-text preview --input batch.json --json
beambench-cli variable-text batch --input batch.json --json
```

`library.json` can contain `{"path":"art.bbart","name":"Workshop art"}`. `csv.json` contains `{"path":"names.csv"}`. File paths in these command inputs are resolved relative to the CLI's working directory, including output files that do not yet exist.

For variable-text preview, supply `object_id` and `config`. Batch generation also takes `offset_step`. Obtain a config example from `agent workflows`, set `source.totalCopies`, and customize its template and CSV data. Preview returns the resolved strings; batch creates editable copies and advances the source in one undo step. The standalone CSV loader returns `headers` and `rows`.

Art-library commands support external files, editable selection snapshots, loading/saving, names/categories/tags, thumbnails, moving or copying items, and insertion at explicit coordinates. Use the returned `library_id` and item `id` for subsequent commands.

## Quality tests

```sh
beambench-cli quality-test preview --input test.json --json
beambench-cli quality-test export --input export-test.json --json
beambench-cli quality-test create-on-canvas --input test.json --json
beambench-cli quality-test frame --input test.json --confirm-motion --json
beambench-cli quality-test start --input test.json --confirm-motion --confirm-laser-on --json
```

The input wraps the settings in `request`, for example `{"request":{"kind":"material"}}`. Material, focus, and interval requests accept their desktop settings. The catalog includes full default requests. Export also requires a `path`. Preview, export, frame and start use transient test geometry; `create-on-canvas` adds editable Material Test artwork to the current project and supports undo. Recipes can be imported/exported with `quality-test import-recipes` and `quality-test export-recipes`.

Focus tests retain the active profile's Z capability checks and material-height requirements. Framing requires motion confirmation; starting requires motion and laser confirmation. Confirmations must be CLI flags, not fields inside the input file.

## HTTP contract and coverage

Native operations use `POST /api/v1/workflows/{group}`, with groups `project`, `vector`, `art_library`, `variable_text`, and `quality_test`. The body contains `command` plus the typed input fields. For example:

```json
{"command":"set_material_height","value":3}
```

Quality-test hardware calls also require the corresponding confirmation booleans. `POST /api/v1/workflows/nest` accepts the nesting input above. `GET /api/v1/workflows/schema` exposes the catalog and examples. Unknown commands and unknown top-level input fields are rejected. Native commands retain their desktop result shapes; errors are structured JSON. Expensive operations run away from the API worker threads. Native commands wait for completion, including nesting with no time limit; interrupting the CLI does not cancel work already running in the app. Concurrent design transactions return a busy error.

The command catalog describes accepted fields and Rust types; it is not a JSON Schema document. Runtime deserialization and service validation enforce the input contract. The CLI constructs native subcommands from the same definitions used to deserialize and execute API requests. A coverage test checks desktop command names and arguments against those definitions.

Agents can perform the project and machine workflows listed here without reproducing GUI gestures. View layout, interactive drawing gestures, operating-system printing, and physical device/material actions are separate concerns. Camera capture and overlay rendering still require the desktop frontend bridge. Hardware checks in software do not establish that a physical controller has been tested.

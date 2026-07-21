# AI Agent Work Surface

Schema version: `1`. App version field is emitted by the running binary.

Breaking contract changes bump `schema_version`; additive fields do not.

## Bootstrap

Run `beambench-cli --help` to discover namespaces, run `beambench-cli <namespace> --help` for command flags, run `beambench-cli agent capabilities --json` once per app binary/version, then run `beambench-cli agent state --json` before each significant action. Poll agent state around every 1000ms when monitoring broad app state, and poll job progress around every 500ms during active jobs.

```bash
beambench-cli --help
beambench-cli agent capabilities --json
beambench-cli agent state --json
beambench-cli design schema --json
beambench-cli design describe --json
beambench-cli design plan plan.json --json
```

## Safety

- Motion commands require `--confirm-motion` or `confirm_motion: true`.
- Laser-emitting commands require `--confirm-laser-on` or `confirm_laser_on: true`.
- Raw G-code commands require `--confirm-raw-gcode` or `confirm_raw_gcode: true`.
- Air-assist diagnostics require `--confirm-air-assist` or `confirm_air_assist: true`.
- Profile preset application requires `--confirm-diff` or `confirm_diff: true`.
- Extra confirmation flags on non-risky commands are accepted silently.
- `SELECTION_STALE` is advisory and does not block commands.

## Capabilities

`debug_refs` in the JSON registry are human traceability metadata only. Agents should use the CLI and API fields, not Tauri or menu command names, as invokable interfaces.

| ID | Status | Kind | CLI | API | Confirmations |
| --- | --- | --- | --- | --- | --- |
| `agent.capabilities` | `supported` | `observe` | `beambench-cli agent capabilities --json` | `GET /api/v1/agent/capabilities` |  |
| `agent.state` | `supported` | `observe` | `beambench-cli agent state --json` | `GET /api/v1/agent/state` |  |
| `agent.guide` | `supported` | `observe` | `beambench-cli agent guide --json` | `GET /api/v1/agent/guide` |  |
| `design.describe` | `supported` | `observe` | `beambench-cli design describe --json` | `GET /api/v1/design/describe` |  |
| `design.schema` | `supported` | `observe` | `beambench-cli design schema --json` | `GET /api/v1/design/schema` |  |
| `design.transaction` | `supported` | `design_transaction` | `beambench-cli design plan <plan.json> --json; beambench-cli design apply <plan.json> --json` | `POST /api/v1/design/transaction/apply` |  |
| `design.render` | `supported` | `observe` | `beambench-cli design render --svg <path> --json` | `POST /api/v1/design/render` |  |
| `project.file` | `partial` | `file_io` | `beambench-cli project open <project.lzrproj> --json; beambench-cli project save --json; beambench-cli project save-as <project.lzrproj> --json; beambench-cli project close --json; beambench-cli project import-svg --layer <layer-id> <file> --json; beambench-cli project import-image --layer <layer-id> <file> --json` | `POST /api/v1/projects/save` |  |
| `app.settings` | `partial` | `machine_config` | `beambench-cli agent state --json` | `GET /api/v1/app/settings` |  |
| `project.undo_redo` | `supported` | `mutate_project` | `beambench-cli project undo --json; beambench-cli project redo --json` | `POST /api/v1/projects/undo` |  |
| `edit.selection_and_clipboard` | `ui_only` | `ui_only` |  |  |  |
| `design.arrange` | `partial` | `design_transaction` | `beambench-cli design plan <plan.json> --json; beambench-cli design apply <plan.json> --json` | `POST /api/v1/design/transaction/apply` |  |
| `machine.status` | `supported` | `observe` | `beambench-cli machine status --json` | `GET /api/v1/machine/status` |  |
| `machine.discovery` | `supported` | `machine_config` | `beambench-cli machine discover --json; beambench-cli machine connect --port <port> --baud <baud> --json` | `POST /api/v1/machine/connect` |  |
| `machine.controller_connection` | `supported` | `machine_config` | `beambench-cli machine connect-serial --port <port> --controller <controller> --json; beambench-cli machine connect-network --host <host> --controller <controller> --json; beambench-cli machine list-lihuiyu-usb --json; beambench-cli machine connect-lihuiyu --bus-id <bus> --device-address <address> --port-numbers <chain> --json` | `POST /api/v1/machine/connect/controller/serial` |  |
| `machine.home` | `supported` | `hardware_motion` | `beambench-cli machine home --confirm-motion --json` | `POST /api/v1/machine/home` | confirm_motion |
| `machine.jog` | `supported` | `hardware_motion` | `beambench-cli machine jog <x_mm> <y_mm> --feed <rate> --confirm-motion --json` | `POST /api/v1/machine/jog` | confirm_motion |
| `machine.frame` | `supported` | `hardware_motion` | `beambench-cli job frame --confirm-motion --json` | `POST /api/v1/jobs/frame` | confirm_motion |
| `machine.emergency_stop` | `supported` | `hardware_motion` | `beambench-cli machine emergency-stop --json` | `POST /api/v1/machine/emergency-stop` |  |
| `machine.raw_gcode` | `supported` | `raw_gcode` | `beambench-cli console send <line> --confirm-raw-gcode --json` | `POST /api/v1/console` | confirm_raw_gcode |
| `machine.test_air` | `supported` | `machine_config` | `beambench-cli machine test-air --duration-ms 1000 --confirm-air-assist --json` | `POST /api/v1/machine/test-air` | confirm_air_assist |
| `job.preflight` | `supported` | `job_prepare` | `beambench-cli job preflight <project.lzrproj> --json` | `POST /api/v1/jobs/preflight` |  |
| `job.start` | `supported` | `hardware_laser` | `beambench-cli job run <project.lzrproj> --port <port> --confirm-motion --confirm-laser-on --json` | `POST /api/v1/jobs/start` | confirm_motion, confirm_laser_on |
| `job.pause_cancel` | `supported` | `job_prepare` | `beambench-cli job pause --json; beambench-cli job resume --json; beambench-cli job cancel --json` | `POST /api/v1/jobs/cancel` |  |
| `profiles.manage` | `supported` | `machine_config` | `beambench-cli profile list --json; beambench-cli profile show <profile> --json; beambench-cli profile create --name <name> --json; beambench-cli profile update <profile> --name <name> --json; beambench-cli profile delete <profile> --json; beambench-cli profile activate <profile> --json` | `GET /api/v1/profiles` |  |
| `profiles.presets` | `supported` | `machine_config` | `beambench-cli profile presets --json; beambench-cli profile suggest --json; beambench-cli profile apply-preset <preset-id> --confirm-diff --json` | `GET /api/v1/profiles/presets` | confirm_diff |
| `camera.overlay` | `supported` | `observe` | `beambench-cli camera doctor --json; beambench-cli camera list --json; beambench-cli camera state --json; beambench-cli camera capture --camera <camera-id> --json; beambench-cli camera overlay render --view fit --json; beambench-cli camera overlay show --json; beambench-cli camera overlay hide --json; beambench-cli camera overlay opacity 0.45 --json; beambench-cli camera overlay fit-to-bed --json; beambench-cli camera overlay discard --json; beambench-cli camera overlay save-alignment --json; beambench-cli camera overlay set-transform --x 0 --y 0 --scale 1 --rotation-deg 0 --json; beambench-cli camera overlay nudge --dx 1 --dy 0 --json; beambench-cli camera overlay scale --factor 1.1 --json; beambench-cli camera overlay rotate --deg 90 --json` | `GET /api/v1/camera/state` |  |
| `materials.manage` | `supported` | `machine_config` | `beambench-cli material list --json; beambench-cli material add --name <name> --material <material> --speed <rate> --power <percent> --passes <passes> --json; beambench-cli material remove <id> --json` | `GET /api/v1/materials` |  |
| `macros.manage` | `partial` | `machine_config` | `beambench-cli macro list --json; beambench-cli macro add --name <name> --description <description> --commands <commands> --json; beambench-cli macro remove <id> --json; beambench-cli macro run <id> --json` | `GET /api/v1/macros` |  |
| `art_library.manage` | `ui_only` | `ui_only` |  |  |  |
| `window.view_controls` | `ui_only` | `ui_only` |  |  |  |
| `help.menu` | `ignored` | `ui_only` |  |  |  |

## Regeneration

Regenerate this checked-in guide with:

```bash
cargo run -p beambench-cli -- agent guide --markdown > docs/AI_AGENT_WORK_SURFACE.md
```

# Command-line interface

For current-session machine controls and the native editing, nesting, quality-test, variable-text, and library commands, see [Native CLI workflows](cli-workflows.md).

The CLI combines local file commands with a client for the running Beam Bench app. Use the CLI binary from the same release as the app. Run `beambench-cli --help` and `beambench-cli <command> --help` for the complete command and option lists.

## App connection and state

Enable Local API in Beam Bench's General settings and leave the app running for API commands. The CLI reads the shared settings file to discover the port, connects to `127.0.0.1`, and bypasses HTTP proxies and redirects. `BEAMBENCH_CONFIG_DIR` selects the same alternate settings directory used by the service. A saved disabled flag does not prevent a request to an already running API.

| Commands | State they use |
| --- | --- |
| `project create`, `project info`, `asset` | Local project files |
| `export gcode`, `job dry-run` | Local project and saved machine profile; shared service output checks |
| `preview generate`, `preview stats` | Local project and saved planning settings; no live machine position |
| `agent guide --markdown`, `version`, `ports`, `diagnostics export` | Local build information, generated guide, or port enumeration |
| `feedback save`, `feedback diagnostics` | Local diagnostic context and optional project file |
| `feedback submit` | Sends a report to the configured feedback endpoint |
| Other `project` commands, `profile`, `import`, vector-format `export`, `machine`, `vector`, `design`, `job`, `console`, `material`, `macro`, other `agent` commands | Running app's local API and current project/session |
| `camera` | App API; capture and overlay rendering also need the app's frontend bridge |

API project commands change the app's current project. `job preflight <file>` also opens that file in the app. Save your work before switching projects. Profile commands read and update the running app, so its in-memory profiles and saved settings stay in sync. Local feedback preparation does not update recent files or rewrite settings.

## Files and output

Relative file paths are interpreted from the CLI's working directory, including paths sent to the app. The destination directory must exist. Quote paths containing spaces.

```sh
beambench-cli project info "jobs/medal.lzrproj" --json
beambench-cli project open "jobs/medal.lzrproj"
beambench-cli export svg --path "exports/medal.svg"
beambench-cli export svg > "exports/medal.svg"
beambench-cli export pdf > "exports/medal.pdf"
```

Without `--path`, SVG, DXF, EPS, and AI exports write their text to stdout. PDF writes binary PDF bytes. With `--json`, these commands return the API's JSON response instead; PDF content remains base64 encoded in that response.

## Offline planning and G-code

```sh
beambench-cli job dry-run "jobs/medal.lzrproj" --json
beambench-cli export gcode "jobs/medal.lzrproj" "exports/medal.gcode"
```

These commands load the project directly, apply the desktop's legacy layer migrations, and use its checked G-code generator. They do not open or replace the app's current project.

The project-assigned machine profile takes precedence over the saved active profile. That profile must exist in this computer's settings. A project snapshot contains bed and speed metadata, but does not contain the complete power and Z configuration needed to reconstruct an output profile. A missing referenced profile is an error; the CLI does not silently choose another machine.

The saved profile supplies output settings such as maximum S value, constant-power mode, Z configuration, and custom headers/footers. Empty output, failed planning entries, invalid raster travel, and placement outside the configured workspace block G-code output before the destination is written. The CLI also rejects using the input project's resolved path as the G-code destination.

Offline output has no live machine position. Current Position and rotary output require export through the connected app. User Origin requires an origin saved in the project. Preview generation can work without a profile when none is assigned, but preview statistics are not permission to run a job. Preview rejects incomplete plans; dry-run additionally performs the G-code output checks.

## Jobs and confirmations

Commands that move the machine, enable the laser, send raw G-code, or enable air assist require the corresponding global confirmation flags. Inspect a command's help and the [agent work surface](./AI_AGENT_WORK_SURFACE.md) for its requirements.

Macro execution requires `--confirm-raw-gcode`. A macro containing a laser-enabling command also requires `--confirm-laser-on`. The API checks every command in the macro for these requirements before sending the first one.

`job run` is a serial connect/open/preflight/start/wait workflow. It requires a port, `--confirm-motion`, and `--confirm-laser-on`. Any connection failure stops the workflow. If the app is already connected, disconnect deliberately before using this command; it will not reuse that connection on an “already connected” error.

`job preflight` returns the complete report and exits nonzero for a failed preflight. A warning report can be inspected with a zero exit status. `job run` requires a full pass and stops for either failures or warnings, preserving the report. There is no CLI override for accepting preflight advisories in this workflow.

The app owns the job after it starts. Interrupting the CLI with Ctrl+C, closing its terminal, or losing the API connection does not cancel the machine's job. Use `beambench-cli job cancel` or Stop in the app. `job progress` can inspect the current job from another CLI invocation. Completion, cancellation, streaming failure, and an unexpected return to Idle terminate the waiting command; failure reports retain the controller's error details when available.

## Scripting and troubleshooting

Runtime `--json` results are one JSON document on stdout. Human-readable errors go to stderr without that flag. Argument parsing errors come from the command parser and can be text even with `--json`.

| Exit status | Meaning |
| --- | --- |
| `0` | Command completed; inspect report fields for advisory or diagnostic status |
| `1` | Operation failed, including failed preflight or streaming |
| `2` | Local API connection/response failure, or command-line parsing error |
| `3` | Explicit argument validation failures that use the CLI validation status |
| `4` | Required confirmation is missing |

API error payloads retain their original fields. Service codes use snake_case, such as `busy` and `stale_revision`; confirmation errors use `CONFIRMATION_REQUIRED`. Design retry handles a transient `busy` response, but does not retry a stale revision against a changed project.

Coordinates accept negative numbers. Floating-point arguments must be finite; `NaN` and infinity are rejected before they can become null JSON fields. For a boolean that defaults to true, use an explicit value when disabling it, for example `--use-g0-for-overscan false` on profile creation.

`camera doctor --json` checks the saved settings, live API, frontend bridge, and reported devices. Inspect its `ok` field and checks; the command itself can finish successfully while reporting a setup problem. Camera renders use unique filenames. `--keep` files survive automatic cleanup, and ordinary temporary renders are eligible for cleanup after 24 hours.

## Audit and release checks

See the [September 6 CLI audit](./reviews/2026-09-06-cli-audit.md) for confirmed defects, regression coverage, and remaining platform and physical-controller verification.

The CLI does not expose every desktop feature. See the [feature coverage check](./reviews/2026-09-06-cli-feature-coverage.md) for API-only operations and missing automation workflows.

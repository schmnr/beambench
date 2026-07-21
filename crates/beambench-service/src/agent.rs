use std::collections::HashSet;

use beambench_common::machine::SessionState;
use beambench_core::{AppSettings, MachineProfile, ObjectData, Project};
use chrono::Utc;
use serde::Serialize;
use serde_json::{Value, json};

use crate::context::ServiceContext;
use crate::error::{ServiceError, ServiceResult};
use crate::{events, ops};

pub const AGENT_SCHEMA_VERSION: u32 = 1;
pub const SELECTION_STALE_MS: f64 = 300_000.0;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct AgentSelectionSnapshot {
    pub selected_object_ids: Vec<String>,
    pub selected_layer_id: Option<String>,
    pub project_id: Option<String>,
    pub frontend_updated_at_ms: f64,
    pub received_at_ms: f64,
}

#[derive(Debug, Clone)]
pub struct AgentSelectionSyncInput {
    pub selected_object_ids: Vec<String>,
    pub selected_layer_id: Option<String>,
    pub project_id: Option<String>,
    pub frontend_updated_at_ms: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentCliRef {
    pub command: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentApiRef {
    pub method: &'static str,
    pub path: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentDebugRefs {
    pub tauri_command: Option<&'static str>,
    pub menu_command: Option<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentCapability {
    pub id: &'static str,
    pub title: &'static str,
    pub group: &'static str,
    pub kind: &'static str,
    pub status: &'static str,
    pub gaps: Vec<&'static str>,
    pub cli: Option<AgentCliRef>,
    pub api: Option<AgentApiRef>,
    pub debug_refs: AgentDebugRefs,
    pub confirmation: Vec<&'static str>,
    pub notes: Vec<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentWarning {
    pub code: &'static str,
    pub message: String,
    pub details: Value,
}

fn lock_err(name: &str, e: impl std::fmt::Display) -> ServiceError {
    ServiceError::internal(format!("Failed to lock {name}: {e}"))
}

fn now_ms() -> f64 {
    Utc::now().timestamp_millis() as f64
}

fn cap(
    id: &'static str,
    title: &'static str,
    group: &'static str,
    kind: &'static str,
    status: &'static str,
    cli: Option<&'static str>,
    api: Option<(&'static str, &'static str)>,
    menu_command: Option<&'static str>,
    tauri_command: Option<&'static str>,
    confirmation: Vec<&'static str>,
    gaps: Vec<&'static str>,
    notes: Vec<&'static str>,
) -> AgentCapability {
    AgentCapability {
        id,
        title,
        group,
        kind,
        status,
        gaps,
        cli: cli.map(|command| AgentCliRef { command }),
        api: api.map(|(method, path)| AgentApiRef { method, path }),
        debug_refs: AgentDebugRefs {
            tauri_command,
            menu_command,
        },
        confirmation,
        notes,
    }
}

pub fn capabilities() -> Vec<AgentCapability> {
    vec![
        cap(
            "agent.capabilities",
            "Discover Agent Capabilities",
            "agent",
            "observe",
            "supported",
            Some("beambench-cli agent capabilities --json"),
            Some(("GET", "/api/v1/agent/capabilities")),
            None,
            None,
            vec![],
            vec![],
            vec!["Run once per app version, then cache until app_version changes."],
        ),
        cap(
            "agent.state",
            "Inspect Agent State",
            "agent",
            "observe",
            "supported",
            Some("beambench-cli agent state --json"),
            Some(("GET", "/api/v1/agent/state")),
            None,
            None,
            vec![],
            vec![],
            vec!["Run before each significant action."],
        ),
        cap(
            "agent.guide",
            "Read Agent Guide",
            "agent",
            "observe",
            "supported",
            Some("beambench-cli agent guide --json"),
            Some(("GET", "/api/v1/agent/guide")),
            None,
            None,
            vec![],
            vec![],
            vec!["The markdown guide is generated from this same registry."],
        ),
        cap(
            "design.describe",
            "Describe Current Design",
            "design",
            "observe",
            "supported",
            Some("beambench-cli design describe --json"),
            Some(("GET", "/api/v1/design/describe")),
            None,
            None,
            vec![],
            vec![],
            vec!["Includes canvas object/layer summaries and render hints."],
        ),
        cap(
            "design.schema",
            "Inspect Design Transaction Schema",
            "design",
            "observe",
            "supported",
            Some("beambench-cli design schema --json"),
            Some(("GET", "/api/v1/design/schema")),
            None,
            None,
            vec![],
            vec![],
            vec!["Use before composing design transaction plans."],
        ),
        cap(
            "design.transaction",
            "Apply One-Shot Design Transaction",
            "design",
            "design_transaction",
            "supported",
            Some(
                "beambench-cli design plan <plan.json> --json; beambench-cli design apply <plan.json> --json",
            ),
            Some(("POST", "/api/v1/design/transaction/apply")),
            Some("tools.*"),
            None,
            vec![],
            vec![],
            vec!["There is no begin/commit transaction session in V1."],
        ),
        cap(
            "design.render",
            "Render Canvas Snapshot",
            "design",
            "observe",
            "supported",
            Some("beambench-cli design render --svg <path> --json"),
            Some(("POST", "/api/v1/design/render")),
            Some("window.preview"),
            None,
            vec![],
            vec![],
            vec!["Use SVG for vector inspection and PNG/JPG when bitmap content matters."],
        ),
        cap(
            "project.file",
            "Project File Operations",
            "project",
            "file_io",
            "partial",
            Some(
                "beambench-cli project open <project.lzrproj> --json; beambench-cli project save --json; beambench-cli project save-as <project.lzrproj> --json; beambench-cli project close --json; beambench-cli project import-svg --layer <layer-id> <file> --json; beambench-cli project import-image --layer <layer-id> <file> --json",
            ),
            Some(("POST", "/api/v1/projects/save")),
            Some("file.*"),
            None,
            vec![],
            vec!["Some desktop file-picker flows remain UI-only."],
            vec![
                "Project open/save/import/export commands are available through existing project/export groups.",
            ],
        ),
        cap(
            "app.settings",
            "Application Settings",
            "app",
            "machine_config",
            "partial",
            Some("beambench-cli agent state --json"),
            Some(("GET", "/api/v1/app/settings")),
            Some("app.preferences"),
            None,
            vec![],
            vec!["Interactive settings dialogs remain UI-led in V1."],
            vec!["Agent state exposes settings summary and API bind mode."],
        ),
        cap(
            "project.undo_redo",
            "Undo And Redo",
            "project",
            "mutate_project",
            "supported",
            Some("beambench-cli project undo --json; beambench-cli project redo --json"),
            Some(("POST", "/api/v1/projects/undo")),
            Some("edit.undo"),
            None,
            vec![],
            vec![],
            vec![],
        ),
        cap(
            "edit.selection_and_clipboard",
            "Selection And Clipboard Actions",
            "edit",
            "ui_only",
            "ui_only",
            None,
            None,
            Some("edit.*"),
            None,
            vec![],
            vec!["Clipboard is frontend-owned in V1."],
            vec!["Agents can inspect last-known selection through agent state."],
        ),
        cap(
            "design.arrange",
            "Arrange And Transform Selection",
            "design",
            "design_transaction",
            "partial",
            Some(
                "beambench-cli design plan <plan.json> --json; beambench-cli design apply <plan.json> --json",
            ),
            Some(("POST", "/api/v1/design/transaction/apply")),
            Some("arrange.*"),
            None,
            vec![],
            vec![
                "Some interactive arrange helpers depend on frontend selection or live laser position.",
            ],
            vec![
                "Use design transactions for align, distribute, transforms, arrays, grouping, and mirror-across-line where possible.",
            ],
        ),
        cap(
            "machine.status",
            "Inspect Machine Status",
            "machine",
            "observe",
            "supported",
            Some("beambench-cli machine status --json"),
            Some(("GET", "/api/v1/machine/status")),
            None,
            None,
            vec![],
            vec![],
            vec![],
        ),
        cap(
            "machine.discovery",
            "Discover And Connect Machines",
            "machine",
            "machine_config",
            "supported",
            Some(
                "beambench-cli machine discover --json; beambench-cli machine connect --port <port> --baud <baud> --json",
            ),
            Some(("POST", "/api/v1/machine/connect")),
            None,
            None,
            vec![],
            vec![],
            vec![],
        ),
        cap(
            "machine.controller_connection",
            "Connect With An Explicit Controller",
            "machine",
            "machine_config",
            "supported",
            Some(
                "beambench-cli machine connect-serial --port <port> --controller <controller> --json; beambench-cli machine connect-network --host <host> --controller <controller> --json; beambench-cli machine list-lihuiyu-usb --json; beambench-cli machine connect-lihuiyu --bus-id <bus> --device-address <address> --port-numbers <chain> --json",
            ),
            Some(("POST", "/api/v1/machine/connect/controller/serial")),
            None,
            None,
            vec![],
            vec![],
            vec![
                "Network connections use POST /api/v1/machine/connect/controller/network.",
                "Lihuiyu discovery uses GET /api/v1/machine/usb/lihuiyu and connection uses POST /api/v1/machine/connect/usb.",
                "When a connection returns status=challenge, continue the backend-owned attempt with POST /api/v1/machine/connect/controller/continue or the CLI continue-controller command.",
            ],
        ),
        cap(
            "machine.home",
            "Home Machine",
            "machine",
            "hardware_motion",
            "supported",
            Some("beambench-cli machine home --confirm-motion --json"),
            Some(("POST", "/api/v1/machine/home")),
            None,
            Some("machine_home"),
            vec!["confirm_motion"],
            vec![],
            vec![],
        ),
        cap(
            "machine.jog",
            "Jog Machine",
            "machine",
            "hardware_motion",
            "supported",
            Some("beambench-cli machine jog <x_mm> <y_mm> --feed <rate> --confirm-motion --json"),
            Some(("POST", "/api/v1/machine/jog")),
            Some("arrange.jog_laser.*"),
            Some("machine_jog"),
            vec!["confirm_motion"],
            vec![],
            vec![],
        ),
        cap(
            "machine.frame",
            "Frame Job Boundary",
            "machine",
            "hardware_motion",
            "supported",
            Some("beambench-cli job frame --confirm-motion --json"),
            Some(("POST", "/api/v1/jobs/frame")),
            Some("laser.*"),
            Some("frame_job"),
            vec!["confirm_motion"],
            vec![],
            vec!["If laser-on framing is requested, confirm_laser_on is also required."],
        ),
        cap(
            "machine.emergency_stop",
            "Emergency Stop",
            "machine",
            "hardware_motion",
            "supported",
            Some("beambench-cli machine emergency-stop --json"),
            Some(("POST", "/api/v1/machine/emergency-stop")),
            None,
            Some("emergency_stop"),
            vec![],
            vec![],
            vec!["Emergency stop is intentionally callable without confirmation."],
        ),
        cap(
            "machine.raw_gcode",
            "Send Raw G-code",
            "machine",
            "raw_gcode",
            "supported",
            Some("beambench-cli console send <line> --confirm-raw-gcode --json"),
            Some(("POST", "/api/v1/console")),
            None,
            Some("send_gcode_line"),
            vec!["confirm_raw_gcode"],
            vec![],
            vec!["Lines containing M3/M4 or S>0 also require confirm_laser_on."],
        ),
        cap(
            "machine.test_air",
            "Test Air Assist",
            "machine",
            "machine_config",
            "supported",
            Some("beambench-cli machine test-air --duration-ms 1000 --confirm-air-assist --json"),
            Some(("POST", "/api/v1/machine/test-air")),
            None,
            None,
            vec!["confirm_air_assist"],
            vec![],
            vec!["Toggles configured GRBL air assist only; no motion and no laser-on commands."],
        ),
        cap(
            "job.preflight",
            "Run Job Preflight",
            "job",
            "job_prepare",
            "supported",
            Some("beambench-cli job preflight <project.lzrproj> --json"),
            Some(("POST", "/api/v1/jobs/preflight")),
            None,
            None,
            vec![],
            vec![],
            vec![],
        ),
        cap(
            "job.start",
            "Start Laser Job",
            "job",
            "hardware_laser",
            "supported",
            Some(
                "beambench-cli job run <project.lzrproj> --port <port> --confirm-motion --confirm-laser-on --json",
            ),
            Some(("POST", "/api/v1/jobs/start")),
            None,
            Some("start_job"),
            vec!["confirm_motion", "confirm_laser_on"],
            vec![],
            vec![],
        ),
        cap(
            "job.pause_cancel",
            "Pause Resume Or Cancel Job",
            "job",
            "job_prepare",
            "supported",
            Some(
                "beambench-cli job pause --json; beambench-cli job resume --json; beambench-cli job cancel --json",
            ),
            Some(("POST", "/api/v1/jobs/cancel")),
            None,
            None,
            vec![],
            vec![],
            vec!["Pause, resume, and cancel are callable without confirmation."],
        ),
        cap(
            "profiles.manage",
            "Manage Machine Profiles",
            "profile",
            "machine_config",
            "supported",
            Some(
                "beambench-cli profile list --json; beambench-cli profile show <profile> --json; beambench-cli profile create --name <name> --json; beambench-cli profile update <profile> --name <name> --json; beambench-cli profile delete <profile> --json; beambench-cli profile activate <profile> --json",
            ),
            Some(("GET", "/api/v1/profiles")),
            None,
            None,
            vec![],
            vec![],
            vec![],
        ),
        cap(
            "profiles.presets",
            "Profile Presets",
            "profile",
            "machine_config",
            "supported",
            Some(
                "beambench-cli profile presets --json; beambench-cli profile suggest --json; beambench-cli profile apply-preset <preset-id> --confirm-diff --json",
            ),
            Some(("GET", "/api/v1/profiles/presets")),
            None,
            None,
            vec!["confirm_diff"],
            vec![],
            vec![
                "Preset suggestions are conservative and require clear firmware identity matches.",
            ],
        ),
        cap(
            "camera.overlay",
            "Camera Overlay And Calibration",
            "camera",
            "observe",
            "supported",
            Some(
                "beambench-cli camera doctor --json; beambench-cli camera list --json; beambench-cli camera state --json; beambench-cli camera capture --camera <camera-id> --json; beambench-cli camera overlay render --view fit --json; beambench-cli camera overlay show --json; beambench-cli camera overlay hide --json; beambench-cli camera overlay opacity 0.45 --json; beambench-cli camera overlay fit-to-bed --json; beambench-cli camera overlay discard --json; beambench-cli camera overlay save-alignment --json; beambench-cli camera overlay set-transform --x 0 --y 0 --scale 1 --rotation-deg 0 --json; beambench-cli camera overlay nudge --dx 1 --dy 0 --json; beambench-cli camera overlay scale --factor 1.1 --json; beambench-cli camera overlay rotate --deg 90 --json",
            ),
            Some(("GET", "/api/v1/camera/state")),
            None,
            None,
            vec![],
            vec![],
            vec![
                "Exact real-camera capture and canvas render require the Beam Bench app frontend to be open.",
                "Run camera doctor first when capture or render fails; it reports API reachability, frontend bridge state, and camera devices.",
            ],
        ),
        cap(
            "materials.manage",
            "Manage Material Presets",
            "material",
            "machine_config",
            "supported",
            Some(
                "beambench-cli material list --json; beambench-cli material add --name <name> --material <material> --speed <rate> --power <percent> --passes <passes> --json; beambench-cli material remove <id> --json",
            ),
            Some(("GET", "/api/v1/materials")),
            None,
            None,
            vec![],
            vec![],
            vec![],
        ),
        cap(
            "macros.manage",
            "Manage Macros",
            "macro",
            "machine_config",
            "partial",
            Some(
                "beambench-cli macro list --json; beambench-cli macro add --name <name> --description <description> --commands <commands> --json; beambench-cli macro remove <id> --json; beambench-cli macro run <id> --json",
            ),
            Some(("GET", "/api/v1/macros")),
            None,
            None,
            vec![],
            vec!["Macro execution safety is not fully classified in V1."],
            vec![],
        ),
        cap(
            "art_library.manage",
            "Manage Art Libraries",
            "art_library",
            "ui_only",
            "ui_only",
            None,
            None,
            None,
            None,
            vec![],
            vec!["Art library browsing and placement remain frontend-owned in V1."],
            vec![],
        ),
        cap(
            "window.view_controls",
            "Window And View Controls",
            "window",
            "ui_only",
            "ui_only",
            None,
            None,
            Some("window.*"),
            None,
            vec![],
            vec!["View state is frontend-owned in V1."],
            vec![],
        ),
        cap(
            "help.menu",
            "Help And Support Menu",
            "help",
            "ui_only",
            "ignored",
            None,
            None,
            Some("help.*"),
            None,
            vec![],
            vec![],
            vec!["Not useful as an agent-facing automation surface."],
        ),
    ]
}

pub fn warning_codes() -> Vec<&'static str> {
    vec![
        "NO_ACTIVE_PROJECT",
        "PROJECT_HAS_UNSAVED_CHANGES",
        "MISSING_MACHINE_PROFILE",
        "PROFILE_PROJECT_BED_MISMATCH",
        "EXISTING_OUT_OF_BED_GEOMETRY",
        "MISSING_ASSETS",
        "FRONTEND_SELECTION_NOT_SYNCED",
        "SELECTION_STALE",
        "SELECTION_PROJECT_MISMATCH",
        "SELECTION_OBJECT_MISSING",
        "MACHINE_DISCONNECTED",
        "MACHINE_BUSY_JOB_RUNNING",
        "API_REMOTE_BIND_ENABLED",
    ]
}

pub fn kind_values() -> Vec<&'static str> {
    vec![
        "observe",
        "mutate_project",
        "file_io",
        "design_transaction",
        "machine_config",
        "job_prepare",
        "hardware_motion",
        "hardware_laser",
        "raw_gcode",
        "ui_only",
    ]
}

pub fn capabilities_response() -> Value {
    json!({
        "schema_version": AGENT_SCHEMA_VERSION,
        "app_version": env!("CARGO_PKG_VERSION"),
        "generated_at": events::timestamp(),
        "debug_refs_note": "debug_refs are human traceability metadata only; agents should not treat them as invokable interfaces.",
        "status_values": ["supported", "partial", "ui_only", "planned", "ignored"],
        "kind_values": kind_values(),
        "warning_codes": warning_codes(),
        "capabilities": capabilities(),
    })
}

pub fn guide_response() -> Value {
    json!({
        "schema_version": AGENT_SCHEMA_VERSION,
        "app_version": env!("CARGO_PKG_VERSION"),
        "generated_at": events::timestamp(),
        "bootstrap": [
            { "step": 1, "command": "beambench-cli --help", "purpose": "Discover top-level CLI namespaces exposed by this app binary." },
            { "step": 2, "command": "beambench-cli agent capabilities --json", "purpose": "Discover this app version's supported, partial, UI-only, planned, and ignored capabilities." },
            { "step": 3, "command": "beambench-cli agent state --json", "purpose": "Inspect current project, selection, profile, machine, job, API, and warnings before acting." },
            { "step": 4, "command": "beambench-cli design schema --json", "purpose": "Load the design transaction operation schema before creating or editing canvas content." },
            { "step": 5, "command": "beambench-cli design describe --json", "purpose": "Inspect the current canvas and render/design command hints." },
            { "step": 6, "command": "beambench-cli design plan plan.json --json", "purpose": "Dry-run a one-shot design transaction before applying it." }
        ],
        "examples": [
            {
                "title": "Inspect and render the current design",
                "commands": [
                    "beambench-cli agent state --json",
                    "beambench-cli design describe --json",
                    "beambench-cli design render --svg canvas.svg --json"
                ]
            },
            {
                "title": "Diagnose and inspect camera overlay",
                "commands": [
                    "beambench-cli camera doctor --json",
                    "beambench-cli camera state --json",
                    "beambench-cli camera overlay render --view fit --json"
                ]
            },
            {
                "title": "Safely move the laser",
                "commands": [
                    "beambench-cli agent state --json",
                    "beambench-cli machine jog 1 0 --feed 1000 --confirm-motion --json"
                ]
            },
            {
                "title": "Dry-run then apply a generated design",
                "commands": [
                    "beambench-cli design schema --json",
                    "beambench-cli design plan plan.json --json",
                    "beambench-cli design apply plan.json --json"
                ]
            }
        ],
        "safety_summary": {
            "motion_flag": "--confirm-motion",
            "laser_flag": "--confirm-laser-on",
            "raw_gcode_flag": "--confirm-raw-gcode",
            "air_assist_flag": "--confirm-air-assist",
            "diff_flag": "--confirm-diff"
        },
        "polling": {
            "state_ms": 1000,
            "active_job_progress_ms": 500
        }
    })
}

pub fn guide_markdown() -> String {
    let caps = capabilities();
    let mut out = String::new();
    out.push_str("# AI Agent Work Surface\n\n");
    out.push_str(&format!(
        "Schema version: `{}`. App version field is emitted by the running binary.\n\n",
        AGENT_SCHEMA_VERSION
    ));
    out.push_str("Breaking contract changes bump `schema_version`; additive fields do not.\n\n");
    out.push_str("## Bootstrap\n\n");
    out.push_str("Run `beambench-cli --help` to discover namespaces, run `beambench-cli <namespace> --help` for command flags, run `beambench-cli agent capabilities --json` once per app binary/version, then run `beambench-cli agent state --json` before each significant action. Poll agent state around every 1000ms when monitoring broad app state, and poll job progress around every 500ms during active jobs.\n\n");
    out.push_str("```bash\n");
    out.push_str("beambench-cli --help\n");
    out.push_str("beambench-cli agent capabilities --json\n");
    out.push_str("beambench-cli agent state --json\n");
    out.push_str("beambench-cli design schema --json\n");
    out.push_str("beambench-cli design describe --json\n");
    out.push_str("beambench-cli design plan plan.json --json\n");
    out.push_str("```\n\n");
    out.push_str("## Safety\n\n");
    out.push_str("- Motion commands require `--confirm-motion` or `confirm_motion: true`.\n");
    out.push_str(
        "- Laser-emitting commands require `--confirm-laser-on` or `confirm_laser_on: true`.\n",
    );
    out.push_str(
        "- Raw G-code commands require `--confirm-raw-gcode` or `confirm_raw_gcode: true`.\n",
    );
    out.push_str(
        "- Air-assist diagnostics require `--confirm-air-assist` or `confirm_air_assist: true`.\n",
    );
    out.push_str(
        "- Profile preset application requires `--confirm-diff` or `confirm_diff: true`.\n",
    );
    out.push_str("- Extra confirmation flags on non-risky commands are accepted silently.\n");
    out.push_str("- `SELECTION_STALE` is advisory and does not block commands.\n\n");
    out.push_str("## Capabilities\n\n");
    out.push_str("`debug_refs` in the JSON registry are human traceability metadata only. Agents should use the CLI and API fields, not Tauri or menu command names, as invokable interfaces.\n\n");
    out.push_str("| ID | Status | Kind | CLI | API | Confirmations |\n");
    out.push_str("| --- | --- | --- | --- | --- | --- |\n");
    for cap in caps {
        let cli = cap
            .cli
            .as_ref()
            .map(|c| format!("`{}`", c.command.replace('|', "\\|")))
            .unwrap_or_else(|| "".to_string());
        let api = cap
            .api
            .as_ref()
            .map(|a| format!("`{} {}`", a.method, a.path))
            .unwrap_or_else(|| "".to_string());
        let confirmations = if cap.confirmation.is_empty() {
            "".to_string()
        } else {
            cap.confirmation.join(", ")
        };
        out.push_str(&format!(
            "| `{}` | `{}` | `{}` | {} | {} | {} |\n",
            cap.id, cap.status, cap.kind, cli, api, confirmations
        ));
    }
    out.push_str("\n## Regeneration\n\n");
    out.push_str("Regenerate this checked-in guide with:\n\n");
    out.push_str("```bash\n");
    out.push_str(
        "cargo run -p beambench-cli -- agent guide --markdown > docs/AI_AGENT_WORK_SURFACE.md\n",
    );
    out.push_str("```\n");
    out
}

fn active_profile(settings: &AppSettings) -> Option<MachineProfile> {
    settings
        .active_profile_id
        .and_then(|id| settings.machine_profiles.iter().find(|p| p.id == id))
        .cloned()
}

fn warning(code: &'static str, message: impl Into<String>, details: Value) -> AgentWarning {
    AgentWarning {
        code,
        message: message.into(),
        details,
    }
}

fn project_has_out_of_bed_geometry(project: &Project, bed_w: f64, bed_h: f64) -> Vec<String> {
    project
        .objects
        .iter()
        .filter(|object| object.visible)
        .filter(|object| {
            object.bounds.min.x < 0.0
                || object.bounds.min.y < 0.0
                || object.bounds.max.x > bed_w
                || object.bounds.max.y > bed_h
        })
        .map(|object| object.id.to_string())
        .collect()
}

fn missing_asset_keys(project: &Project) -> Vec<String> {
    let assets: HashSet<String> = project
        .assets
        .iter()
        .map(|asset| asset.id.to_string())
        .collect();
    let loaded_asset_data: HashSet<String> = project
        .asset_data
        .keys()
        .map(|asset_id| asset_id.to_string())
        .collect();
    let mut missing = Vec::new();
    for object in &project.objects {
        if let ObjectData::RasterImage { asset_key, .. } = &object.data {
            if !assets.contains(asset_key) && !loaded_asset_data.contains(asset_key) {
                missing.push(asset_key.clone());
            }
        }
    }
    missing.sort();
    missing.dedup();
    missing
}

pub fn sync_selection(ctx: &ServiceContext, input: AgentSelectionSyncInput) -> ServiceResult<()> {
    let active_project_id = {
        let project = ctx.project.lock().map_err(|e| lock_err("project", e))?;
        project
            .as_ref()
            .map(|project| project.metadata.project_id.to_string())
    };

    if input.project_id != active_project_id {
        return Ok(());
    }

    let mut guard = ctx
        .agent_selection
        .lock()
        .map_err(|e| lock_err("agent_selection", e))?;
    if guard
        .as_ref()
        .is_some_and(|existing| existing.frontend_updated_at_ms > input.frontend_updated_at_ms)
    {
        return Ok(());
    }
    *guard = Some(AgentSelectionSnapshot {
        selected_object_ids: input.selected_object_ids,
        selected_layer_id: input.selected_layer_id,
        project_id: input.project_id,
        frontend_updated_at_ms: input.frontend_updated_at_ms,
        received_at_ms: now_ms(),
    });
    Ok(())
}

pub fn agent_state(ctx: &ServiceContext) -> ServiceResult<Value> {
    let generated_at = events::timestamp();
    let generated_at_ms = now_ms();
    let project = ctx
        .project
        .lock()
        .map_err(|e| lock_err("project", e))?
        .clone();
    let project_path = ctx
        .project_path
        .lock()
        .map_err(|e| lock_err("project_path", e))?
        .clone();
    let settings = ctx
        .settings
        .lock()
        .map_err(|e| lock_err("settings", e))?
        .clone();
    let undo_state = ctx
        .history
        .lock()
        .map_err(|e| lock_err("history", e))?
        .state();
    let selection = ctx
        .agent_selection
        .lock()
        .map_err(|e| lock_err("agent_selection", e))?
        .clone();
    let machine = ops::machine::runtime_state(ctx)?;

    let mut warnings = Vec::new();
    if !settings.api_localhost_only {
        warnings.push(warning(
            "API_REMOTE_BIND_ENABLED",
            "The HTTP API is configured for remote binding. V1 assumes local-first access and adds no token auth.",
            json!({ "api_localhost_only": false }),
        ));
    }

    let active_profile = active_profile(&settings);
    if settings.active_profile_id.is_none() || active_profile.is_none() {
        warnings.push(warning(
            "MISSING_MACHINE_PROFILE",
            "No active machine profile is configured.",
            json!({ "active_profile_id": settings.active_profile_id }),
        ));
    }

    let current_project_id = project
        .as_ref()
        .map(|project| project.metadata.project_id.to_string());

    let project_json = if let Some(project) = project.as_ref() {
        if project.dirty {
            warnings.push(warning(
                "PROJECT_HAS_UNSAVED_CHANGES",
                "The current project has unsaved changes.",
                json!({ "project_id": project.metadata.project_id }),
            ));
        }

        if let Some(profile) = active_profile.as_ref() {
            if (profile.bed_width_mm - project.workspace.bed_width_mm).abs() > 1e-6
                || (profile.bed_height_mm - project.workspace.bed_height_mm).abs() > 1e-6
            {
                warnings.push(warning(
                    "PROFILE_PROJECT_BED_MISMATCH",
                    "The active machine profile bed size does not match the project workspace.",
                    json!({
                        "profile_bed": { "width_mm": profile.bed_width_mm, "height_mm": profile.bed_height_mm },
                        "project_bed": { "width_mm": project.workspace.bed_width_mm, "height_mm": project.workspace.bed_height_mm },
                    }),
                ));
            }
        }

        if let Some(snapshot) = project.machine_profile_snapshot.as_ref() {
            if (snapshot.bed_width_mm - project.workspace.bed_width_mm).abs() > 1e-6
                || (snapshot.bed_height_mm - project.workspace.bed_height_mm).abs() > 1e-6
            {
                warnings.push(warning(
                    "PROFILE_PROJECT_BED_MISMATCH",
                    "The project machine-profile snapshot does not match the current workspace size.",
                    json!({
                        "snapshot_bed": { "width_mm": snapshot.bed_width_mm, "height_mm": snapshot.bed_height_mm },
                        "project_bed": { "width_mm": project.workspace.bed_width_mm, "height_mm": project.workspace.bed_height_mm },
                    }),
                ));
            }
        }

        let out_of_bed = project_has_out_of_bed_geometry(
            project,
            project.workspace.bed_width_mm,
            project.workspace.bed_height_mm,
        );
        if !out_of_bed.is_empty() {
            warnings.push(warning(
                "EXISTING_OUT_OF_BED_GEOMETRY",
                "Visible geometry exists outside the project bed rectangle.",
                json!({ "object_ids": out_of_bed }),
            ));
        }

        let missing_assets = missing_asset_keys(project);
        if !missing_assets.is_empty() {
            warnings.push(warning(
                "MISSING_ASSETS",
                "Some raster image objects reference missing assets.",
                json!({ "asset_keys": missing_assets }),
            ));
        }

        Some(json!({
            "id": project.metadata.project_id,
            "name": project.metadata.project_name,
            "path": project_path.map(|path| path.to_string_lossy().to_string()),
            "dirty": project.dirty,
            "undo": undo_state,
            "layer_count": project.layers.len(),
            "object_count": project.objects.len(),
            "asset_count": project.assets.len(),
            "workspace": project.workspace,
            "machine_profile_id": project.machine_profile_id,
            "machine_profile_snapshot": project.machine_profile_snapshot,
        }))
    } else {
        warnings.push(warning(
            "NO_ACTIVE_PROJECT",
            "No project is currently loaded.",
            json!({}),
        ));
        None
    };

    let selection_json = match selection.as_ref() {
        Some(selection) => {
            let freshness = generated_at_ms - selection.received_at_ms;
            let mut missing_ids = Vec::new();
            let mut project_mismatch = false;
            if selection.project_id != current_project_id {
                project_mismatch = true;
                warnings.push(warning(
                    "SELECTION_PROJECT_MISMATCH",
                    "The last synced frontend selection belongs to a different project.",
                    json!({
                        "selection_project_id": selection.project_id,
                        "active_project_id": current_project_id,
                    }),
                ));
            }
            if let Some(project) = project.as_ref() {
                let existing_ids: HashSet<String> = project
                    .objects
                    .iter()
                    .map(|object| object.id.to_string())
                    .collect();
                missing_ids = selection
                    .selected_object_ids
                    .iter()
                    .filter(|id| !existing_ids.contains(*id))
                    .cloned()
                    .collect();
                if !missing_ids.is_empty() {
                    warnings.push(warning(
                        "SELECTION_OBJECT_MISSING",
                        "The last synced frontend selection references objects that no longer exist.",
                        json!({ "object_ids": missing_ids }),
                    ));
                }
            }
            if freshness > SELECTION_STALE_MS || project_mismatch || !missing_ids.is_empty() {
                warnings.push(warning(
                    "SELECTION_STALE",
                    "The last synced frontend selection is stale or no longer resolves cleanly.",
                    json!({
                        "selection_freshness_ms": freshness.max(0.0),
                        "threshold_ms": SELECTION_STALE_MS,
                        "project_mismatch": project_mismatch,
                        "missing_object_ids": missing_ids,
                    }),
                ));
            }
            json!({
                "selected_object_ids": selection.selected_object_ids,
                "selected_layer_id": selection.selected_layer_id,
                "project_id": selection.project_id,
                "frontend_updated_at_ms": selection.frontend_updated_at_ms,
                "received_at_ms": selection.received_at_ms,
                "selection_freshness_ms": freshness.max(0.0),
            })
        }
        None => {
            warnings.push(warning(
                "FRONTEND_SELECTION_NOT_SYNCED",
                "The frontend has not synced a selection snapshot yet.",
                json!({}),
            ));
            Value::Null
        }
    };

    if machine.session_state == SessionState::Disconnected {
        warnings.push(warning(
            "MACHINE_DISCONNECTED",
            "No machine session is connected.",
            json!({ "session_state": machine.session_state }),
        ));
    }
    if machine.job_active {
        warnings.push(warning(
            "MACHINE_BUSY_JOB_RUNNING",
            "A machine job is currently running.",
            json!({ "job_progress": machine.job_progress }),
        ));
    }
    let camera = ops::camera::get_camera_state(ctx).ok();

    Ok(json!({
        "schema_version": AGENT_SCHEMA_VERSION,
        "app_version": env!("CARGO_PKG_VERSION"),
        "generated_at": generated_at,
        "app": {
            "version": env!("CARGO_PKG_VERSION"),
            "settings": {
                "display_unit": settings.display_unit,
                "autosave_enabled": settings.autosave_enabled,
                "machine_profile_count": settings.machine_profiles.len(),
                "active_profile_id": settings.active_profile_id,
            },
            "api": {
                "enabled": settings.api_enabled,
                "port": settings.api_port,
                "localhost_only": settings.api_localhost_only,
                "bind_mode": if settings.api_localhost_only { "loopback" } else { "remote" },
            },
        },
        "project": project_json,
        "selection": selection_json,
        "active_profile": active_profile.map(|profile| json!({
            "id": profile.id,
            "name": profile.name,
            "bed_width_mm": profile.bed_width_mm,
            "bed_height_mm": profile.bed_height_mm,
            "origin": profile.origin,
            "firmware_type": profile.firmware_type,
            "max_speed_mm_min": profile.max_speed_mm_min,
            "max_power_percent": profile.max_power_percent,
            "preset_id": profile.preset_id,
            "preset_version": profile.preset_version,
        })),
        "machine": machine,
        "camera": camera,
        "command_hints": {
            "capabilities": "beambench-cli agent capabilities --json",
            "state": "beambench-cli agent state --json",
            "render_svg": "beambench-cli design render --svg canvas.svg --json",
            "render_png": "beambench-cli design render --png canvas.png --json",
            "design_schema": "beambench-cli design schema --json",
            "design_plan": "beambench-cli design plan plan.json --json",
            "design_apply": "beambench-cli design apply plan.json --json",
            "camera_state": "beambench-cli camera state --json",
            "camera_render": "beambench-cli camera overlay render --json",
            "connect_serial_controller": "beambench-cli machine connect-serial --port <port> --controller <controller> --json",
            "connect_network_controller": "beambench-cli machine connect-network --host <host> --controller <controller> --json",
            "list_lihuiyu_usb": "beambench-cli machine list-lihuiyu-usb --json",
            "events": "/api/v1/events",
        },
        "warnings": warnings,
    }))
}

pub fn menu_command_covered(command: &str) -> bool {
    capabilities().iter().any(|cap| {
        cap.debug_refs
            .menu_command
            .is_some_and(|pattern| pattern_matches(pattern, command))
    }) || ignored_menu_command_patterns()
        .iter()
        .any(|pattern| pattern_matches(pattern, command))
}

pub fn ignored_menu_command_patterns() -> Vec<&'static str> {
    vec![
        "app.about",
        "app.quit",
        "file.recent.*",
        "file.recent.empty",
        "language.*",
    ]
}

fn pattern_matches(pattern: &str, value: &str) -> bool {
    if let Some(prefix) = pattern.strip_suffix('*') {
        value.starts_with(prefix)
    } else {
        pattern == value
    }
}

fn strip_gcode_comments(line: &str) -> String {
    let mut out = String::new();
    let mut in_parens = false;
    for ch in line.chars() {
        if in_parens {
            if ch == ')' {
                in_parens = false;
            }
            continue;
        }
        match ch {
            ';' => break,
            '(' => in_parens = true,
            _ => out.push(ch),
        }
    }
    out
}

pub fn raw_gcode_requires_laser_confirmation(line: &str) -> bool {
    let stripped = strip_gcode_comments(line).to_ascii_uppercase();
    let chars: Vec<char> = stripped.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let letter = chars[i];
        if !letter.is_ascii_alphabetic() {
            i += 1;
            continue;
        }
        i += 1;
        while i < chars.len() && chars[i].is_ascii_whitespace() {
            i += 1;
        }
        let start = i;
        if i < chars.len() && (chars[i] == '+' || chars[i] == '-') {
            i += 1;
        }
        while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
            i += 1;
        }
        let word: String = chars[start..i].iter().collect();
        let value = word.parse::<f64>().ok();
        if letter == 'M' && value.is_some_and(|v| (v - 3.0).abs() < 1e-9 || (v - 4.0).abs() < 1e-9)
        {
            return true;
        }
        if letter == 'S' && value.is_some_and(|v| v > 0.0) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use beambench_common::{Bounds, Point2D};
    use beambench_core::{MachineProfile, ProjectObject, ShapeKind};
    use std::collections::BTreeSet;

    #[test]
    fn raw_gcode_detection_ignores_comments_and_off_commands() {
        assert!(!raw_gcode_requires_laser_confirmation("; M3 S1000"));
        assert!(!raw_gcode_requires_laser_confirmation("(M3 S1000) G0 X0"));
        assert!(!raw_gcode_requires_laser_confirmation("M5 S0"));
        assert!(!raw_gcode_requires_laser_confirmation("G1 X1 S0"));
        assert!(raw_gcode_requires_laser_confirmation("M3"));
        assert!(raw_gcode_requires_laser_confirmation("M04"));
        assert!(raw_gcode_requires_laser_confirmation("G1X1Y2S0.5"));
    }

    #[test]
    fn capabilities_have_required_registry_invariants() {
        let kind_values = kind_values();
        let declared_kinds = kind_values.iter().copied().collect::<BTreeSet<_>>();
        let used_kinds = capabilities()
            .into_iter()
            .map(|cap| cap.kind)
            .collect::<BTreeSet<_>>();
        assert_eq!(
            declared_kinds, used_kinds,
            "kind_values must match the capability kind vocabulary exactly"
        );

        for cap in capabilities() {
            assert!(
                kind_values.contains(&cap.kind),
                "capability {} has undocumented kind {}",
                cap.id,
                cap.kind
            );
            if cap.status == "supported" {
                assert!(
                    cap.cli.is_some() || cap.api.is_some(),
                    "supported capability {} needs a CLI or API ref",
                    cap.id
                );
            }
            if cap.status == "partial" {
                assert!(
                    !cap.gaps.is_empty(),
                    "partial capability {} must document gaps",
                    cap.id
                );
            }
        }
    }

    #[test]
    fn explicit_controller_connections_are_agent_discoverable() {
        let capability = capabilities()
            .into_iter()
            .find(|capability| capability.id == "machine.controller_connection")
            .expect("explicit controller connection capability must be registered");

        assert_eq!(capability.status, "supported");
        assert_eq!(
            capability.api.as_ref().map(|api| api.path),
            Some("/api/v1/machine/connect/controller/serial")
        );
        let cli = capability
            .cli
            .as_ref()
            .expect("controller connection capability needs CLI guidance")
            .command;
        assert!(cli.contains("connect-serial"));
        assert!(cli.contains("connect-network"));
        assert!(cli.contains("connect-lihuiyu"));
        assert!(
            capability
                .notes
                .iter()
                .any(|note| note.contains("/connect/controller/continue"))
        );
    }

    #[test]
    fn selection_sync_is_latest_wins_by_frontend_timestamp() {
        let ctx = ServiceContext::new();
        sync_selection(
            &ctx,
            AgentSelectionSyncInput {
                selected_object_ids: vec!["new".into()],
                selected_layer_id: None,
                project_id: None,
                frontend_updated_at_ms: 200.0,
            },
        )
        .unwrap();
        sync_selection(
            &ctx,
            AgentSelectionSyncInput {
                selected_object_ids: vec!["old".into()],
                selected_layer_id: None,
                project_id: None,
                frontend_updated_at_ms: 100.0,
            },
        )
        .unwrap();
        let snapshot = ctx.agent_selection.lock().unwrap().clone().unwrap();
        assert_eq!(snapshot.selected_object_ids, vec!["new"]);
    }

    #[test]
    fn selection_sync_discards_mismatched_project_payload() {
        let ctx = ServiceContext::new();
        let project = Project::new("Active");
        let active_id = project.metadata.project_id.to_string();
        *ctx.project.lock().unwrap() = Some(project);

        sync_selection(
            &ctx,
            AgentSelectionSyncInput {
                selected_object_ids: vec!["wrong".into()],
                selected_layer_id: None,
                project_id: Some("other-project".into()),
                frontend_updated_at_ms: 200.0,
            },
        )
        .unwrap();
        assert!(ctx.agent_selection.lock().unwrap().is_none());

        sync_selection(
            &ctx,
            AgentSelectionSyncInput {
                selected_object_ids: vec![],
                selected_layer_id: None,
                project_id: Some(active_id),
                frontend_updated_at_ms: 300.0,
            },
        )
        .unwrap();
        assert!(ctx.agent_selection.lock().unwrap().is_some());
    }

    #[test]
    fn agent_state_reports_selection_missing_as_advisory_warning() {
        let ctx = ServiceContext::new();
        let mut project = Project::new("Active");
        project.ensure_default_layer();
        let project_id = project.metadata.project_id.to_string();
        *ctx.project.lock().unwrap() = Some(project);
        sync_selection(
            &ctx,
            AgentSelectionSyncInput {
                selected_object_ids: vec!["missing-object".into()],
                selected_layer_id: None,
                project_id: Some(project_id),
                frontend_updated_at_ms: 100.0,
            },
        )
        .unwrap();

        let state = agent_state(&ctx).unwrap();
        let codes: Vec<&str> = state["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|w| w["code"].as_str())
            .collect();
        assert!(codes.contains(&"SELECTION_OBJECT_MISSING"));
        assert!(codes.contains(&"SELECTION_STALE"));
    }

    #[test]
    fn agent_state_reports_project_and_profile_warnings() {
        let mut settings = AppSettings::default();
        settings.api_localhost_only = false;
        let mut profile = MachineProfile::default();
        profile.bed_width_mm = 50.0;
        profile.bed_height_mm = 50.0;
        profile.preset_id = Some("sculpfun_s30_pro_max_20w".to_string());
        profile.preset_version = Some(1);
        settings.active_profile_id = Some(profile.id);
        settings.machine_profiles.push(profile);
        let ctx = ServiceContext::with_settings(settings);
        let mut project = Project::new("Bounds");
        let layer_id = project.ensure_default_layer();
        let object = ProjectObject::new(
            "Out",
            layer_id,
            Bounds::new(Point2D::new(410.0, 0.0), Point2D::new(420.0, 10.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        );
        project.objects.push(object);
        project.dirty = true;
        *ctx.project.lock().unwrap() = Some(project);

        let state = agent_state(&ctx).unwrap();
        let codes: Vec<&str> = state["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|w| w["code"].as_str())
            .collect();
        assert!(codes.contains(&"PROJECT_HAS_UNSAVED_CHANGES"));
        assert!(codes.contains(&"PROFILE_PROJECT_BED_MISMATCH"));
        assert!(codes.contains(&"EXISTING_OUT_OF_BED_GEOMETRY"));
        assert!(codes.contains(&"API_REMOTE_BIND_ENABLED"));
        assert_eq!(
            state["active_profile"]["preset_id"],
            "sculpfun_s30_pro_max_20w"
        );
        assert_eq!(state["active_profile"]["preset_version"], 1);
    }
}

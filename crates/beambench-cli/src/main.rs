//! Beam Bench CLI — command-line interface sharing the Rust core.

use beambench_common::feedback::{
    FeedbackKind, FeedbackReportInput, FeedbackSourceContext, SubmitFeedbackResponse,
};
use beambench_common::machine::{DiscoveryTcpTarget, DiscoveryUsbTarget};
use clap::{Args, Parser, Subcommand, ValueEnum};
use reqwest::{Method, StatusCode};
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const DESIGN_RENDER_SCHEMA_VERSION: u32 = 1;
const DESIGN_RENDER_TEMP_PREFIX: &str = "beambench-design-render-";
const DESIGN_RENDER_DEFAULT_MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);
const CAMERA_AGENT_SCHEMA_VERSION: u32 = 1;
const CAMERA_RENDER_TEMP_PREFIX: &str = "beambench-camera-render-";

#[derive(Parser)]
#[command(
    name = "beambench",
    about = "Beam Bench laser engraving CLI",
    after_help = "Free software licensed under GPL-3.0-or-later. Source: https://github.com/schmnr/beambench"
)]
struct Cli {
    /// Output as JSON instead of human-readable text
    #[arg(long, global = true)]
    json: bool,

    /// Confirm commands that can move the machine
    #[arg(long, global = true, default_value_t = false)]
    confirm_motion: bool,

    /// Confirm commands that can turn on the laser
    #[arg(long, global = true, default_value_t = false)]
    confirm_laser_on: bool,

    /// Confirm commands that send raw G-code
    #[arg(long, global = true, default_value_t = false)]
    confirm_raw_gcode: bool,

    /// Confirm commands that can toggle air assist
    #[arg(long, global = true, default_value_t = false)]
    confirm_air_assist: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Clone, Copy)]
struct ConfirmFlags {
    confirm_motion: bool,
    confirm_laser_on: bool,
    confirm_raw_gcode: bool,
    confirm_air_assist: bool,
}

#[derive(Subcommand)]
#[allow(clippy::large_enum_variant)]
enum Commands {
    /// Agent-oriented discovery, state, and guide commands
    Agent {
        #[command(subcommand)]
        command: AgentCmd,
    },
    /// Project operations
    Project {
        #[command(subcommand)]
        command: ProjectCmd,
    },
    /// Export operations
    Export {
        #[command(subcommand)]
        command: ExportCmd,
    },
    /// Machine operations
    Machine {
        #[command(subcommand)]
        command: MachineCmd,
    },
    /// Camera operations
    Camera {
        #[command(subcommand)]
        command: CameraCmd,
    },
    /// Preview operations
    Preview {
        #[command(subcommand)]
        command: PreviewCmd,
    },
    /// Vector editing operations
    Vector {
        #[command(subcommand)]
        command: VectorCmd,
    },
    /// Agent-oriented design transaction operations
    Design {
        #[command(subcommand)]
        command: DesignCmd,
    },
    /// Diagnostics operations
    Diagnostics {
        #[command(subcommand)]
        command: DiagnosticsCmd,
    },
    /// Bug-report and feedback operations
    Feedback {
        #[command(subcommand)]
        command: FeedbackCmd,
    },
    /// Machine profile operations
    Profile {
        #[command(subcommand)]
        command: ProfileCmd,
    },
    /// Asset operations
    Asset {
        #[command(subcommand)]
        command: AssetCmd,
    },
    /// Job operations
    Job {
        #[command(subcommand)]
        command: JobCmd,
    },
    /// Console operations
    Console {
        #[command(subcommand)]
        command: ConsoleCmd,
    },
    /// Material preset operations
    Material {
        #[command(subcommand)]
        command: MaterialCmd,
    },
    /// Macro operations
    Macro {
        #[command(subcommand)]
        command: MacroCmd,
    },
    /// Import operations (DXF, PDF, AI)
    Import {
        #[command(subcommand)]
        command: ImportCmd,
    },
    /// List available serial ports
    Ports,
    /// Show application version
    Version,
}

#[derive(Subcommand)]
enum AgentCmd {
    /// Print the agent capability registry
    Capabilities,
    /// Print current app/project/machine/selection state
    State,
    /// Print the agent bootstrap guide
    Guide {
        /// Emit generated markdown instead of JSON
        #[arg(long)]
        markdown: bool,
    },
}

#[derive(Subcommand)]
enum ProjectCmd {
    /// Create a new project
    Create {
        /// Project name
        name: String,
        /// Output path (.lzrproj)
        #[arg(short, long)]
        output: Option<String>,
    },
    /// Show project info
    Info {
        /// Path to .lzrproj file
        path: String,
    },
    /// Open a project in the local API host
    Open {
        /// Path to .lzrproj file
        path: String,
    },
    /// Save the current project via the local API
    Save,
    /// Save the current project to a new path via the local API
    SaveAs {
        /// Output path for the current project
        path: String,
    },
    /// Close the current project via the local API
    Close,
    /// Undo the last project mutation via the local API
    Undo,
    /// Redo the last undone project mutation via the local API
    Redo,
    /// Import an SVG file into the current project via the local API
    ImportSvg {
        #[arg(long)]
        layer: String,
        file: String,
    },
    /// Import an image file into the current project via the local API
    ImportImage {
        #[arg(long)]
        layer: String,
        file: String,
    },
    /// Import multiple files into the current project via the local API
    ImportFiles {
        #[arg(long)]
        layer: String,
        files: Vec<String>,
    },
}

#[derive(Subcommand)]
enum ExportCmd {
    /// Export G-code from a project
    Gcode {
        /// Path to .lzrproj file
        input: String,
        /// Output G-code file path
        output: String,
    },
    /// Export SVG from current project via local API
    Svg {
        /// Output SVG file path
        #[arg(long)]
        path: Option<String>,
        /// Export selection only
        #[arg(long, default_value_t = false)]
        selection_only: bool,
        /// Object IDs to export (required when --selection-only is set)
        #[arg(long, value_delimiter = ',')]
        selected_ids: Vec<String>,
    },
    /// Export DXF from current project via local API
    Dxf {
        /// Output DXF file path
        #[arg(long)]
        path: Option<String>,
        /// Export selection only
        #[arg(long, default_value_t = false)]
        selection_only: bool,
        /// Object IDs to export (required when --selection-only is set)
        #[arg(long, value_delimiter = ',')]
        selected_ids: Vec<String>,
    },
    /// Export PDF from current project via local API
    Pdf {
        /// Output PDF file path
        #[arg(long)]
        path: Option<String>,
        /// Export selection only
        #[arg(long, default_value_t = false)]
        selection_only: bool,
        /// Object IDs to export (required when --selection-only is set)
        #[arg(long, value_delimiter = ',')]
        selected_ids: Vec<String>,
    },
    /// Export EPS from current project via local API
    Eps {
        /// Output EPS file path
        #[arg(long)]
        path: Option<String>,
        /// Export selection only
        #[arg(long, default_value_t = false)]
        selection_only: bool,
        /// Object IDs to export (required when --selection-only is set)
        #[arg(long, value_delimiter = ',')]
        selected_ids: Vec<String>,
    },
    /// Export AI (Adobe Illustrator) from current project via local API
    Ai {
        /// Output AI file path
        #[arg(long)]
        path: Option<String>,
        /// Export selection only
        #[arg(long, default_value_t = false)]
        selection_only: bool,
        /// Object IDs to export (required when --selection-only is set)
        #[arg(long, value_delimiter = ',')]
        selected_ids: Vec<String>,
    },
}

fn known_controller_selection(driver: &str) -> Value {
    serde_json::json!({ "mode": "known_driver", "driver": driver })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum ControllerArg {
    AutoDetect,
    Grbl,
    FluidNc,
    GrblHal,
    LaserPecker,
    Marlin,
    Snapmaker,
    Smoothieware,
    Ruida,
    Lihuiyu,
    GenericGrblCompatible,
}

impl ControllerArg {
    fn selection_json(self) -> Value {
        match self {
            Self::AutoDetect => serde_json::json!({ "mode": "auto_detect" }),
            Self::Grbl => known_controller_selection("grbl"),
            Self::FluidNc => known_controller_selection("fluid_nc"),
            Self::GrblHal => known_controller_selection("grbl_hal"),
            Self::LaserPecker => known_controller_selection("laser_pecker"),
            Self::Marlin => known_controller_selection("marlin"),
            Self::Snapmaker => known_controller_selection("snapmaker"),
            Self::Smoothieware => known_controller_selection("smoothieware"),
            Self::Ruida => known_controller_selection("ruida"),
            Self::Lihuiyu => known_controller_selection("lihuiyu"),
            Self::GenericGrblCompatible => {
                serde_json::json!({ "mode": "generic_grbl_compatible" })
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum SerialControllerArg {
    AutoDetect,
    Grbl,
    FluidNc,
    GrblHal,
    LaserPecker,
    Marlin,
    Snapmaker,
    Smoothieware,
    GenericGrblCompatible,
}

impl SerialControllerArg {
    fn selection_json(self) -> Value {
        match self {
            Self::AutoDetect => ControllerArg::AutoDetect.selection_json(),
            Self::Grbl => ControllerArg::Grbl.selection_json(),
            Self::FluidNc => ControllerArg::FluidNc.selection_json(),
            Self::GrblHal => ControllerArg::GrblHal.selection_json(),
            Self::LaserPecker => ControllerArg::LaserPecker.selection_json(),
            Self::Marlin => ControllerArg::Marlin.selection_json(),
            Self::Snapmaker => ControllerArg::Snapmaker.selection_json(),
            Self::Smoothieware => ControllerArg::Smoothieware.selection_json(),
            Self::GenericGrblCompatible => ControllerArg::GenericGrblCompatible.selection_json(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum NetworkControllerArg {
    AutoDetect,
    FluidNc,
    GrblHal,
    LaserPecker,
    Ruida,
}

impl NetworkControllerArg {
    fn selection_json(self) -> Value {
        match self {
            Self::AutoDetect => ControllerArg::AutoDetect.selection_json(),
            Self::FluidNc => ControllerArg::FluidNc.selection_json(),
            Self::GrblHal => ControllerArg::GrblHal.selection_json(),
            Self::LaserPecker => ControllerArg::LaserPecker.selection_json(),
            Self::Ruida => ControllerArg::Ruida.selection_json(),
        }
    }

    const fn default_port(self) -> u16 {
        match self {
            Self::Ruida => 50200,
            Self::LaserPecker => 8888,
            Self::AutoDetect | Self::FluidNc | Self::GrblHal => 23,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum ControllerDecisionArg {
    UseDetected,
    ContinueSelected,
    Cancel,
}

impl ControllerDecisionArg {
    const fn as_api_str(self) -> &'static str {
        match self {
            Self::UseDetected => "use_detected",
            Self::ContinueSelected => "continue_selected_experimentally",
            Self::Cancel => "cancel",
        }
    }
}

#[derive(Subcommand)]
enum MachineCmd {
    /// Connect to a machine through the legacy GRBL serial path
    Connect {
        /// Serial port name
        #[arg(short, long)]
        port: String,
        /// Baud rate
        #[arg(short, long, default_value = "115200")]
        baud: u32,
    },
    /// Connect to an explicitly selected or auto-detected serial controller
    ConnectSerial {
        /// Serial port name
        #[arg(short, long)]
        port: String,
        /// Baud rate
        #[arg(short, long, default_value = "115200")]
        baud: u32,
        /// Controller adapter to use
        #[arg(long, value_enum, default_value = "auto-detect")]
        controller: SerialControllerArg,
    },
    /// Connect to an explicitly selected or auto-detected network controller
    ConnectNetwork {
        /// Controller hostname or IP address
        #[arg(long)]
        host: String,
        /// TCP or UDP port; defaults to 23, 8888 for LaserPecker, or 50200 for Ruida
        #[arg(long)]
        port: Option<u16>,
        /// Controller adapter to use
        #[arg(long, value_enum, default_value = "auto-detect")]
        controller: NetworkControllerArg,
    },
    /// Continue a controller-choice challenge returned by a connection command
    ContinueController {
        /// Backend-owned connection attempt ID
        attempt_id: String,
        /// Controller adapter to use for the continuation
        #[arg(long, value_enum)]
        controller: ControllerArg,
        /// Required when resolving a detected/selected controller mismatch
        #[arg(long, value_enum)]
        decision: Option<ControllerDecisionArg>,
    },
    /// Connect to a discovered candidate
    ConnectCandidate {
        /// Discovery candidate id
        candidate_id: String,
    },
    /// List stock K40/Lihuiyu CH341 USB controllers
    ListLihuiyuUsb,
    /// Connect to a stock K40/Lihuiyu controller over USB
    ConnectLihuiyu {
        /// USB bus ID reported by list-lihuiyu-usb
        #[arg(long)]
        bus_id: String,
        /// Current USB device address
        #[arg(long)]
        device_address: u8,
        /// Stable physical USB port chain, such as 1,3
        #[arg(long, value_delimiter = ',')]
        port_numbers: Vec<u8>,
    },
    /// Discover available machines
    Discover {
        /// Manual TCP discovery target in host:port format
        #[arg(long = "tcp")]
        tcp_targets: Vec<String>,
        /// Manual USB packet discovery target path
        #[arg(long = "usb")]
        usb_targets: Vec<String>,
    },
    /// Disconnect the current machine session
    Disconnect,
    /// Home the machine via the local API
    Home,
    /// Unlock the machine via the local API
    Unlock,
    /// Jog the machine via the local API
    Jog {
        /// Jog delta X in mm
        #[arg(allow_hyphen_values = true)]
        x_mm: f64,
        /// Jog delta Y in mm
        #[arg(allow_hyphen_values = true)]
        y_mm: f64,
        /// Feed rate in mm/min
        #[arg(short, long, default_value = "1500")]
        feed: f64,
    },
    /// Set the current work origin via the local API
    SetOrigin,
    /// Reset the work origin via the local API
    ResetOrigin,
    /// Emergency stop via the local API
    EmergencyStop,
    /// Show machine status
    Status,
    /// Toggle configured air assist on briefly, then off
    TestAir {
        /// Duration to leave air assist on, in milliseconds
        #[arg(long, default_value_t = 1000)]
        duration_ms: u64,
    },
}

#[derive(Subcommand)]
enum CameraCmd {
    /// Diagnose API, app bridge, camera device, and overlay readiness for agents
    Doctor,
    /// List camera devices from the local API host
    List,
    /// Select or clear the camera stored on the active profile
    Select {
        #[arg(long)]
        camera_id: Option<String>,
    },
    /// Capture a snapshot frame for the active or specified camera
    Capture {
        #[arg(long)]
        camera: Option<String>,
        #[arg(long)]
        output: Option<String>,
    },
    /// Inspect the agent-facing camera overlay state
    State,
    /// Control and render the camera overlay
    Overlay {
        #[command(subcommand)]
        command: CameraOverlayCmd,
    },
    /// Solve a camera calibration model from point pairs
    Calibrate {
        #[arg(long)]
        camera: Option<String>,
        #[arg(long)]
        points: String,
        #[arg(long, default_value_t = false)]
        save: bool,
    },
    /// Solve a camera alignment transform from point pairs
    Align {
        #[arg(long)]
        camera: Option<String>,
        #[arg(long)]
        points: String,
        #[arg(long, default_value_t = false)]
        save: bool,
    },
    /// Reset the saved camera calibration on the active profile
    ResetCalibration {
        #[arg(long)]
        camera: Option<String>,
    },
    /// Reset the saved camera alignment on the active profile
    ResetAlignment {
        #[arg(long)]
        camera: Option<String>,
    },
}

#[derive(Subcommand)]
enum CameraOverlayCmd {
    /// Render the current camera overlay canvas to a PNG
    Render {
        #[arg(long)]
        output: Option<String>,
        #[arg(long, value_enum, default_value_t = CameraOverlayViewArg::Fit)]
        view: CameraOverlayViewArg,
        #[arg(long, default_value_t = false)]
        keep: bool,
    },
    /// Show the camera overlay
    Show,
    /// Hide the camera overlay
    Hide,
    /// Set camera overlay opacity, 0.0 to 1.0
    Opacity { value: f64 },
    /// Fit the current frame to the bed as a draft transform
    FitToBed,
    /// Discard unsaved draft overlay edits
    Discard,
    /// Save the current draft transform as camera alignment
    SaveAlignment,
    /// Set absolute transform fields, preserving omitted values
    SetTransform {
        #[arg(long)]
        x: Option<f64>,
        #[arg(long)]
        y: Option<f64>,
        #[arg(long)]
        scale: Option<f64>,
        #[arg(long)]
        rotation_deg: Option<f64>,
    },
    /// Translate the overlay in workspace millimeters
    Nudge {
        #[arg(long)]
        dx: f64,
        #[arg(long)]
        dy: f64,
    },
    /// Multiply the current overlay scale by a factor
    Scale {
        #[arg(long)]
        factor: f64,
    },
    /// Add rotation in degrees
    Rotate {
        #[arg(long)]
        deg: f64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum CameraOverlayViewArg {
    Fit,
    Current,
}

impl CameraOverlayViewArg {
    fn as_api_str(self) -> &'static str {
        match self {
            CameraOverlayViewArg::Fit => "fit",
            CameraOverlayViewArg::Current => "current",
        }
    }
}

#[derive(Subcommand)]
enum PreviewCmd {
    /// Generate preview data for a project file
    Generate {
        /// Path to .lzrproj file
        input: String,
    },
    /// Show execution plan statistics for a project
    Stats {
        /// Path to .lzrproj file
        input: String,
    },
}

#[derive(Subcommand)]
enum VectorCmd {
    /// Convert an object into a vector path in the current project
    ConvertToPath { object_id: String },
    /// Boolean-union two objects in the current project
    BooleanUnion { object_a: String, object_b: String },
    /// Boolean-subtract object B from object A in the current project
    BooleanSubtract { object_a: String, object_b: String },
    /// Group objects in the current project
    Group { object_ids: Vec<String> },
    /// Ungroup a group object in the current project
    Ungroup { group_id: String },
    /// Fetch the editable path for a vector object
    EditablePath { object_id: String },
    /// Update a node or handle in a vector path
    UpdateNode {
        object_id: String,
        #[arg(long)]
        subpath: usize,
        #[arg(long)]
        command: usize,
        #[arg(long)]
        x: f64,
        #[arg(long)]
        y: f64,
        #[arg(long)]
        handle: Option<String>,
    },
    /// Delete a node from a vector path
    DeleteNode {
        object_id: String,
        #[arg(long)]
        subpath: usize,
        #[arg(long)]
        command: usize,
    },
    /// Insert a node on a segment
    InsertNode {
        object_id: String,
        #[arg(long)]
        subpath: usize,
        #[arg(long)]
        command: usize,
        #[arg(long)]
        t: f64,
    },
    /// Scale a vector path to new bounds
    ScaleToBounds {
        object_id: String,
        #[arg(long)]
        min_x: f64,
        #[arg(long)]
        min_y: f64,
        #[arg(long)]
        max_x: f64,
        #[arg(long)]
        max_y: f64,
    },
    /// Normalize selected objects for planner use
    Normalize { object_ids: Vec<String> },
}

#[derive(Subcommand)]
enum DesignCmd {
    /// Describe the current app canvas
    Describe,
    /// Print the agent design operation schema
    Schema,
    /// Render the current canvas to a visual PNG or SVG artifact
    Render {
        /// Write visual SVG to this path.
        #[arg(long)]
        svg: Option<String>,
        /// Write visual PNG to this path. If both --svg and --png are omitted, writes a temporary PNG.
        #[arg(long)]
        png: Option<String>,
        /// Export selection only
        #[arg(long, default_value_t = false)]
        selection_only: bool,
        /// Object IDs to render (used with --selection-only)
        #[arg(long, value_delimiter = ',')]
        selected_ids: Vec<String>,
        /// PNG resolution in pixels per millimeter
        #[arg(long, default_value_t = 4.0)]
        pixels_per_mm: f64,
    },
    /// Delete temporary design render artifacts
    CleanupRenders {
        /// Delete all temporary design renders regardless of age
        #[arg(long, default_value_t = false)]
        all: bool,
        /// Delete temporary renders older than this many hours
        #[arg(long, default_value_t = 24)]
        older_than_hours: u64,
    },
    /// Dry-run a design transaction plan
    Plan {
        /// Path to the transaction plan JSON file
        plan: String,
    },
    /// Apply a design transaction plan as one undoable mutation
    Apply {
        /// Path to the transaction plan JSON file
        plan: String,
        /// Retry while another design transaction is running
        #[arg(long)]
        wait_ms: Option<u64>,
    },
}

#[derive(Subcommand)]
enum DiagnosticsCmd {
    /// Export version, platform, and arch info as JSON
    Export {
        /// Output file path (prints to stdout if omitted)
        #[arg(short, long)]
        output: Option<String>,
    },
}

#[derive(Subcommand)]
enum FeedbackCmd {
    /// Save a diagnostic feedback report to a local JSON or ZIP file
    Save {
        #[command(flatten)]
        report: FeedbackReportArgs,
        /// Output report path. Defaults to beambench-report-{kind}-{timestamp}.json or .zip.
        #[arg(short, long)]
        output: Option<String>,
    },
    /// Submit a diagnostic feedback report to the Beam Bench feedback endpoint
    Submit {
        #[command(flatten)]
        report: FeedbackReportArgs,
        /// Feedback API endpoint. Defaults to BEAMBENCH_FEEDBACK_ENDPOINT or production.
        #[arg(long)]
        endpoint: Option<String>,
    },
    /// Print the current connection diagnostics snapshot
    Diagnostics {
        /// Optional .lzrproj path to include project metadata in the diagnostic context
        #[arg(long)]
        project: Option<String>,
    },
}

#[derive(Args, Debug, Clone)]
struct FeedbackReportArgs {
    /// Report kind
    #[arg(long, value_enum, default_value = "bug")]
    kind: CliFeedbackKind,
    /// Optional report title
    #[arg(long)]
    title: Option<String>,
    /// Report description. Required for bug and crash reports.
    #[arg(long)]
    description: Option<String>,
    /// Optional internal note or connectivity note
    #[arg(long)]
    notes: Option<String>,
    /// Optional reply-to email address
    #[arg(long)]
    reply_to: Option<String>,
    /// Optional .lzrproj path to load for project metadata or attachment
    #[arg(long)]
    project: Option<String>,
    /// Include the loaded project as an attachment
    #[arg(long)]
    include_project: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum CliFeedbackKind {
    Bug,
    Connectivity,
    Crash,
}

impl From<CliFeedbackKind> for FeedbackKind {
    fn from(value: CliFeedbackKind) -> Self {
        match value {
            CliFeedbackKind::Bug => FeedbackKind::Bug,
            CliFeedbackKind::Connectivity => FeedbackKind::Connectivity,
            CliFeedbackKind::Crash => FeedbackKind::Crash,
        }
    }
}

#[derive(Subcommand)]
enum ProfileCmd {
    /// List all machine profiles
    List,
    /// Show details of a machine profile by id or name
    Show {
        /// Profile id or name
        lookup: String,
    },
    /// List built-in profile presets
    Presets,
    /// Suggest a profile preset from the connected machine firmware identity
    Suggest,
    /// Preview the field diff for applying a preset
    PresetDiff {
        /// Preset ID
        preset_id: String,
        /// Profile id or name; defaults to active profile
        #[arg(long)]
        profile: Option<String>,
    },
    /// Apply a machine profile preset
    ApplyPreset {
        /// Preset ID
        preset_id: String,
        /// Profile id or name; defaults to active profile
        #[arg(long)]
        profile: Option<String>,
        /// Confirm the field diff and apply it
        #[arg(long, default_value_t = false)]
        confirm_diff: bool,
    },
    /// Create a machine profile in local settings
    Create {
        #[arg(long)]
        name: String,
        #[arg(long, default_value_t = 200.0)]
        bed_width_mm: f64,
        #[arg(long, default_value_t = 200.0)]
        bed_height_mm: f64,
        #[arg(long, default_value_t = 3000.0)]
        max_speed_mm_min: f64,
        #[arg(long, default_value_t = 100.0)]
        max_power_percent: f64,
        #[arg(long, default_value_t = 1000)]
        s_value_max: u32,
        #[arg(long, default_value_t = false)]
        homing_enabled: bool,
        #[arg(long, default_value_t = 115200)]
        default_baud_rate: u32,
        #[arg(long, default_value = "grbl")]
        firmware_type: String,
        #[arg(long, default_value = "")]
        notes: String,
        #[arg(long, default_value = "top_left")]
        origin: String,
        #[arg(long, default_value_t = 0.0)]
        laser_offset_x: f64,
        #[arg(long, default_value_t = 0.0)]
        laser_offset_y: f64,
        #[arg(long, default_value_t = false)]
        enable_laser_offset: bool,
        #[arg(long, default_value_t = false)]
        swap_xy: bool,
        #[arg(long, default_value_t = false)]
        job_checklist: bool,
        #[arg(long, default_value_t = false)]
        frame_continuously: bool,
        #[arg(long, default_value_t = 0.0)]
        tab_pulse_width_ms: f64,
        #[arg(long, default_value_t = false)]
        cnc_machine: bool,
        #[arg(long)]
        selected_camera_id: Option<String>,
        #[arg(long, default_value_t = false)]
        use_constant_power: bool,
        #[arg(long, default_value_t = false)]
        emit_s_every_g1: bool,
        #[arg(long, default_value_t = true)]
        use_g0_for_overscan: bool,
        #[arg(long, default_value_t = false)]
        enable_scanning_offset: bool,
        #[arg(long = "scanning-offset")]
        scanning_offsets: Vec<String>,
        #[arg(long, default_value_t = 0.0)]
        dot_width_mm: f64,
        #[arg(long, default_value_t = false)]
        enable_dot_width: bool,
    },
    /// Update a machine profile in local settings
    Update {
        /// Profile id or name
        lookup: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        bed_width_mm: Option<f64>,
        #[arg(long)]
        bed_height_mm: Option<f64>,
        #[arg(long)]
        max_speed_mm_min: Option<f64>,
        #[arg(long)]
        max_power_percent: Option<f64>,
        #[arg(long)]
        s_value_max: Option<u32>,
        #[arg(long)]
        homing_enabled: Option<bool>,
        #[arg(long)]
        default_baud_rate: Option<u32>,
        #[arg(long)]
        firmware_type: Option<String>,
        #[arg(long)]
        notes: Option<String>,
        #[arg(long)]
        origin: Option<String>,
        #[arg(long)]
        laser_offset_x: Option<f64>,
        #[arg(long)]
        laser_offset_y: Option<f64>,
        #[arg(long)]
        enable_laser_offset: Option<bool>,
        #[arg(long)]
        swap_xy: Option<bool>,
        #[arg(long)]
        job_checklist: Option<bool>,
        #[arg(long)]
        frame_continuously: Option<bool>,
        #[arg(long)]
        tab_pulse_width_ms: Option<f64>,
        #[arg(long)]
        cnc_machine: Option<bool>,
        #[arg(long)]
        selected_camera_id: Option<String>,
        #[arg(long)]
        use_constant_power: Option<bool>,
        #[arg(long)]
        emit_s_every_g1: Option<bool>,
        #[arg(long)]
        use_g0_for_overscan: Option<bool>,
        #[arg(long)]
        enable_scanning_offset: Option<bool>,
        #[arg(long = "scanning-offset")]
        scanning_offsets: Option<Vec<String>>,
        #[arg(long)]
        dot_width_mm: Option<f64>,
        #[arg(long)]
        enable_dot_width: Option<bool>,
    },
    /// Delete a machine profile from local settings
    Delete {
        /// Profile id or name
        lookup: String,
    },
    /// Activate a machine profile in local settings
    Activate {
        /// Profile id or name
        lookup: String,
    },
    /// Clear the active machine profile in local settings
    Deactivate,
    /// Bootstrap a machine profile from a discovery candidate
    Bootstrap {
        candidate_id: String,
        #[arg(long)]
        profile_name: Option<String>,
        #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
        activate: bool,
    },
}

#[derive(Subcommand)]
enum AssetCmd {
    /// List assets in a project file
    List {
        /// Path to .lzrproj file
        path: String,
    },
    /// Export an asset's binary data to a file
    Export {
        /// Path to .lzrproj file
        path: String,
        /// Asset UUID
        asset_id: String,
        /// Output file path
        #[arg(short, long)]
        output: String,
    },
    /// Import an asset file into a project
    Import {
        /// Path to .lzrproj file
        project: String,
        /// Path to asset file to import
        file: String,
        /// Output path for updated project (defaults to overwriting input)
        #[arg(short, long)]
        output: Option<String>,
    },
}

#[derive(Subcommand)]
enum JobCmd {
    /// Run preflight against the current project after opening the given file via the local API
    Preflight {
        /// Path to .lzrproj file
        input: String,
    },
    /// Full blocking workflow: connect, load, plan, preflight, stream, wait
    Run {
        /// Path to .lzrproj file
        input: String,
        /// Serial port name
        #[arg(short, long)]
        port: String,
        /// Baud rate
        #[arg(short, long, default_value = "115200")]
        baud: u32,
    },
    /// Load, plan, generate G-code, report stats (no machine needed)
    DryRun {
        /// Path to .lzrproj file
        input: String,
    },
    /// Pause the active job via the local HTTP API
    Pause,
    /// Resume the active job via the local HTTP API
    Resume,
    /// Cancel the active job via the local HTTP API
    Cancel,
    /// Show current job progress via the local HTTP API
    Progress,
    /// Start a framing pass via the local HTTP API
    Frame {
        /// Optionally open a project before framing
        #[arg(short, long)]
        input: Option<String>,
        /// Frame mode to use
        #[arg(long, default_value = "rectangular")]
        mode: String,
        /// Frame only the specified object ids
        #[arg(long, value_delimiter = ',')]
        selected_ids: Vec<String>,
        /// Request low-power laser-on framing
        #[arg(long, default_value_t = false)]
        laser_on: bool,
    },
}

#[derive(Subcommand)]
enum ConsoleCmd {
    /// Send a G-code line to the active machine session
    Send {
        /// G-code line to send
        line: String,
    },
    /// Show console log entries
    Log {
        /// Maximum number of entries to show
        #[arg(long, default_value = "100")]
        limit: usize,
    },
    /// Interactive console mode: read G-code lines from stdin, send each to the machine, print responses. Exit on EOF or "exit"/"quit".
    Interactive,
}

#[derive(Subcommand)]
enum MaterialCmd {
    /// List all material presets
    List,
    /// Add a new material preset
    Add {
        #[arg(long)]
        name: String,
        #[arg(long)]
        material: String,
        #[arg(long)]
        speed: f64,
        #[arg(long)]
        power: f64,
        #[arg(long)]
        passes: usize,
    },
    /// Remove a material preset by ID
    Remove {
        /// Material preset ID (UUID)
        id: String,
    },
}

#[derive(Subcommand)]
enum MacroCmd {
    /// List all macros
    List,
    /// Add a new macro
    Add {
        #[arg(long)]
        name: String,
        #[arg(long)]
        description: String,
        /// G-code commands (comma-separated)
        #[arg(long)]
        commands: String,
    },
    /// Remove a macro by ID
    Remove {
        /// Macro ID (UUID)
        id: String,
    },
    /// Run a macro by ID
    Run {
        /// Macro ID (UUID)
        id: String,
    },
}

#[derive(Subcommand)]
enum ImportCmd {
    /// Import DXF file into current project via local API
    Dxf {
        /// Path to DXF file
        file: String,
        /// Layer ID to import into
        #[arg(long)]
        layer: String,
    },
    /// Import PDF file into current project via local API
    Pdf {
        /// Path to PDF file
        file: String,
        /// Layer ID to import into
        #[arg(long)]
        layer: String,
    },
    /// Import AI file into current project via local API
    Ai {
        /// Path to AI (Adobe Illustrator) file
        file: String,
        /// Layer ID to import into
        #[arg(long)]
        layer: String,
    },
}

fn main() {
    let cli = Cli::parse();
    let json = cli.json;

    if let Err(e) = run(cli) {
        let exit_code = cli_exit_code(e.as_ref());
        if json {
            println!("{}", format_cli_error_json(e.as_ref()));
        } else {
            eprintln!("Error: {e}");
        }
        std::process::exit(exit_code);
    }
}

#[derive(Debug)]
struct CliExitError {
    code: i32,
    message: String,
    body: Option<Value>,
}

impl std::fmt::Display for CliExitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for CliExitError {}

fn cli_exit(code: i32, message: impl Into<String>) -> Box<dyn std::error::Error> {
    Box::new(CliExitError {
        code,
        message: message.into(),
        body: None,
    })
}

fn cli_exit_with_body(
    code: i32,
    message: impl Into<String>,
    body: Value,
) -> Box<dyn std::error::Error> {
    Box::new(CliExitError {
        code,
        message: message.into(),
        body: Some(body),
    })
}

fn confirmation_required_error(missing: &[&str]) -> Box<dyn std::error::Error> {
    let message = match missing {
        ["confirm_laser_on"] => {
            "This command can turn on the laser and requires explicit confirmation."
        }
        ["confirm_raw_gcode"] => {
            "This command sends raw G-code and requires explicit confirmation."
        }
        ["confirm_air_assist"] => {
            "This command activates air assist and requires explicit confirmation."
        }
        _ => "This command can move the machine and requires explicit confirmation.",
    };
    cli_exit_with_body(
        4,
        message,
        serde_json::json!({
            "error_code": "CONFIRMATION_REQUIRED",
            "missing": missing,
            "message": message,
        }),
    )
}

fn cli_exit_code(e: &(dyn std::error::Error + 'static)) -> i32 {
    e.downcast_ref::<CliExitError>()
        .map(|err| err.code)
        .unwrap_or(1)
}

fn format_cli_error_json(e: &(dyn std::error::Error + 'static)) -> serde_json::Value {
    if let Some(err) = e.downcast_ref::<CliExitError>() {
        if let Some(body) = &err.body {
            return body.clone();
        }
        return serde_json::json!({
            "error": {
                "code": err.code,
                "message": err.message,
            }
        });
    }
    serde_json::json!({
        "error": {
            "code": "cli_error",
            "message": e.to_string(),
        }
    })
}

fn parse_workspace_origin(
    value: &str,
) -> Result<beambench_core::WorkspaceOrigin, Box<dyn std::error::Error>> {
    match value {
        "top_left" => Ok(beambench_core::WorkspaceOrigin::TopLeft),
        "bottom_left" => Ok(beambench_core::WorkspaceOrigin::BottomLeft),
        _ => Err(format!("Invalid origin '{value}', expected top_left or bottom_left").into()),
    }
}

fn parse_scanning_offset_entry(
    value: &str,
) -> Result<beambench_core::ScanningOffsetEntry, Box<dyn std::error::Error>> {
    let (speed, offset) = value
        .split_once(':')
        .ok_or_else(|| format!("Invalid scanning offset '{value}', expected speed:offset"))?;
    Ok(beambench_core::ScanningOffsetEntry {
        speed_mm_min: speed.parse()?,
        offset_mm: offset.parse()?,
    })
}

fn handle_agent(cmd: AgentCmd, json: bool) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        AgentCmd::Capabilities => {
            let response = local_api_json_request(Method::GET, "/api/v1/agent/capabilities", None)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                let count = response["capabilities"]
                    .as_array()
                    .map(|items| items.len())
                    .unwrap_or(0);
                println!("Agent capabilities: {count}");
                println!("Use --json for the machine-readable registry.");
            }
        }
        AgentCmd::State => {
            let response = local_api_json_request(Method::GET, "/api/v1/agent/state", None)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                let project = response["project"]["name"].as_str().unwrap_or("No project");
                let warnings = response["warnings"]
                    .as_array()
                    .map(|items| items.len())
                    .unwrap_or(0);
                println!("Project: {project}");
                println!("Warnings: {warnings}");
                println!("Use --json for full state.");
            }
        }
        AgentCmd::Guide { markdown } => {
            if markdown {
                print!("{}", beambench_service::agent::guide_markdown());
            } else {
                let response = local_api_json_request(Method::GET, "/api/v1/agent/guide", None)?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&response)?);
                } else {
                    for step in response["bootstrap"].as_array().into_iter().flatten() {
                        let command = step["command"].as_str().unwrap_or("");
                        let purpose = step["purpose"].as_str().unwrap_or("");
                        println!("{command} - {purpose}");
                    }
                }
            }
        }
    }
    Ok(())
}

fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    let confirmations = ConfirmFlags {
        confirm_motion: cli.confirm_motion,
        confirm_laser_on: cli.confirm_laser_on,
        confirm_raw_gcode: cli.confirm_raw_gcode,
        confirm_air_assist: cli.confirm_air_assist,
    };
    match cli.command {
        Commands::Version => {
            if cli.json {
                println!(
                    "{}",
                    serde_json::json!({
                        "version": env!("CARGO_PKG_VERSION"),
                        "license": "GPL-3.0-or-later",
                        "source": "https://github.com/schmnr/beambench"
                    })
                );
            } else {
                println!("beambench {}", env!("CARGO_PKG_VERSION"));
                println!("License: GPL-3.0-or-later");
                println!("Source: https://github.com/schmnr/beambench");
            }
        }
        Commands::Ports => {
            let ports = beambench_serial::list_available_ports()
                .map_err(|e| format!("Failed to list ports: {e}"))?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&ports)?);
            } else if ports.is_empty() {
                println!("No serial ports found");
            } else {
                for port in &ports {
                    println!("{}", port.port_name);
                }
            }
        }
        Commands::Agent { command } => handle_agent(command, cli.json)?,
        Commands::Project { command } => handle_project(command, cli.json)?,
        Commands::Export { command } => handle_export(command, cli.json)?,
        Commands::Machine { command } => handle_machine(command, cli.json, confirmations)?,
        Commands::Camera { command } => handle_camera(command, cli.json)?,
        Commands::Preview { command } => handle_preview(command, cli.json)?,
        Commands::Vector { command } => handle_vector(command, cli.json)?,
        Commands::Design { command } => handle_design(command, cli.json)?,
        Commands::Diagnostics { command } => handle_diagnostics(command, cli.json)?,
        Commands::Feedback { command } => handle_feedback(command, cli.json)?,
        Commands::Profile { command } => handle_profile(command, cli.json)?,
        Commands::Asset { command } => handle_asset(command, cli.json)?,
        Commands::Job { command } => handle_job(command, cli.json, confirmations)?,
        Commands::Console { command } => handle_console(command, cli.json, confirmations)?,
        Commands::Material { command } => handle_material(command, cli.json)?,
        Commands::Macro { command } => handle_macro(command, cli.json)?,
        Commands::Import { command } => handle_import(command, cli.json)?,
    }
    Ok(())
}

fn handle_project(cmd: ProjectCmd, json: bool) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        ProjectCmd::Info { path } => {
            let project = beambench_project::load_project(std::path::Path::new(&path))?;
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "name": project.metadata.project_name,
                        "format_version": project.metadata.format_version,
                        "layers": project.layers.len(),
                        "objects": project.objects.len(),
                        "assets": project.assets.len(),
                    }))?
                );
            } else {
                println!("Project: {}", project.metadata.project_name);
                println!("Format version: {}", project.metadata.format_version);
                println!("Layers: {}", project.layers.len());
                println!("Objects: {}", project.objects.len());
                println!("Assets: {}", project.assets.len());
            }
        }
        ProjectCmd::Open { path } => {
            let response = local_api_json_request(
                Method::POST,
                "/api/v1/projects/open",
                Some(serde_json::json!({ "path": path })),
            )?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                println!(
                    "Opened project '{}'",
                    response["metadata"]["project_name"]
                        .as_str()
                        .unwrap_or("Unknown")
                );
            }
        }
        ProjectCmd::Create { name, output } => {
            let project = beambench_core::Project::new(&name);
            let output_path = output.unwrap_or_else(|| format!("{name}.lzrproj"));
            beambench_project::save_project(&project, std::path::Path::new(&output_path))?;
            if json {
                println!("{}", serde_json::json!({"path": output_path, "name": name}));
            } else {
                println!("Created project '{name}' at {output_path}");
            }
        }
        ProjectCmd::Save => {
            let response = local_api_json_request(Method::POST, "/api/v1/projects/save", None)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                println!(
                    "Saved project to {}",
                    response["path"].as_str().unwrap_or("current path")
                );
            }
        }
        ProjectCmd::SaveAs { path } => {
            let response = local_api_json_request(
                Method::POST,
                "/api/v1/projects/save-as",
                Some(serde_json::json!({ "path": path })),
            )?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                println!(
                    "Saved project to {}",
                    response["path"].as_str().unwrap_or("requested path")
                );
            }
        }
        ProjectCmd::Close => {
            let response = local_api_json_request(Method::POST, "/api/v1/projects/close", None)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                println!("Closed current project");
            }
        }
        ProjectCmd::Undo => {
            let response = local_api_json_request(Method::POST, "/api/v1/projects/undo", None)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                println!(
                    "Undo restored project '{}'",
                    response["metadata"]["project_name"]
                        .as_str()
                        .unwrap_or("Unknown")
                );
            }
        }
        ProjectCmd::Redo => {
            let response = local_api_json_request(Method::POST, "/api/v1/projects/redo", None)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                println!(
                    "Redo restored project '{}'",
                    response["metadata"]["project_name"]
                        .as_str()
                        .unwrap_or("Unknown")
                );
            }
        }
        ProjectCmd::ImportSvg { layer, file } => {
            let response = local_api_json_request(
                Method::POST,
                "/api/v1/projects/import/svg",
                Some(serde_json::json!({
                    "file_path": file,
                    "layer_id": layer,
                })),
            )?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                println!(
                    "Imported {} object(s) from SVG",
                    response["count"].as_u64().unwrap_or(0)
                );
            }
        }
        ProjectCmd::ImportImage { layer, file } => {
            let response = local_api_json_request(
                Method::POST,
                "/api/v1/projects/import/image",
                Some(serde_json::json!({
                    "file_path": file,
                    "layer_id": layer,
                })),
            )?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                println!(
                    "Imported image object '{}'",
                    response["name"].as_str().unwrap_or("Unknown")
                );
            }
        }
        ProjectCmd::ImportFiles { layer, files } => {
            let response = local_api_json_request(
                Method::POST,
                "/api/v1/projects/import/files",
                Some(serde_json::json!({
                    "file_paths": files,
                    "layer_id": layer,
                })),
            )?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                println!(
                    "Imported {} object(s) from file batch",
                    response["count"].as_u64().unwrap_or(0)
                );
            }
        }
    }
    Ok(())
}

fn handle_export(cmd: ExportCmd, json: bool) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        ExportCmd::Gcode { input, output } => {
            // 1. Load project
            let project = beambench_project::load_project(std::path::Path::new(&input))?;

            // 2. Build execution plan
            let plan = beambench_planner::build_plan(&project)?;

            // 3. Generate G-code
            let config = gcode_config_for_project(&project);
            let gcode_lines = beambench_grbl::generate_gcode(&plan, &config)?;

            // 4. Write to file
            let gcode_content = gcode_lines.join("\n");
            std::fs::write(&output, &gcode_content)?;

            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "output": output,
                        "lines": gcode_lines.len(),
                        "bytes": gcode_content.len(),
                    })
                );
            } else {
                println!(
                    "Exported {} lines of G-code to {}",
                    gcode_lines.len(),
                    output
                );
            }
        }
        ExportCmd::Svg {
            path,
            selection_only,
            selected_ids,
        } => {
            let response = local_api_json_request(
                Method::POST,
                "/api/v1/export/svg",
                Some(serde_json::json!({
                    "path": path,
                    "selection_only": selection_only,
                    "selected_ids": selected_ids,
                })),
            )?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                println!("SVG export requested");
            }
        }
        ExportCmd::Dxf {
            path,
            selection_only,
            selected_ids,
        } => {
            let response = local_api_json_request(
                Method::POST,
                "/api/v1/export/dxf",
                Some(serde_json::json!({
                    "path": path,
                    "selection_only": selection_only,
                    "selected_ids": selected_ids,
                })),
            )?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                println!("DXF export requested");
            }
        }
        ExportCmd::Pdf {
            path,
            selection_only,
            selected_ids,
        } => {
            let response = local_api_json_request(
                Method::POST,
                "/api/v1/export/pdf",
                Some(serde_json::json!({
                    "path": path,
                    "selection_only": selection_only,
                    "selected_ids": selected_ids,
                })),
            )?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                println!("PDF export requested");
            }
        }
        ExportCmd::Eps {
            path,
            selection_only,
            selected_ids,
        } => {
            let response = local_api_json_request(
                Method::POST,
                "/api/v1/export/eps",
                Some(serde_json::json!({
                    "path": path,
                    "selection_only": selection_only,
                    "selected_ids": selected_ids,
                })),
            )?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                println!("EPS export requested");
            }
        }
        ExportCmd::Ai {
            path,
            selection_only,
            selected_ids,
        } => {
            let response = local_api_json_request(
                Method::POST,
                "/api/v1/export/ai",
                Some(serde_json::json!({
                    "path": path,
                    "selection_only": selection_only,
                    "selected_ids": selected_ids,
                })),
            )?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                println!("AI export requested");
            }
        }
    }
    Ok(())
}

fn gcode_config_for_project(project: &beambench_core::Project) -> beambench_grbl::GcodeConfig {
    let mut config = beambench_grbl::GcodeConfig::default();
    // `build_plan` already includes finish-position travel. Avoid adding a
    // second postamble move to the same finish point.
    config.finish_position = beambench_core::FinishPosition::DontMove;
    config.air_assist_cut_entry_ids = project
        .layers
        .iter()
        .flat_map(|layer| layer.entries.iter())
        .filter(|entry| entry.air_assist)
        .map(|entry| entry.id.to_string())
        .collect();
    config
}

fn print_controller_connection_response(
    response: &Value,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if json {
        println!("{}", serde_json::to_string_pretty(response)?);
        return Ok(());
    }
    match response["status"].as_str() {
        Some("connected") => {
            let driver = response["choice"]["driver"]
                .as_str()
                .unwrap_or("controller");
            let endpoint = response["endpoint"]["type"]
                .as_str()
                .unwrap_or("connection");
            println!("Connected using {driver} over {endpoint}");
        }
        Some("challenge") => {
            let attempt_id = response["attempt_id"].as_str().unwrap_or("unknown");
            println!("Controller choice required for attempt {attempt_id}");
            println!("{}", serde_json::to_string_pretty(response)?);
            println!(
                "Continue with: beambench machine continue-controller {attempt_id} --controller <controller>"
            );
        }
        _ => println!("{}", serde_json::to_string_pretty(response)?),
    }
    Ok(())
}

fn handle_machine(
    cmd: MachineCmd,
    json: bool,
    confirmations: ConfirmFlags,
) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        MachineCmd::Connect { port, baud } => {
            let response = local_api_json_request(
                Method::POST,
                "/api/v1/machine/connect",
                Some(serde_json::json!({
                    "port": port,
                    "baud_rate": baud,
                })),
            )?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                println!("Machine connect requested successfully");
            }
        }
        MachineCmd::ConnectSerial {
            port,
            baud,
            controller,
        } => {
            let response = local_api_json_request(
                Method::POST,
                "/api/v1/machine/connect/controller/serial",
                Some(serde_json::json!({
                    "port_name": port,
                    "baud_rate": baud,
                    "selection": controller.selection_json(),
                })),
            )?;
            print_controller_connection_response(&response, json)?;
        }
        MachineCmd::ConnectNetwork {
            host,
            port,
            controller,
        } => {
            let response = local_api_json_request(
                Method::POST,
                "/api/v1/machine/connect/controller/network",
                Some(serde_json::json!({
                    "host": host,
                    "port": port.unwrap_or_else(|| controller.default_port()),
                    "selection": controller.selection_json(),
                })),
            )?;
            print_controller_connection_response(&response, json)?;
        }
        MachineCmd::ContinueController {
            attempt_id,
            controller,
            decision,
        } => {
            let response = local_api_json_request(
                Method::POST,
                "/api/v1/machine/connect/controller/continue",
                Some(serde_json::json!({
                    "attempt_id": attempt_id,
                    "selection": controller.selection_json(),
                    "decision": decision.map(ControllerDecisionArg::as_api_str),
                })),
            )?;
            print_controller_connection_response(&response, json)?;
        }
        MachineCmd::ConnectCandidate { candidate_id } => {
            let response = local_api_json_request(
                Method::POST,
                "/api/v1/machine/connect",
                Some(serde_json::json!({
                    "candidate_id": candidate_id,
                })),
            )?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                println!("Machine connect-by-candidate requested successfully");
            }
        }
        MachineCmd::ListLihuiyuUsb => {
            let response =
                local_api_json_request(Method::GET, "/api/v1/machine/usb/lihuiyu", None)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else if let Some(devices) = response.as_array() {
                if devices.is_empty() {
                    println!("No Lihuiyu M2/M3 Nano USB controllers found");
                } else {
                    for device in devices {
                        let bus = device["bus_id"].as_str().unwrap_or("unknown");
                        let address = device["device_address"].as_u64().unwrap_or(0);
                        let ports = device["port_numbers"]
                            .as_array()
                            .map(|items| {
                                items
                                    .iter()
                                    .filter_map(serde_json::Value::as_u64)
                                    .map(|value| value.to_string())
                                    .collect::<Vec<_>>()
                                    .join(".")
                            })
                            .unwrap_or_default();
                        let name = device["product"]
                            .as_str()
                            .or_else(|| device["manufacturer"].as_str())
                            .unwrap_or("CH341 USB");
                        println!("{name}: bus {bus}, address {address}, ports {ports}");
                    }
                }
            }
        }
        MachineCmd::ConnectLihuiyu {
            bus_id,
            device_address,
            port_numbers,
        } => {
            let response = local_api_json_request(
                Method::POST,
                "/api/v1/machine/connect/usb",
                Some(serde_json::json!({
                    "bus_id": bus_id,
                    "device_address": device_address,
                    "port_numbers": port_numbers,
                })),
            )?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                println!("Lihuiyu USB connection requested successfully");
            }
        }
        MachineCmd::Discover {
            tcp_targets,
            usb_targets,
        } => {
            let tcp_targets: Result<Vec<_>, _> = tcp_targets
                .into_iter()
                .map(|target| parse_tcp_target(&target))
                .collect();
            let usb_targets = usb_targets
                .into_iter()
                .map(|device_path| DiscoveryUsbTarget {
                    device_path,
                    manufacturer: None,
                    product: None,
                })
                .collect::<Vec<_>>();
            let response = local_api_json_request(
                Method::POST,
                "/api/v1/machine/discover",
                Some(serde_json::json!({
                    "tcp_targets": tcp_targets?,
                    "usb_targets": usb_targets,
                })),
            )?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                println!(
                    "Discovery completed with {} candidate(s)",
                    response["candidates"]
                        .as_array()
                        .map(|items| items.len())
                        .unwrap_or(0)
                );
            }
        }
        MachineCmd::Disconnect => {
            let response =
                local_api_json_request(Method::POST, "/api/v1/machine/disconnect", None)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                println!("Machine disconnect requested successfully");
            }
        }
        MachineCmd::Home => {
            let response = local_api_json_request(
                Method::POST,
                "/api/v1/machine/home",
                Some(serde_json::json!({
                    "confirm_motion": confirmations.confirm_motion,
                })),
            )?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                println!("Machine homing requested successfully");
            }
        }
        MachineCmd::Unlock => {
            let response = local_api_json_request(Method::POST, "/api/v1/machine/unlock", None)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                println!("Machine unlock requested successfully");
            }
        }
        MachineCmd::Jog { x_mm, y_mm, feed } => {
            let response = local_api_json_request(
                Method::POST,
                "/api/v1/machine/jog",
                Some(serde_json::json!({
                    "x_mm": x_mm,
                    "y_mm": y_mm,
                    "feed_rate": feed,
                    "confirm_motion": confirmations.confirm_motion,
                })),
            )?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                println!("Machine jog requested successfully");
            }
        }
        MachineCmd::SetOrigin => {
            let response =
                local_api_json_request(Method::POST, "/api/v1/machine/set-origin", None)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                println!("Work origin set successfully");
            }
        }
        MachineCmd::ResetOrigin => {
            let response =
                local_api_json_request(Method::POST, "/api/v1/machine/reset-origin", None)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                println!("Work origin reset successfully");
            }
        }
        MachineCmd::EmergencyStop => {
            let response =
                local_api_json_request(Method::POST, "/api/v1/machine/emergency-stop", None)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                println!("Emergency stop requested successfully");
            }
        }
        MachineCmd::Status => {
            let response = local_api_json_request(Method::GET, "/api/v1/machine/status", None)?;
            println!("{}", serde_json::to_string_pretty(&response)?);
        }
        MachineCmd::TestAir { duration_ms } => {
            if !confirmations.confirm_air_assist {
                return Err(confirmation_required_error(&["confirm_air_assist"]));
            }
            let response = local_api_json_request(
                Method::POST,
                "/api/v1/machine/test-air",
                Some(serde_json::json!({
                    "duration_ms": duration_ms,
                    "confirm_air_assist": confirmations.confirm_air_assist,
                })),
            )?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                println!("Air assist test completed");
            }
        }
    }
    Ok(())
}

fn handle_preview(cmd: PreviewCmd, json: bool) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        PreviewCmd::Generate { input } => {
            let project = beambench_project::load_project(std::path::Path::new(&input))?;
            let plan = beambench_planner::build_plan(&project)?;
            let preview = beambench_preview::distill_preview(&plan);

            if json {
                println!("{}", serde_json::to_string_pretty(&preview)?);
            } else {
                let vector_paths: usize = preview
                    .layers
                    .iter()
                    .map(|layer| layer.vector_paths.len())
                    .sum();
                let raster_regions: usize = preview
                    .layers
                    .iter()
                    .map(|layer| layer.raster_regions.len())
                    .sum();
                println!("Preview generated:");
                println!("  Layers: {}", preview.layers.len());
                println!("  Vector paths: {vector_paths}");
                println!("  Raster regions: {raster_regions}");
                println!("  Travel moves: {}", preview.travel_moves.len());
                println!(
                    "  Estimated duration: {:.1} s",
                    preview.stats.estimated_duration_secs
                );
            }
        }
        PreviewCmd::Stats { input } => {
            let project = beambench_project::load_project(std::path::Path::new(&input))?;
            let plan = beambench_planner::build_plan(&project)?;

            let segment_count = plan.segments.len();
            let travel_distance_mm = plan.total_distance_mm;
            let estimated_duration_secs = plan.estimated_duration_secs;

            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "segments": segment_count,
                        "travel_distance_mm": travel_distance_mm,
                        "estimated_duration_secs": estimated_duration_secs,
                    }))?
                );
            } else {
                println!("Segments: {segment_count}");
                println!("Travel distance: {travel_distance_mm:.2} mm");
                println!("Estimated duration: {estimated_duration_secs:.1} s");
            }
        }
    }
    Ok(())
}

fn load_json_file<T: DeserializeOwned>(path: &str) -> Result<T, Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read JSON file '{path}': {e}"))?;
    serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse JSON file '{path}': {e}").into())
}

fn camera_doctor_report() -> Value {
    let settings_file = settings_path().ok();
    let settings_result = load_settings();
    let settings_error = settings_result.as_ref().err().map(|e| e.to_string());
    let settings = settings_result.ok().flatten();
    let effective_settings = settings.clone().unwrap_or_default();
    let api_base_url = {
        let host = if effective_settings.api_localhost_only {
            "127.0.0.1"
        } else {
            "localhost"
        };
        format!("http://{host}:{}", effective_settings.api_port)
    };

    let mut checks = Vec::new();
    let mut next_steps = Vec::new();
    let mut notes = Vec::new();

    if let Some(error) = settings_error.as_ref() {
        checks.push(camera_doctor_check(
            "settings.read",
            "fail",
            format!("Could not read local settings: {error}"),
        ));
        next_steps
            .push("Fix or remove the Beam Bench settings file, then reopen the app.".to_string());
    } else {
        checks.push(camera_doctor_check(
            "settings.read",
            "ok",
            "Settings file loaded or defaults are available",
        ));
    }

    if effective_settings.api_enabled {
        checks.push(camera_doctor_check(
            "api.enabled",
            "ok",
            "Local API is enabled in effective settings",
        ));
    } else {
        checks.push(camera_doctor_check(
            "api.enabled",
            "fail",
            "Local API is disabled in settings",
        ));
        next_steps.push(
            "Enable Settings > General > Local API, or enable it in settings.json.".to_string(),
        );
    }

    let agent_state = local_api_json_request_with_status(Method::GET, "/api/v1/agent/state", None)
        .map_err(|e| e.to_string());
    let api_reachable = matches!(agent_state, Ok((status, _)) if status.is_success());
    if api_reachable {
        checks.push(camera_doctor_check(
            "api.reachable",
            "ok",
            format!("Local API responded at {api_base_url}"),
        ));
    } else {
        let error = match &agent_state {
            Ok((status, body)) => local_api_error_message(*status, "/api/v1/agent/state", body),
            Err(error) => error.clone(),
        };
        checks.push(camera_doctor_check(
            "api.reachable",
            "fail",
            format!("Local API is not reachable at {api_base_url}: {error}"),
        ));
        next_steps.push(
            "Open Beam Bench and leave it running; agent camera capture/render requires the app."
                .to_string(),
        );
    }

    let camera_state = if api_reachable {
        local_api_json_request_with_status(Method::GET, "/api/v1/camera/state", None)
            .ok()
            .and_then(|(status, body)| status.is_success().then_some(body))
    } else {
        None
    };
    let camera_devices = if api_reachable {
        local_api_json_request_with_status(Method::GET, "/api/v1/camera/devices", None)
            .ok()
            .and_then(|(status, body)| status.is_success().then_some(body))
    } else {
        None
    };

    let frontend_bridge_connected = camera_state
        .as_ref()
        .and_then(|state| state["frontend_bridge_connected"].as_bool())
        .unwrap_or(false);
    if frontend_bridge_connected {
        checks.push(camera_doctor_check(
            "frontend.bridge",
            "ok",
            "Frontend camera bridge is connected",
        ));
    } else if api_reachable {
        checks.push(camera_doctor_check(
            "frontend.bridge",
            "fail",
            "Frontend camera bridge is not connected",
        ));
        next_steps.push("Bring the Beam Bench window forward and keep it open before capture or overlay render.".to_string());
    }

    let reported_devices = camera_devices
        .as_ref()
        .and_then(|value| value["devices"].as_array())
        .cloned()
        .unwrap_or_default();
    let native_device_count = reported_devices
        .iter()
        .filter(|device| device["backend_kind"].as_str() == Some("native"))
        .count();
    let mock_device_count = reported_devices.len().saturating_sub(native_device_count);
    if api_reachable {
        if native_device_count > 0 {
            checks.push(camera_doctor_check(
                "camera.devices",
                "ok",
                format!("{native_device_count} physical camera device(s) reported"),
            ));
        } else {
            let detail = if mock_device_count > 0 {
                format!(
                    "No physical cameras were reported; {mock_device_count} development mock camera(s) are available"
                )
            } else {
                "No physical camera devices were reported".to_string()
            };
            checks.push(camera_doctor_check("camera.devices", "warn", detail));
            next_steps.push(
                "Check camera connection and macOS Camera permission for the Beam Bench app."
                    .to_string(),
            );
        }
    }

    if cfg!(target_os = "macos") {
        notes.push(
            "On macOS, real-camera capture must be granted to the Beam Bench .app bundle; the CLI cannot grant Camera permission."
                .to_string(),
        );
    }

    let ok = checks
        .iter()
        .all(|check| check["status"].as_str() != Some("fail"));

    serde_json::json!({
        "schema_version": CAMERA_AGENT_SCHEMA_VERSION,
        "ok": ok,
        "settings": {
            "path": settings_file.map(|path| path.to_string_lossy().to_string()),
            "present": settings.is_some(),
            "error": settings_error,
            "api_enabled": effective_settings.api_enabled,
            "api_port": effective_settings.api_port,
            "api_localhost_only": effective_settings.api_localhost_only,
        },
        "api": {
            "base_url": api_base_url,
            "reachable": api_reachable,
        },
        "frontend": {
            "camera_bridge_connected": frontend_bridge_connected,
        },
        "camera": {
            "device_count": reported_devices.len(),
            "physical_device_count": native_device_count,
            "mock_device_count": mock_device_count,
            "selected_camera_id": camera_state.as_ref().and_then(|state| state["selected_camera_id"].as_str()),
            "has_frame": camera_state.as_ref().and_then(|state| state["frame"].as_object()).is_some(),
            "overlay_status": camera_state.as_ref().and_then(|state| state["display"]["status"].as_str()),
            "overlay_visible": camera_state.as_ref().and_then(|state| state["display"]["overlay_visible"].as_bool()),
        },
        "checks": checks,
        "next_steps": next_steps,
        "notes": notes,
    })
}

fn camera_doctor_check(
    id: impl Into<String>,
    status: impl Into<String>,
    message: impl Into<String>,
) -> Value {
    serde_json::json!({
        "id": id.into(),
        "status": status.into(),
        "message": message.into(),
    })
}

fn print_camera_doctor_summary(report: &Value) {
    let status = if report["ok"].as_bool().unwrap_or(false) {
        "ok"
    } else {
        "needs attention"
    };
    println!("Camera doctor: {status}");
    if let Some(checks) = report["checks"].as_array() {
        for check in checks {
            println!(
                "- {}: {}",
                check["id"].as_str().unwrap_or("check"),
                check["message"].as_str().unwrap_or("")
            );
        }
    }
    if let Some(next_steps) = report["next_steps"].as_array()
        && !next_steps.is_empty()
    {
        println!("Next steps:");
        for step in next_steps {
            println!("- {}", step.as_str().unwrap_or(""));
        }
    }
    if let Some(notes) = report["notes"].as_array()
        && !notes.is_empty()
    {
        println!("Notes:");
        for note in notes {
            println!("- {}", note.as_str().unwrap_or(""));
        }
    }
}

fn handle_camera(cmd: CameraCmd, json: bool) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        CameraCmd::Doctor => {
            let report = camera_doctor_report();
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                print_camera_doctor_summary(&report);
            }
        }
        CameraCmd::List => {
            let response = local_api_json_request(Method::GET, "/api/v1/camera/devices", None)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                let devices = response["devices"].as_array().cloned().unwrap_or_default();
                if devices.is_empty() {
                    println!("No camera devices found");
                } else {
                    for device in devices {
                        println!(
                            "{} [{}] {}x{}",
                            device["display_name"].as_str().unwrap_or("Unknown"),
                            device["camera_id"].as_str().unwrap_or("unknown"),
                            device["width_px"].as_u64().unwrap_or(0),
                            device["height_px"].as_u64().unwrap_or(0),
                        );
                    }
                }
            }
        }
        CameraCmd::Select { camera_id } => {
            let response = local_api_json_request(
                Method::POST,
                "/api/v1/camera/select",
                Some(serde_json::json!({ "camera_id": camera_id })),
            )?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else if let Some(camera_id) = response["selected_camera_id"].as_str() {
                println!("Selected camera {camera_id}");
            } else {
                println!("Cleared selected camera");
            }
        }
        CameraCmd::Capture { camera, output } => {
            let response = local_api_json_request(
                Method::POST,
                "/api/v1/camera/capture",
                Some(serde_json::json!({ "camera_id": camera, "output_path": output })),
            )?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                let handle = response["frame"]["handle_id"]
                    .as_str()
                    .unwrap_or("unknown handle");
                let file_path = response["latest_capture"]["path"]
                    .as_str()
                    .or_else(|| response["frame"]["file_path"].as_str())
                    .unwrap_or("unknown file");
                println!("Captured frame {handle} to {file_path}",);
            }
        }
        CameraCmd::State => {
            let response = local_api_json_request(Method::GET, "/api/v1/camera/state", None)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                print_camera_state_summary(&response);
            }
        }
        CameraCmd::Overlay { command } => handle_camera_overlay(command, json)?,
        CameraCmd::Calibrate {
            camera,
            points,
            save,
        } => {
            let points: beambench_common::CalibrationPointSet = load_json_file(&points)?;
            let response = local_api_json_request(
                Method::POST,
                "/api/v1/camera/calibration/solve",
                Some(serde_json::json!({
                    "camera_id": camera.clone().ok_or("camera id is required for calibration")?,
                    "points": points,
                })),
            )?;
            if save {
                let saved = local_api_json_request(
                    Method::POST,
                    "/api/v1/camera/calibration/save",
                    Some(serde_json::json!({
                        "camera_id": camera,
                        "calibration": response["calibration"].clone(),
                    })),
                )?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&saved)?);
                } else {
                    println!(
                        "Saved camera calibration with quality {:.3}",
                        saved["quality_score"].as_f64().unwrap_or(0.0)
                    );
                }
            } else if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                println!(
                    "Solved camera calibration with quality {:.3}",
                    response["calibration"]["quality_score"]
                        .as_f64()
                        .unwrap_or(0.0)
                );
            }
        }
        CameraCmd::Align {
            camera,
            points,
            save,
        } => {
            let points: beambench_common::AlignmentPointSet = load_json_file(&points)?;
            let response = local_api_json_request(
                Method::POST,
                "/api/v1/camera/alignment/solve",
                Some(serde_json::json!({
                    "camera_id": camera,
                    "points": points,
                })),
            )?;
            if save {
                let saved = local_api_json_request(
                    Method::POST,
                    "/api/v1/camera/alignment/save",
                    Some(serde_json::json!({
                        "camera_id": camera,
                        "alignment": response.clone(),
                    })),
                )?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&saved)?);
                } else {
                    println!(
                        "Saved camera alignment with quality {:.3}",
                        saved["quality_score"].as_f64().unwrap_or(0.0)
                    );
                }
            } else if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                println!(
                    "Solved camera alignment with quality {:.3}",
                    response["quality_score"].as_f64().unwrap_or(0.0)
                );
            }
        }
        CameraCmd::ResetCalibration { camera } => {
            let response = local_api_json_request(
                Method::POST,
                "/api/v1/camera/calibration/reset",
                Some(serde_json::json!({ "camera_id": camera })),
            )?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                println!("Camera calibration reset");
            }
        }
        CameraCmd::ResetAlignment { camera } => {
            let response = local_api_json_request(
                Method::POST,
                "/api/v1/camera/alignment/reset",
                Some(serde_json::json!({ "camera_id": camera })),
            )?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                println!("Camera alignment reset");
            }
        }
    }
    Ok(())
}

fn handle_camera_overlay(
    cmd: CameraOverlayCmd,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        CameraOverlayCmd::Render { output, view, keep } => {
            let (path, temporary, cleaned) = if let Some(output) = output {
                (output, false, 0)
            } else {
                let cache_dir = camera_render_cache_dir();
                let cleaned = cleanup_camera_render_artifacts(&cache_dir)?;
                let path = camera_render_temp_path(&cache_dir)?;
                (path.to_string_lossy().to_string(), !keep, cleaned)
            };
            let response = local_api_json_request(
                Method::POST,
                "/api/v1/camera/overlay/render",
                Some(serde_json::json!({
                    "output_path": path,
                    "view": view.as_api_str(),
                    "keep": keep || !temporary,
                })),
            )?;
            let output = serde_json::json!({
                "schema_version": CAMERA_AGENT_SCHEMA_VERSION,
                "format": "png",
                "path": response["path"].as_str().unwrap_or(""),
                "view": response["view"].as_str().unwrap_or(view.as_api_str()),
                "temporary": response["temporary"].as_bool().unwrap_or(temporary),
                "bytes": response["bytes"].as_u64().unwrap_or(0),
                "cleaned": cleaned,
            });
            if json {
                println!("{}", serde_json::to_string_pretty(&output)?);
            } else {
                println!(
                    "Rendered camera overlay: {}",
                    output["path"].as_str().unwrap_or("")
                );
            }
        }
        CameraOverlayCmd::Show => {
            print_overlay_state_response(
                update_camera_overlay_display(Some(true), None, None)?,
                json,
            )?;
        }
        CameraOverlayCmd::Hide => {
            print_overlay_state_response(
                update_camera_overlay_display(Some(false), None, None)?,
                json,
            )?;
        }
        CameraOverlayCmd::Opacity { value } => {
            print_overlay_state_response(
                update_camera_overlay_display(None, Some(value), None)?,
                json,
            )?;
        }
        CameraOverlayCmd::FitToBed => {
            let response =
                local_api_json_request(Method::POST, "/api/v1/camera/overlay/fit-to-bed", None)?;
            print_overlay_state_response(response, json)?;
        }
        CameraOverlayCmd::Discard => {
            let response =
                local_api_json_request(Method::POST, "/api/v1/camera/overlay/discard", None)?;
            print_overlay_state_response(response, json)?;
        }
        CameraOverlayCmd::SaveAlignment => {
            let response = local_api_json_request(
                Method::POST,
                "/api/v1/camera/overlay/save-alignment",
                None,
            )?;
            print_overlay_state_response(response, json)?;
        }
        CameraOverlayCmd::SetTransform {
            x,
            y,
            scale,
            rotation_deg,
        } => {
            let response = local_api_json_request(
                Method::POST,
                "/api/v1/camera/overlay/transform",
                Some(serde_json::json!({
                    "set": {
                        "x": x,
                        "y": y,
                        "scale": scale,
                        "rotation_deg": rotation_deg,
                    }
                })),
            )?;
            print_overlay_state_response(response, json)?;
        }
        CameraOverlayCmd::Nudge { dx, dy } => {
            let response = local_api_json_request(
                Method::POST,
                "/api/v1/camera/overlay/transform",
                Some(serde_json::json!({ "nudge": { "dx": dx, "dy": dy } })),
            )?;
            print_overlay_state_response(response, json)?;
        }
        CameraOverlayCmd::Scale { factor } => {
            let response = local_api_json_request(
                Method::POST,
                "/api/v1/camera/overlay/transform",
                Some(serde_json::json!({ "scale_factor": factor })),
            )?;
            print_overlay_state_response(response, json)?;
        }
        CameraOverlayCmd::Rotate { deg } => {
            let response = local_api_json_request(
                Method::POST,
                "/api/v1/camera/overlay/transform",
                Some(serde_json::json!({ "rotate_deg": deg })),
            )?;
            print_overlay_state_response(response, json)?;
        }
    }
    Ok(())
}

fn update_camera_overlay_display(
    overlay_visible: Option<bool>,
    overlay_opacity: Option<f64>,
    overlay_adjust_mode: Option<bool>,
) -> Result<Value, Box<dyn std::error::Error>> {
    local_api_json_request(
        Method::PATCH,
        "/api/v1/camera/overlay/display",
        Some(serde_json::json!({
            "overlay_visible": overlay_visible,
            "overlay_opacity": overlay_opacity,
            "overlay_adjust_mode": overlay_adjust_mode,
        })),
    )
}

fn print_overlay_state_response(
    response: Value,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if json {
        println!("{}", serde_json::to_string_pretty(&response)?);
    } else {
        print_camera_state_summary(&response);
    }
    Ok(())
}

fn print_camera_state_summary(response: &Value) {
    let status = response["display"]["status"].as_str().unwrap_or("unknown");
    let visible = response["display"]["overlay_visible"]
        .as_bool()
        .unwrap_or(false);
    let opacity = response["display"]["overlay_opacity"]
        .as_f64()
        .unwrap_or(0.0);
    let frame = response["frame"]["file_path"].as_str().unwrap_or("none");
    println!("Camera overlay: {status}; visible={visible}; opacity={opacity:.2}; frame={frame}");
}

fn camera_render_cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("beam-bench")
        .join("camera-renders")
}

fn camera_render_temp_path(cache_dir: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    std::fs::create_dir_all(cache_dir)
        .map_err(|e| format!("Failed to create camera render cache directory: {e}"))?;
    Ok(cache_dir.join(format!("{CAMERA_RENDER_TEMP_PREFIX}latest.png")))
}

fn cleanup_camera_render_artifacts(cache_dir: &Path) -> Result<usize, Box<dyn std::error::Error>> {
    if !cache_dir.exists() {
        return Ok(0);
    }
    let mut deleted = 0usize;
    for entry in std::fs::read_dir(cache_dir)
        .map_err(|e| format!("Failed to read camera render cache directory: {e}"))?
    {
        let entry = entry.map_err(|e| format!("Failed to read camera render cache entry: {e}"))?;
        let path = entry.path();
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if file_name.starts_with(CAMERA_RENDER_TEMP_PREFIX)
            && path.extension().and_then(|ext| ext.to_str()) == Some("png")
        {
            std::fs::remove_file(&path)
                .map_err(|e| format!("Failed to delete camera render artifact: {e}"))?;
            deleted += 1;
        }
    }
    Ok(deleted)
}

fn handle_vector(cmd: VectorCmd, json: bool) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        VectorCmd::ConvertToPath { object_id } => {
            let response = local_api_json_request(
                Method::POST,
                "/api/v1/vector/convert-to-path",
                Some(serde_json::json!({ "object_id": object_id })),
            )?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                println!(
                    "Converted '{}' to a vector path",
                    response["name"].as_str().unwrap_or("object")
                );
            }
        }
        VectorCmd::BooleanUnion { object_a, object_b } => {
            let response = local_api_json_request(
                Method::POST,
                "/api/v1/vector/boolean/union",
                Some(serde_json::json!({
                    "object_id_a": object_a,
                    "object_id_b": object_b,
                })),
            )?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                println!(
                    "Created union object '{}'",
                    response["name"].as_str().unwrap_or("Union")
                );
            }
        }
        VectorCmd::BooleanSubtract { object_a, object_b } => {
            let response = local_api_json_request(
                Method::POST,
                "/api/v1/vector/boolean/subtract",
                Some(serde_json::json!({
                    "object_id_a": object_a,
                    "object_id_b": object_b,
                })),
            )?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                println!(
                    "Created subtract object '{}'",
                    response["name"].as_str().unwrap_or("Subtract")
                );
            }
        }
        VectorCmd::Group { object_ids } => {
            let response = local_api_json_request(
                Method::POST,
                "/api/v1/vector/group",
                Some(serde_json::json!({ "object_ids": object_ids })),
            )?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                println!(
                    "Created group '{}'",
                    response["name"].as_str().unwrap_or("Group")
                );
            }
        }
        VectorCmd::Ungroup { group_id } => {
            let response = local_api_json_request(
                Method::POST,
                "/api/v1/vector/ungroup",
                Some(serde_json::json!({ "object_id": group_id })),
            )?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                println!(
                    "Ungrouped {} child object(s)",
                    response["count"].as_u64().unwrap_or(0)
                );
            }
        }
        VectorCmd::EditablePath { object_id } => {
            let response = local_api_json_request(
                Method::GET,
                &format!("/api/v1/vector/{object_id}/editable-path"),
                None,
            )?;
            println!("{}", serde_json::to_string_pretty(&response)?);
        }
        VectorCmd::UpdateNode {
            object_id,
            subpath,
            command,
            x,
            y,
            handle,
        } => {
            let response = local_api_json_request(
                Method::POST,
                &format!("/api/v1/vector/{object_id}/nodes/update"),
                Some(serde_json::json!({
                    "subpath_idx": subpath,
                    "command_idx": command,
                    "x": x,
                    "y": y,
                    "handle_type": handle,
                })),
            )?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                println!(
                    "Updated node on '{}'",
                    response["name"].as_str().unwrap_or("object")
                );
            }
        }
        VectorCmd::DeleteNode {
            object_id,
            subpath,
            command,
        } => {
            let response = local_api_json_request(
                Method::POST,
                &format!("/api/v1/vector/{object_id}/nodes/delete"),
                Some(serde_json::json!({
                    "subpath_idx": subpath,
                    "command_idx": command,
                })),
            )?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                println!(
                    "Deleted node on '{}'",
                    response["name"].as_str().unwrap_or("object")
                );
            }
        }
        VectorCmd::InsertNode {
            object_id,
            subpath,
            command,
            t,
        } => {
            let response = local_api_json_request(
                Method::POST,
                &format!("/api/v1/vector/{object_id}/nodes/insert"),
                Some(serde_json::json!({
                    "subpath_idx": subpath,
                    "command_idx": command,
                    "t": t,
                })),
            )?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                println!(
                    "Inserted node on '{}'",
                    response["name"].as_str().unwrap_or("object")
                );
            }
        }
        VectorCmd::ScaleToBounds {
            object_id,
            min_x,
            min_y,
            max_x,
            max_y,
        } => {
            let response = local_api_json_request(
                Method::POST,
                &format!("/api/v1/vector/{object_id}/scale-to-bounds"),
                Some(serde_json::json!({
                    "min_x": min_x,
                    "min_y": min_y,
                    "max_x": max_x,
                    "max_y": max_y,
                })),
            )?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                println!(
                    "Scaled '{}' to new bounds",
                    response["name"].as_str().unwrap_or("object")
                );
            }
        }
        VectorCmd::Normalize { object_ids } => {
            let response = local_api_json_request(
                Method::POST,
                "/api/v1/vector/normalize",
                Some(serde_json::json!({ "object_ids": object_ids })),
            )?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                println!(
                    "Normalized {} object(s) for planner use",
                    response["count"].as_u64().unwrap_or(0)
                );
            }
        }
    }
    Ok(())
}

fn handle_design(cmd: DesignCmd, json: bool) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        DesignCmd::Describe => {
            let response = design_api_json_request(Method::GET, "/api/v1/design/describe", None)?;
            print_design_describe(&response, json)?;
        }
        DesignCmd::Schema => {
            let response = design_api_json_request(Method::GET, "/api/v1/design/schema", None)?;
            print_design_schema(&response, json)?;
        }
        DesignCmd::Render {
            svg,
            png,
            selection_only,
            selected_ids,
            pixels_per_mm,
        } => {
            render_design(svg, png, selection_only, selected_ids, pixels_per_mm, json)?;
        }
        DesignCmd::CleanupRenders {
            all,
            older_than_hours,
        } => {
            cleanup_design_renders_cmd(all, older_than_hours, json)?;
        }
        DesignCmd::Plan { plan } => {
            let plan = read_design_plan(&plan)?;
            let (status, response) = design_api_json_response(
                Method::POST,
                "/api/v1/design/transaction/plan",
                Some(plan),
            )?;
            finish_design_transaction(status, response, json)?;
        }
        DesignCmd::Apply { plan, wait_ms } => {
            let plan = read_design_plan(&plan)?;
            let (status, response) = apply_design_transaction(plan, wait_ms)?;
            finish_design_transaction(status, response, json)?;
        }
    }
    Ok(())
}

fn render_design(
    svg: Option<String>,
    png: Option<String>,
    selection_only: bool,
    selected_ids: Vec<String>,
    pixels_per_mm: f64,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if svg.is_some() && png.is_some() {
        return Err(cli_exit(3, "Use only one of --svg or --png"));
    }
    let format = if svg.is_some() { "svg" } else { "png" };
    let explicit_path = svg.or(png);
    let (path, temporary, cleaned) = if let Some(path) = explicit_path {
        (path, false, 0)
    } else {
        let cache_dir = design_render_cache_dir();
        let cleaned = cleanup_design_render_artifacts(
            &cache_dir,
            DESIGN_RENDER_DEFAULT_MAX_AGE,
            false,
            SystemTime::now(),
        )?;
        let path = design_render_temp_path(&cache_dir, format)?;
        (path.to_string_lossy().to_string(), true, cleaned)
    };

    let response = design_api_json_request(
        Method::POST,
        "/api/v1/design/render",
        Some(serde_json::json!({
            "format": format,
            "path": path,
            "selection_only": selection_only,
            "selected_ids": selected_ids,
            "pixels_per_mm": pixels_per_mm,
        })),
    )?;
    let bytes = response
        .get("bytes")
        .and_then(Value::as_u64)
        .or_else(|| std::fs::metadata(&path).ok().map(|meta| meta.len()))
        .unwrap_or(0);

    let output = serde_json::json!({
        "schema_version": DESIGN_RENDER_SCHEMA_VERSION,
        "format": format,
        "path": path,
        "temporary": temporary,
        "bytes": bytes,
        "cleaned": cleaned,
    });
    if json {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else if temporary {
        println!(
            "Rendered {}: {} (temporary; old temp renders cleaned automatically)",
            format.to_uppercase(),
            output["path"].as_str().unwrap_or("")
        );
    } else {
        println!(
            "Rendered {}: {}",
            format.to_uppercase(),
            output["path"].as_str().unwrap_or("")
        );
    }
    Ok(())
}

fn cleanup_design_renders_cmd(
    all: bool,
    older_than_hours: u64,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let cache_dir = design_render_cache_dir();
    let older_than = Duration::from_secs(older_than_hours.saturating_mul(60 * 60));
    let deleted = cleanup_design_render_artifacts(&cache_dir, older_than, all, SystemTime::now())?;
    let output = serde_json::json!({
        "schema_version": DESIGN_RENDER_SCHEMA_VERSION,
        "directory": cache_dir,
        "deleted": deleted,
        "all": all,
        "older_than_hours": if all { Value::Null } else { Value::from(older_than_hours) },
    });
    if json {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("Deleted {deleted} temporary design render artifact(s)");
    }
    Ok(())
}

fn design_render_cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("beam-bench")
        .join("design-renders")
}

fn design_render_temp_path(
    cache_dir: &Path,
    extension: &str,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    std::fs::create_dir_all(cache_dir)
        .map_err(|e| format!("Failed to create design render cache directory: {e}"))?;
    Ok(cache_dir.join(format!(
        "{DESIGN_RENDER_TEMP_PREFIX}{}.{}",
        uuid::Uuid::new_v4(),
        extension
    )))
}

fn cleanup_design_render_artifacts(
    cache_dir: &Path,
    older_than: Duration,
    all: bool,
    now: SystemTime,
) -> Result<usize, Box<dyn std::error::Error>> {
    if !cache_dir.exists() {
        return Ok(0);
    }

    let mut deleted = 0;
    for entry in std::fs::read_dir(cache_dir)
        .map_err(|e| format!("Failed to read design render cache directory: {e}"))?
    {
        let entry = entry.map_err(|e| format!("Failed to read design render cache entry: {e}"))?;
        let path = entry.path();
        if !is_design_render_artifact(&path) {
            continue;
        }
        let should_delete = if all {
            true
        } else {
            let modified = entry
                .metadata()
                .and_then(|metadata| metadata.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            now.duration_since(modified).unwrap_or_default() >= older_than
        };
        if should_delete {
            std::fs::remove_file(&path)
                .map_err(|e| format!("Failed to delete design render artifact: {e}"))?;
            deleted += 1;
        }
    }
    Ok(deleted)
}

fn is_design_render_artifact(path: &Path) -> bool {
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
    file_name.starts_with(DESIGN_RENDER_TEMP_PREFIX)
        && matches!(extension.to_ascii_lowercase().as_str(), "svg" | "png")
}

fn read_design_plan(path: &str) -> Result<Value, Box<dyn std::error::Error>> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| cli_exit(2, format!("Failed to read design plan '{path}': {e}")))?;
    let value: Value = serde_json::from_str(&text)
        .map_err(|e| cli_exit(3, format!("Malformed design plan JSON in '{path}': {e}")))?;
    let Some(schema_version) = value.get("schema_version") else {
        return Err(cli_exit(3, "Design plan is missing schema_version"));
    };
    if schema_version.as_u64() != Some(1) {
        return Err(cli_exit(
            3,
            format!("Unsupported design schema version '{schema_version}'"),
        ));
    }
    Ok(value)
}

fn design_api_json_request(
    method: Method,
    path: &str,
    body: Option<Value>,
) -> Result<Value, Box<dyn std::error::Error>> {
    let (status, response) = design_api_json_response(method, path, body)?;
    if status.is_success() {
        Ok(response)
    } else {
        Err(cli_exit_with_body(
            2,
            format!("Design API request failed ({status}) for {path}"),
            response,
        ))
    }
}

fn design_api_json_response(
    method: Method,
    path: &str,
    body: Option<Value>,
) -> Result<(StatusCode, Value), Box<dyn std::error::Error>> {
    local_api_json_request_with_status(method, path, body).map_err(|e| cli_exit(2, e.to_string()))
}

fn apply_design_transaction(
    plan: Value,
    wait_ms: Option<u64>,
) -> Result<(StatusCode, Value), Box<dyn std::error::Error>> {
    let wait_ms = wait_ms.unwrap_or(0);
    let started = std::time::Instant::now();
    loop {
        let (status, response) = design_api_json_response(
            Method::POST,
            "/api/v1/design/transaction/apply",
            Some(plan.clone()),
        )?;
        let is_busy = status == StatusCode::CONFLICT
            && response.pointer("/error/code").and_then(Value::as_str) == Some("BUSY");
        if !is_busy || wait_ms == 0 {
            return Ok((status, response));
        }

        let elapsed_ms = started.elapsed().as_millis() as u64;
        if elapsed_ms >= wait_ms {
            return Ok((status, response));
        }
        let remaining_ms = wait_ms.saturating_sub(elapsed_ms);
        std::thread::sleep(std::time::Duration::from_millis(remaining_ms.min(100)));
    }
}

fn finish_design_transaction(
    status: StatusCode,
    response: Value,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if !status.is_success() || response.get("error").is_some_and(|error| !error.is_null()) {
        let message = design_error_message(&response)
            .unwrap_or_else(|| format!("Design transaction failed with status {status}"));
        if json {
            return Err(cli_exit_with_body(1, message, response));
        }
        return Err(cli_exit(1, message));
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&response)?);
        return Ok(());
    }

    let summary = &response["summary"];
    let action = if response["applied"].as_bool().unwrap_or(false) {
        "Applied"
    } else {
        "Plan valid"
    };
    println!(
        "{}: {} operation(s); created {}, modified {}, deleted {}.",
        action,
        summary["op_count"].as_u64().unwrap_or(0),
        json_array_len(summary.get("created_object_ids")),
        json_array_len(summary.get("modified_object_ids")),
        json_array_len(summary.get("deleted_object_ids"))
    );
    if let Some(transaction_id) = response["transaction_id"].as_str() {
        println!("Transaction: {transaction_id}");
    }
    print_design_warnings(&response);
    Ok(())
}

fn print_design_describe(response: &Value, json: bool) -> Result<(), Box<dyn std::error::Error>> {
    if json {
        println!("{}", serde_json::to_string_pretty(response)?);
        return Ok(());
    }

    let Some(project) = response.get("project").filter(|value| !value.is_null()) else {
        println!("No active project");
        print_design_warnings(response);
        return Ok(());
    };
    let name = project
        .pointer("/metadata/project_name")
        .and_then(Value::as_str)
        .unwrap_or("Untitled");
    let object_count = json_array_len(project.get("objects"));
    let layer_count = json_array_len(project.get("layers"));
    let bed_width = project
        .pointer("/workspace/bed_width_mm")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let bed_height = project
        .pointer("/workspace/bed_height_mm")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    println!("{name}: {object_count} object(s), {layer_count} layer(s)");
    println!("Bed: {bed_width:.2} x {bed_height:.2} mm");
    print_design_warnings(response);
    Ok(())
}

fn print_design_schema(response: &Value, json: bool) -> Result<(), Box<dyn std::error::Error>> {
    if json {
        println!("{}", serde_json::to_string_pretty(response)?);
        return Ok(());
    }
    println!(
        "Design schema v{}: {} operation(s)",
        response["schema_version"].as_u64().unwrap_or(0),
        json_array_len(response.get("operations"))
    );
    println!("Use --json for the full operation contract.");
    Ok(())
}

fn print_design_warnings(response: &Value) {
    if let Some(warnings) = response.get("warnings").and_then(Value::as_array) {
        if !warnings.is_empty() {
            println!("Warnings:");
            for warning in warnings {
                println!("  - {}", warning.as_str().unwrap_or("<unknown>"));
            }
        }
    }
}

fn design_error_message(response: &Value) -> Option<String> {
    let error = response.get("error")?;
    let message = error.get("message").and_then(Value::as_str)?;
    let code = error.get("code").and_then(Value::as_str).unwrap_or("ERROR");
    match error.get("op_index").and_then(Value::as_u64) {
        Some(index) => Some(format!("{code} at operation {index}: {message}")),
        None => Some(format!("{code}: {message}")),
    }
}

fn json_array_len(value: Option<&Value>) -> usize {
    value.and_then(Value::as_array).map(Vec::len).unwrap_or(0)
}

fn handle_diagnostics(cmd: DiagnosticsCmd, json: bool) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        DiagnosticsCmd::Export { output } => {
            let diag = serde_json::json!({
                "version": env!("CARGO_PKG_VERSION"),
                "platform": std::env::consts::OS,
                "arch": std::env::consts::ARCH,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            });

            let content = serde_json::to_string_pretty(&diag)?;

            if let Some(path) = output {
                std::fs::write(&path, &content)?;
                if json {
                    println!(
                        "{}",
                        serde_json::json!({"path": path, "bytes": content.len()})
                    );
                } else {
                    println!("Diagnostics written to {path}");
                }
            } else {
                // No output path — print to stdout
                println!("{content}");
            }
        }
    }
    Ok(())
}

fn handle_feedback(cmd: FeedbackCmd, json: bool) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        FeedbackCmd::Save { report, output } => {
            let ctx = feedback_context(report.project.as_deref())?;
            let input = feedback_input_from_args(&report)?;
            let path = output.map(PathBuf::from).unwrap_or_else(|| {
                PathBuf::from(beambench_service::ops::feedback::default_report_filename(
                    input.kind,
                    input.include_project_file,
                ))
            });
            let saved = beambench_service::ops::feedback::save_feedback_report(&ctx, input, &path)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&saved)?);
            } else {
                println!(
                    "Feedback report saved to {} ({} bytes)",
                    saved.path, saved.size_bytes
                );
            }
        }
        FeedbackCmd::Submit { report, endpoint } => {
            let ctx = feedback_context(report.project.as_deref())?;
            let input = feedback_input_from_args(&report)?;
            let transport = beambench_service::ops::feedback::build_submit_request_for_transport(
                &ctx,
                input.clone(),
            )?;
            let body = transport.body;
            let bundle = transport.bundle;
            let submitted = submit_feedback_body(body, feedback_endpoint(endpoint))?;
            beambench_service::ops::feedback::record_feedback_submission(
                &input,
                submitted.report_id.clone(),
            )?;
            beambench_service::ops::feedback::delete_included_panic_files(
                &ctx,
                &bundle.recent_panics,
            );
            if json {
                println!("{}", serde_json::to_string_pretty(&submitted)?);
            } else {
                println!("Feedback report submitted: {}", submitted.report_id);
            }
        }
        FeedbackCmd::Diagnostics { project } => {
            let ctx = feedback_context(project.as_deref())?;
            let snapshot = beambench_service::ops::feedback::get_connection_diagnostics(&ctx)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&snapshot)?);
            } else {
                println!("Diagnostics captured at {}", snapshot.captured_at);
                println!("Machine state: {:?}", snapshot.machine.session_state);
                if snapshot.ports_detected.is_empty() {
                    println!("Detected serial ports: none");
                } else {
                    println!("Detected serial ports:");
                    for port in snapshot.ports_detected {
                        let vid = port.vendor_id.as_deref().unwrap_or("-");
                        let pid = port.product_id.as_deref().unwrap_or("-");
                        println!(
                            "  {} vid={} pid={} in_use_by_beambench={} available={}",
                            port.name, vid, pid, port.in_use_by_beambench, port.available
                        );
                    }
                }
            }
        }
    }
    Ok(())
}

fn feedback_context(
    project_path: Option<&str>,
) -> Result<beambench_service::ServiceContext, Box<dyn std::error::Error>> {
    let ctx = beambench_service::ServiceContext::new();
    if let Some(path) = project_path {
        beambench_service::ops::persistence::open_project_from_path(&ctx, path)?;
    }
    Ok(ctx)
}

fn feedback_input_from_args(
    args: &FeedbackReportArgs,
) -> Result<FeedbackReportInput, Box<dyn std::error::Error>> {
    if args.include_project && args.project.is_none() {
        return Err(cli_exit(
            3,
            "--include-project requires --project <path-to-project.lzrproj>",
        ));
    }
    Ok(FeedbackReportInput {
        kind: args.kind.into(),
        title: optional_cli_string(args.title.as_deref()),
        description: optional_cli_string(args.description.as_deref()),
        notes: optional_cli_string(args.notes.as_deref()),
        reply_to_email: optional_cli_string(args.reply_to.as_deref()),
        include_project_file: args.include_project,
        source_context: Some(FeedbackSourceContext {
            source: "cli".to_owned(),
            error_message: None,
            stack: None,
            feature: Some("beambench-cli".to_owned()),
            correlation_ts: Some(chrono::Utc::now().to_rfc3339()),
        }),
    })
}

fn optional_cli_string(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn feedback_endpoint(endpoint: Option<String>) -> String {
    endpoint
        .or_else(|| std::env::var("BEAMBENCH_FEEDBACK_ENDPOINT").ok())
        .unwrap_or_else(|| "https://beambench.com/api/feedback/report".to_owned())
}

fn submit_feedback_body(
    body: Vec<u8>,
    endpoint: String,
) -> Result<SubmitFeedbackResponse, Box<dyn std::error::Error>> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| cli_exit(2, format!("Failed to initialize feedback client: {e}")))?;
    let response = client
        .post(&endpoint)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(body)
        .send()
        .map_err(|e| {
            if e.is_timeout() {
                cli_exit_with_body(
                    2,
                    "Feedback submission timed out. Save the report to a file and try again later.",
                    serde_json::json!({
                        "error": {
                            "code": "feedback_timeout",
                            "message": "Feedback submission timed out. Save the report to a file and try again later.",
                        }
                    }),
                )
            } else {
                cli_exit(2, format!("Failed to submit feedback report to {endpoint}: {e}"))
            }
        })?;
    let status = response.status();
    let text = response.text().unwrap_or_default();
    if !status.is_success() {
        return Err(feedback_status_error(status, text));
    }
    serde_json::from_str(&text)
        .map_err(|e| cli_exit(2, format!("Invalid feedback response from {endpoint}: {e}")))
}

fn feedback_status_error(status: StatusCode, text: String) -> Box<dyn std::error::Error> {
    let parsed = serde_json::from_str::<Value>(&text).ok();
    let code = if status == StatusCode::TOO_MANY_REQUESTS {
        "rate_limited"
    } else if status == StatusCode::PAYLOAD_TOO_LARGE {
        "too_large"
    } else if status.is_client_error() {
        "feedback_rejected"
    } else {
        "feedback_server_error"
    };
    let message = if status == StatusCode::TOO_MANY_REQUESTS {
        "Too many reports recently. Save the report to a file and try again later.".to_owned()
    } else if status == StatusCode::PAYLOAD_TOO_LARGE {
        "Feedback report is too large. Save the report to a file instead.".to_owned()
    } else if status.is_client_error() {
        format!("Feedback report was rejected by the server ({status}).")
    } else {
        format!("Feedback server returned {status}. Save the report to a file and try again later.")
    };
    let body = parsed.unwrap_or_else(|| {
        serde_json::json!({
            "error": {
                "code": code,
                "message": message,
            },
            "status": status.as_u16(),
        })
    });
    cli_exit_with_body(if status.is_server_error() { 2 } else { 1 }, message, body)
}

fn handle_profile(cmd: ProfileCmd, json: bool) -> Result<(), Box<dyn std::error::Error>> {
    let ctx = beambench_service::ServiceContext::new();

    match cmd {
        ProfileCmd::Presets => {
            let response = local_api_json_request(Method::GET, "/api/v1/profiles/presets", None)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                for preset in response["presets"].as_array().into_iter().flatten() {
                    println!(
                        "{} - {}",
                        preset["id"].as_str().unwrap_or("unknown"),
                        preset["name"].as_str().unwrap_or("Unnamed")
                    );
                }
            }
        }
        ProfileCmd::Suggest => {
            let response =
                local_api_json_request(Method::GET, "/api/v1/profiles/suggestion", None)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else if let Some(suggestion) = response["suggestion"].as_str() {
                println!("Suggested preset: {suggestion}");
            } else {
                println!(
                    "No preset suggestion ({})",
                    response["reason"].as_str().unwrap_or("unknown")
                );
            }
        }
        ProfileCmd::PresetDiff { preset_id, profile } => {
            let profile_id = resolve_api_profile_id(profile.as_deref())?;
            let response = local_api_json_request(
                Method::GET,
                &format!("/api/v1/profiles/{profile_id}/preset-diff/{preset_id}"),
                None,
            )?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                print_profile_diff(&response["diff"]);
            }
        }
        ProfileCmd::ApplyPreset {
            preset_id,
            profile,
            confirm_diff,
        } => {
            let profile_id = resolve_api_profile_id(profile.as_deref())?;
            let response = local_api_json_request(
                Method::POST,
                &format!("/api/v1/profiles/{profile_id}/preset"),
                Some(serde_json::json!({
                    "preset_id": preset_id,
                    "confirm_diff": confirm_diff,
                })),
            )?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                print_profile_diff(&response["diff"]);
                println!(
                    "Preset applied to profile {}",
                    response["profile"]["name"].as_str().unwrap_or("unknown")
                );
            }
        }
        ProfileCmd::List => {
            let profiles = beambench_service::ops::profiles::list_profiles(&ctx)?;
            let active_profile_id = beambench_service::ops::profiles::get_active_profile_id(&ctx)?;
            if json {
                let profiles: Vec<_> = profiles
                    .iter()
                    .map(|p| {
                        serde_json::json!({
                            "id": p.id.to_string(),
                            "name": p.name,
                            "preset_id": p.preset_id,
                            "preset_version": p.preset_version,
                            "bed_width_mm": p.bed_width_mm,
                            "bed_height_mm": p.bed_height_mm,
                            "active": active_profile_id == Some(p.id),
                        })
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&profiles)?);
            } else if profiles.is_empty() {
                println!("No machine profiles configured");
            } else {
                for profile in &profiles {
                    println!(
                        "{}{} ({}x{} mm)",
                        if active_profile_id == Some(profile.id) {
                            "* "
                        } else {
                            "  "
                        },
                        profile.name,
                        profile.bed_width_mm,
                        profile.bed_height_mm
                    );
                }
            }
        }
        ProfileCmd::Show { lookup } => {
            let profile = resolve_profile_lookup(&ctx, &lookup)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&profile)?);
            } else {
                println!("ID: {}", profile.id);
                println!("Name: {}", profile.name);
                println!(
                    "Bed size: {}x{} mm",
                    profile.bed_width_mm, profile.bed_height_mm
                );
                println!("Max speed: {} mm/min", profile.max_speed_mm_min);
                println!("Max power: {}%", profile.max_power_percent);
                println!(
                    "Homing: {}",
                    if profile.homing_enabled { "yes" } else { "no" }
                );
                println!("Baud rate: {}", profile.default_baud_rate);
                println!("Firmware: {}", profile.firmware_type);
                if !profile.notes.is_empty() {
                    println!("Notes: {}", profile.notes);
                }
            }
        }
        ProfileCmd::Create {
            name,
            bed_width_mm,
            bed_height_mm,
            max_speed_mm_min,
            max_power_percent,
            s_value_max,
            homing_enabled,
            default_baud_rate,
            firmware_type,
            notes,
            origin,
            laser_offset_x,
            laser_offset_y,
            enable_laser_offset,
            swap_xy,
            job_checklist,
            frame_continuously,
            tab_pulse_width_ms,
            cnc_machine,
            selected_camera_id,
            use_constant_power,
            emit_s_every_g1,
            use_g0_for_overscan,
            enable_scanning_offset,
            scanning_offsets,
            dot_width_mm,
            enable_dot_width,
        } => {
            let scanning_offsets = scanning_offsets
                .iter()
                .map(|value| parse_scanning_offset_entry(value))
                .collect::<Result<Vec<_>, _>>()?;
            let profile = beambench_service::ops::profiles::save_profile(
                &ctx,
                beambench_service::ops::profiles::SaveProfileInput {
                    profile_id: None,
                    name,
                    preset_id: None,
                    preset_version: None,
                    bed_width_mm,
                    bed_height_mm,
                    max_speed_mm_min,
                    max_power_percent,
                    s_value_max,
                    homing_enabled,
                    default_baud_rate,
                    firmware_type,
                    notes,
                    selected_camera_id,
                    camera_calibration: None,
                    camera_alignment: None,
                    origin: parse_workspace_origin(&origin)?,
                    laser_offset_x,
                    laser_offset_y,
                    enable_laser_offset,
                    swap_xy,
                    job_checklist,
                    frame_continuously,
                    laser_on_when_framing: false,
                    tab_pulse_width_ms,
                    cnc_machine,
                    use_constant_power,
                    emit_s_every_g1,
                    use_g0_for_overscan,
                    air_assist_on_gcode: "M7".to_string(),
                    air_assist_off_gcode: "M9".to_string(),
                    air_assist_on_delay_ms: 0,
                    job_header_gcode: String::new(),
                    job_footer_gcode: String::new(),
                    transfer_mode: beambench_core::TransferMode::Buffered,
                    preferred_default_origin: None,
                    scanning_offsets,
                    enable_scanning_offset,
                    dot_width_mm,
                    enable_dot_width,
                    supports_z_moves: false,
                    z_move_feed_mm_min: 300.0,
                    ruida_table_axis: beambench_core::RuidaTableAxis::Disabled,
                    enable_laser_fire_button: false,
                    default_fire_power_percent: 1.0,
                    quality_test_settings: Default::default(),
                },
            )?;
            if json {
                println!("{}", serde_json::to_string_pretty(&profile)?);
            } else {
                println!("Created profile '{}' ({})", profile.name, profile.id);
            }
        }
        ProfileCmd::Update {
            lookup,
            name,
            bed_width_mm,
            bed_height_mm,
            max_speed_mm_min,
            max_power_percent,
            s_value_max,
            homing_enabled,
            default_baud_rate,
            firmware_type,
            notes,
            origin,
            laser_offset_x,
            laser_offset_y,
            enable_laser_offset,
            swap_xy,
            job_checklist,
            frame_continuously,
            tab_pulse_width_ms,
            cnc_machine,
            selected_camera_id,
            use_constant_power,
            emit_s_every_g1,
            use_g0_for_overscan,
            enable_scanning_offset,
            scanning_offsets,
            dot_width_mm,
            enable_dot_width,
        } => {
            let existing = resolve_profile_lookup(&ctx, &lookup)?;
            let scanning_offsets = scanning_offsets
                .map(|entries| {
                    entries
                        .iter()
                        .map(|value| parse_scanning_offset_entry(value))
                        .collect::<Result<Vec<_>, _>>()
                })
                .transpose()?;
            let profile = beambench_service::ops::profiles::save_profile(
                &ctx,
                beambench_service::ops::profiles::SaveProfileInput {
                    profile_id: Some(existing.id),
                    name: name.unwrap_or(existing.name),
                    preset_id: existing.preset_id,
                    preset_version: existing.preset_version,
                    bed_width_mm: bed_width_mm.unwrap_or(existing.bed_width_mm),
                    bed_height_mm: bed_height_mm.unwrap_or(existing.bed_height_mm),
                    max_speed_mm_min: max_speed_mm_min.unwrap_or(existing.max_speed_mm_min),
                    max_power_percent: max_power_percent.unwrap_or(existing.max_power_percent),
                    s_value_max: s_value_max.unwrap_or(existing.s_value_max),
                    homing_enabled: homing_enabled.unwrap_or(existing.homing_enabled),
                    default_baud_rate: default_baud_rate.unwrap_or(existing.default_baud_rate),
                    firmware_type: firmware_type.unwrap_or(existing.firmware_type),
                    notes: notes.unwrap_or(existing.notes),
                    selected_camera_id: selected_camera_id.or(existing.selected_camera_id),
                    camera_calibration: existing.camera_calibration,
                    camera_alignment: existing.camera_alignment,
                    origin: origin
                        .as_deref()
                        .map(parse_workspace_origin)
                        .transpose()?
                        .unwrap_or(existing.origin),
                    laser_offset_x: laser_offset_x.unwrap_or(existing.laser_offset_x),
                    laser_offset_y: laser_offset_y.unwrap_or(existing.laser_offset_y),
                    enable_laser_offset: enable_laser_offset
                        .unwrap_or(existing.enable_laser_offset),
                    swap_xy: swap_xy.unwrap_or(existing.swap_xy),
                    job_checklist: job_checklist.unwrap_or(existing.job_checklist),
                    frame_continuously: frame_continuously.unwrap_or(existing.frame_continuously),
                    laser_on_when_framing: existing.laser_on_when_framing,
                    tab_pulse_width_ms: tab_pulse_width_ms.unwrap_or(existing.tab_pulse_width_ms),
                    cnc_machine: cnc_machine.unwrap_or(existing.cnc_machine),
                    use_constant_power: use_constant_power.unwrap_or(existing.use_constant_power),
                    emit_s_every_g1: emit_s_every_g1.unwrap_or(existing.emit_s_every_g1),
                    use_g0_for_overscan: use_g0_for_overscan
                        .unwrap_or(existing.use_g0_for_overscan),
                    air_assist_on_gcode: existing.air_assist_on_gcode,
                    air_assist_off_gcode: existing.air_assist_off_gcode,
                    air_assist_on_delay_ms: existing.air_assist_on_delay_ms,
                    job_header_gcode: existing.job_header_gcode,
                    job_footer_gcode: existing.job_footer_gcode,
                    transfer_mode: existing.transfer_mode,
                    preferred_default_origin: existing.preferred_default_origin,
                    scanning_offsets: scanning_offsets.unwrap_or(existing.scanning_offsets),
                    enable_scanning_offset: enable_scanning_offset
                        .unwrap_or(existing.enable_scanning_offset),
                    dot_width_mm: dot_width_mm.unwrap_or(existing.dot_width_mm),
                    enable_dot_width: enable_dot_width.unwrap_or(existing.enable_dot_width),
                    supports_z_moves: existing.supports_z_moves,
                    z_move_feed_mm_min: existing.z_move_feed_mm_min,
                    ruida_table_axis: existing.ruida_table_axis,
                    enable_laser_fire_button: existing.enable_laser_fire_button,
                    default_fire_power_percent: existing.default_fire_power_percent,
                    quality_test_settings: existing.quality_test_settings,
                },
            )?;
            if json {
                println!("{}", serde_json::to_string_pretty(&profile)?);
            } else {
                println!("Updated profile '{}' ({})", profile.name, profile.id);
            }
        }
        ProfileCmd::Delete { lookup } => {
            let profile = resolve_profile_lookup(&ctx, &lookup)?;
            beambench_service::ops::profiles::delete_profile(&ctx, profile.id)?;
            if json {
                println!(
                    "{}",
                    serde_json::json!({ "deleted": true, "profile_id": profile.id.to_string() })
                );
            } else {
                println!("Deleted profile '{}'", profile.name);
            }
        }
        ProfileCmd::Activate { lookup } => {
            let profile = resolve_profile_lookup(&ctx, &lookup)?;
            beambench_service::ops::profiles::set_active_profile(&ctx, Some(profile.id))?;
            if json {
                println!(
                    "{}",
                    serde_json::json!({ "active_profile_id": profile.id.to_string() })
                );
            } else {
                println!("Activated profile '{}'", profile.name);
            }
        }
        ProfileCmd::Deactivate => {
            beambench_service::ops::profiles::set_active_profile(&ctx, None)?;
            if json {
                println!(
                    "{}",
                    serde_json::json!({ "active_profile_id": serde_json::Value::Null })
                );
            } else {
                println!("Cleared active profile");
            }
        }
        ProfileCmd::Bootstrap {
            candidate_id,
            profile_name,
            activate,
        } => {
            let response = local_api_json_request(
                Method::POST,
                "/api/v1/profiles/bootstrap",
                Some(serde_json::json!({
                    "candidate_id": candidate_id,
                    "profile_name": profile_name,
                    "activate": activate,
                })),
            )?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                let profile = &response["profile"];
                println!(
                    "Bootstrapped profile '{}' ({})",
                    profile["name"].as_str().unwrap_or("Unknown"),
                    profile["id"].as_str().unwrap_or("unknown-id")
                );
            }
        }
    }
    Ok(())
}

fn resolve_api_profile_id(lookup: Option<&str>) -> Result<String, Box<dyn std::error::Error>> {
    let response = local_api_json_request(Method::GET, "/api/v1/profiles", None)?;
    let profiles = response["profiles"].as_array().ok_or_else(|| {
        cli_exit(
            1,
            "Profile list response did not include a profiles array".to_string(),
        )
    })?;
    if let Some(lookup) = lookup {
        profiles
            .iter()
            .find(|profile| {
                profile["id"].as_str() == Some(lookup) || profile["name"].as_str() == Some(lookup)
            })
            .and_then(|profile| profile["id"].as_str())
            .map(str::to_string)
            .ok_or_else(|| cli_exit(1, format!("Profile not found: {lookup}")))
    } else {
        response["active_profile_id"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| cli_exit(1, "No active profile configured"))
    }
}

fn print_profile_diff(diff: &serde_json::Value) {
    let Some(items) = diff.as_array() else {
        println!("No diff returned");
        return;
    };
    if items.is_empty() {
        println!("No profile changes needed");
        return;
    }
    for item in items {
        println!(
            "{}: {} -> {}",
            item["field"].as_str().unwrap_or("field"),
            item["old"],
            item["new"]
        );
    }
}

fn handle_asset(cmd: AssetCmd, json: bool) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        AssetCmd::List { path } => {
            let project = beambench_project::load_project(std::path::Path::new(&path))?;

            if json {
                let assets: Vec<_> = project
                    .assets
                    .iter()
                    .map(|a| {
                        serde_json::json!({
                            "id": a.id.to_string(),
                            "filename": a.original_filename,
                            "media_type": a.media_type.extension(),
                            "byte_size": a.byte_size,
                            "width_px": a.width_px,
                            "height_px": a.height_px,
                        })
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&assets)?);
            } else if project.assets.is_empty() {
                println!("No assets in project");
            } else {
                for asset in &project.assets {
                    let dims = match (asset.width_px, asset.height_px) {
                        (Some(w), Some(h)) => format!(" ({w}x{h}px)"),
                        _ => String::new(),
                    };
                    println!(
                        "  {} {} [{}] {} bytes{}",
                        asset.id,
                        asset.original_filename,
                        asset.media_type.extension(),
                        asset.byte_size,
                        dims,
                    );
                }
            }
        }
        AssetCmd::Export {
            path,
            asset_id,
            output,
        } => {
            let project = beambench_project::load_project(std::path::Path::new(&path))?;

            let uuid = uuid::Uuid::parse_str(&asset_id)
                .map_err(|e| format!("Invalid asset ID '{asset_id}': {e}"))?;
            let id: beambench_core::AssetId = beambench_common::Id::from_uuid(uuid);

            let data = project
                .get_asset_data(id)
                .ok_or_else(|| format!("Asset '{asset_id}' not found in project"))?;

            std::fs::write(&output, data)?;

            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "asset_id": asset_id,
                        "output": output,
                        "bytes": data.len(),
                    })
                );
            } else {
                println!("Exported {} bytes to {output}", data.len());
            }
        }
        AssetCmd::Import {
            project: project_path,
            file,
            output,
        } => {
            let mut project = beambench_project::load_project(std::path::Path::new(&project_path))?;

            let file_path = std::path::Path::new(&file);
            let data = std::fs::read(file_path)
                .map_err(|e| format!("Failed to read file '{}': {e}", file))?;

            let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");
            let media_type = beambench_core::AssetMediaType::from_extension(ext)
                .ok_or_else(|| format!("Unsupported file type: {ext}"))?;

            let filename = file_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string();

            let byte_size = data.len() as u64;
            let asset = beambench_core::Asset::new(&filename, media_type, byte_size, None, None);
            let asset_id = asset.id.to_string();
            project.add_asset(asset, data);

            let output_path = output.unwrap_or(project_path);
            beambench_project::save_project(&project, std::path::Path::new(&output_path))?;

            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "asset_id": asset_id,
                        "filename": filename,
                        "output": output_path,
                    })
                );
            } else {
                println!("Imported '{filename}' as asset {asset_id}");
                println!("Saved project to {output_path}");
            }
        }
    }
    Ok(())
}

fn settings_path() -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    Ok(dirs::config_dir()
        .ok_or("Could not determine config directory")?
        .join("beam-bench")
        .join("settings.json"))
}

fn load_settings() -> Result<Option<beambench_core::AppSettings>, Box<dyn std::error::Error>> {
    let path = settings_path()?;
    if !path.exists() {
        return Ok(None);
    }

    let contents =
        std::fs::read_to_string(&path).map_err(|e| format!("Failed to read settings: {e}"))?;
    let settings: beambench_core::AppSettings =
        serde_json::from_str(&contents).map_err(|e| format!("Failed to parse settings: {e}"))?;
    Ok(Some(settings))
}

fn resolve_profile_lookup(
    ctx: &beambench_service::ServiceContext,
    lookup: &str,
) -> Result<beambench_core::MachineProfile, Box<dyn std::error::Error>> {
    let profile_lookup = match uuid::Uuid::parse_str(lookup) {
        Ok(uuid) => beambench_service::ops::profiles::ProfileLookup::Id(
            beambench_common::Id::from_uuid(uuid),
        ),
        Err(_) => beambench_service::ops::profiles::ProfileLookup::Name(lookup.to_string()),
    };
    Ok(beambench_service::ops::profiles::get_profile(
        ctx,
        profile_lookup,
    )?)
}

fn parse_tcp_target(value: &str) -> Result<DiscoveryTcpTarget, Box<dyn std::error::Error>> {
    let (host, port) = value
        .rsplit_once(':')
        .ok_or_else(|| format!("Invalid TCP target '{value}'. Expected host:port"))?;
    let port = port
        .parse::<u16>()
        .map_err(|e| format!("Invalid TCP target port in '{value}': {e}"))?;
    if host.is_empty() {
        return Err(format!("Invalid TCP target '{value}'. Host cannot be empty").into());
    }
    Ok(DiscoveryTcpTarget {
        host: host.to_string(),
        port,
        label: None,
    })
}

fn local_api_base_url() -> Result<String, Box<dyn std::error::Error>> {
    let settings = load_settings()?.unwrap_or_default();
    // Treat the persisted API settings as connection hints, not as an
    // authoritative availability gate. The running app can enable/reload the
    // local API before the settings file catches up; if no listener exists the
    // request path below will still return the documented API-unreachable exit.
    let host = if settings.api_localhost_only {
        "127.0.0.1"
    } else {
        "localhost"
    };
    Ok(format!("http://{host}:{}", settings.api_port))
}

fn post_job_control(action: &str, json: bool) -> Result<(), Box<dyn std::error::Error>> {
    let response = local_api_json_request(Method::POST, &format!("/api/v1/jobs/{action}"), None)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&response)?);
    } else {
        println!("Job {action} requested successfully");
    }

    Ok(())
}

fn local_api_json_request(
    method: Method,
    path: &str,
    body: Option<serde_json::Value>,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let (status, value) = local_api_json_request_with_status(method, path, body)?;
    if !status.is_success() {
        return Err(local_api_status_error(status, path, value));
    }
    Ok(value)
}

fn local_api_error_message(status: StatusCode, path: &str, value: &Value) -> String {
    value["message"]
        .as_str()
        .or_else(|| value["error"]["message"].as_str())
        .map(str::to_string)
        .unwrap_or_else(|| format!("API request failed ({status}) for {path}"))
}

fn local_api_status_error(
    status: StatusCode,
    path: &str,
    value: Value,
) -> Box<dyn std::error::Error> {
    let message = local_api_error_message(status, path, &value);
    if value["error_code"] == "CONFIRMATION_REQUIRED" {
        cli_exit_with_body(4, message, value)
    } else {
        cli_exit_with_body(1, message, value)
    }
}

fn local_api_json_request_with_status(
    method: Method,
    path: &str,
    body: Option<serde_json::Value>,
) -> Result<(StatusCode, serde_json::Value), Box<dyn std::error::Error>> {
    let base = local_api_base_url()?;
    let url = format!("{base}{path}");
    // Controller connection endpoints legitimately run long: serial
    // auto-detect sweeps GRBL baud rates (banner waits per baud), then probes
    // Marlin and Smoothieware with bootloader sleeps and identity timeouts —
    // roughly 45s worst case. reqwest's default 30s total timeout made the
    // CLI report failure while the backend went on to connect anyway.
    let timeout = if path.contains("/machine/connect") {
        Duration::from_secs(120)
    } else {
        Duration::from_secs(30)
    };
    let client = reqwest::blocking::Client::builder()
        .timeout(timeout)
        .build()
        .map_err(|e| cli_exit(2, format!("Failed to build HTTP client: {e}")))?;
    let request = client.request(method, &url);
    let response = if let Some(body) = body {
        request
            .json(&body)
            .send()
            .map_err(|e| cli_exit(2, format!("Failed to reach local API at {url}: {e}")))?
    } else {
        request
            .send()
            .map_err(|e| cli_exit(2, format!("Failed to reach local API at {url}: {e}")))?
    };
    let status = response.status();
    let text = response.text().unwrap_or_default();
    if text.trim().is_empty() {
        Ok((status, serde_json::json!({})))
    } else {
        let value = serde_json::from_str(&text).map_err(|e| {
            cli_exit(
                2,
                format!("Failed to parse JSON response from {path}: {e}. Body: {text}"),
            )
        })?;
        Ok((status, value))
    }
}

fn api_open_project(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    local_api_json_request(
        Method::POST,
        "/api/v1/projects/open",
        Some(serde_json::json!({ "path": path })),
    )?;
    Ok(())
}

fn handle_job(
    cmd: JobCmd,
    json: bool,
    confirmations: ConfirmFlags,
) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        JobCmd::Preflight { input } => {
            api_open_project(&input)?;
            let report = local_api_json_request(Method::POST, "/api/v1/jobs/preflight", None)?;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        JobCmd::DryRun { input } => {
            let project = beambench_project::load_project(std::path::Path::new(&input))?;
            let plan = beambench_planner::build_plan(&project)?;
            let config = gcode_config_for_project(&project);
            let gcode_lines = beambench_grbl::generate_gcode(&plan, &config)?;

            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "segments": plan.segments.len(),
                        "gcode_lines": gcode_lines.len(),
                        "travel_distance_mm": plan.total_distance_mm,
                        "estimated_duration_secs": plan.estimated_duration_secs,
                        "warnings": plan.warnings.len(),
                    }))?
                );
            } else {
                println!("Dry run complete:");
                println!("  Segments: {}", plan.segments.len());
                println!("  G-code lines: {}", gcode_lines.len());
                println!("  Travel distance: {:.2} mm", plan.total_distance_mm);
                println!(
                    "  Estimated duration: {:.1} s",
                    plan.estimated_duration_secs
                );
                if !plan.warnings.is_empty() {
                    println!("  Warnings: {}", plan.warnings.len());
                    for w in &plan.warnings {
                        println!("    - {w:?}");
                    }
                }
            }
        }
        JobCmd::Run { input, port, baud } => {
            if !confirmations.confirm_motion {
                return Err(confirmation_required_error(&["confirm_motion"]));
            }
            if !confirmations.confirm_laser_on {
                return Err(confirmation_required_error(&["confirm_laser_on"]));
            }

            // 1. Connect to machine via local API
            if !json {
                eprintln!("Connecting to {port} at {baud} baud...");
            }
            let (connect_status, connect_body) = local_api_json_request_with_status(
                Method::POST,
                "/api/v1/machine/connect",
                Some(serde_json::json!({
                    "port": port,
                    "baud_rate": baud,
                })),
            )?;
            if !connect_status.is_success() {
                let message = local_api_error_message(
                    connect_status,
                    "/api/v1/machine/connect",
                    &connect_body,
                );
                if connect_status == StatusCode::CONFLICT && message.contains("Already connected") {
                    if !json {
                        eprintln!("Already connected; using the active machine session.");
                    }
                } else {
                    return Err(local_api_status_error(
                        connect_status,
                        "/api/v1/machine/connect",
                        connect_body,
                    ));
                }
            }
            if !json {
                eprintln!("Connected.");
            }

            // 2. Open project in the API host
            if !json {
                eprintln!("Opening project...");
            }
            api_open_project(&input)?;

            // 3. Run preflight using the same service-backed path as the app/API
            let report: beambench_common::machine::PreflightReport = serde_json::from_value(
                local_api_json_request(Method::POST, "/api/v1/jobs/preflight", None)?,
            )?;
            match report.outcome {
                beambench_common::machine::PreflightOutcome::Fail => {
                    if json {
                        println!("{}", serde_json::to_string_pretty(&report)?);
                    }
                    return Err("Preflight checks failed".into());
                }
                beambench_common::machine::PreflightOutcome::PassWithWarnings => {
                    return Err("Preflight passed with warnings".into());
                }
                beambench_common::machine::PreflightOutcome::Pass => {
                    if !json {
                        eprintln!("Preflight passed.");
                    }
                }
            }

            // 4. Start job
            local_api_json_request(
                Method::POST,
                "/api/v1/jobs/start",
                Some(serde_json::json!({
                    "confirm_motion": confirmations.confirm_motion,
                    "confirm_laser_on": confirmations.confirm_laser_on,
                })),
            )?;

            if !json {
                eprintln!("Job started. Streaming G-code...");
            }

            // 5. Poll loop
            let mut last_print = std::time::Instant::now();
            let mut last_ack = 0usize;
            let mut saw_active_job = false;
            loop {
                let progress: beambench_common::machine::JobProgress = serde_json::from_value(
                    local_api_json_request(Method::GET, "/api/v1/jobs/progress", None)?,
                )?;
                if progress.total_lines > 0
                    || !matches!(progress.state, beambench_common::machine::JobState::Idle)
                {
                    saw_active_job = true;
                }

                // Print progress every second or when significant progress occurs
                if !json {
                    let now = std::time::Instant::now();
                    let ack_delta = progress.acknowledged_lines.saturating_sub(last_ack);
                    if now.duration_since(last_print).as_secs() >= 1 || ack_delta >= 50 {
                        let pct = if progress.total_lines > 0 {
                            100.0 * progress.acknowledged_lines as f64 / progress.total_lines as f64
                        } else {
                            0.0
                        };
                        eprint!(
                            "\rProgress: {}/{} lines ({:.1}%) \u{2014} elapsed {:.1}s, remaining ~{:.1}s    ",
                            progress.acknowledged_lines,
                            progress.total_lines,
                            pct,
                            progress.elapsed_secs,
                            progress.estimated_remaining_secs,
                        );
                        last_print = now;
                        last_ack = progress.acknowledged_lines;
                    }
                }

                match progress.state {
                    beambench_common::machine::JobState::Completed => {
                        if json {
                            println!("{}", serde_json::to_string_pretty(&progress)?);
                        } else {
                            eprintln!();
                            println!(
                                "Job completed: {} lines in {:.1}s",
                                progress.total_lines, progress.elapsed_secs,
                            );
                        }
                        break;
                    }
                    beambench_common::machine::JobState::Failed => {
                        return Err("Job failed during streaming".into());
                    }
                    beambench_common::machine::JobState::Cancelled => {
                        return Err("Job was cancelled".into());
                    }
                    beambench_common::machine::JobState::Idle
                        if progress.total_lines == 0 && saw_active_job =>
                    {
                        return Err(
                            "Job stopped before completion; it may have been cancelled or interrupted"
                                .into(),
                        );
                    }
                    _ => {}
                }

                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
        JobCmd::Pause => post_job_control("pause", json)?,
        JobCmd::Resume => post_job_control("resume", json)?,
        JobCmd::Cancel => post_job_control("cancel", json)?,
        JobCmd::Progress => {
            let progress = local_api_json_request(Method::GET, "/api/v1/jobs/progress", None)?;
            println!("{}", serde_json::to_string_pretty(&progress)?);
        }
        JobCmd::Frame {
            input,
            mode,
            selected_ids,
            laser_on,
        } => {
            if let Some(path) = input {
                api_open_project(&path)?;
            }
            let response = local_api_json_request(
                Method::POST,
                "/api/v1/jobs/frame",
                Some(serde_json::json!({
                    "frame_mode": mode,
                    "selected_object_ids": selected_ids,
                    "laser_on_override": laser_on,
                    "confirm_motion": confirmations.confirm_motion,
                    "confirm_laser_on": confirmations.confirm_laser_on,
                })),
            )?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                println!("Frame requested successfully");
            }
        }
    }
    Ok(())
}

fn handle_console(
    cmd: ConsoleCmd,
    json: bool,
    confirmations: ConfirmFlags,
) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        ConsoleCmd::Send { line } => {
            let response = local_api_json_request(
                Method::POST,
                "/api/v1/console",
                Some(serde_json::json!({
                    "line": line,
                    "confirm_raw_gcode": confirmations.confirm_raw_gcode,
                    "confirm_laser_on": confirmations.confirm_laser_on,
                })),
            )?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                println!("G-code line sent: {line}");
            }
        }
        ConsoleCmd::Log { limit } => {
            let response = local_api_json_request(
                Method::GET,
                &format!("/api/v1/console?limit={limit}"),
                None,
            )?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                if let Some(entries) = response["entries"].as_array() {
                    for entry in entries {
                        let direction = entry["direction"].as_str().unwrap_or("?");
                        let content = entry["content"].as_str().unwrap_or("");
                        println!("[{direction}] {content}");
                    }
                }
            }
        }
        ConsoleCmd::Interactive => {
            if !json {
                eprintln!(
                    "Beam Bench interactive console. Type G-code commands, or \"exit\"/\"quit\" to leave."
                );
            }
            let stdin = std::io::stdin();
            let mut line_buf = String::new();
            loop {
                line_buf.clear();
                if !json {
                    eprint!("> ");
                }
                let bytes_read = stdin.read_line(&mut line_buf)?;
                if bytes_read == 0 {
                    // EOF
                    break;
                }
                let trimmed = line_buf.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if trimmed.eq_ignore_ascii_case("exit") || trimmed.eq_ignore_ascii_case("quit") {
                    break;
                }
                match local_api_json_request(
                    Method::POST,
                    "/api/v1/console",
                    Some(serde_json::json!({
                        "line": trimmed,
                        "confirm_raw_gcode": confirmations.confirm_raw_gcode,
                        "confirm_laser_on": confirmations.confirm_laser_on,
                    })),
                ) {
                    Ok(response) => {
                        if json {
                            println!("{}", serde_json::to_string_pretty(&response)?);
                        } else {
                            println!("ok: {}", response["line"].as_str().unwrap_or(trimmed));
                        }
                    }
                    Err(e) => {
                        if json {
                            println!("{}", serde_json::json!({"error": e.to_string()}));
                        } else {
                            eprintln!("error: {e}");
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

fn handle_material(cmd: MaterialCmd, json: bool) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        MaterialCmd::List => {
            let response = local_api_json_request(Method::GET, "/api/v1/materials", None)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                if let Some(presets) = response["presets"].as_array() {
                    for preset in presets {
                        let id = preset["id"].as_str().unwrap_or("?");
                        let name = preset["name"].as_str().unwrap_or("?");
                        let material = preset["material"].as_str().unwrap_or("?");
                        let speed = preset["speed_mm_min"].as_f64().unwrap_or(0.0);
                        let power = preset["power_percent"].as_f64().unwrap_or(0.0);
                        let passes = preset["passes"].as_u64().unwrap_or(0);
                        println!(
                            "{id} | {name} | {material} | {speed} mm/min | {power}% | {passes} passes"
                        );
                    }
                }
            }
        }
        MaterialCmd::Add {
            name,
            material,
            speed,
            power,
            passes,
        } => {
            let preset = build_material_add_preset(&name, &material, speed, power, passes);
            let response = local_api_json_request(Method::POST, "/api/v1/materials", Some(preset))?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                println!("Material preset '{name}' added successfully");
            }
        }
        MaterialCmd::Remove { id } => {
            let response =
                local_api_json_request(Method::DELETE, &format!("/api/v1/materials/{id}"), None)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                println!("Material preset removed successfully");
            }
        }
    }
    Ok(())
}

fn build_material_add_preset(
    name: &str,
    material: &str,
    speed: f64,
    power: f64,
    passes: usize,
) -> serde_json::Value {
    serde_json::json!({
        "id": uuid::Uuid::new_v4().to_string(),
        "name": name,
        "material": material,
        "thickness_mm": 3.0,
        "operation": "cut",
        "speed_mm_min": speed,
        "power_percent": power,
        "passes": passes,
        "dpi": null,
        "notes": "",
    })
}

fn handle_macro(cmd: MacroCmd, json: bool) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        MacroCmd::List => {
            let response = local_api_json_request(Method::GET, "/api/v1/macros", None)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                if let Some(macros) = response["macros"].as_array() {
                    for macro_def in macros {
                        let id = macro_def["id"].as_str().unwrap_or("?");
                        let name = macro_def["name"].as_str().unwrap_or("?");
                        let description = macro_def["description"].as_str().unwrap_or("");
                        println!("{id} | {name} | {description}");
                    }
                }
            }
        }
        MacroCmd::Add {
            name,
            description,
            commands,
        } => {
            let command_list: Vec<String> =
                commands.split(',').map(|s| s.trim().to_string()).collect();
            let macro_def = serde_json::json!({
                "id": uuid::Uuid::new_v4().to_string(),
                "name": name,
                "description": description,
                "commands": command_list,
            });
            let response = local_api_json_request(Method::POST, "/api/v1/macros", Some(macro_def))?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                println!("Macro '{name}' added successfully");
            }
        }
        MacroCmd::Remove { id } => {
            let response =
                local_api_json_request(Method::DELETE, &format!("/api/v1/macros/{id}"), None)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                println!("Macro removed successfully");
            }
        }
        MacroCmd::Run { id } => {
            let response =
                local_api_json_request(Method::POST, &format!("/api/v1/macros/{id}/run"), None)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                let commands_sent = response["commands_sent"].as_u64().unwrap_or(0);
                println!("Macro executed: {commands_sent} commands sent");
            }
        }
    }
    Ok(())
}

fn handle_import(cmd: ImportCmd, json: bool) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        ImportCmd::Dxf { file, layer } => {
            let response = local_api_json_request(
                Method::POST,
                "/api/v1/projects/import/dxf",
                Some(serde_json::json!({
                    "file_path": file,
                    "layer_id": layer,
                })),
            )?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                println!("DXF import requested");
            }
        }
        ImportCmd::Pdf { file, layer } => {
            let response = local_api_json_request(
                Method::POST,
                "/api/v1/projects/import/pdf",
                Some(serde_json::json!({
                    "file_path": file,
                    "layer_id": layer,
                })),
            )?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                println!("PDF import requested");
            }
        }
        ImportCmd::Ai { file, layer } => {
            let response = local_api_json_request(
                Method::POST,
                "/api/v1/projects/import/ai",
                Some(serde_json::json!({
                    "file_path": file,
                    "layer_id": layer,
                })),
            )?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                println!("AI import requested");
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn cli_material_add_payload_uses_snake_case_operation() {
        let payload = build_material_add_preset("3mm Ply", "Wood", 1000.0, 80.0, 2);

        assert_eq!(payload["operation"], "cut");
    }

    #[test]
    fn cli_version_parses() {
        let cli = Cli::try_parse_from(["beambench", "version"]).unwrap();
        assert!(matches!(cli.command, Commands::Version));
    }

    #[test]
    fn cli_ports_parses() {
        let cli = Cli::try_parse_from(["beambench", "ports"]).unwrap();
        assert!(matches!(cli.command, Commands::Ports));
    }

    #[test]
    fn cli_export_gcode_parses() {
        let cli = Cli::try_parse_from([
            "beambench",
            "export",
            "gcode",
            "input.lzrproj",
            "output.gcode",
        ])
        .unwrap();
        assert!(matches!(cli.command, Commands::Export { .. }));
    }

    #[test]
    fn cli_json_flag() {
        let cli = Cli::try_parse_from(["beambench", "--json", "version"]).unwrap();
        assert!(cli.json);
    }

    #[test]
    fn cli_project_info_parses() {
        let cli = Cli::try_parse_from(["beambench", "project", "info", "test.lzrproj"]).unwrap();
        assert!(matches!(cli.command, Commands::Project { .. }));
    }

    #[test]
    fn cli_project_create_parses() {
        let cli = Cli::try_parse_from(["beambench", "project", "create", "My Project"]).unwrap();
        assert!(matches!(cli.command, Commands::Project { .. }));
    }

    #[test]
    fn cli_project_create_with_output_parses() {
        let cli = Cli::try_parse_from([
            "beambench",
            "project",
            "create",
            "My Project",
            "--output",
            "custom.lzrproj",
        ])
        .unwrap();
        match cli.command {
            Commands::Project {
                command: ProjectCmd::Create { name, output },
            } => {
                assert_eq!(name, "My Project");
                assert_eq!(output, Some("custom.lzrproj".to_string()));
            }
            _ => panic!("Expected Project Create command"),
        }
    }

    #[test]
    fn cli_project_open_parses() {
        let cli = Cli::try_parse_from(["beambench", "project", "open", "demo.lzrproj"]).unwrap();
        match cli.command {
            Commands::Project {
                command: ProjectCmd::Open { path },
            } => assert_eq!(path, "demo.lzrproj"),
            _ => panic!("Expected Project Open command"),
        }
    }

    #[test]
    fn cli_project_save_as_parses() {
        let cli = Cli::try_parse_from(["beambench", "project", "save-as", "out.lzrproj"]).unwrap();
        match cli.command {
            Commands::Project {
                command: ProjectCmd::SaveAs { path },
            } => assert_eq!(path, "out.lzrproj"),
            _ => panic!("Expected Project SaveAs command"),
        }
    }

    #[test]
    fn cli_project_undo_parses() {
        let cli = Cli::try_parse_from(["beambench", "project", "undo"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Project {
                command: ProjectCmd::Undo
            }
        ));
    }

    #[test]
    fn cli_project_save_parses() {
        let cli = Cli::try_parse_from(["beambench", "project", "save"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Project {
                command: ProjectCmd::Save
            }
        ));
    }

    #[test]
    fn cli_project_close_parses() {
        let cli = Cli::try_parse_from(["beambench", "project", "close"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Project {
                command: ProjectCmd::Close
            }
        ));
    }

    #[test]
    fn cli_project_redo_parses() {
        let cli = Cli::try_parse_from(["beambench", "project", "redo"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Project {
                command: ProjectCmd::Redo
            }
        ));
    }

    #[test]
    fn cli_project_import_svg_parses() {
        let cli = Cli::try_parse_from([
            "beambench",
            "project",
            "import-svg",
            "--layer",
            "layer-id",
            "demo.svg",
        ])
        .unwrap();
        match cli.command {
            Commands::Project {
                command: ProjectCmd::ImportSvg { layer, file },
            } => {
                assert_eq!(layer, "layer-id");
                assert_eq!(file, "demo.svg");
            }
            _ => panic!("Expected Project ImportSvg command"),
        }
    }

    #[test]
    fn cli_project_import_files_parses() {
        let cli = Cli::try_parse_from([
            "beambench",
            "project",
            "import-files",
            "--layer",
            "layer-id",
            "a.svg",
            "b.png",
        ])
        .unwrap();
        match cli.command {
            Commands::Project {
                command: ProjectCmd::ImportFiles { layer, files },
            } => {
                assert_eq!(layer, "layer-id");
                assert_eq!(files, vec!["a.svg".to_string(), "b.png".to_string()]);
            }
            _ => panic!("Expected Project ImportFiles command"),
        }
    }

    #[test]
    fn cli_machine_connect_parses() {
        let cli =
            Cli::try_parse_from(["beambench", "machine", "connect", "--port", "/dev/ttyUSB0"])
                .unwrap();
        match cli.command {
            Commands::Machine {
                command: MachineCmd::Connect { port, baud },
            } => {
                assert_eq!(port, "/dev/ttyUSB0");
                assert_eq!(baud, 115200);
            }
            _ => panic!("Expected Machine Connect command"),
        }
    }

    #[test]
    fn cli_machine_connect_custom_baud_parses() {
        let cli = Cli::try_parse_from([
            "beambench",
            "machine",
            "connect",
            "--port",
            "/dev/ttyUSB0",
            "--baud",
            "9600",
        ])
        .unwrap();
        match cli.command {
            Commands::Machine {
                command: MachineCmd::Connect { port, baud },
            } => {
                assert_eq!(port, "/dev/ttyUSB0");
                assert_eq!(baud, 9600);
            }
            _ => panic!("Expected Machine Connect command"),
        }
    }

    #[test]
    fn cli_machine_explicit_controller_connections_parse() {
        let serial = Cli::try_parse_from([
            "beambench",
            "machine",
            "connect-serial",
            "--port",
            "/dev/ttyUSB0",
            "--baud",
            "250000",
            "--controller",
            "marlin",
        ])
        .unwrap();
        match serial.command {
            Commands::Machine {
                command:
                    MachineCmd::ConnectSerial {
                        port,
                        baud,
                        controller,
                    },
            } => {
                assert_eq!(port, "/dev/ttyUSB0");
                assert_eq!(baud, 250000);
                assert_eq!(controller, SerialControllerArg::Marlin);
                assert_eq!(
                    controller.selection_json(),
                    serde_json::json!({ "mode": "known_driver", "driver": "marlin" })
                );
            }
            _ => panic!("Expected Machine ConnectSerial command"),
        }

        let network = Cli::try_parse_from([
            "beambench",
            "machine",
            "connect-network",
            "--host",
            "192.168.1.100",
            "--controller",
            "ruida",
        ])
        .unwrap();
        match network.command {
            Commands::Machine {
                command:
                    MachineCmd::ConnectNetwork {
                        host,
                        port,
                        controller,
                    },
            } => {
                assert_eq!(host, "192.168.1.100");
                assert_eq!(port, None);
                assert_eq!(controller, NetworkControllerArg::Ruida);
                assert_eq!(controller.default_port(), 50200);
            }
            _ => panic!("Expected Machine ConnectNetwork command"),
        }
    }

    #[test]
    fn cli_controller_values_match_the_public_release_scope() {
        let values = |variants: &[ControllerArg]| {
            variants
                .iter()
                .filter_map(|variant| variant.to_possible_value())
                .map(|value| value.get_name().to_string())
                .collect::<Vec<_>>()
        };

        assert_eq!(
            values(ControllerArg::value_variants()),
            [
                "auto-detect",
                "grbl",
                "fluid-nc",
                "grbl-hal",
                "laser-pecker",
                "marlin",
                "snapmaker",
                "smoothieware",
                "ruida",
                "lihuiyu",
                "generic-grbl-compatible",
            ]
        );

        let serial = SerialControllerArg::value_variants()
            .iter()
            .filter_map(|variant| variant.to_possible_value())
            .map(|value| value.get_name().to_string())
            .collect::<Vec<_>>();
        assert_eq!(
            serial,
            [
                "auto-detect",
                "grbl",
                "fluid-nc",
                "grbl-hal",
                "laser-pecker",
                "marlin",
                "snapmaker",
                "smoothieware",
                "generic-grbl-compatible",
            ]
        );

        let network = NetworkControllerArg::value_variants()
            .iter()
            .filter_map(|variant| variant.to_possible_value())
            .map(|value| value.get_name().to_string())
            .collect::<Vec<_>>();
        assert_eq!(
            network,
            [
                "auto-detect",
                "fluid-nc",
                "grbl-hal",
                "laser-pecker",
                "ruida"
            ]
        );
        assert_eq!(NetworkControllerArg::LaserPecker.default_port(), 8888);
    }

    #[test]
    fn cli_machine_controller_challenge_continuation_parses() {
        let cli = Cli::try_parse_from([
            "beambench",
            "machine",
            "continue-controller",
            "attempt-123",
            "--controller",
            "grbl",
            "--decision",
            "use-detected",
        ])
        .unwrap();
        match cli.command {
            Commands::Machine {
                command:
                    MachineCmd::ContinueController {
                        attempt_id,
                        controller,
                        decision,
                    },
            } => {
                assert_eq!(attempt_id, "attempt-123");
                assert_eq!(controller, ControllerArg::Grbl);
                assert_eq!(decision, Some(ControllerDecisionArg::UseDetected));
                assert_eq!(decision.unwrap().as_api_str(), "use_detected");
            }
            _ => panic!("Expected Machine ContinueController command"),
        }
    }

    #[test]
    fn cli_machine_connect_candidate_parses() {
        let cli =
            Cli::try_parse_from(["beambench", "machine", "connect-candidate", "candidate-123"])
                .unwrap();
        match cli.command {
            Commands::Machine {
                command: MachineCmd::ConnectCandidate { candidate_id },
            } => assert_eq!(candidate_id, "candidate-123"),
            _ => panic!("Expected Machine ConnectCandidate command"),
        }
    }

    #[test]
    fn cli_machine_lihuiyu_usb_commands_parse() {
        let list = Cli::try_parse_from(["beambench", "machine", "list-lihuiyu-usb"]).unwrap();
        assert!(matches!(
            list.command,
            Commands::Machine {
                command: MachineCmd::ListLihuiyuUsb
            }
        ));

        let connect = Cli::try_parse_from([
            "beambench",
            "machine",
            "connect-lihuiyu",
            "--bus-id",
            "20",
            "--device-address",
            "7",
            "--port-numbers",
            "1,3",
        ])
        .unwrap();
        match connect.command {
            Commands::Machine {
                command:
                    MachineCmd::ConnectLihuiyu {
                        bus_id,
                        device_address,
                        port_numbers,
                    },
            } => {
                assert_eq!(bus_id, "20");
                assert_eq!(device_address, 7);
                assert_eq!(port_numbers, vec![1, 3]);
            }
            _ => panic!("Expected Machine ConnectLihuiyu command"),
        }
    }

    #[test]
    fn cli_machine_discover_parses() {
        let cli = Cli::try_parse_from([
            "beambench",
            "machine",
            "discover",
            "--tcp",
            "192.168.0.10:50200",
            "--usb",
            "/dev/mock-usb",
        ])
        .unwrap();
        match cli.command {
            Commands::Machine {
                command:
                    MachineCmd::Discover {
                        tcp_targets,
                        usb_targets,
                    },
            } => {
                assert_eq!(tcp_targets, vec!["192.168.0.10:50200".to_string()]);
                assert_eq!(usb_targets, vec!["/dev/mock-usb".to_string()]);
            }
            _ => panic!("Expected Machine Discover command"),
        }
    }

    #[test]
    fn cli_machine_status_parses() {
        let cli = Cli::try_parse_from(["beambench", "machine", "status"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Machine {
                command: MachineCmd::Status
            }
        ));
    }

    #[test]
    fn cli_machine_disconnect_parses() {
        let cli = Cli::try_parse_from(["beambench", "machine", "disconnect"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Machine {
                command: MachineCmd::Disconnect
            }
        ));
    }

    #[test]
    fn cli_machine_unlock_parses() {
        let cli = Cli::try_parse_from(["beambench", "machine", "unlock"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Machine {
                command: MachineCmd::Unlock
            }
        ));
    }

    #[test]
    fn cli_machine_jog_parses() {
        let cli = Cli::try_parse_from([
            "beambench",
            "machine",
            "jog",
            "1.5",
            "2.0",
            "--feed",
            "1200",
        ])
        .unwrap();
        match cli.command {
            Commands::Machine {
                command: MachineCmd::Jog { x_mm, y_mm, feed },
            } => {
                assert_eq!(x_mm, 1.5);
                assert_eq!(y_mm, 2.0);
                assert_eq!(feed, 1200.0);
            }
            _ => panic!("Expected Machine Jog command"),
        }
    }

    #[test]
    fn cli_machine_jog_accepts_negative_deltas_without_separator() {
        let cli = Cli::try_parse_from(["beambench", "machine", "jog", "-5", "0", "--feed", "600"])
            .unwrap();
        match cli.command {
            Commands::Machine {
                command: MachineCmd::Jog { x_mm, y_mm, feed },
            } => {
                assert_eq!(x_mm, -5.0);
                assert_eq!(y_mm, 0.0);
                assert_eq!(feed, 600.0);
            }
            _ => panic!("Expected Machine Jog command"),
        }
    }

    #[test]
    fn cli_machine_test_air_parses_confirmation() {
        let cli = Cli::try_parse_from([
            "beambench",
            "--confirm-air-assist",
            "machine",
            "test-air",
            "--duration-ms",
            "250",
        ])
        .unwrap();
        assert!(cli.confirm_air_assist);
        match cli.command {
            Commands::Machine {
                command: MachineCmd::TestAir { duration_ms },
            } => assert_eq!(duration_ms, 250),
            _ => panic!("Expected Machine TestAir command"),
        }
    }

    #[test]
    fn cli_machine_test_air_requires_air_confirmation_before_api() {
        let cli = Cli::try_parse_from(["beambench", "machine", "test-air"]).unwrap();

        let err = run(cli).unwrap_err();
        let body = format_cli_error_json(err.as_ref());

        assert_eq!(cli_exit_code(err.as_ref()), 4);
        assert_eq!(body["error_code"], "CONFIRMATION_REQUIRED");
        assert_eq!(body["missing"][0], "confirm_air_assist");
        assert!(body["message"].as_str().unwrap().contains("air assist"));
    }

    #[test]
    fn cli_camera_list_parses() {
        let cli = Cli::try_parse_from(["beambench", "camera", "list"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Camera {
                command: CameraCmd::List
            }
        ));
    }

    #[test]
    fn cli_camera_doctor_parses() {
        let cli = Cli::try_parse_from(["beambench", "camera", "doctor", "--json"]).unwrap();
        assert!(cli.json);
        assert!(matches!(
            cli.command,
            Commands::Camera {
                command: CameraCmd::Doctor
            }
        ));
    }

    #[test]
    fn cli_camera_select_parses() {
        let cli =
            Cli::try_parse_from(["beambench", "camera", "select", "--camera-id", "cam-a"]).unwrap();
        match cli.command {
            Commands::Camera {
                command: CameraCmd::Select { camera_id },
            } => assert_eq!(camera_id, Some("cam-a".to_string())),
            _ => panic!("Expected Camera Select command"),
        }
    }

    #[test]
    fn cli_camera_capture_parses() {
        let cli =
            Cli::try_parse_from(["beambench", "camera", "capture", "--camera", "cam-a"]).unwrap();
        match cli.command {
            Commands::Camera {
                command: CameraCmd::Capture { camera, output },
            } => {
                assert_eq!(camera, Some("cam-a".to_string()));
                assert_eq!(output, None);
            }
            _ => panic!("Expected Camera Capture command"),
        }
    }

    #[test]
    fn cli_camera_state_parses() {
        let cli = Cli::try_parse_from(["beambench", "camera", "state"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Camera {
                command: CameraCmd::State
            }
        ));
    }

    #[test]
    fn cli_camera_overlay_render_parses_default_fit_view() {
        let cli = Cli::try_parse_from(["beambench", "camera", "overlay", "render"]).unwrap();
        match cli.command {
            Commands::Camera {
                command:
                    CameraCmd::Overlay {
                        command: CameraOverlayCmd::Render { output, view, keep },
                    },
            } => {
                assert_eq!(output, None);
                assert_eq!(view, CameraOverlayViewArg::Fit);
                assert!(!keep);
            }
            _ => panic!("Expected Camera Overlay Render command"),
        }
    }

    #[test]
    fn cli_camera_overlay_set_transform_allows_partials() {
        let cli = Cli::try_parse_from([
            "beambench",
            "camera",
            "overlay",
            "set-transform",
            "--rotation-deg",
            "12.5",
        ])
        .unwrap();
        match cli.command {
            Commands::Camera {
                command:
                    CameraCmd::Overlay {
                        command:
                            CameraOverlayCmd::SetTransform {
                                x,
                                y,
                                scale,
                                rotation_deg,
                            },
                    },
            } => {
                assert_eq!(x, None);
                assert_eq!(y, None);
                assert_eq!(scale, None);
                assert_eq!(rotation_deg, Some(12.5));
            }
            _ => panic!("Expected Camera Overlay SetTransform command"),
        }
    }

    #[test]
    fn cli_camera_calibrate_parses() {
        let cli = Cli::try_parse_from([
            "beambench",
            "camera",
            "calibrate",
            "--camera",
            "cam-a",
            "--points",
            "points.json",
            "--save",
        ])
        .unwrap();
        match cli.command {
            Commands::Camera {
                command:
                    CameraCmd::Calibrate {
                        camera,
                        points,
                        save,
                    },
            } => {
                assert_eq!(camera, Some("cam-a".to_string()));
                assert_eq!(points, "points.json");
                assert!(save);
            }
            _ => panic!("Expected Camera Calibrate command"),
        }
    }

    #[test]
    fn cli_camera_align_parses() {
        let cli = Cli::try_parse_from(["beambench", "camera", "align", "--points", "align.json"])
            .unwrap();
        match cli.command {
            Commands::Camera {
                command:
                    CameraCmd::Align {
                        camera,
                        points,
                        save,
                    },
            } => {
                assert_eq!(camera, None);
                assert_eq!(points, "align.json");
                assert!(!save);
            }
            _ => panic!("Expected Camera Align command"),
        }
    }

    #[test]
    fn cli_camera_reset_calibration_parses() {
        let cli = Cli::try_parse_from(["beambench", "camera", "reset-calibration"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Camera {
                command: CameraCmd::ResetCalibration { .. }
            }
        ));
    }

    #[test]
    fn cli_camera_reset_alignment_parses() {
        let cli = Cli::try_parse_from(["beambench", "camera", "reset-alignment"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Camera {
                command: CameraCmd::ResetAlignment { .. }
            }
        ));
    }

    #[test]
    fn cli_vector_convert_to_path_parses() {
        let cli =
            Cli::try_parse_from(["beambench", "vector", "convert-to-path", "object-id"]).unwrap();
        match cli.command {
            Commands::Vector {
                command: VectorCmd::ConvertToPath { object_id },
            } => assert_eq!(object_id, "object-id"),
            _ => panic!("Expected Vector ConvertToPath command"),
        }
    }

    #[test]
    fn cli_vector_update_node_parses() {
        let cli = Cli::try_parse_from([
            "beambench",
            "vector",
            "update-node",
            "object-id",
            "--subpath",
            "1",
            "--command",
            "2",
            "--x",
            "3.5",
            "--y",
            "4.5",
            "--handle",
            "in",
        ])
        .unwrap();
        match cli.command {
            Commands::Vector {
                command:
                    VectorCmd::UpdateNode {
                        object_id,
                        subpath,
                        command,
                        x,
                        y,
                        handle,
                    },
            } => {
                assert_eq!(object_id, "object-id");
                assert_eq!(subpath, 1);
                assert_eq!(command, 2);
                assert_eq!(x, 3.5);
                assert_eq!(y, 4.5);
                assert_eq!(handle, Some("in".to_string()));
            }
            _ => panic!("Expected Vector UpdateNode command"),
        }
    }

    #[test]
    fn cli_design_describe_accepts_json_after_subcommand() {
        let cli = Cli::try_parse_from(["beambench", "design", "describe", "--json"]).unwrap();
        assert!(cli.json);
        assert!(matches!(
            cli.command,
            Commands::Design {
                command: DesignCmd::Describe
            }
        ));
    }

    #[test]
    fn cli_design_plan_parses() {
        let cli = Cli::try_parse_from(["beambench", "design", "plan", "plan.json"]).unwrap();
        match cli.command {
            Commands::Design {
                command: DesignCmd::Plan { plan },
            } => assert_eq!(plan, "plan.json"),
            _ => panic!("Expected Design Plan command"),
        }
    }

    #[test]
    fn cli_design_apply_wait_ms_parses() {
        let cli = Cli::try_parse_from([
            "beambench",
            "design",
            "apply",
            "plan.json",
            "--wait-ms",
            "2500",
        ])
        .unwrap();
        match cli.command {
            Commands::Design {
                command: DesignCmd::Apply { plan, wait_ms },
            } => {
                assert_eq!(plan, "plan.json");
                assert_eq!(wait_ms, Some(2500));
            }
            _ => panic!("Expected Design Apply command"),
        }
    }

    #[test]
    fn cli_design_render_parses_temp_png_default() {
        let cli = Cli::try_parse_from(["beambench", "design", "render", "--json"]).unwrap();
        assert!(cli.json);
        match cli.command {
            Commands::Design {
                command:
                    DesignCmd::Render {
                        svg,
                        png,
                        selection_only,
                        selected_ids,
                        pixels_per_mm,
                    },
            } => {
                assert_eq!(svg, None);
                assert_eq!(png, None);
                assert!(!selection_only);
                assert!(selected_ids.is_empty());
                assert_eq!(pixels_per_mm, 4.0);
            }
            _ => panic!("Expected Design Render command"),
        }
    }

    #[test]
    fn cli_design_render_parses_explicit_svg_path() {
        let cli = Cli::try_parse_from([
            "beambench",
            "design",
            "render",
            "--svg",
            "/tmp/canvas.svg",
            "--selection-only",
            "--selected-ids",
            "a,b",
        ])
        .unwrap();
        match cli.command {
            Commands::Design {
                command:
                    DesignCmd::Render {
                        svg,
                        png,
                        selection_only,
                        selected_ids,
                        pixels_per_mm,
                    },
            } => {
                assert_eq!(svg.as_deref(), Some("/tmp/canvas.svg"));
                assert_eq!(png, None);
                assert!(selection_only);
                assert_eq!(selected_ids, vec!["a".to_string(), "b".to_string()]);
                assert_eq!(pixels_per_mm, 4.0);
            }
            _ => panic!("Expected Design Render command"),
        }
    }

    #[test]
    fn cli_design_render_parses_png_path_and_resolution() {
        let cli = Cli::try_parse_from([
            "beambench",
            "design",
            "render",
            "--png",
            "/tmp/canvas.png",
            "--pixels-per-mm",
            "8",
        ])
        .unwrap();
        match cli.command {
            Commands::Design {
                command:
                    DesignCmd::Render {
                        svg,
                        png,
                        selection_only,
                        selected_ids,
                        pixels_per_mm,
                    },
            } => {
                assert_eq!(svg, None);
                assert_eq!(png.as_deref(), Some("/tmp/canvas.png"));
                assert!(!selection_only);
                assert!(selected_ids.is_empty());
                assert_eq!(pixels_per_mm, 8.0);
            }
            _ => panic!("Expected Design Render command"),
        }
    }

    #[test]
    fn cli_design_cleanup_renders_parses() {
        let cli = Cli::try_parse_from([
            "beambench",
            "design",
            "cleanup-renders",
            "--all",
            "--older-than-hours",
            "2",
        ])
        .unwrap();
        match cli.command {
            Commands::Design {
                command:
                    DesignCmd::CleanupRenders {
                        all,
                        older_than_hours,
                    },
            } => {
                assert!(all);
                assert_eq!(older_than_hours, 2);
            }
            _ => panic!("Expected Design CleanupRenders command"),
        }
    }

    #[test]
    fn cli_agent_commands_parse_json_and_markdown() {
        let cli = Cli::try_parse_from(["beambench", "agent", "capabilities", "--json"]).unwrap();
        assert!(cli.json);
        assert!(matches!(
            cli.command,
            Commands::Agent {
                command: AgentCmd::Capabilities
            }
        ));

        let cli = Cli::try_parse_from(["beambench", "agent", "state", "--json"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Agent {
                command: AgentCmd::State
            }
        ));

        let cli = Cli::try_parse_from(["beambench", "agent", "guide", "--markdown"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Agent {
                command: AgentCmd::Guide { markdown: true }
            }
        ));
    }

    #[test]
    fn capability_cli_refs_parse_as_real_commands() {
        for cap in beambench_service::agent::capabilities() {
            let Some(cli_ref) = cap.cli else {
                continue;
            };
            for command_ref in cli_ref
                .command
                .split(';')
                .map(str::trim)
                .filter(|part| !part.is_empty())
            {
                let argv = sample_capability_command(command_ref);
                Cli::try_parse_from(argv.clone()).unwrap_or_else(|err| {
                    panic!(
                        "capability {} points at unparsable CLI ref {:?}: {}",
                        cap.id, argv, err
                    )
                });
            }
        }
    }

    fn sample_capability_command(command_ref: &str) -> Vec<String> {
        let mut argv = Vec::new();
        for token in command_ref.split_whitespace() {
            if token == "beambench-cli" {
                argv.push(token.to_string());
                continue;
            }
            if token == "<files...>" {
                argv.push("/tmp/input.svg".to_string());
                argv.push("/tmp/input.png".to_string());
                continue;
            }
            let replacement = match token {
                "<plan.json>" => Some("/tmp/plan.json"),
                "<path>" => Some("/tmp/output.svg"),
                "<project.lzrproj>" => Some("/tmp/project.lzrproj"),
                "<file>" => Some("/tmp/input.svg"),
                "<layer-id>" => Some("00000000-0000-0000-0000-000000000001"),
                "<camera-id>" => Some("camera-mock-overhead"),
                "<profile>" => Some("test-profile"),
                "<preset-id>" => Some("generic_grbl_diode"),
                "<port>" => Some("/dev/tty.test"),
                "<baud>" => Some("115200"),
                "<controller>" => Some("auto-detect"),
                "<host>" => Some("127.0.0.1"),
                "<bus>" => Some("1"),
                "<address>" => Some("2"),
                "<chain>" => Some("1,3"),
                "<x_mm>" | "<y_mm>" | "<rate>" | "<percent>" | "<passes>" => Some("1"),
                "<line>" => Some("G0 X0"),
                "<name>" => Some("name"),
                "<material>" => Some("material"),
                "<description>" => Some("description"),
                "<commands>" => Some("G0 X0"),
                "<id>" => Some("00000000-0000-0000-0000-000000000002"),
                _ => None,
            };
            argv.push(replacement.unwrap_or(token).to_string());
        }
        assert_eq!(
            argv.first().map(String::as_str),
            Some("beambench-cli"),
            "capability CLI ref must use the shipped binary name: {}",
            command_ref
        );
        assert!(
            !argv
                .iter()
                .any(|arg| arg.contains('<') || arg.contains('>')),
            "capability CLI ref still contains an unresolved placeholder: {:?}",
            argv
        );
        assert!(
            !argv
                .iter()
                .any(|arg| arg.contains('/') && !arg.starts_with('/')),
            "capability CLI ref should list separate concrete commands, not slash shorthand: {:?}",
            argv
        );
        assert!(
            !argv.iter().any(|arg| arg == "..."),
            "capability CLI ref still contains ellipsis shorthand: {:?}",
            argv
        );
        argv
    }

    #[test]
    fn cli_global_confirmation_flags_parse_on_non_risky_commands() {
        let cli = Cli::try_parse_from([
            "beambench",
            "machine",
            "status",
            "--confirm-motion",
            "--confirm-laser-on",
            "--confirm-raw-gcode",
        ])
        .unwrap();
        assert!(cli.confirm_motion);
        assert!(cli.confirm_laser_on);
        assert!(cli.confirm_raw_gcode);
        assert!(matches!(
            cli.command,
            Commands::Machine {
                command: MachineCmd::Status
            }
        ));
    }

    #[test]
    fn confirmation_required_errors_map_to_exit_4() {
        let body = serde_json::json!({
            "error_code": "CONFIRMATION_REQUIRED",
            "missing": ["confirm_motion"],
            "message": "This command can move the machine and requires explicit confirmation."
        });
        let err = cli_exit_with_body(4, "confirmation required", body.clone());
        assert_eq!(cli_exit_code(err.as_ref()), 4);
        assert_eq!(format_cli_error_json(err.as_ref()), body);
    }

    #[test]
    fn generated_agent_markdown_matches_checked_in_doc() {
        let doc_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("docs/AI_AGENT_WORK_SURFACE.md");
        let doc = std::fs::read_to_string(doc_path).unwrap();
        assert_eq!(beambench_service::agent::guide_markdown(), doc);
    }

    #[test]
    fn cleanup_design_render_artifacts_deletes_only_owned_artifacts() {
        let dir = std::env::temp_dir().join(format!(
            "beambench-render-cleanup-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let owned = dir.join(format!("{DESIGN_RENDER_TEMP_PREFIX}owned.svg"));
        let future_owned = dir.join(format!("{DESIGN_RENDER_TEMP_PREFIX}owned.png"));
        let unrelated = dir.join("user-export.svg");
        std::fs::write(&owned, "<svg/>").unwrap();
        std::fs::write(&future_owned, "png").unwrap();
        std::fs::write(&unrelated, "<svg/>").unwrap();

        let deleted =
            cleanup_design_render_artifacts(&dir, Duration::from_secs(0), true, SystemTime::now())
                .unwrap();

        assert_eq!(deleted, 2);
        assert!(!owned.exists());
        assert!(!future_owned.exists());
        assert!(unrelated.exists());
        let _ = std::fs::remove_file(unrelated);
        let _ = std::fs::remove_dir(dir);
    }

    #[test]
    fn cli_exit_error_json_preserves_body_and_exit_code() {
        let body = serde_json::json!({
            "schema_version": 1,
            "applied": false,
            "error": { "code": "INVALID_FIELD", "message": "bad input" },
            "warnings": []
        });
        let err = cli_exit_with_body(1, "bad input", body.clone());
        assert_eq!(cli_exit_code(err.as_ref()), 1);
        assert_eq!(format_cli_error_json(err.as_ref()), body);
    }

    fn write_temp_plan(contents: &str) -> String {
        let path = std::env::temp_dir().join(format!(
            "beambench-design-plan-{}.json",
            uuid::Uuid::new_v4()
        ));
        std::fs::write(&path, contents).unwrap();
        path.to_string_lossy().to_string()
    }

    #[test]
    fn cli_design_plan_missing_file_is_exit_2() {
        let path = std::env::temp_dir().join(format!(
            "beambench-missing-plan-{}.json",
            uuid::Uuid::new_v4()
        ));
        let err = read_design_plan(&path.to_string_lossy()).unwrap_err();
        assert_eq!(cli_exit_code(err.as_ref()), 2);
    }

    #[test]
    fn cli_design_plan_malformed_json_is_exit_3() {
        let path = write_temp_plan("{bad json");
        let err = read_design_plan(&path).unwrap_err();
        assert_eq!(cli_exit_code(err.as_ref()), 3);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn cli_design_plan_missing_schema_version_is_exit_3() {
        let path = write_temp_plan(r#"{"operations":[],"options":{}}"#);
        let err = read_design_plan(&path).unwrap_err();
        assert_eq!(cli_exit_code(err.as_ref()), 3);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn cli_design_plan_unsupported_schema_version_is_exit_3() {
        let path = write_temp_plan(r#"{"schema_version":2,"operations":[],"options":{}}"#);
        let err = read_design_plan(&path).unwrap_err();
        assert_eq!(cli_exit_code(err.as_ref()), 3);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn cli_error_json_shape_is_stable() {
        let err: Box<dyn std::error::Error> = Box::new(std::io::Error::other("boom"));
        let json = format_cli_error_json(err.as_ref());
        assert_eq!(json["error"]["code"], "cli_error");
        assert_eq!(json["error"]["message"], "boom");
    }

    #[test]
    fn cli_machine_set_origin_parses() {
        let cli = Cli::try_parse_from(["beambench", "machine", "set-origin"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Machine {
                command: MachineCmd::SetOrigin
            }
        ));
    }

    #[test]
    fn cli_machine_reset_origin_parses() {
        let cli = Cli::try_parse_from(["beambench", "machine", "reset-origin"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Machine {
                command: MachineCmd::ResetOrigin
            }
        ));
    }

    #[test]
    fn cli_machine_emergency_stop_parses() {
        let cli = Cli::try_parse_from(["beambench", "machine", "emergency-stop"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Machine {
                command: MachineCmd::EmergencyStop
            }
        ));
    }

    #[test]
    fn cli_no_json_flag_by_default() {
        let cli = Cli::try_parse_from(["beambench", "version"]).unwrap();
        assert!(!cli.json);
    }

    #[test]
    fn cli_missing_subcommand_fails() {
        let result = Cli::try_parse_from(["beambench"]);
        assert!(result.is_err());
    }

    #[test]
    fn cli_preview_stats_parses() {
        let cli = Cli::try_parse_from(["beambench", "preview", "stats", "input.lzrproj"]).unwrap();
        match cli.command {
            Commands::Preview {
                command: PreviewCmd::Stats { input },
            } => {
                assert_eq!(input, "input.lzrproj");
            }
            _ => panic!("Expected Preview Stats command"),
        }
    }

    #[test]
    fn cli_preview_generate_parses() {
        let cli =
            Cli::try_parse_from(["beambench", "preview", "generate", "input.lzrproj"]).unwrap();
        match cli.command {
            Commands::Preview {
                command: PreviewCmd::Generate { input },
            } => assert_eq!(input, "input.lzrproj"),
            _ => panic!("Expected Preview Generate command"),
        }
    }

    #[test]
    fn cli_diagnostics_export_parses() {
        let cli = Cli::try_parse_from(["beambench", "diagnostics", "export"]).unwrap();
        match cli.command {
            Commands::Diagnostics {
                command: DiagnosticsCmd::Export { output },
            } => {
                assert!(output.is_none());
            }
            _ => panic!("Expected Diagnostics Export command"),
        }
    }

    #[test]
    fn cli_diagnostics_export_with_output_parses() {
        let cli = Cli::try_parse_from([
            "beambench",
            "diagnostics",
            "export",
            "--output",
            "diag.json",
        ])
        .unwrap();
        match cli.command {
            Commands::Diagnostics {
                command: DiagnosticsCmd::Export { output },
            } => {
                assert_eq!(output, Some("diag.json".to_string()));
            }
            _ => panic!("Expected Diagnostics Export command"),
        }
    }

    #[test]
    fn cli_feedback_save_parses_report_args() {
        let cli = Cli::try_parse_from([
            "beambench",
            "feedback",
            "save",
            "--kind",
            "crash",
            "--title",
            "Crash on startup",
            "--description",
            "The app crashed",
            "--reply-to",
            "user@example.com",
            "--project",
            "fixture.lzrproj",
            "--include-project",
            "--output",
            "report.zip",
        ])
        .unwrap();
        match cli.command {
            Commands::Feedback {
                command: FeedbackCmd::Save { report, output },
            } => {
                assert_eq!(report.kind, CliFeedbackKind::Crash);
                assert_eq!(report.title.as_deref(), Some("Crash on startup"));
                assert_eq!(report.description.as_deref(), Some("The app crashed"));
                assert_eq!(report.reply_to.as_deref(), Some("user@example.com"));
                assert_eq!(report.project.as_deref(), Some("fixture.lzrproj"));
                assert!(report.include_project);
                assert_eq!(output.as_deref(), Some("report.zip"));
            }
            _ => panic!("Expected Feedback Save command"),
        }
    }

    #[test]
    fn cli_feedback_submit_parses_endpoint_and_connectivity_kind() {
        let cli = Cli::try_parse_from([
            "beambench",
            "--json",
            "feedback",
            "submit",
            "--kind",
            "connectivity",
            "--notes",
            "Port does not answer",
            "--endpoint",
            "http://127.0.0.1:3004/api/feedback/report",
        ])
        .unwrap();
        assert!(cli.json);
        match cli.command {
            Commands::Feedback {
                command: FeedbackCmd::Submit { report, endpoint },
            } => {
                assert_eq!(report.kind, CliFeedbackKind::Connectivity);
                assert_eq!(report.notes.as_deref(), Some("Port does not answer"));
                assert_eq!(
                    endpoint.as_deref(),
                    Some("http://127.0.0.1:3004/api/feedback/report")
                );
            }
            _ => panic!("Expected Feedback Submit command"),
        }
    }

    #[test]
    fn cli_feedback_diagnostics_parses_optional_project() {
        let cli = Cli::try_parse_from([
            "beambench",
            "feedback",
            "diagnostics",
            "--project",
            "p.lzrproj",
        ])
        .unwrap();
        match cli.command {
            Commands::Feedback {
                command: FeedbackCmd::Diagnostics { project },
            } => assert_eq!(project.as_deref(), Some("p.lzrproj")),
            _ => panic!("Expected Feedback Diagnostics command"),
        }
    }

    #[test]
    fn cli_feedback_include_project_requires_project_path() {
        let args = FeedbackReportArgs {
            kind: CliFeedbackKind::Bug,
            title: Some("Bug".to_owned()),
            description: Some("Description".to_owned()),
            notes: None,
            reply_to: None,
            project: None,
            include_project: true,
        };
        let err = feedback_input_from_args(&args).unwrap_err();
        assert_eq!(cli_exit_code(err.as_ref()), 3);
    }

    #[test]
    fn cli_profile_list_parses() {
        let cli = Cli::try_parse_from(["beambench", "profile", "list"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Profile {
                command: ProfileCmd::List
            }
        ));
    }

    #[test]
    fn cli_profile_show_parses() {
        let cli = Cli::try_parse_from(["beambench", "profile", "show", "My Laser"]).unwrap();
        match cli.command {
            Commands::Profile {
                command: ProfileCmd::Show { lookup },
            } => {
                assert_eq!(lookup, "My Laser");
            }
            _ => panic!("Expected Profile Show command"),
        }
    }

    #[test]
    fn cli_profile_preset_commands_parse() {
        let presets = Cli::try_parse_from(["beambench", "profile", "presets"]).unwrap();
        assert!(matches!(
            presets.command,
            Commands::Profile {
                command: ProfileCmd::Presets
            }
        ));

        let diff = Cli::try_parse_from([
            "beambench",
            "profile",
            "preset-diff",
            "sculpfun_s30_pro_max_20w",
            "--profile",
            "My Laser",
        ])
        .unwrap();
        match diff.command {
            Commands::Profile {
                command: ProfileCmd::PresetDiff { preset_id, profile },
            } => {
                assert_eq!(preset_id, "sculpfun_s30_pro_max_20w");
                assert_eq!(profile.as_deref(), Some("My Laser"));
            }
            _ => panic!("Expected preset diff"),
        }

        let apply = Cli::try_parse_from([
            "beambench",
            "profile",
            "apply-preset",
            "sculpfun_s30_pro_max_20w",
            "--confirm-diff",
        ])
        .unwrap();
        match apply.command {
            Commands::Profile {
                command:
                    ProfileCmd::ApplyPreset {
                        preset_id,
                        confirm_diff,
                        ..
                    },
            } => {
                assert_eq!(preset_id, "sculpfun_s30_pro_max_20w");
                assert!(confirm_diff);
            }
            _ => panic!("Expected apply preset"),
        }
    }

    #[test]
    fn cli_profile_create_parses() {
        let cli = Cli::try_parse_from([
            "beambench",
            "profile",
            "create",
            "--name",
            "My Laser",
            "--bed-width-mm",
            "400",
            "--use-constant-power",
            "--emit-s-every-g1",
            "--use-g0-for-overscan",
            "--enable-scanning-offset",
            "--scanning-offset",
            "1000:0.1",
            "--dot-width-mm",
            "0.15",
            "--enable-dot-width",
        ])
        .unwrap();
        match cli.command {
            Commands::Profile {
                command:
                    ProfileCmd::Create {
                        name,
                        bed_width_mm,
                        use_constant_power,
                        emit_s_every_g1,
                        use_g0_for_overscan,
                        enable_scanning_offset,
                        scanning_offsets,
                        dot_width_mm,
                        enable_dot_width,
                        ..
                    },
            } => {
                assert_eq!(name, "My Laser");
                assert_eq!(bed_width_mm, 400.0);
                assert!(use_constant_power);
                assert!(emit_s_every_g1);
                assert!(use_g0_for_overscan);
                assert!(enable_scanning_offset);
                assert_eq!(scanning_offsets, vec!["1000:0.1".to_string()]);
                assert_eq!(dot_width_mm, 0.15);
                assert!(enable_dot_width);
            }
            _ => panic!("Expected Profile Create command"),
        }
    }

    #[test]
    fn cli_profile_update_parses() {
        let cli = Cli::try_parse_from([
            "beambench",
            "profile",
            "update",
            "My Laser",
            "--firmware-type",
            "grbl-hal",
            "--use-constant-power",
            "true",
            "--scanning-offset",
            "2000:0.2",
        ])
        .unwrap();
        match cli.command {
            Commands::Profile {
                command:
                    ProfileCmd::Update {
                        lookup,
                        firmware_type,
                        use_constant_power,
                        scanning_offsets,
                        ..
                    },
            } => {
                assert_eq!(lookup, "My Laser");
                assert_eq!(firmware_type, Some("grbl-hal".to_string()));
                assert_eq!(use_constant_power, Some(true));
                assert_eq!(scanning_offsets, Some(vec!["2000:0.2".to_string()]));
            }
            _ => panic!("Expected Profile Update command"),
        }
    }

    #[test]
    fn parse_scanning_offset_entry_parses_speed_and_offset() {
        let entry = parse_scanning_offset_entry("1500:0.12").unwrap();
        assert_eq!(entry.speed_mm_min, 1500.0);
        assert_eq!(entry.offset_mm, 0.12);
    }

    #[test]
    fn cli_profile_activate_parses() {
        let cli = Cli::try_parse_from(["beambench", "profile", "activate", "My Laser"]).unwrap();
        match cli.command {
            Commands::Profile {
                command: ProfileCmd::Activate { lookup },
            } => assert_eq!(lookup, "My Laser"),
            _ => panic!("Expected Profile Activate command"),
        }
    }

    #[test]
    fn cli_profile_deactivate_parses() {
        let cli = Cli::try_parse_from(["beambench", "profile", "deactivate"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Profile {
                command: ProfileCmd::Deactivate
            }
        ));
    }

    #[test]
    fn cli_profile_bootstrap_parses() {
        let cli = Cli::try_parse_from([
            "beambench",
            "profile",
            "bootstrap",
            "candidate-1",
            "--profile-name",
            "Ruida Bootstrapped",
            "--activate",
            "false",
        ])
        .unwrap();
        match cli.command {
            Commands::Profile {
                command:
                    ProfileCmd::Bootstrap {
                        candidate_id,
                        profile_name,
                        activate,
                    },
            } => {
                assert_eq!(candidate_id, "candidate-1");
                assert_eq!(profile_name, Some("Ruida Bootstrapped".to_string()));
                assert!(!activate);
            }
            _ => panic!("Expected Profile Bootstrap command"),
        }
    }

    #[test]
    fn format_cli_error_json_uses_stable_shape() {
        let value = format_cli_error_json(&std::io::Error::other("boom"));
        assert_eq!(value["error"]["code"], "cli_error");
        assert_eq!(value["error"]["message"], "boom");
    }

    #[test]
    fn cli_asset_list_parses() {
        let cli = Cli::try_parse_from(["beambench", "asset", "list", "project.lzrproj"]).unwrap();
        match cli.command {
            Commands::Asset {
                command: AssetCmd::List { path },
            } => {
                assert_eq!(path, "project.lzrproj");
            }
            _ => panic!("Expected Asset List command"),
        }
    }

    #[test]
    fn cli_asset_export_parses() {
        let cli = Cli::try_parse_from([
            "beambench",
            "asset",
            "export",
            "project.lzrproj",
            "550e8400-e29b-41d4-a716-446655440000",
            "--output",
            "photo.png",
        ])
        .unwrap();
        match cli.command {
            Commands::Asset {
                command:
                    AssetCmd::Export {
                        path,
                        asset_id,
                        output,
                    },
            } => {
                assert_eq!(path, "project.lzrproj");
                assert_eq!(asset_id, "550e8400-e29b-41d4-a716-446655440000");
                assert_eq!(output, "photo.png");
            }
            _ => panic!("Expected Asset Export command"),
        }
    }

    #[test]
    fn cli_job_run_parses() {
        let cli = Cli::try_parse_from([
            "beambench",
            "job",
            "run",
            "project.lzrproj",
            "--port",
            "/dev/ttyUSB0",
        ])
        .unwrap();
        match cli.command {
            Commands::Job {
                command: JobCmd::Run { input, port, baud },
            } => {
                assert_eq!(input, "project.lzrproj");
                assert_eq!(port, "/dev/ttyUSB0");
                assert_eq!(baud, 115200);
            }
            _ => panic!("Expected Job Run command"),
        }
    }

    #[test]
    fn cli_job_run_custom_baud_parses() {
        let cli = Cli::try_parse_from([
            "beambench",
            "job",
            "run",
            "project.lzrproj",
            "--port",
            "COM3",
            "--baud",
            "9600",
        ])
        .unwrap();
        match cli.command {
            Commands::Job {
                command: JobCmd::Run { input, port, baud },
            } => {
                assert_eq!(input, "project.lzrproj");
                assert_eq!(port, "COM3");
                assert_eq!(baud, 9600);
            }
            _ => panic!("Expected Job Run command"),
        }
    }

    #[test]
    fn cli_job_run_requires_motion_confirmation_before_connecting() {
        let cli = Cli::try_parse_from([
            "beambench",
            "job",
            "run",
            "missing-project.lzrproj",
            "--port",
            "/definitely/missing",
        ])
        .unwrap();
        let err = run(cli).unwrap_err();
        let body = format_cli_error_json(err.as_ref());
        assert_eq!(cli_exit_code(err.as_ref()), 4);
        assert_eq!(body["error_code"], "CONFIRMATION_REQUIRED");
        assert_eq!(body["missing"][0], "confirm_motion");
    }

    #[test]
    fn cli_job_run_requires_laser_confirmation_before_connecting() {
        let cli = Cli::try_parse_from([
            "beambench",
            "--confirm-motion",
            "job",
            "run",
            "missing-project.lzrproj",
            "--port",
            "/definitely/missing",
        ])
        .unwrap();
        let err = run(cli).unwrap_err();
        let body = format_cli_error_json(err.as_ref());
        assert_eq!(cli_exit_code(err.as_ref()), 4);
        assert_eq!(body["error_code"], "CONFIRMATION_REQUIRED");
        assert_eq!(body["missing"][0], "confirm_laser_on");
    }

    #[test]
    fn cli_job_dry_run_parses() {
        let cli = Cli::try_parse_from(["beambench", "job", "dry-run", "project.lzrproj"]).unwrap();
        match cli.command {
            Commands::Job {
                command: JobCmd::DryRun { input },
            } => {
                assert_eq!(input, "project.lzrproj");
            }
            _ => panic!("Expected Job DryRun command"),
        }
    }

    #[test]
    fn cli_job_preflight_parses() {
        let cli =
            Cli::try_parse_from(["beambench", "job", "preflight", "project.lzrproj"]).unwrap();
        match cli.command {
            Commands::Job {
                command: JobCmd::Preflight { input },
            } => {
                assert_eq!(input, "project.lzrproj");
            }
            _ => panic!("Expected Job Preflight command"),
        }
    }

    #[test]
    fn cli_job_pause_parses() {
        let cli = Cli::try_parse_from(["beambench", "job", "pause"]).unwrap();
        match cli.command {
            Commands::Job {
                command: JobCmd::Pause,
            } => {}
            _ => panic!("Expected Job Pause command"),
        }
    }

    #[test]
    fn cli_job_resume_parses() {
        let cli = Cli::try_parse_from(["beambench", "job", "resume"]).unwrap();
        match cli.command {
            Commands::Job {
                command: JobCmd::Resume,
            } => {}
            _ => panic!("Expected Job Resume command"),
        }
    }

    #[test]
    fn cli_job_cancel_parses() {
        let cli = Cli::try_parse_from(["beambench", "job", "cancel"]).unwrap();
        match cli.command {
            Commands::Job {
                command: JobCmd::Cancel,
            } => {}
            _ => panic!("Expected Job Cancel command"),
        }
    }

    #[test]
    fn cli_job_progress_parses() {
        let cli = Cli::try_parse_from(["beambench", "job", "progress"]).unwrap();
        match cli.command {
            Commands::Job {
                command: JobCmd::Progress,
            } => {}
            _ => panic!("Expected Job Progress command"),
        }
    }

    #[test]
    fn cli_job_frame_parses() {
        let cli = Cli::try_parse_from([
            "beambench",
            "job",
            "frame",
            "--input",
            "project.lzrproj",
            "--mode",
            "rubber_band",
            "--selected-ids",
            "obj-1,obj-2",
        ])
        .unwrap();
        match cli.command {
            Commands::Job {
                command:
                    JobCmd::Frame {
                        input,
                        mode,
                        selected_ids,
                        laser_on,
                    },
            } => {
                assert_eq!(input, Some("project.lzrproj".to_string()));
                assert_eq!(mode, "rubber_band");
                assert_eq!(selected_ids, vec!["obj-1".to_string(), "obj-2".to_string()]);
                assert!(!laser_on);
            }
            _ => panic!("Expected Job Frame command"),
        }
    }

    #[test]
    fn cli_job_run_missing_port_fails() {
        let result = Cli::try_parse_from(["beambench", "job", "run", "project.lzrproj"]);
        assert!(result.is_err());
    }

    #[test]
    fn cli_asset_import_parses() {
        let cli = Cli::try_parse_from([
            "beambench",
            "asset",
            "import",
            "project.lzrproj",
            "photo.png",
        ])
        .unwrap();
        match cli.command {
            Commands::Asset {
                command:
                    AssetCmd::Import {
                        project,
                        file,
                        output,
                    },
            } => {
                assert_eq!(project, "project.lzrproj");
                assert_eq!(file, "photo.png");
                assert!(output.is_none());
            }
            _ => panic!("Expected Asset Import command"),
        }
    }

    #[test]
    fn cli_asset_import_with_output_parses() {
        let cli = Cli::try_parse_from([
            "beambench",
            "asset",
            "import",
            "project.lzrproj",
            "photo.png",
            "--output",
            "updated.lzrproj",
        ])
        .unwrap();
        match cli.command {
            Commands::Asset {
                command:
                    AssetCmd::Import {
                        project,
                        file,
                        output,
                    },
            } => {
                assert_eq!(project, "project.lzrproj");
                assert_eq!(file, "photo.png");
                assert_eq!(output, Some("updated.lzrproj".to_string()));
            }
            _ => panic!("Expected Asset Import command"),
        }
    }

    #[test]
    fn cli_machine_home_parses() {
        let cli = Cli::try_parse_from(["beambench", "machine", "home"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Machine {
                command: MachineCmd::Home
            }
        ));
    }

    #[test]
    fn cli_asset_export_missing_output_fails() {
        let result = Cli::try_parse_from([
            "beambench",
            "asset",
            "export",
            "project.lzrproj",
            "550e8400-e29b-41d4-a716-446655440000",
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn cli_console_send_parses() {
        let cli = Cli::try_parse_from(["beambench", "console", "send", "G0 X10"]).unwrap();
        match cli.command {
            Commands::Console {
                command: ConsoleCmd::Send { line },
            } => assert_eq!(line, "G0 X10"),
            _ => panic!("Expected Console Send command"),
        }
    }

    #[test]
    fn cli_console_log_parses() {
        let cli = Cli::try_parse_from(["beambench", "console", "log", "--limit", "50"]).unwrap();
        match cli.command {
            Commands::Console {
                command: ConsoleCmd::Log { limit },
            } => assert_eq!(limit, 50),
            _ => panic!("Expected Console Log command"),
        }
    }

    #[test]
    fn cli_console_log_default_limit_parses() {
        let cli = Cli::try_parse_from(["beambench", "console", "log"]).unwrap();
        match cli.command {
            Commands::Console {
                command: ConsoleCmd::Log { limit },
            } => assert_eq!(limit, 100),
            _ => panic!("Expected Console Log command"),
        }
    }

    #[test]
    fn cli_console_interactive_parses() {
        let cli = Cli::try_parse_from(["beambench", "console", "interactive"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Console {
                command: ConsoleCmd::Interactive
            }
        ));
    }

    #[test]
    fn cli_material_list_parses() {
        let cli = Cli::try_parse_from(["beambench", "material", "list"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Material {
                command: MaterialCmd::List
            }
        ));
    }

    #[test]
    fn cli_material_add_parses() {
        let cli = Cli::try_parse_from([
            "beambench",
            "material",
            "add",
            "--name",
            "Plywood 3mm",
            "--material",
            "Wood",
            "--speed",
            "500",
            "--power",
            "80",
            "--passes",
            "2",
        ])
        .unwrap();
        match cli.command {
            Commands::Material {
                command:
                    MaterialCmd::Add {
                        name,
                        material,
                        speed,
                        power,
                        passes,
                    },
            } => {
                assert_eq!(name, "Plywood 3mm");
                assert_eq!(material, "Wood");
                assert_eq!(speed, 500.0);
                assert_eq!(power, 80.0);
                assert_eq!(passes, 2);
            }
            _ => panic!("Expected Material Add command"),
        }
    }

    #[test]
    fn cli_material_remove_parses() {
        let cli = Cli::try_parse_from([
            "beambench",
            "material",
            "remove",
            "550e8400-e29b-41d4-a716-446655440000",
        ])
        .unwrap();
        match cli.command {
            Commands::Material {
                command: MaterialCmd::Remove { id },
            } => assert_eq!(id, "550e8400-e29b-41d4-a716-446655440000"),
            _ => panic!("Expected Material Remove command"),
        }
    }

    #[test]
    fn cli_macro_list_parses() {
        let cli = Cli::try_parse_from(["beambench", "macro", "list"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Macro {
                command: MacroCmd::List
            }
        ));
    }

    #[test]
    fn cli_macro_add_parses() {
        let cli = Cli::try_parse_from([
            "beambench",
            "macro",
            "add",
            "--name",
            "Home & Zero",
            "--description",
            "Home then zero out",
            "--commands",
            "$H, G92 X0 Y0",
        ])
        .unwrap();
        match cli.command {
            Commands::Macro {
                command:
                    MacroCmd::Add {
                        name,
                        description,
                        commands,
                    },
            } => {
                assert_eq!(name, "Home & Zero");
                assert_eq!(description, "Home then zero out");
                assert_eq!(commands, "$H, G92 X0 Y0");
            }
            _ => panic!("Expected Macro Add command"),
        }
    }

    #[test]
    fn cli_macro_remove_parses() {
        let cli = Cli::try_parse_from([
            "beambench",
            "macro",
            "remove",
            "550e8400-e29b-41d4-a716-446655440000",
        ])
        .unwrap();
        match cli.command {
            Commands::Macro {
                command: MacroCmd::Remove { id },
            } => assert_eq!(id, "550e8400-e29b-41d4-a716-446655440000"),
            _ => panic!("Expected Macro Remove command"),
        }
    }

    #[test]
    fn cli_macro_run_parses() {
        let cli = Cli::try_parse_from([
            "beambench",
            "macro",
            "run",
            "550e8400-e29b-41d4-a716-446655440000",
        ])
        .unwrap();
        match cli.command {
            Commands::Macro {
                command: MacroCmd::Run { id },
            } => assert_eq!(id, "550e8400-e29b-41d4-a716-446655440000"),
            _ => panic!("Expected Macro Run command"),
        }
    }

    #[test]
    fn cli_import_dxf_parses() {
        let cli = Cli::try_parse_from([
            "beambench",
            "import",
            "dxf",
            "drawing.dxf",
            "--layer",
            "layer-id",
        ])
        .unwrap();
        match cli.command {
            Commands::Import {
                command: ImportCmd::Dxf { file, layer },
            } => {
                assert_eq!(file, "drawing.dxf");
                assert_eq!(layer, "layer-id");
            }
            _ => panic!("Expected Import Dxf command"),
        }
    }

    #[test]
    fn cli_import_pdf_parses() {
        let cli = Cli::try_parse_from([
            "beambench",
            "import",
            "pdf",
            "design.pdf",
            "--layer",
            "layer-id",
        ])
        .unwrap();
        match cli.command {
            Commands::Import {
                command: ImportCmd::Pdf { file, layer },
            } => {
                assert_eq!(file, "design.pdf");
                assert_eq!(layer, "layer-id");
            }
            _ => panic!("Expected Import Pdf command"),
        }
    }

    #[test]
    fn cli_import_ai_parses() {
        let cli = Cli::try_parse_from([
            "beambench",
            "import",
            "ai",
            "artwork.ai",
            "--layer",
            "layer-id",
        ])
        .unwrap();
        match cli.command {
            Commands::Import {
                command: ImportCmd::Ai { file, layer },
            } => {
                assert_eq!(file, "artwork.ai");
                assert_eq!(layer, "layer-id");
            }
            _ => panic!("Expected Import Ai command"),
        }
    }

    #[test]
    fn cli_import_dxf_missing_layer_fails() {
        let result = Cli::try_parse_from(["beambench", "import", "dxf", "drawing.dxf"]);
        assert!(result.is_err());
    }
}

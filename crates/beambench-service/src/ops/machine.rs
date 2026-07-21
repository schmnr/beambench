use std::collections::HashMap;
use std::net::ToSocketAddrs;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use beambench_common::StartFromMode;
use beambench_common::controller_choice::{
    ControllerChoiceOutcome, ControllerConnectionEndpoint, ControllerConnectionResult,
    ControllerDriverId, ControllerMismatchDecision, ControllerSelection,
    ExplicitControllerSelection, PositiveControllerIdentity, ResolvedControllerChoice,
};
use beambench_common::feedback::DiagnosticTerminalJob;
use beambench_common::geometry::{Bounds, Point2D};
use beambench_common::machine::{
    ControllerEvidenceState, ControllerFamily, ControllerModel, ControllerProductTier,
    DeviceCapabilities, DeviceIdentity, JobProgress, JobState, MachineConnectionTarget,
    MachineRunState, MachineStatus, PortInfo, PreflightCheck, PreflightOutcome, PreflightReport,
    SessionState, TransportKind,
};
use beambench_core::{MachineProfile, MachineProfileId, Project, RuidaTableAxis, Workspace};
use beambench_grbl::{
    FluidNcNetworkAdapter, FluidNcSerialAdapter, GrblFamilyAdapter, GrblFamilyIdentityProbeConfig,
    GrblFamilyIdentityProbeOutcome, GrblFamilyIdentityProbeResult, GrblHalNetworkAdapter,
    GrblHalSerialAdapter, GrblResponse, GrblSession, GrblSettings, commands as grbl_commands,
};
use beambench_lihuiyu::{
    LihuiyuCompilationConfig, LihuiyuCompiledJob, LihuiyuCoordinateTransform, LihuiyuUsbDeviceInfo,
    LihuiyuUsbIo, LihuiyuUsbSelector, NativeLihuiyuUsbIo, compile_lihuiyu_job,
    enumerate_lihuiyu_usb_devices,
};
use beambench_marlin::{
    MarlinGcodeConfig, MarlinPowerScale, MarlinSerialSession, MarlinSerialSessionConfig,
    SnapmakerGcodeConfig, SnapmakerLaserMode, generate_marlin_gcode, generate_snapmaker_gcode,
};
use beambench_planner::{
    ExecutionPlan, anchor_segments_to_target, apply_workspace_origin_transform, build_frame_plan,
    build_hull_frame_plan, calculate_plan_bounds, flip_bounds_y,
};
use beambench_ruida::{
    RuidaCompilationConfig, RuidaCompiledJob, RuidaCoordinateTransform, RuidaJogAxis,
    RuidaPowerOverride, RuidaUdpIo, compile_ruida_job,
};
use beambench_serial::{RealSerialTransport, TcpLineTransport, list_available_ports};
use beambench_smoothieware::{
    SmoothiewareGcodeConfig, SmoothiewarePowerMode, SmoothiewarePowerScale,
    SmoothiewareSerialSession, SmoothiewareSerialSessionConfig, generate_smoothieware_gcode,
};
use beambench_streamer::{
    JobController, check_profile_mismatch, check_raster_motion_bounds, check_tool_layers,
    run_preflight,
};
use serde::Serialize;
use serde_json::json;

use crate::context::{LaserFireState, ServiceContext};
use crate::error::{ServiceError, ServiceResult};
use crate::events;
use crate::lihuiyu_runtime::LihuiyuRuntimeSession;
use crate::ops::{controller_choice, discovery, planning};
use crate::persist::persist_settings_to_disk;
use crate::ruida_runtime::RuidaRuntimeSession;
use crate::runtime::{
    ActiveJobHandle, GrblRuntimeSession, MachineSessionHandle, MarlinRuntimeJob,
    MarlinRuntimeSession, PendingControllerConnection, PendingControllerSession,
    SmoothiewareRuntimeJob, SmoothiewareRuntimeSession,
};

const MAX_BANNER_POLLS: usize = 50;
const DEFAULT_MOVE_FEED_RATE_MM_MIN: f64 = 3000.0;
const FIRE_KEEPALIVE_GRACE: Duration = Duration::from_millis(900);
const FIRE_KEEPALIVE_INTERVAL_MS: u64 = 250;
const FIRE_MAX_HOLD: Duration = Duration::from_secs(10);
const FIRE_DEADMAN_POLL: Duration = Duration::from_millis(100);
const TERMINAL_JOB_CONSOLE_LIMIT: usize = 120;
const CONTROLLER_CONNECTION_CHALLENGE_TTL: Duration = Duration::from_secs(120);
const CONTROLLER_COMPATIBILITY_STATUS_TIMEOUT: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SerialControllerProtocol {
    Grbl,
    Marlin,
    Smoothieware,
}

fn protocol_for_selection(selection: &ControllerSelection) -> Option<SerialControllerProtocol> {
    match selection {
        ControllerSelection::KnownDriver {
            driver: ControllerDriverId::Marlin | ControllerDriverId::Snapmaker,
        } => Some(SerialControllerProtocol::Marlin),
        ControllerSelection::KnownDriver {
            driver: ControllerDriverId::Smoothieware,
        } => Some(SerialControllerProtocol::Smoothieware),
        ControllerSelection::KnownDriver {
            driver: ControllerDriverId::Ruida | ControllerDriverId::Lihuiyu,
        } => None,
        ControllerSelection::KnownDriver { .. } | ControllerSelection::GenericGrblCompatible => {
            Some(SerialControllerProtocol::Grbl)
        }
        ControllerSelection::AutoDetect | ControllerSelection::Unknown => None,
    }
}

fn protocol_for_pending(session: &PendingControllerSession) -> SerialControllerProtocol {
    match session {
        PendingControllerSession::Grbl { .. } => SerialControllerProtocol::Grbl,
        PendingControllerSession::Marlin { .. } => SerialControllerProtocol::Marlin,
        PendingControllerSession::Smoothieware { .. } => SerialControllerProtocol::Smoothieware,
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct MachineRuntimeState {
    pub controller_family: Option<ControllerFamily>,
    pub controller_model: Option<ControllerModel>,
    pub controller_driver: Option<ControllerDriverId>,
    pub controller_selection: Option<ExplicitControllerSelection>,
    pub detected_controller_identity: Option<PositiveControllerIdentity>,
    pub experimental_mode: bool,
    pub product_tier: Option<ControllerProductTier>,
    pub evidence_state: Option<ControllerEvidenceState>,
    pub transport_kind: Option<TransportKind>,
    pub capabilities: Option<DeviceCapabilities>,
    pub session_state: SessionState,
    pub machine_status: Option<MachineStatus>,
    pub machine_coordinates_valid: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub controller_info: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub controller_settings: Option<HashMap<String, String>>,
    pub job_active: bool,
    pub job_progress: Option<JobProgress>,
    /// Non-None when the last `tick_job` call failed.  Callers can use this
    /// to distinguish "no job running" (`job_progress: None, job_tick_error: None`)
    /// from "job polling broken" (`job_progress: None, job_tick_error: Some(...)`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_tick_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ProfileAutoSync {
    pub profile_id: MachineProfileId,
    pub profile_name: String,
    pub created_profile: bool,
    pub source: &'static str,
    pub bed_width_mm: Option<f64>,
    pub bed_height_mm: Option<f64>,
    pub max_speed_mm_min: Option<f64>,
    pub s_value_max: Option<u32>,
    pub homing_enabled: Option<bool>,
    pub project_workspace_synced: bool,
}

#[derive(Debug, Clone)]
pub struct ConnectMachineInput {
    pub target: MachineConnectionTarget,
}

#[derive(Debug, Clone)]
pub struct BeginControllerConnectionInput {
    pub port_name: String,
    pub baud_rate: u32,
    pub selection: ControllerSelection,
}

#[derive(Debug, Clone)]
pub struct BeginNetworkControllerConnectionInput {
    pub host: String,
    pub port: u16,
    pub selection: ControllerSelection,
}

#[derive(Debug, Clone)]
pub struct BeginUsbControllerConnectionInput {
    pub bus_id: String,
    pub device_address: u8,
    pub port_numbers: Vec<u8>,
    pub selection: ControllerSelection,
}

#[derive(Debug, Clone)]
pub struct ContinueControllerConnectionInput {
    pub attempt_id: String,
    pub selection: ControllerSelection,
    pub decision: Option<ControllerMismatchDecision>,
}

#[derive(Debug, Clone)]
pub struct JogMachineInput {
    pub x_mm: f64,
    pub y_mm: f64,
    pub z_mm: Option<f64>,
    pub feed_rate: f64,
    pub continuous: bool,
}

#[derive(Debug, Clone)]
pub struct MoveLaserInput {
    pub x: f64,
    pub y: f64,
    pub z: Option<f64>,
    pub feed_rate: f64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LaserFireStartResult {
    pub token: String,
    pub power_percent: f64,
    pub s_value: u32,
    pub keepalive_interval_ms: u64,
    pub max_duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TestAirAssistResult {
    pub tested: bool,
    pub duration_ms: u64,
    pub air_assist_on_gcode: String,
    pub air_assist_off_gcode: String,
    pub advisory_text: Option<String>,
}

fn lock_err(name: &str, e: impl std::fmt::Display) -> ServiceError {
    ServiceError::internal(format!("Failed to lock {name}: {e}"))
}

fn invalid_capability(action: &str, family: ControllerFamily) -> ServiceError {
    ServiceError::invalid_state(format!(
        "{action} is not supported by the connected {family:?} controller"
    ))
}

pub(crate) fn marlin_gcode_config(gcode: beambench_grbl::GcodeConfig) -> MarlinGcodeConfig {
    let maximum = gcode.s_value_max;
    MarlinGcodeConfig {
        gcode,
        power_scale: MarlinPowerScale::Custom { maximum },
        ..MarlinGcodeConfig::default()
    }
}

fn snapmaker_gcode_config(gcode: beambench_grbl::GcodeConfig) -> SnapmakerGcodeConfig {
    let laser_mode = if gcode.use_constant_power {
        SnapmakerLaserMode::Constant
    } else {
        SnapmakerLaserMode::Dynamic
    };
    SnapmakerGcodeConfig { gcode, laser_mode }
}

pub(crate) fn acknowledged_gcode_commands(
    driver: ControllerDriverId,
    plan: &beambench_planner::ExecutionPlan,
    gcode: beambench_grbl::GcodeConfig,
) -> Result<Vec<String>, String> {
    match driver {
        ControllerDriverId::Marlin => generate_marlin_gcode(plan, &marlin_gcode_config(gcode))
            .map_err(|error| format!("Marlin G-code generation failed: {error}")),
        ControllerDriverId::Snapmaker => {
            generate_snapmaker_gcode(plan, &snapmaker_gcode_config(gcode))
                .map_err(|error| format!("Snapmaker G-code generation failed: {error}"))
        }
        _ => Err(format!(
            "{driver:?} is not an acknowledged Marlin-derived G-code driver"
        )),
    }
}

pub(crate) fn smoothieware_gcode_commands(
    session: &SmoothiewareRuntimeSession,
    plan: &beambench_planner::ExecutionPlan,
    gcode: beambench_grbl::GcodeConfig,
) -> Result<Vec<String>, String> {
    let maximum_s_value = session
        .laser_maximum_s_value()
        .ok_or_else(|| "Smoothieware laser power scale is unavailable".to_string())?;
    let power_mode = if gcode.use_constant_power {
        SmoothiewarePowerMode::Constant
    } else {
        SmoothiewarePowerMode::SpeedProportional
    };
    generate_smoothieware_gcode(
        plan,
        &SmoothiewareGcodeConfig {
            gcode,
            power_mode,
            power_scale: SmoothiewarePowerScale { maximum_s_value },
        },
    )
    .map_err(|error| format!("Smoothieware G-code generation failed: {error}"))
}

pub(crate) fn compile_ruida_execution_plan(
    plan: &ExecutionPlan,
    project: &Project,
    profile: &MachineProfile,
) -> Result<RuidaCompiledJob, String> {
    if project.start_from == StartFromMode::CurrentPosition {
        return Err(
            "Ruida RDC6442S does not report an absolute current position; choose Absolute Coordinates or User Origin before running the job"
                .to_string(),
        );
    }
    let mut air_assist_cut_entry_ids = Vec::new();
    let mut min_power_overrides = Vec::new();
    for entry in project.layers.iter().flat_map(|layer| layer.entries.iter()) {
        let cut_entry_id = entry.id.to_string();
        if entry.air_assist {
            air_assist_cut_entry_ids.push(cut_entry_id.clone());
        }
        min_power_overrides.push(RuidaPowerOverride {
            cut_entry_id,
            min_power_percent: entry.power_min_percent,
        });
    }
    let config = RuidaCompilationConfig {
        coordinate_transform: RuidaCoordinateTransform {
            bed_width_mm: profile.bed_width_mm,
            bed_height_mm: profile.bed_height_mm,
            ..RuidaCoordinateTransform::default()
        },
        air_assist_cut_entry_ids,
        min_power_overrides,
        ..RuidaCompilationConfig::default()
    };
    compile_ruida_job(plan, &config)
        .map_err(|error| format!("Ruida job compilation failed: {error}"))
}

pub(crate) fn compile_lihuiyu_execution_plan(
    plan: &ExecutionPlan,
    project: &Project,
    profile: &MachineProfile,
) -> Result<LihuiyuCompiledJob, String> {
    if project.start_from == StartFromMode::CurrentPosition {
        return Err(
            "Lihuiyu M2/M3 Nano does not report an absolute current position; choose Absolute Coordinates or User Origin before running the job"
                .to_string(),
        );
    }
    let config = LihuiyuCompilationConfig {
        coordinate_transform: LihuiyuCoordinateTransform {
            bed_width_mm: profile.bed_width_mm,
            bed_height_mm: profile.bed_height_mm,
            ..LihuiyuCoordinateTransform::default()
        },
        ..LihuiyuCompilationConfig::default()
    };
    compile_lihuiyu_job(plan, &config)
        .map_err(|error| format!("Lihuiyu job compilation failed: {error}"))
}

/// Controller-agnostic preflight rows shared by every non-GRBL session:
/// session ready, machine idle, plan not empty, burn bounds within the bed,
/// and raster motion (including overscan travel) within the bed. Returns the
/// rows plus whether they all passed.
fn generic_preflight_checks(
    session_state: SessionState,
    run_state: MachineRunState,
    plan: &ExecutionPlan,
    profile: &MachineProfile,
) -> (Vec<PreflightCheck>, bool) {
    let session_ready = session_state == SessionState::Ready;
    let machine_idle = run_state == MachineRunState::Idle;
    let plan_not_empty = !plan.segments.is_empty();
    let bounds_fit = plan.bounds.min.x >= 0.0
        && plan.bounds.min.y >= 0.0
        && plan.bounds.max.x <= profile.bed_width_mm
        && plan.bounds.max.y <= profile.bed_height_mm;
    let mut checks = vec![
        PreflightCheck {
            category: "connection".to_string(),
            description: "Session is in Ready state".to_string(),
            passed: session_ready,
            message: if session_ready {
                "Connected and ready".to_string()
            } else {
                format!("Session state: {session_state:?}")
            },
        },
        PreflightCheck {
            category: "machine".to_string(),
            description: "Machine is idle".to_string(),
            passed: machine_idle,
            message: format!("Machine state: {run_state:?}"),
        },
        PreflightCheck {
            category: "plan".to_string(),
            description: "Plan has segments".to_string(),
            passed: plan_not_empty,
            message: format!("{} segments", plan.segments.len()),
        },
        PreflightCheck {
            category: "bounds".to_string(),
            description: "Plan fits within machine bed".to_string(),
            passed: bounds_fit,
            message: format!(
                "Plan bounds ({:.1},{:.1} to {:.1},{:.1}); bed {:.0}x{:.0}mm",
                plan.bounds.min.x,
                plan.bounds.min.y,
                plan.bounds.max.x,
                plan.bounds.max.y,
                profile.bed_width_mm,
                profile.bed_height_mm
            ),
        },
    ];
    let raster_motion = check_raster_motion_bounds(plan, profile);
    let raster_motion_ok = raster_motion.as_ref().is_none_or(|check| check.passed);
    if let Some(check) = raster_motion {
        checks.push(check);
    }
    (
        checks,
        session_ready && machine_idle && plan_not_empty && bounds_fit && raster_motion_ok,
    )
}

fn run_ruida_preflight(
    session: &RuidaRuntimeSession,
    plan: &ExecutionPlan,
    project: &Project,
    profile: &MachineProfile,
) -> PreflightReport {
    let (mut checks, basics_ok) = generic_preflight_checks(
        session.session_state(),
        session.machine_status().run_state,
        plan,
        profile,
    );
    let compilation = compile_ruida_execution_plan(plan, project, profile);
    let compilation_ok = compilation.is_ok();
    checks.push(PreflightCheck {
        category: "controller".to_string(),
        description: "Plan compiles for Ruida RDC6442S".to_string(),
        passed: compilation_ok,
        message: compilation
            .map(|job| {
                format!(
                    "Compiled {} controller layers into {} bytes",
                    job.layers.len(),
                    job.clear_bytes.len()
                )
            })
            .unwrap_or_else(|error| error),
    });
    PreflightReport {
        outcome: if basics_ok && compilation_ok {
            PreflightOutcome::Pass
        } else {
            PreflightOutcome::Fail
        },
        checks,
    }
}

fn run_lihuiyu_preflight(
    session: &LihuiyuRuntimeSession,
    plan: &ExecutionPlan,
    project: &Project,
    profile: &MachineProfile,
) -> PreflightReport {
    let (mut checks, basics_ok) = generic_preflight_checks(
        session.session_state(),
        session.machine_status().run_state,
        plan,
        profile,
    );
    let compilation = compile_lihuiyu_execution_plan(plan, project, profile);
    let compilation_ok = compilation.is_ok();
    checks.push(PreflightCheck {
        category: "controller".to_string(),
        description: "Plan compiles for Lihuiyu M2-compatible mode".to_string(),
        passed: compilation_ok,
        message: compilation
            .map(|job| {
                format!(
                    "Compiled {} controller packets with binary panel-set power",
                    job.packets.len()
                )
            })
            .unwrap_or_else(|error| error),
    });
    PreflightReport {
        outcome: if basics_ok && compilation_ok {
            PreflightOutcome::Pass
        } else {
            PreflightOutcome::Fail
        },
        checks,
    }
}

fn require_ready_for_motion(session: &MachineSessionHandle, action: &str) -> ServiceResult<()> {
    if session.session_state() != SessionState::Ready {
        return Err(ServiceError::invalid_state(format!(
            "{action} requires a ready machine session. Current session state is {:?}; unlock or reconnect before moving.",
            session.session_state()
        )));
    }
    Ok(())
}

pub(super) fn require_idle_ready_for_motion(
    session: &MachineSessionHandle,
    action: &str,
) -> ServiceResult<()> {
    require_ready_for_motion(session, action)?;
    let run_state = session.machine_status().run_state;
    if run_state != MachineRunState::Idle {
        return Err(ServiceError::invalid_state(format!(
            "{action} requires an idle machine. Current run state is {run_state:?}."
        )));
    }
    Ok(())
}

fn recover_ready_if_idle_after_validation(session: &mut MachineSessionHandle) -> ServiceResult<()> {
    let MachineSessionHandle::Grbl(grbl) = session else {
        return Ok(());
    };
    if grbl.session_state() == SessionState::Validating
        && grbl.last_status().run_state == MachineRunState::Idle
    {
        grbl.mark_ready()
            .map_err(|e| ServiceError::machine(e.to_string()))?;
    }
    Ok(())
}

fn job_is_active(progress: Option<&JobProgress>) -> bool {
    matches!(
        progress.map(|progress| progress.state),
        Some(JobState::Preparing | JobState::ReadyToRun | JobState::Running | JobState::Paused)
    )
}

fn active_profile(ctx: &ServiceContext) -> ServiceResult<MachineProfile> {
    let settings = ctx.settings.lock().map_err(|e| lock_err("settings", e))?;
    Ok(settings
        .active_profile_id
        .and_then(|id| settings.machine_profiles.iter().find(|p| p.id == id))
        .cloned()
        .unwrap_or_default())
}

fn positive_finite(value: Option<f64>) -> Option<f64> {
    value.filter(|value| value.is_finite() && *value > 0.0)
}

fn require_positive_finite(value: f64, name: &str) -> ServiceResult<()> {
    if !value.is_finite() || value <= 0.0 {
        return Err(ServiceError::invalid_input(format!(
            "{name} must be a positive finite number"
        )));
    }
    Ok(())
}

fn require_finite(value: f64, name: &str) -> ServiceResult<()> {
    if !value.is_finite() {
        return Err(ServiceError::invalid_input(format!(
            "{name} must be a finite number"
        )));
    }
    Ok(())
}

fn validate_optional_z(
    profile: &MachineProfile,
    z: Option<f64>,
    action: &str,
) -> ServiceResult<()> {
    if z.is_some_and(|value| !value.is_finite()) {
        return Err(ServiceError::invalid_input("z must be a finite number"));
    }
    if z.is_some() && !profile.supports_z_moves {
        return Err(ServiceError::invalid_state(format!(
            "{action} includes Z but the active profile does not support Z moves"
        )));
    }
    Ok(())
}

fn nearly_equal(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

fn grbl_axis_max_speed(settings: &GrblSettings) -> Option<f64> {
    match (
        positive_finite(settings.max_rate_x()),
        positive_finite(settings.max_rate_y()),
    ) {
        (Some(x), Some(y)) => Some(x.min(y)),
        (Some(x), None) => Some(x),
        (None, Some(y)) => Some(y),
        (None, None) => None,
    }
}

fn apply_grbl_settings_to_profile(
    profile: &mut MachineProfile,
    grbl_settings: &GrblSettings,
    baud_rate: Option<u32>,
) -> ProfileAutoSync {
    let bed_width_mm = positive_finite(grbl_settings.max_travel_x());
    let bed_height_mm = positive_finite(grbl_settings.max_travel_y());
    let preset_effective_bed = profile.preset_id.as_deref().and_then(|preset_id| {
        crate::ops::profiles::profile_presets()
            .into_iter()
            .find(|preset| preset.id == preset_id)
            .map(|preset| (preset.bed_width_mm, preset.bed_height_mm))
    });
    let max_speed_mm_min = grbl_axis_max_speed(grbl_settings);
    let s_value_max =
        positive_finite(grbl_settings.max_spindle_speed()).map(|value| value.round() as u32);
    let homing_enabled = grbl_settings.get(22).map(|value| value != 0.0);

    let (applied_bed_width_mm, applied_bed_height_mm) =
        if let Some((preset_width, preset_height)) = preset_effective_bed {
            // GRBL $130/$131 report axis travel, which can be larger than the
            // effective engraving area for preset machines with large modules.
            profile.bed_width_mm = preset_width;
            profile.bed_height_mm = preset_height;
            (Some(preset_width), Some(preset_height))
        } else {
            if let Some(value) = bed_width_mm {
                profile.bed_width_mm = value;
            }
            if let Some(value) = bed_height_mm {
                profile.bed_height_mm = value;
            }
            (bed_width_mm, bed_height_mm)
        };
    if let Some(value) = max_speed_mm_min {
        profile.max_speed_mm_min = value;
    }
    if let Some(value) = s_value_max {
        profile.s_value_max = value.max(1);
    }
    if let Some(value) = homing_enabled {
        profile.homing_enabled = value;
    }
    if let Some(value) = baud_rate {
        profile.default_baud_rate = value;
    }
    profile.firmware_type = "grbl".to_string();

    ProfileAutoSync {
        profile_id: profile.id,
        profile_name: profile.name.clone(),
        created_profile: false,
        source: "grbl_settings",
        bed_width_mm: applied_bed_width_mm,
        bed_height_mm: applied_bed_height_mm,
        max_speed_mm_min,
        s_value_max,
        homing_enabled,
        project_workspace_synced: false,
    }
}

fn workspace_from_profile(profile: &MachineProfile) -> Workspace {
    Workspace {
        bed_width_mm: profile.bed_width_mm,
        bed_height_mm: profile.bed_height_mm,
        origin: profile.origin,
    }
}

pub(crate) fn sync_open_project_workspace_from_profile(
    ctx: &ServiceContext,
    profile: &MachineProfile,
    source: &'static str,
) -> ServiceResult<bool> {
    let workspace = workspace_from_profile(profile);
    let snapshot = profile.snapshot();
    let mut project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let Some(project) = project_guard.as_mut() else {
        return Ok(false);
    };

    let should_sync_bound_project = project.machine_profile_id == Some(profile.id);
    let should_bind_empty_project = project.machine_profile_id.is_none()
        && project.objects.is_empty()
        && project.layers.is_empty()
        && project.assets.is_empty();
    if !should_sync_bound_project && !should_bind_empty_project {
        return Ok(false);
    }

    let changed = project.workspace != workspace
        || project.machine_profile_id != Some(profile.id)
        || project.machine_profile_snapshot.as_ref() != Some(&snapshot);
    if !changed {
        return Ok(false);
    }

    project.workspace = workspace;
    project.machine_profile_id = Some(profile.id);
    project.machine_profile_snapshot = Some(snapshot);
    project.dirty = true;
    let project_id = project.metadata.project_id;
    drop(project_guard);

    crate::ops::planning::invalidate_plan_cache(ctx)?;
    ctx.emit_event(
        "project.machine_profile.synced",
        json!({
            "project_id": project_id,
            "machine_profile_id": profile.id,
            "workspace": {
                "bed_width_mm": profile.bed_width_mm,
                "bed_height_mm": profile.bed_height_mm,
                "origin": profile.origin,
            },
            "source": source,
        }),
    );
    Ok(true)
}

fn profile_changed_by_sync(before: &MachineProfile, after: &MachineProfile) -> bool {
    !nearly_equal(before.bed_width_mm, after.bed_width_mm)
        || !nearly_equal(before.bed_height_mm, after.bed_height_mm)
        || !nearly_equal(before.max_speed_mm_min, after.max_speed_mm_min)
        || before.s_value_max != after.s_value_max
        || before.homing_enabled != after.homing_enabled
        || before.default_baud_rate != after.default_baud_rate
        || before.firmware_type != after.firmware_type
}

fn sync_active_profile_from_grbl_settings(
    ctx: &ServiceContext,
    grbl_settings: &GrblSettings,
    baud_rate: Option<u32>,
) -> ServiceResult<Option<ProfileAutoSync>> {
    if grbl_settings.is_empty() {
        return Ok(None);
    }

    let mut settings = ctx.settings.lock().map_err(|e| lock_err("settings", e))?;
    let active_index = settings.active_profile_id.and_then(|id| {
        settings
            .machine_profiles
            .iter()
            .position(|profile| profile.id == id)
    });

    let (profile_index, created_profile) = match active_index {
        Some(index) => (index, false),
        None => {
            let mut profile = MachineProfile {
                name: "GRBL Auto-Detected".to_string(),
                ..MachineProfile::default()
            };
            let profile_id = profile.id;
            profile.firmware_type = "grbl".to_string();
            settings.active_profile_id = Some(profile_id);
            settings.machine_profiles.push(profile);
            (settings.machine_profiles.len() - 1, true)
        }
    };

    let before = settings.machine_profiles[profile_index].clone();
    let mut sync = apply_grbl_settings_to_profile(
        &mut settings.machine_profiles[profile_index],
        grbl_settings,
        baud_rate,
    );
    sync.created_profile = created_profile;
    let after = settings.machine_profiles[profile_index].clone();
    let changed = created_profile || profile_changed_by_sync(&before, &after);
    drop(settings);
    sync.project_workspace_synced =
        sync_open_project_workspace_from_profile(ctx, &after, "grbl_settings")?;

    if changed {
        crate::ops::planning::invalidate_plan_cache(ctx)?;
        persist_settings_to_disk(ctx);
        ctx.emit_event(
            "profile.saved",
            json!({
                "profile": events::profile_summary(&after),
                "source": "grbl_settings",
            }),
        );
        if created_profile {
            ctx.emit_event(
                "profile.activated",
                json!({
                    "profile_id": after.id,
                    "source": "grbl_settings",
                }),
            );
        }
        ctx.emit_event(
            "profile.auto_synced",
            json!({
                "profile": events::profile_summary(&after),
                "sync": sync,
            }),
        );
    }

    Ok(Some(sync))
}

fn remember_job_progress(ctx: &ServiceContext, progress: Option<JobProgress>) -> ServiceResult<()> {
    let mut guard = ctx
        .last_job_progress
        .lock()
        .map_err(|e| lock_err("last_job_progress", e))?;
    *guard = progress;
    Ok(())
}

fn clear_retained_terminal_job(ctx: &ServiceContext) -> ServiceResult<()> {
    let mut guard = ctx
        .last_terminal_job
        .lock()
        .map_err(|e| lock_err("last_terminal_job", e))?;
    *guard = None;
    Ok(())
}

fn retain_terminal_job_diagnostic(
    ctx: &ServiceContext,
    reason: &str,
    progress: Option<JobProgress>,
    error: Option<String>,
    job: Option<&ActiveJobHandle>,
    session: Option<&MachineSessionHandle>,
) {
    let diagnostic = DiagnosticTerminalJob {
        captured_at: chrono::Utc::now().to_rfc3339(),
        reason: reason.to_string(),
        progress,
        error,
        job_tick_loop_running: ctx.job_tick_loop_running.load(Ordering::Acquire),
        session_state: session.map(MachineSessionHandle::session_state),
        machine_run_state: session.map(|session| session.machine_status().run_state),
        job_console: job
            .map(|job| job.console_entries(TERMINAL_JOB_CONSOLE_LIMIT))
            .unwrap_or_default(),
        session_console: session
            .map(|session| session.console_entries(TERMINAL_JOB_CONSOLE_LIMIT))
            .unwrap_or_default(),
    };

    if let Ok(mut guard) = ctx.last_terminal_job.lock() {
        *guard = Some(diagnostic);
    }
}

fn emit_job_progress_if_changed(ctx: &ServiceContext, progress: &JobProgress) -> ServiceResult<()> {
    let should_emit = {
        let mut guard = ctx
            .last_job_progress
            .lock()
            .map_err(|e| lock_err("last_job_progress", e))?;
        let changed = guard.as_ref().is_none_or(|last| {
            last.state != progress.state
                || last.acknowledged_lines != progress.acknowledged_lines
                || last.sent_lines != progress.sent_lines
        });
        if changed {
            *guard = Some(progress.clone());
        }
        changed
    };

    if should_emit {
        ctx.emit_event("job.progress", events::job_summary(progress));
    }

    Ok(())
}

/// Open the port at one baud rate and wait for the GRBL banner: DTR reset
/// first, then a soft reset (Ctrl-X) for boards that do not reset on DTR.
fn try_grbl_banner_handshake(
    ctx: &ServiceContext,
    port_name: &str,
    baud_rate: u32,
) -> ServiceResult<GrblBannerAttempt> {
    ctx.push_connection_event(
        "open_attempt",
        Some(port_name.to_string()),
        Some(baud_rate),
        Some("Opening GRBL serial connection".to_string()),
        None,
    );
    let transport = RealSerialTransport::new(port_name, baud_rate);
    let mut session = Box::new(GrblSession::new(Box::new(transport)));
    if let Err(error) = session.connect() {
        let message = error.to_string();
        ctx.push_connection_event(
            "open_failed",
            Some(port_name.to_string()),
            Some(baud_rate),
            None,
            Some(message.clone()),
        );
        return Err(ServiceError::machine(message));
    }
    ctx.push_connection_event(
        "transport_open",
        Some(port_name.to_string()),
        Some(baud_rate),
        Some("Serial transport opened".to_string()),
        None,
    );

    // Phase 1: Wait for banner after DTR reset (transport toggles DTR on open)
    ctx.push_connection_event(
        "banner_wait",
        Some(port_name.to_string()),
        Some(baud_rate),
        Some("Waiting for GRBL banner after DTR reset".to_string()),
        None,
    );
    let mut banner_received = false;
    for _ in 0..MAX_BANNER_POLLS {
        let responses = session
            .poll()
            .map_err(|e| ServiceError::machine(e.to_string()))?;
        if responses
            .iter()
            .any(|r| matches!(r, GrblResponse::Banner(_) | GrblResponse::Alarm(_)))
        {
            banner_received = true;
            ctx.push_connection_event(
                "banner_received",
                Some(port_name.to_string()),
                Some(baud_rate),
                Some("GRBL banner or alarm response received".to_string()),
                None,
            );
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    // Phase 2: If DTR reset didn't produce a banner, try GRBL soft-reset (Ctrl-X / 0x18).
    // This handles boards that don't reset on DTR, or when the device was already
    // connected and the banner was missed. Skipped when the bytes received so
    // far were undecodable: that is a baud mismatch, and a reset cannot fix it —
    // failing fast here lets the baud fallback retry sooner.
    if !banner_received
        && session.session_state() != SessionState::Alarm
        && !session.saw_undecodable_data()
    {
        ctx.push_connection_event(
            "soft_reset",
            Some(port_name.to_string()),
            Some(baud_rate),
            Some("No DTR banner; sending GRBL soft reset".to_string()),
            None,
        );
        let _ = session.send_realtime(&[0x18]);
        std::thread::sleep(std::time::Duration::from_millis(500));

        for _ in 0..MAX_BANNER_POLLS {
            let responses = session
                .poll()
                .map_err(|e| ServiceError::machine(e.to_string()))?;
            if responses
                .iter()
                .any(|r| matches!(r, GrblResponse::Banner(_) | GrblResponse::Alarm(_)))
            {
                banner_received = true;
                ctx.push_connection_event(
                    "banner_received",
                    Some(port_name.to_string()),
                    Some(baud_rate),
                    Some("GRBL banner or alarm response received after soft reset".to_string()),
                    None,
                );
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }

    if !banner_received && session.session_state() != SessionState::Alarm {
        let garbage_seen = session.saw_undecodable_data();
        session.disconnect().ok();
        return Ok(GrblBannerAttempt::NoBanner { garbage_seen });
    }

    Ok(GrblBannerAttempt::Connected(session))
}

/// Outcome of one open-port-and-wait-for-banner attempt at a single baud rate.
enum GrblBannerAttempt {
    /// Banner (or alarm) received — session is live and ready for the
    /// post-banner queries.
    Connected(Box<GrblSession>),
    /// No banner. `garbage_seen` means bytes arrived but were not valid
    /// UTF-8 — the signature of a baud-rate mismatch.
    NoBanner { garbage_seen: bool },
}

const STANDARD_GRBL_BAUD_RATE: u32 = 115_200;

/// VMS LX2b controllers use this rate and can remain completely silent when
/// opened at GRBL's usual 115200 baud, so this fallback must not depend on
/// receiving unreadable bytes first.
const LX2B_BAUD_RATE: u32 = 921_600;

/// Older GRBL builds and clone boards sometimes use these rates. Trying them
/// remains gated on unreadable data so a silent non-GRBL port does not turn
/// every connection attempt into a long baud-rate sweep.
const LEGACY_FALLBACK_BAUD_RATES: &[u32] = &[9_600, 57_600];

fn grbl_fallback_baud_rates(configured_baud: u32, unreadable_data_seen: bool) -> Vec<u32> {
    let mut rates = Vec::with_capacity(3);
    if configured_baud != LX2B_BAUD_RATE
        && (configured_baud == STANDARD_GRBL_BAUD_RATE || unreadable_data_seen)
    {
        rates.push(LX2B_BAUD_RATE);
    }
    if unreadable_data_seen {
        rates.extend(
            LEGACY_FALLBACK_BAUD_RATES
                .iter()
                .copied()
                .filter(|&rate| rate != configured_baud),
        );
    }
    rates
}

fn open_grbl_for_connection(
    ctx: &ServiceContext,
    port_name: String,
    baud_rate: u32,
) -> ServiceResult<(Box<GrblSession>, u32)> {
    let mut active_baud = baud_rate;
    let mut attempt = try_grbl_banner_handshake(ctx, &port_name, active_baud)?;
    let mut unreadable_data_seen =
        matches!(attempt, GrblBannerAttempt::NoBanner { garbage_seen: true });
    let mut attempted_bauds = vec![active_baud];

    if matches!(attempt, GrblBannerAttempt::NoBanner { .. }) {
        for fallback in grbl_fallback_baud_rates(baud_rate, unreadable_data_seen) {
            ctx.push_connection_event(
                "baud_retry",
                Some(port_name.clone()),
                Some(fallback),
                Some(format!(
                    "No valid GRBL banner at {active_baud} baud; retrying at {fallback}"
                )),
                None,
            );
            attempt = try_grbl_banner_handshake(ctx, &port_name, fallback)?;
            active_baud = fallback;
            attempted_bauds.push(fallback);
            if matches!(attempt, GrblBannerAttempt::NoBanner { garbage_seen: true }) {
                unreadable_data_seen = true;
            }
            if matches!(attempt, GrblBannerAttempt::Connected(_)) {
                break;
            }
        }
    }

    let session = match attempt {
        GrblBannerAttempt::Connected(session) => session,
        GrblBannerAttempt::NoBanner { garbage_seen } => {
            ctx.push_connection_event(
                "banner_timeout",
                Some(port_name.clone()),
                Some(active_baud),
                None,
                Some("Timeout waiting for GRBL banner".to_string()),
            );
            let tried_bauds = attempted_bauds
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            return Err(ServiceError::machine(
                if garbage_seen || unreadable_data_seen {
                    format!(
                        "The controller sent unreadable data. Tried {tried_bauds} baud without a \
                     valid GRBL response. Check the controller's baud rate and \
                     select it in the connection panel."
                    )
                } else {
                    format!(
                        "Timeout waiting for GRBL banner after trying {tried_bauds} baud. \
                     Is this a GRBL device?"
                    )
                },
            ));
        }
    };
    Ok((session, active_baud))
}

fn finish_grbl_connection(
    ctx: &ServiceContext,
    mut session: Box<GrblSession>,
    port_name: &str,
    baud_rate: Option<u32>,
    controller_info_already_queried: bool,
    require_fresh_status: bool,
    query_settings: bool,
) -> ServiceResult<Box<GrblSession>> {
    ctx.push_connection_event(
        "status_query",
        Some(port_name.to_string()),
        baud_rate,
        Some("Polling GRBL status".to_string()),
        None,
    );
    session
        .poll_status()
        .map_err(|e| ServiceError::machine(e.to_string()))?;
    let status_deadline = Instant::now() + CONTROLLER_COMPATIBILITY_STATUS_TIMEOUT;
    let mut fresh_status_seen = false;
    loop {
        let responses = session
            .poll()
            .map_err(|e| ServiceError::machine(e.to_string()))?;
        if responses
            .iter()
            .any(|response| matches!(response, GrblResponse::Status(_)))
        {
            fresh_status_seen = true;
            break;
        }
        if !require_fresh_status || Instant::now() >= status_deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    if require_fresh_status && !fresh_status_seen {
        let _ = session.disconnect();
        return Err(ServiceError::machine(
            "Controller did not return a fresh GRBL status report during the compatibility handshake",
        ));
    }
    if require_fresh_status {
        let status = session.last_status();
        if !matches!(
            status.run_state,
            MachineRunState::Idle | MachineRunState::Alarm
        ) {
            let run_state = status.run_state;
            let _ = session.disconnect();
            return Err(ServiceError::machine(format!(
                "Controller reported {run_state:?} during the compatibility handshake; stop the machine before connecting"
            )));
        }
        if status.spindle_speed > 0.0 {
            let _ = session.disconnect();
            return Err(ServiceError::machine(
                "Controller reported active output during the compatibility handshake; turn the output off before connecting",
            ));
        }
    }

    if !controller_info_already_queried {
        ctx.push_connection_event(
            "controller_info_query",
            Some(port_name.to_string()),
            baud_rate,
            Some("Requesting GRBL controller info".to_string()),
            None,
        );
        let _ = session.send_command(&grbl_commands::controller_info());
        std::thread::sleep(std::time::Duration::from_millis(100));
        let _ = session.poll();
    }

    if session.session_state() != SessionState::Alarm && query_settings {
        ctx.push_connection_event(
            "settings_query",
            Some(port_name.to_string()),
            baud_rate,
            Some("Requesting GRBL settings".to_string()),
            None,
        );
        session
            .request_settings()
            .map_err(|e| ServiceError::machine(e.to_string()))?;
        std::thread::sleep(std::time::Duration::from_millis(200));
        let _ = session.poll();
        if session.last_status().run_state != MachineRunState::Alarm {
            session
                .mark_ready()
                .map_err(|e| ServiceError::machine(e.to_string()))?;
            ctx.push_connection_event(
                "ready",
                Some(port_name.to_string()),
                baud_rate,
                Some("GRBL session is ready".to_string()),
                None,
            );
        }
    } else if session.session_state() != SessionState::Alarm {
        session
            .mark_ready()
            .map_err(|e| ServiceError::machine(e.to_string()))?;
        ctx.push_connection_event(
            "ready",
            Some(port_name.to_string()),
            baud_rate,
            Some("GRBL-compatible session is ready".to_string()),
            None,
        );
    }

    Ok(session)
}

fn connect_grbl(
    ctx: &ServiceContext,
    port_name: String,
    baud_rate: u32,
) -> ServiceResult<(MachineSessionHandle, u32)> {
    let (session, active_baud) = open_grbl_for_connection(ctx, port_name.clone(), baud_rate)?;
    let session = finish_grbl_connection(
        ctx,
        session,
        &port_name,
        Some(active_baud),
        false,
        false,
        true,
    )?;
    Ok((
        MachineSessionHandle::Grbl(GrblRuntimeSession::legacy(*session)),
        active_baud,
    ))
}

fn open_marlin_for_connection(
    ctx: &ServiceContext,
    port_name: &str,
    baud_rate: u32,
) -> ServiceResult<(
    MarlinSerialSession,
    beambench_marlin::MarlinIdentityProbeResult,
)> {
    ctx.push_connection_event(
        "transport_open",
        Some(port_name.to_string()),
        Some(baud_rate),
        Some("Opening serial transport for Marlin".to_string()),
        None,
    );
    let transport = RealSerialTransport::new(port_name, baud_rate);
    let mut session =
        MarlinSerialSession::new(Box::new(transport), MarlinSerialSessionConfig::default());
    session
        .connect()
        .map_err(|error| ServiceError::machine(error.to_string()))?;

    // RealSerialTransport toggles DTR on open. Give common Arduino-class
    // Marlin boards time to leave the bootloader before issuing M115.
    std::thread::sleep(Duration::from_millis(1_500));
    ctx.push_connection_event(
        "identity_probe",
        Some(port_name.to_string()),
        Some(baud_rate),
        Some("Sending the read-only Marlin M115 identity query".to_string()),
        None,
    );
    let probe = session
        .probe_identity()
        .map_err(|error| ServiceError::machine(error.to_string()))?;
    Ok((session, probe))
}

fn finish_marlin_connection_attempt(
    ctx: &ServiceContext,
    port_name: String,
    baud_rate: u32,
    selection: ControllerSelection,
    device_identity: DeviceIdentity,
) -> ServiceResult<ControllerConnectionResult> {
    let (session, probe) = open_marlin_for_connection(ctx, &port_name, baud_rate)?;
    let probed = controller_choice::resolve_marlin_probe_choice(
        &controller_choice::ResolveMarlinProbeChoiceInput {
            selection: selection.clone(),
            device_identity: device_identity.clone(),
            remembered_override: None,
            decision: None,
        },
        probe,
    );
    let pending = PendingControllerConnection {
        attempt_id: uuid::Uuid::new_v4().to_string(),
        endpoint: ControllerConnectionEndpoint::Serial {
            port_name,
            baud_rate,
        },
        device_identity,
        selection,
        session: PendingControllerSession::Marlin {
            probe: probed.probe,
            session,
        },
        created_at: Instant::now(),
    };
    finish_controller_connection_resolution(ctx, pending, probed.resolution)
}

fn open_smoothieware_for_connection(
    ctx: &ServiceContext,
    port_name: &str,
    baud_rate: u32,
) -> ServiceResult<(
    SmoothiewareSerialSession,
    beambench_smoothieware::SmoothiewareIdentityProbeResult,
)> {
    ctx.push_connection_event(
        "transport_open",
        Some(port_name.to_string()),
        Some(baud_rate),
        Some("Opening serial transport for Smoothieware".to_string()),
        None,
    );
    let transport = RealSerialTransport::new(port_name, baud_rate);
    let mut session = SmoothiewareSerialSession::new(
        Box::new(transport),
        SmoothiewareSerialSessionConfig::default(),
    );
    session
        .connect()
        .map_err(|error| ServiceError::machine(error.to_string()))?;

    // Allow boards that reset when DTR changes to finish startup before M115.
    std::thread::sleep(Duration::from_millis(1_500));
    ctx.push_connection_event(
        "identity_probe",
        Some(port_name.to_string()),
        Some(baud_rate),
        Some("Reading Smoothieware firmware and laser configuration".to_string()),
        None,
    );
    let probe = session
        .probe_identity()
        .map_err(|error| ServiceError::machine(error.to_string()))?;
    Ok((session, probe))
}

fn finish_smoothieware_connection_attempt(
    ctx: &ServiceContext,
    port_name: String,
    baud_rate: u32,
    selection: ControllerSelection,
    device_identity: DeviceIdentity,
) -> ServiceResult<ControllerConnectionResult> {
    let (session, probe) = open_smoothieware_for_connection(ctx, &port_name, baud_rate)?;
    let probed = controller_choice::resolve_smoothieware_probe_choice(
        &controller_choice::ResolveSmoothiewareProbeChoiceInput {
            selection: selection.clone(),
            device_identity: device_identity.clone(),
            remembered_override: None,
            decision: None,
        },
        probe,
    );
    let pending = PendingControllerConnection {
        attempt_id: uuid::Uuid::new_v4().to_string(),
        endpoint: ControllerConnectionEndpoint::Serial {
            port_name,
            baud_rate,
        },
        device_identity,
        selection,
        session: PendingControllerSession::Smoothieware {
            probe: probed.probe,
            session,
        },
        created_at: Instant::now(),
    };
    finish_controller_connection_resolution(ctx, pending, probed.resolution)
}

fn finish_grbl_connection_attempt(
    ctx: &ServiceContext,
    port_name: String,
    baud_rate: u32,
    selection: ControllerSelection,
    device_identity: DeviceIdentity,
) -> ServiceResult<ControllerConnectionResult> {
    let (mut session, active_baud_rate) =
        open_grbl_for_connection(ctx, port_name.clone(), baud_rate)?;
    ctx.push_connection_event(
        "identity_probe",
        Some(port_name.clone()),
        Some(active_baud_rate),
        Some("Sending read-only GRBL-family identity queries".to_string()),
        None,
    );
    let probed = controller_choice::probe_and_resolve_grbl_family_choice(
        &mut session,
        GrblFamilyIdentityProbeConfig::default(),
        &controller_choice::ResolveGrblFamilyProbeChoiceInput {
            selection: selection.clone(),
            device_identity: device_identity.clone(),
            transport_kind: TransportKind::Serial,
            remembered_override: None,
            decision: None,
        },
    )
    .map_err(|error| ServiceError::machine(error.to_string()))?;
    let pending = PendingControllerConnection {
        attempt_id: uuid::Uuid::new_v4().to_string(),
        endpoint: ControllerConnectionEndpoint::Serial {
            port_name,
            baud_rate: active_baud_rate,
        },
        device_identity,
        selection,
        session: PendingControllerSession::Grbl {
            probe: probed.probe,
            session,
        },
        created_at: Instant::now(),
    };
    finish_controller_connection_resolution(ctx, pending, probed.resolution)
}

fn laserpecker_protocol_probe() -> GrblFamilyIdentityProbeResult {
    // The official LX2 profile disables controller-settings fetches, while the
    // published serial profiles do not require them. The named adapter relies
    // on the fresh GRBL status handshake performed by the transport opener
    // instead of sending `$I`, `$I+`, or `$$`.
    GrblFamilyIdentityProbeResult {
        identity: Default::default(),
        controller_info: GrblFamilyIdentityProbeOutcome::NotAttempted,
        extended_controller_info: GrblFamilyIdentityProbeOutcome::NotAttempted,
    }
}

fn finish_laserpecker_grbl_connection_attempt(
    ctx: &ServiceContext,
    session: Box<GrblSession>,
    endpoint: ControllerConnectionEndpoint,
    selection: ControllerSelection,
    device_identity: DeviceIdentity,
) -> ServiceResult<ControllerConnectionResult> {
    let probed = controller_choice::resolve_grbl_family_probe_choice(
        &controller_choice::ResolveGrblFamilyProbeChoiceInput {
            selection: selection.clone(),
            device_identity: device_identity.clone(),
            transport_kind: endpoint.transport_kind(),
            remembered_override: None,
            decision: None,
        },
        laserpecker_protocol_probe(),
    );
    let pending = PendingControllerConnection {
        attempt_id: uuid::Uuid::new_v4().to_string(),
        endpoint,
        device_identity,
        selection,
        session: PendingControllerSession::Grbl {
            probe: probed.probe,
            session,
        },
        created_at: Instant::now(),
    };
    finish_controller_connection_resolution(ctx, pending, probed.resolution)
}

fn open_laserpecker_serial_for_connection(
    ctx: &ServiceContext,
    port_name: &str,
    baud_rate: u32,
) -> ServiceResult<Box<GrblSession>> {
    ctx.push_connection_event(
        "transport_open",
        Some(port_name.to_string()),
        Some(baud_rate),
        Some("Opening LaserPecker serial transport without a DTR reset".to_string()),
        None,
    );
    let transport = RealSerialTransport::new_without_dtr(port_name, baud_rate);
    let mut session = Box::new(GrblSession::new(Box::new(transport)));
    session
        .connect()
        .map_err(|error| ServiceError::machine(error.to_string()))?;

    let status_count = session.status_report_count();
    ctx.push_connection_event(
        "status_query",
        Some(port_name.to_string()),
        Some(baud_rate),
        Some("Validating LaserPecker with a GRBL status query".to_string()),
        None,
    );
    session
        .poll_status()
        .map_err(|error| ServiceError::machine(error.to_string()))?;
    let deadline = Instant::now() + CONTROLLER_COMPATIBILITY_STATUS_TIMEOUT;
    while session.status_report_count() == status_count && Instant::now() < deadline {
        session
            .poll()
            .map_err(|error| ServiceError::machine(error.to_string()))?;
        if session.status_report_count() == status_count {
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    if session.status_report_count() == status_count {
        let _ = session.disconnect();
        return Err(ServiceError::machine(
            "The LaserPecker controller did not return a GRBL status report",
        ));
    }
    session
        .begin_validation_from_fresh_status(status_count)
        .map_err(|error| ServiceError::machine(error.to_string()))?;
    if !matches!(
        session.last_status().run_state,
        MachineRunState::Idle | MachineRunState::Alarm
    ) {
        let run_state = session.last_status().run_state;
        let _ = session.disconnect();
        return Err(ServiceError::machine(format!(
            "LaserPecker reported {run_state:?} while connecting; stop the machine before reconnecting"
        )));
    }
    if session.last_status().spindle_speed > 0.0 {
        let _ = session.disconnect();
        return Err(ServiceError::machine(
            "LaserPecker reported active output while connecting; turn the output off before reconnecting",
        ));
    }

    Ok(session)
}

fn finish_laserpecker_serial_connection_attempt(
    ctx: &ServiceContext,
    port_name: String,
    baud_rate: u32,
    selection: ControllerSelection,
    device_identity: DeviceIdentity,
) -> ServiceResult<ControllerConnectionResult> {
    let session = open_laserpecker_serial_for_connection(ctx, &port_name, baud_rate)?;
    finish_laserpecker_grbl_connection_attempt(
        ctx,
        session,
        ControllerConnectionEndpoint::Serial {
            port_name,
            baud_rate,
        },
        selection,
        device_identity,
    )
}

fn network_device_identity(host: &str, port: u16) -> DeviceIdentity {
    let endpoint = ControllerConnectionEndpoint::Tcp {
        host: host.to_string(),
        port,
    };
    DeviceIdentity {
        display_name: endpoint.display_name(),
        host: Some(host.to_string()),
        tcp_port: Some(port),
        udp_port: None,
        ..DeviceIdentity::default()
    }
}

fn ruida_device_identity(host: &str, port: u16) -> DeviceIdentity {
    let endpoint = ControllerConnectionEndpoint::Udp {
        host: host.to_string(),
        port,
    };
    DeviceIdentity {
        display_name: endpoint.display_name(),
        host: Some(host.to_string()),
        udp_port: Some(port),
        ..DeviceIdentity::default()
    }
}

fn lihuiyu_device_identity(device: &LihuiyuUsbDeviceInfo) -> DeviceIdentity {
    DeviceIdentity {
        display_name: device
            .product
            .clone()
            .unwrap_or_else(|| "Lihuiyu M2/M3 Nano".to_string()),
        manufacturer: device.manufacturer.clone(),
        description: Some("CH341 USB laser controller".to_string()),
        product: device.product.clone(),
        serial_number: device.serial_number.clone(),
        vendor_id: Some(device.vendor_id),
        product_id: Some(device.product_id),
        usb_path: Some(device.stable_id()),
        ..DeviceIdentity::default()
    }
}

fn finish_lihuiyu_usb_connection(
    ctx: &ServiceContext,
    io: impl LihuiyuUsbIo + Send + 'static,
    device: LihuiyuUsbDeviceInfo,
) -> ServiceResult<ControllerConnectionResult> {
    let endpoint = ControllerConnectionEndpoint::Usb {
        device_id: device.stable_id(),
        vendor_id: device.vendor_id,
        product_id: device.product_id,
    };
    let device_identity = lihuiyu_device_identity(&device);
    let session = LihuiyuRuntimeSession::connect(io, device).map_err(|error| {
        ServiceError::machine(format!("Lihuiyu adapter validation failed: {error}"))
    })?;
    let detected_identity = session.detected_identity();
    let choice = ResolvedControllerChoice {
        selection: ExplicitControllerSelection::KnownDriver {
            driver: ControllerDriverId::Lihuiyu,
        },
        driver: ControllerDriverId::Lihuiyu,
        source: beambench_common::controller_choice::ControllerChoiceSource::KnownDriverSelection,
        detected_identity: Some(detected_identity),
        requires_experimental_mode: true,
        mismatch: false,
        override_scope: None,
        requires_experimental_compatibility_handshake: false,
    };
    let state = register_machine_session(
        ctx,
        MachineSessionHandle::Lihuiyu(session),
        json!({
            "target": endpoint,
            "device": device_identity,
            "selection": choice.selection,
        }),
        None,
        Some(&choice),
    )?;
    Ok(ControllerConnectionResult::Connected {
        session_state: state,
        endpoint,
        choice,
    })
}

fn finish_ruida_network_connection(
    ctx: &ServiceContext,
    host: String,
    port: u16,
) -> ServiceResult<ControllerConnectionResult> {
    let device_identity = ruida_device_identity(&host, port);
    let endpoint = ControllerConnectionEndpoint::Udp {
        host: host.clone(),
        port,
    };
    let target = (host.as_str(), port)
        .to_socket_addrs()
        .map_err(|error| ServiceError::machine(format!("Ruida host resolution failed: {error}")))?
        .next()
        .ok_or_else(|| ServiceError::machine("Ruida host did not resolve to an IP address"))?;
    ctx.push_connection_event(
        "transport_open",
        Some(endpoint.display_name()),
        None,
        Some("Opening Ruida Ethernet/UDP transport".to_string()),
        None,
    );
    let io = RuidaUdpIo::for_target(target)
        .map_err(|error| ServiceError::machine(format!("Ruida UDP transport failed: {error}")))?;
    let session = RuidaRuntimeSession::connect(io, endpoint.display_name()).map_err(|error| {
        ServiceError::machine(format!("Ruida adapter validation failed: {error}"))
    })?;
    let detected_identity = session.detected_identity();
    let choice = ResolvedControllerChoice {
        selection: ExplicitControllerSelection::KnownDriver {
            driver: ControllerDriverId::Ruida,
        },
        driver: ControllerDriverId::Ruida,
        source: beambench_common::controller_choice::ControllerChoiceSource::KnownDriverSelection,
        detected_identity: Some(detected_identity),
        requires_experimental_mode: true,
        mismatch: false,
        override_scope: None,
        requires_experimental_compatibility_handshake: false,
    };
    let state = register_machine_session(
        ctx,
        MachineSessionHandle::Ruida(session),
        json!({
            "target": endpoint,
            "device": device_identity,
            "selection": choice.selection,
        }),
        None,
        Some(&choice),
    )?;
    Ok(ControllerConnectionResult::Connected {
        session_state: state,
        endpoint,
        choice,
    })
}

fn open_grbl_network_for_connection(
    ctx: &ServiceContext,
    host: &str,
    port: u16,
) -> ServiceResult<Box<GrblSession>> {
    let endpoint = ControllerConnectionEndpoint::Tcp {
        host: host.to_string(),
        port,
    };
    let display_name = endpoint.display_name();
    ctx.push_connection_event(
        "transport_open",
        Some(display_name.clone()),
        None,
        Some("Opening TCP controller transport".to_string()),
        None,
    );
    let transport = TcpLineTransport::new(host, port);
    let mut session = Box::new(GrblSession::new(Box::new(transport)));
    session
        .connect()
        .map_err(|error| ServiceError::machine(error.to_string()))?;

    // Network clients commonly attach long after controller boot, so no
    // startup banner is expected. A fresh real-time status query proves the
    // GRBL sender protocol without resetting or moving the machine.
    let status_count = session.status_report_count();
    ctx.push_connection_event(
        "status_query",
        Some(display_name),
        None,
        Some("Validating the network controller with a GRBL status query".to_string()),
        None,
    );
    session
        .poll_status()
        .map_err(|error| ServiceError::machine(error.to_string()))?;
    let deadline = Instant::now() + CONTROLLER_COMPATIBILITY_STATUS_TIMEOUT;
    while session.status_report_count() == status_count && Instant::now() < deadline {
        session
            .poll()
            .map_err(|error| ServiceError::machine(error.to_string()))?;
        if session.status_report_count() == status_count {
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    if session.status_report_count() == status_count {
        let _ = session.disconnect();
        return Err(ServiceError::machine(
            "The network controller did not return a GRBL status report",
        ));
    }
    session
        .begin_validation_from_fresh_status(status_count)
        .map_err(|error| ServiceError::machine(error.to_string()))?;
    if !matches!(
        session.last_status().run_state,
        MachineRunState::Idle | MachineRunState::Alarm
    ) {
        let run_state = session.last_status().run_state;
        let _ = session.disconnect();
        return Err(ServiceError::machine(format!(
            "Controller reported {run_state:?} during the network compatibility handshake; stop the machine before connecting"
        )));
    }
    if session.last_status().spindle_speed > 0.0 {
        let _ = session.disconnect();
        return Err(ServiceError::machine(
            "Controller reported active output during the network compatibility handshake; turn the output off before connecting",
        ));
    }

    Ok(session)
}

fn finish_grbl_network_connection_attempt(
    ctx: &ServiceContext,
    host: String,
    port: u16,
    selection: ControllerSelection,
    device_identity: DeviceIdentity,
) -> ServiceResult<ControllerConnectionResult> {
    let endpoint = ControllerConnectionEndpoint::Tcp {
        host: host.clone(),
        port,
    };
    let mut session = open_grbl_network_for_connection(ctx, &host, port)?;
    ctx.push_connection_event(
        "identity_probe",
        Some(endpoint.display_name()),
        None,
        Some("Sending read-only GRBL-family identity queries".to_string()),
        None,
    );
    let probed = controller_choice::probe_and_resolve_grbl_family_choice(
        &mut session,
        GrblFamilyIdentityProbeConfig::default(),
        &controller_choice::ResolveGrblFamilyProbeChoiceInput {
            selection: selection.clone(),
            device_identity: device_identity.clone(),
            transport_kind: TransportKind::Tcp,
            remembered_override: None,
            decision: None,
        },
    )
    .map_err(|error| ServiceError::machine(error.to_string()))?;
    let pending = PendingControllerConnection {
        attempt_id: uuid::Uuid::new_v4().to_string(),
        endpoint,
        device_identity,
        selection,
        session: PendingControllerSession::Grbl {
            probe: probed.probe,
            session,
        },
        created_at: Instant::now(),
    };
    finish_controller_connection_resolution(ctx, pending, probed.resolution)
}

fn finish_laserpecker_network_connection_attempt(
    ctx: &ServiceContext,
    host: String,
    port: u16,
    selection: ControllerSelection,
    device_identity: DeviceIdentity,
) -> ServiceResult<ControllerConnectionResult> {
    let session = open_grbl_network_for_connection(ctx, &host, port)?;
    finish_laserpecker_grbl_connection_attempt(
        ctx,
        session,
        ControllerConnectionEndpoint::Tcp { host, port },
        selection,
        device_identity,
    )
}

/// Returns the connected session and, for GRBL serial candidates, the baud
/// rate that actually worked (the connect may have fallen back from 115200),
/// so the caller persists the real rate to the profile.
fn connect_from_candidate(
    ctx: &ServiceContext,
    candidate_id: &str,
) -> ServiceResult<(MachineSessionHandle, Option<u32>)> {
    let candidate = discovery::find_candidate(ctx, candidate_id)?;

    if let Some(reason) = candidate.unsupported_reason.as_deref() {
        let reason = reason.trim();
        return Err(ServiceError::invalid_state(if reason.is_empty() {
            "Controller support is not available for this discovery candidate".to_string()
        } else {
            format!("Controller support is not available: {reason}")
        }));
    }

    match (
        candidate.controller_family,
        candidate.controller_model,
        candidate.transport_kind,
    ) {
        (ControllerFamily::Gcode, ControllerModel::Grbl, TransportKind::Serial) => {
            let (session, actual_baud) = connect_grbl(
                ctx,
                candidate.identity.port_name.clone().ok_or_else(|| {
                    ServiceError::invalid_input("GRBL discovery candidate missing serial port")
                })?,
                115200,
            )?;
            Ok((session, Some(actual_baud)))
        }
        _ => Err(ServiceError::invalid_state(format!(
            "Controller support is not available for {:?} / {:?} / {:?} discovery candidates",
            candidate.controller_family, candidate.controller_model, candidate.transport_kind
        ))),
    }
}

fn serial_device_identity(port_name: &str) -> DeviceIdentity {
    let port = list_available_ports()
        .ok()
        .and_then(|ports| ports.into_iter().find(|port| port.port_name == port_name));
    match port {
        Some(port) => DeviceIdentity {
            display_name: if port.description.trim().is_empty() {
                port_name.to_string()
            } else {
                port.description.clone()
            },
            manufacturer: (!port.manufacturer.trim().is_empty()).then_some(port.manufacturer),
            description: (!port.description.trim().is_empty()).then_some(port.description),
            product: None,
            serial_number: None,
            vendor_id: port.vid,
            product_id: port.pid,
            port_name: Some(port_name.to_string()),
            host: None,
            tcp_port: None,
            udp_port: None,
            usb_path: None,
        },
        None => DeviceIdentity {
            display_name: port_name.to_string(),
            port_name: Some(port_name.to_string()),
            ..DeviceIdentity::default()
        },
    }
}

fn clear_pending_controller_connection(ctx: &ServiceContext) -> ServiceResult<()> {
    let pending = ctx
        .pending_controller_connection
        .lock()
        .map_err(|e| lock_err("pending_controller_connection", e))?
        .take();
    if let Some(mut pending) = pending {
        pending.session.disconnect();
    }
    Ok(())
}

fn register_machine_session(
    ctx: &ServiceContext,
    session: MachineSessionHandle,
    event_payload: serde_json::Value,
    baud_rate_for_profile: Option<u32>,
    choice: Option<&ResolvedControllerChoice>,
) -> ServiceResult<SessionState> {
    let profile_sync = match &session {
        MachineSessionHandle::Grbl(grbl) if !grbl.experimental_mode() => {
            sync_active_profile_from_grbl_settings(ctx, grbl.settings(), baud_rate_for_profile)?
        }
        MachineSessionHandle::Grbl(_)
        | MachineSessionHandle::Marlin(_)
        | MachineSessionHandle::Smoothieware(_)
        | MachineSessionHandle::Ruida(_)
        | MachineSessionHandle::Lihuiyu(_)
        | MachineSessionHandle::Dsp(_)
        | MachineSessionHandle::Galvo(_) => None,
    };
    let state = session.session_state();
    let family = session.controller_family();
    let model = session.controller_model();
    let driver = session.controller_driver();
    let experimental_mode = session.experimental_mode();
    let product_tier = session.controller_product_tier();
    let evidence_state = session.controller_evidence_state();
    let transport = session.transport_kind();
    let capabilities = session.capabilities();
    let mut session_lock = ctx.session.lock().map_err(|e| lock_err("session", e))?;
    if session_lock.is_some() {
        return Err(ServiceError::conflict(
            "Already connected. Disconnect first.",
        ));
    }
    *session_lock = Some(session);
    drop(session_lock);
    ctx.emit_event(
        "machine.connected",
        json!({
            "controller_family": family,
            "controller_model": model,
            "controller_driver": driver,
            "experimental_mode": experimental_mode,
            "product_tier": product_tier,
            "evidence_state": evidence_state,
            "transport_kind": transport,
            "capabilities": capabilities,
            "session_state": state,
            "connection": event_payload,
            "controller_choice": choice,
            "profile_sync": profile_sync,
        }),
    );
    Ok(state)
}

fn validate_resolved_controller_adapter(
    pending: &PendingControllerConnection,
    choice: &ResolvedControllerChoice,
) -> ServiceResult<()> {
    match choice.driver {
        ControllerDriverId::Grbl => match &pending.session {
            PendingControllerSession::Grbl { .. } => Ok(()),
            PendingControllerSession::Marlin { .. } => Err(ServiceError::invalid_state(
                "The resolved GRBL driver does not match the open Marlin session",
            )),
            PendingControllerSession::Smoothieware { .. } => Err(ServiceError::invalid_state(
                "The resolved GRBL driver does not match the open Smoothieware session",
            )),
        },
        ControllerDriverId::LaserPecker => match &pending.session {
            PendingControllerSession::Grbl { .. } => Ok(()),
            PendingControllerSession::Marlin { .. }
            | PendingControllerSession::Smoothieware { .. } => Err(ServiceError::invalid_state(
                "The resolved LaserPecker driver does not match the open session",
            )),
        },
        ControllerDriverId::FluidNc => {
            // A user-directed compatibility override deliberately runs the
            // shared GRBL sender after a fresh status handshake. A named,
            // non-override FluidNC session additionally requires exact `$I`
            // identity evidence owned by the backend probe.
            if !choice.requires_experimental_compatibility_handshake {
                let PendingControllerSession::Grbl { probe, .. } = &pending.session else {
                    return Err(ServiceError::invalid_state(
                        "The resolved FluidNC driver does not match the open session",
                    ));
                };
                match pending.endpoint.transport_kind() {
                    TransportKind::Serial => FluidNcSerialAdapter::new().validate_probe(probe),
                    TransportKind::Tcp => FluidNcNetworkAdapter::new().validate_probe(probe),
                    TransportKind::UsbPacket => unreachable!(
                        "controller-choice endpoints cannot represent USB packet transports"
                    ),
                    TransportKind::Udp => {
                        unreachable!("GRBL-family adapters cannot use UDP controller endpoints")
                    }
                }
                .map_err(|error| {
                    ServiceError::machine(format!("FluidNC adapter validation failed: {error}"))
                })?;
            }
            Ok(())
        }
        ControllerDriverId::GrblHal => {
            // Named grblHAL activation requires the exact firmware identity
            // evidence captured by the backend-owned `$I`/`$I+` probe.
            if !choice.requires_experimental_compatibility_handshake {
                let PendingControllerSession::Grbl { probe, .. } = &pending.session else {
                    return Err(ServiceError::invalid_state(
                        "The resolved grblHAL driver does not match the open session",
                    ));
                };
                match pending.endpoint.transport_kind() {
                    TransportKind::Serial => GrblHalSerialAdapter::new().validate_probe(probe),
                    TransportKind::Tcp => GrblHalNetworkAdapter::new().validate_probe(probe),
                    TransportKind::UsbPacket => unreachable!(
                        "controller-choice endpoints cannot represent USB packet transports"
                    ),
                    TransportKind::Udp => {
                        unreachable!("GRBL-family adapters cannot use UDP controller endpoints")
                    }
                }
                .map_err(|error| {
                    ServiceError::machine(format!("grblHAL adapter validation failed: {error}"))
                })?;
            }
            Ok(())
        }
        ControllerDriverId::Marlin | ControllerDriverId::Snapmaker => match &pending.session {
            PendingControllerSession::Marlin { .. } => Ok(()),
            PendingControllerSession::Grbl { .. } => Err(ServiceError::invalid_state(
                "The resolved Marlin-derived driver does not match the open GRBL session",
            )),
            PendingControllerSession::Smoothieware { .. } => Err(ServiceError::invalid_state(
                "The resolved Marlin-derived driver does not match the open Smoothieware session",
            )),
        },
        ControllerDriverId::Smoothieware => match &pending.session {
            PendingControllerSession::Smoothieware { .. } => Ok(()),
            PendingControllerSession::Grbl { .. } | PendingControllerSession::Marlin { .. } => {
                Err(ServiceError::invalid_state(
                    "The resolved Smoothieware driver does not match the open session",
                ))
            }
        },
        ControllerDriverId::Ruida => Err(ServiceError::invalid_state(
            "The resolved Ruida driver requires a dedicated UDP session",
        )),
        ControllerDriverId::Lihuiyu => Err(ServiceError::invalid_state(
            "The resolved Lihuiyu driver requires a dedicated CH341 USB session",
        )),
        ControllerDriverId::Unknown => Err(ServiceError::invalid_state(
            "The resolved controller driver is not available in this build",
        )),
    }
}

fn finish_controller_connection_resolution(
    ctx: &ServiceContext,
    mut pending: PendingControllerConnection,
    resolution: beambench_common::controller_choice::ControllerChoiceResolution,
) -> ServiceResult<ControllerConnectionResult> {
    match &resolution.outcome {
        ControllerChoiceOutcome::Resolved { choice } => {
            if let Err(error) = validate_resolved_controller_adapter(&pending, choice) {
                pending.session.disconnect();
                return Err(error);
            }
            let endpoint = pending.endpoint.clone();
            let endpoint_name = endpoint.display_name();
            let baud_rate = endpoint.baud_rate();
            let transport_kind = endpoint.transport_kind();
            let handle = match pending.session {
                PendingControllerSession::Grbl { session, .. } => {
                    let query_settings = choice.driver != ControllerDriverId::LaserPecker;
                    let session = finish_grbl_connection(
                        ctx,
                        session,
                        &endpoint_name,
                        baud_rate,
                        true,
                        true,
                        query_settings,
                    )?;
                    MachineSessionHandle::Grbl(GrblRuntimeSession::from_choice_with_transport(
                        *session,
                        choice,
                        transport_kind,
                    ))
                }
                PendingControllerSession::Marlin { mut session, .. } => {
                    match choice.driver {
                        ControllerDriverId::Snapmaker => session.activate_snapmaker(),
                        _ => session.activate(),
                    }
                    .map_err(|error| {
                        ServiceError::machine(format!(
                            "{:?} adapter validation failed: {error}",
                            choice.driver
                        ))
                    })?;
                    ctx.push_connection_event(
                        "ready",
                        Some(endpoint_name.clone()),
                        baud_rate,
                        Some(format!("{:?} session is ready", choice.driver)),
                        None,
                    );
                    MachineSessionHandle::Marlin(MarlinRuntimeSession::from_choice(session, choice))
                }
                PendingControllerSession::Smoothieware { mut session, .. } => {
                    session.activate().map_err(|error| {
                        ServiceError::machine(format!(
                            "Smoothieware adapter validation failed: {error}"
                        ))
                    })?;
                    ctx.push_connection_event(
                        "ready",
                        Some(endpoint_name),
                        baud_rate,
                        Some("Smoothieware session is ready".to_string()),
                        None,
                    );
                    MachineSessionHandle::Smoothieware(SmoothiewareRuntimeSession::from_choice(
                        session, choice,
                    ))
                }
            };
            let state = register_machine_session(
                ctx,
                handle,
                json!({
                    "target": endpoint,
                    "selection": choice.selection,
                }),
                baud_rate,
                Some(choice),
            )?;
            Ok(ControllerConnectionResult::Connected {
                session_state: state,
                endpoint,
                choice: choice.clone(),
            })
        }
        ControllerChoiceOutcome::Cancelled => {
            pending.session.disconnect();
            Ok(ControllerConnectionResult::Cancelled)
        }
        ControllerChoiceOutcome::SelectionRequired
        | ControllerChoiceOutcome::MismatchDecisionRequired { .. }
        | ControllerChoiceOutcome::Blocked { .. } => {
            let attempt_id = pending.attempt_id.clone();
            let endpoint = pending.endpoint.clone();
            let detected_identity = pending.session.detected_identity();
            *ctx.pending_controller_connection
                .lock()
                .map_err(|e| lock_err("pending_controller_connection", e))? = Some(pending);
            Ok(ControllerConnectionResult::Challenge {
                attempt_id,
                endpoint,
                detected_identity,
                resolution: Box::new(resolution),
            })
        }
    }
}

fn continue_auto_detection_after_grbl(
    ctx: &ServiceContext,
    port_name: String,
    baud_rate: u32,
    device_identity: DeviceIdentity,
    grbl_error: Option<String>,
) -> ServiceResult<ControllerConnectionResult> {
    clear_pending_controller_connection(ctx)?;
    ctx.push_connection_event(
        "protocol_retry",
        Some(port_name.clone()),
        Some(baud_rate),
        Some("GRBL identity was not established; trying Marlin M115".to_string()),
        grbl_error,
    );
    match finish_marlin_connection_attempt(
        ctx,
        port_name.clone(),
        baud_rate,
        ControllerSelection::AutoDetect,
        device_identity.clone(),
    ) {
        Ok(ControllerConnectionResult::Challenge { resolution, .. })
            if matches!(
                resolution.outcome,
                ControllerChoiceOutcome::SelectionRequired
            ) =>
        {
            clear_pending_controller_connection(ctx)?;
        }
        Ok(result) => return Ok(result),
        Err(error) => {
            ctx.push_connection_event(
                "protocol_retry",
                Some(port_name.clone()),
                Some(baud_rate),
                Some("Marlin identity was not established; trying Smoothieware".to_string()),
                Some(error.to_string()),
            );
        }
    }

    ctx.push_connection_event(
        "protocol_retry",
        Some(port_name.clone()),
        Some(baud_rate),
        Some("Trying Smoothieware firmware and laser configuration probes".to_string()),
        None,
    );
    match finish_smoothieware_connection_attempt(
        ctx,
        port_name.clone(),
        baud_rate,
        ControllerSelection::AutoDetect,
        device_identity.clone(),
    ) {
        Ok(ControllerConnectionResult::Challenge { resolution, .. })
            if matches!(
                resolution.outcome,
                ControllerChoiceOutcome::SelectionRequired
            ) =>
        {
            clear_pending_controller_connection(ctx)?;
        }
        Ok(result) => return Ok(result),
        Err(error) => {
            ctx.push_connection_event(
                "protocol_retry",
                Some(port_name.clone()),
                Some(baud_rate),
                Some(
                    "Smoothieware identity was not established; offering controller choices"
                        .to_string(),
                ),
                Some(error.to_string()),
            );
        }
    }

    // No exact adapter identified the controller. Reopen the established GRBL
    // choice session so every explicit controller option remains available.
    clear_pending_controller_connection(ctx)?;
    finish_grbl_connection_attempt(
        ctx,
        port_name,
        baud_rate,
        ControllerSelection::AutoDetect,
        device_identity,
    )
}

pub fn begin_controller_connection(
    ctx: &ServiceContext,
    input: BeginControllerConnectionInput,
) -> ServiceResult<ControllerConnectionResult> {
    if input.port_name.trim().is_empty() {
        return Err(ServiceError::invalid_input("Serial port is required"));
    }
    if input.baud_rate == 0 {
        return Err(ServiceError::invalid_input(
            "Baud rate must be greater than zero",
        ));
    }
    let _connection_gate = ctx
        .controller_connection_gate
        .lock()
        .map_err(|e| lock_err("controller_connection_gate", e))?;
    if ctx
        .session
        .lock()
        .map_err(|e| lock_err("session", e))?
        .is_some()
    {
        return Err(ServiceError::conflict(
            "Already connected. Disconnect first.",
        ));
    }
    clear_pending_controller_connection(ctx)?;
    ctx.machine_coordinates_valid
        .store(false, Ordering::Release);
    ctx.active_jog.store(false, Ordering::Release);

    let port_name = input.port_name;
    let device_identity = serial_device_identity(&port_name);
    if matches!(
        input.selection,
        ControllerSelection::KnownDriver {
            driver: ControllerDriverId::Ruida
        }
    ) {
        return Err(ServiceError::invalid_input(
            "Ruida Experimental uses the Network connection with Ethernet/UDP",
        ));
    }
    if matches!(
        input.selection,
        ControllerSelection::KnownDriver {
            driver: ControllerDriverId::Lihuiyu
        }
    ) {
        return Err(ServiceError::invalid_input(
            "Lihuiyu M2/M3 Nano (Experimental) uses the USB connection",
        ));
    }
    if matches!(
        input.selection,
        ControllerSelection::KnownDriver {
            driver: ControllerDriverId::Marlin | ControllerDriverId::Snapmaker
        }
    ) {
        return finish_marlin_connection_attempt(
            ctx,
            port_name,
            input.baud_rate,
            input.selection,
            device_identity,
        );
    }
    if matches!(
        input.selection,
        ControllerSelection::KnownDriver {
            driver: ControllerDriverId::Smoothieware
        }
    ) {
        return finish_smoothieware_connection_attempt(
            ctx,
            port_name,
            input.baud_rate,
            input.selection,
            device_identity,
        );
    }
    if matches!(
        input.selection,
        ControllerSelection::KnownDriver {
            driver: ControllerDriverId::LaserPecker
        }
    ) {
        return finish_laserpecker_serial_connection_attempt(
            ctx,
            port_name,
            input.baud_rate,
            input.selection,
            device_identity,
        );
    }
    match finish_grbl_connection_attempt(
        ctx,
        port_name.clone(),
        input.baud_rate,
        input.selection.clone(),
        device_identity.clone(),
    ) {
        Ok(ControllerConnectionResult::Challenge { resolution, .. })
            if input.selection == ControllerSelection::AutoDetect
                && matches!(
                    resolution.outcome,
                    ControllerChoiceOutcome::SelectionRequired
                ) =>
        {
            continue_auto_detection_after_grbl(
                ctx,
                port_name,
                input.baud_rate,
                device_identity,
                None,
            )
        }
        Ok(result) => Ok(result),
        Err(grbl_error) if input.selection == ControllerSelection::AutoDetect => {
            continue_auto_detection_after_grbl(
                ctx,
                port_name,
                input.baud_rate,
                device_identity,
                Some(grbl_error.to_string()),
            )
        }
        Err(error) => Err(error),
    }
}

pub fn begin_network_controller_connection(
    ctx: &ServiceContext,
    input: BeginNetworkControllerConnectionInput,
) -> ServiceResult<ControllerConnectionResult> {
    let mut host = input.host.trim().to_string();
    if host.starts_with('[') && host.ends_with(']') && host.len() > 2 {
        host = host[1..host.len() - 1].to_string();
    }
    if host.is_empty() {
        return Err(ServiceError::invalid_input("Network host is required"));
    }
    if host.chars().any(char::is_control) {
        return Err(ServiceError::invalid_input(
            "Network host cannot contain control characters",
        ));
    }
    if host.len() > 255 {
        return Err(ServiceError::invalid_input(
            "Network host must be 255 bytes or fewer",
        ));
    }
    if input.port == 0 {
        return Err(ServiceError::invalid_input(
            "Network port must be greater than zero",
        ));
    }
    match input.selection {
        ControllerSelection::AutoDetect
        | ControllerSelection::KnownDriver {
            driver:
                ControllerDriverId::FluidNc
                | ControllerDriverId::GrblHal
                | ControllerDriverId::LaserPecker
                | ControllerDriverId::Ruida,
        } => {}
        ControllerSelection::KnownDriver {
            driver: ControllerDriverId::Lihuiyu,
        } => {
            return Err(ServiceError::invalid_input(
                "Lihuiyu M2/M3 Nano (Experimental) uses the USB connection",
            ));
        }
        ControllerSelection::KnownDriver { driver } => {
            return Err(ServiceError::invalid_input(format!(
                "Controller {driver:?} is not available over the Network connection"
            )));
        }
        ControllerSelection::GenericGrblCompatible => {
            return Err(ServiceError::invalid_input(
                "Generic GRBL-compatible mode currently uses the Serial connection",
            ));
        }
        ControllerSelection::Unknown => {
            return Err(ServiceError::invalid_input(
                "A known Network controller or Auto-detect selection is required",
            ));
        }
    }
    let _connection_gate = ctx
        .controller_connection_gate
        .lock()
        .map_err(|e| lock_err("controller_connection_gate", e))?;
    if ctx
        .session
        .lock()
        .map_err(|e| lock_err("session", e))?
        .is_some()
    {
        return Err(ServiceError::conflict(
            "Already connected. Disconnect first.",
        ));
    }
    clear_pending_controller_connection(ctx)?;
    ctx.machine_coordinates_valid
        .store(false, Ordering::Release);
    ctx.active_jog.store(false, Ordering::Release);

    if matches!(
        input.selection,
        ControllerSelection::KnownDriver {
            driver: ControllerDriverId::Ruida
        }
    ) {
        return finish_ruida_network_connection(ctx, host, input.port);
    }
    let device_identity = network_device_identity(&host, input.port);
    if matches!(
        input.selection,
        ControllerSelection::KnownDriver {
            driver: ControllerDriverId::LaserPecker
        }
    ) {
        return finish_laserpecker_network_connection_attempt(
            ctx,
            host,
            input.port,
            input.selection,
            device_identity,
        );
    }
    finish_grbl_network_connection_attempt(ctx, host, input.port, input.selection, device_identity)
}

pub fn list_lihuiyu_usb_devices() -> ServiceResult<Vec<LihuiyuUsbDeviceInfo>> {
    enumerate_lihuiyu_usb_devices().map_err(|error| {
        ServiceError::machine(format!("Lihuiyu USB device enumeration failed: {error}"))
    })
}

pub fn begin_usb_controller_connection(
    ctx: &ServiceContext,
    input: BeginUsbControllerConnectionInput,
) -> ServiceResult<ControllerConnectionResult> {
    if input.bus_id.trim().is_empty() {
        return Err(ServiceError::invalid_input("USB bus ID is required"));
    }
    if input.selection
        != (ControllerSelection::KnownDriver {
            driver: ControllerDriverId::Lihuiyu,
        })
    {
        return Err(ServiceError::invalid_input(
            "The USB connection currently supports Lihuiyu M2/M3 Nano (Experimental)",
        ));
    }
    let _connection_gate = ctx
        .controller_connection_gate
        .lock()
        .map_err(|e| lock_err("controller_connection_gate", e))?;
    if ctx
        .session
        .lock()
        .map_err(|e| lock_err("session", e))?
        .is_some()
    {
        return Err(ServiceError::conflict(
            "Already connected. Disconnect first.",
        ));
    }
    clear_pending_controller_connection(ctx)?;
    ctx.machine_coordinates_valid
        .store(false, Ordering::Release);
    ctx.active_jog.store(false, Ordering::Release);

    let selector = LihuiyuUsbSelector {
        bus_id: input.bus_id,
        device_address: input.device_address,
        port_numbers: input.port_numbers,
    };
    let endpoint_name = selector.to_string();
    ctx.push_connection_event(
        "transport_open",
        Some(endpoint_name),
        None,
        Some("Opening Lihuiyu CH341 USB transport".to_string()),
        None,
    );
    let io = NativeLihuiyuUsbIo::open(&selector)
        .map_err(|error| ServiceError::machine(format!("Lihuiyu USB transport failed: {error}")))?;
    let device = io.device_info().clone();
    finish_lihuiyu_usb_connection(ctx, io, device)
}

pub fn continue_controller_connection(
    ctx: &ServiceContext,
    input: ContinueControllerConnectionInput,
) -> ServiceResult<ControllerConnectionResult> {
    let _connection_gate = ctx
        .controller_connection_gate
        .lock()
        .map_err(|e| lock_err("controller_connection_gate", e))?;
    if ctx
        .session
        .lock()
        .map_err(|e| lock_err("session", e))?
        .is_some()
    {
        return Err(ServiceError::conflict(
            "Already connected. Disconnect first.",
        ));
    }
    let mut pending_guard = ctx
        .pending_controller_connection
        .lock()
        .map_err(|e| lock_err("pending_controller_connection", e))?;
    let Some(mut pending) = pending_guard.take() else {
        return Err(ServiceError::invalid_state(
            "No controller connection decision is pending",
        ));
    };
    if pending.attempt_id != input.attempt_id {
        *pending_guard = Some(pending);
        return Err(ServiceError::invalid_input(
            "Controller connection challenge token does not match",
        ));
    }
    drop(pending_guard);
    if pending.created_at.elapsed() > CONTROLLER_CONNECTION_CHALLENGE_TTL {
        pending.session.disconnect();
        return Err(ServiceError::invalid_state(
            "Controller connection decision expired; connect again",
        ));
    }
    let selected_protocol = protocol_for_selection(&input.selection);
    let pending_protocol = protocol_for_pending(&pending.session);
    let laserpecker_selected = matches!(
        input.selection,
        ControllerSelection::KnownDriver {
            driver: ControllerDriverId::LaserPecker
        }
    );
    if input.decision != Some(ControllerMismatchDecision::Cancel)
        && (selected_protocol.is_some_and(|selected| selected != pending_protocol)
            || laserpecker_selected)
        && let ControllerConnectionEndpoint::Serial {
            port_name,
            baud_rate,
        } = pending.endpoint.clone()
    {
        let device_identity = pending.device_identity.clone();
        pending.session.disconnect();
        ctx.push_connection_event(
            "protocol_switch",
            Some(port_name.clone()),
            Some(baud_rate),
            Some("Reopening the connection with the selected controller protocol".to_string()),
            None,
        );
        return match selected_protocol.expect("protocol mismatch requires an explicit selection") {
            SerialControllerProtocol::Grbl => {
                if laserpecker_selected {
                    finish_laserpecker_serial_connection_attempt(
                        ctx,
                        port_name,
                        baud_rate,
                        input.selection,
                        device_identity,
                    )
                } else {
                    finish_grbl_connection_attempt(
                        ctx,
                        port_name,
                        baud_rate,
                        input.selection,
                        device_identity,
                    )
                }
            }
            SerialControllerProtocol::Marlin => finish_marlin_connection_attempt(
                ctx,
                port_name,
                baud_rate,
                input.selection,
                device_identity,
            ),
            SerialControllerProtocol::Smoothieware => finish_smoothieware_connection_attempt(
                ctx,
                port_name,
                baud_rate,
                input.selection,
                device_identity,
            ),
        };
    }
    pending.selection = input.selection.clone();
    let resolution = match &pending.session {
        PendingControllerSession::Grbl { probe, .. } => {
            controller_choice::resolve_grbl_family_probe_choice(
                &controller_choice::ResolveGrblFamilyProbeChoiceInput {
                    selection: input.selection,
                    device_identity: pending.device_identity.clone(),
                    transport_kind: pending.endpoint.transport_kind(),
                    remembered_override: None,
                    decision: input.decision,
                },
                probe.clone(),
            )
            .resolution
        }
        PendingControllerSession::Marlin { probe, .. } => {
            controller_choice::resolve_marlin_probe_choice(
                &controller_choice::ResolveMarlinProbeChoiceInput {
                    selection: input.selection,
                    device_identity: pending.device_identity.clone(),
                    remembered_override: None,
                    decision: input.decision,
                },
                probe.clone(),
            )
            .resolution
        }
        PendingControllerSession::Smoothieware { probe, .. } => {
            controller_choice::resolve_smoothieware_probe_choice(
                &controller_choice::ResolveSmoothiewareProbeChoiceInput {
                    selection: input.selection,
                    device_identity: pending.device_identity.clone(),
                    remembered_override: None,
                    decision: input.decision,
                },
                probe.clone(),
            )
            .resolution
        }
    };
    finish_controller_connection_resolution(ctx, pending, resolution)
}

/// Close a still-pending desktop controller challenge after its bounded
/// lifetime. A resolved, cancelled, replaced, or newer attempt is untouched.
pub fn expire_controller_connection(ctx: &ServiceContext, attempt_id: &str) -> ServiceResult<bool> {
    let _connection_gate = ctx
        .controller_connection_gate
        .lock()
        .map_err(|e| lock_err("controller_connection_gate", e))?;
    let mut pending_guard = ctx
        .pending_controller_connection
        .lock()
        .map_err(|e| lock_err("pending_controller_connection", e))?;
    let should_expire = pending_guard.as_ref().is_some_and(|pending| {
        pending.attempt_id == attempt_id
            && pending.created_at.elapsed() >= CONTROLLER_CONNECTION_CHALLENGE_TTL
    });
    if !should_expire {
        return Ok(false);
    }
    let pending = pending_guard.take();
    drop(pending_guard);
    if let Some(mut pending) = pending {
        pending.session.disconnect();
        ctx.push_connection_event(
            "controller_choice_expired",
            Some(pending.endpoint.display_name()),
            pending.endpoint.baud_rate(),
            Some("Controller connection choice expired".to_string()),
            None,
        );
    }
    Ok(true)
}

fn runtime_summary(
    session: Option<&MachineSessionHandle>,
    job_progress: Option<JobProgress>,
    machine_coordinates_valid: bool,
) -> MachineRuntimeState {
    match session {
        Some(session) => MachineRuntimeState {
            controller_family: Some(session.controller_family()),
            controller_model: Some(session.controller_model()),
            controller_driver: session.controller_driver(),
            controller_selection: session.controller_selection(),
            detected_controller_identity: session.detected_controller_identity(),
            experimental_mode: session.experimental_mode(),
            product_tier: session.controller_product_tier(),
            evidence_state: session.controller_evidence_state(),
            transport_kind: Some(session.transport_kind()),
            capabilities: Some(session.capabilities()),
            session_state: session.session_state(),
            machine_status: Some(session.machine_status()),
            machine_coordinates_valid,
            controller_info: session.controller_info(),
            controller_settings: session.controller_settings(),
            job_active: job_progress.is_some(),
            job_progress,
            job_tick_error: None,
        },
        None => MachineRuntimeState {
            controller_family: None,
            controller_model: None,
            controller_driver: None,
            controller_selection: None,
            detected_controller_identity: None,
            experimental_mode: false,
            product_tier: None,
            evidence_state: None,
            transport_kind: None,
            capabilities: None,
            session_state: SessionState::Disconnected,
            machine_status: None,
            machine_coordinates_valid: false,
            controller_info: None,
            controller_settings: None,
            job_active: false,
            job_progress: None,
            job_tick_error: None,
        },
    }
}

pub fn list_serial_ports_op() -> ServiceResult<Vec<PortInfo>> {
    list_available_ports().map_err(|e| ServiceError::machine(e.to_string()))
}

pub fn runtime_state(ctx: &ServiceContext) -> ServiceResult<MachineRuntimeState> {
    let (job_progress, job_tick_error) = match tick_job(ctx) {
        Ok(p) => (p, None),
        Err(e) => {
            tracing::warn!(error = %e, "tick_job failed");
            (None, Some(e.to_string()))
        }
    };
    let active_job = job_is_active(job_progress.as_ref());
    let mut session_guard = ctx.session.lock().map_err(|e| lock_err("session", e))?;
    if !active_job && let Some(session) = session_guard.as_mut() {
        // Read pending data (including response from previous `?` query).
        session.poll();
        recover_ready_if_idle_after_validation(session)?;
        // Send a fresh `?` so the next poll picks up an up-to-date status report.
        session.query_status();
    }
    let machine_coordinates_valid = ctx.machine_coordinates_valid.load(Ordering::Acquire);
    let mut state = runtime_summary(
        session_guard.as_ref(),
        job_progress,
        machine_coordinates_valid,
    );
    state.job_tick_error = job_tick_error;
    let run_state = state.machine_status.as_ref().map(|status| status.run_state);
    drop(session_guard);
    if run_state.is_some_and(|state| state != MachineRunState::Idle) {
        let _ = force_laser_fire_stop(ctx, "run_state_change");
    }
    Ok(state)
}

pub fn connect_machine(
    ctx: &ServiceContext,
    input: ConnectMachineInput,
) -> ServiceResult<SessionState> {
    let _connection_gate = ctx
        .controller_connection_gate
        .lock()
        .map_err(|e| lock_err("controller_connection_gate", e))?;
    if ctx
        .session
        .lock()
        .map_err(|e| lock_err("session", e))?
        .is_some()
    {
        return Err(ServiceError::conflict(
            "Already connected. Disconnect first.",
        ));
    }
    clear_pending_controller_connection(ctx)?;
    ctx.machine_coordinates_valid
        .store(false, Ordering::Release);
    ctx.active_jog.store(false, Ordering::Release);

    let (session, event_payload, baud_rate_for_profile) = match input.target.clone() {
        MachineConnectionTarget::GrblSerial {
            port_name,
            baud_rate,
        } => {
            // The connect may have succeeded at a fallback baud rate; the
            // working rate is what gets persisted to the profile.
            let (session, actual_baud) = connect_grbl(ctx, port_name.clone(), baud_rate)?;
            (
                session,
                json!({
                    "target": { "type": "grbl_serial", "port_name": port_name, "baud_rate": actual_baud },
                }),
                Some(actual_baud),
            )
        }
        MachineConnectionTarget::DiscoveryCandidate { candidate_id } => {
            let candidate = discovery::find_candidate(ctx, &candidate_id)?;
            let (session, baud_rate_for_profile) = connect_from_candidate(ctx, &candidate_id)?;
            (
                session,
                json!({
                    "target": { "type": "discovery_candidate", "candidate_id": candidate_id },
                    "candidate": candidate,
                }),
                baud_rate_for_profile,
            )
        }
    };

    register_machine_session(ctx, session, event_payload, baud_rate_for_profile, None)
}

pub fn disconnect_machine(ctx: &ServiceContext) -> ServiceResult<()> {
    let _connection_gate = ctx
        .controller_connection_gate
        .lock()
        .map_err(|e| lock_err("controller_connection_gate", e))?;
    let _ = force_laser_fire_stop(ctx, "disconnect");
    clear_pending_controller_connection(ctx)?;
    let mut disconnect_warning: Option<String> = None;
    {
        let mut session_lock = ctx.session.lock().map_err(|e| lock_err("session", e))?;
        if let Some(ref mut session) = *session_lock {
            // Disconnect must always release the session: the phases where the
            // controller refuses a software stop (recovery, unowned activity,
            // power fault) are exactly the ones whose remedy is reconnecting.
            // Surface the failed stop as a warning instead of trapping the
            // user with an undroppable session.
            if let Err(error) = session.disconnect() {
                disconnect_warning = Some(error);
            }
        }
        *session_lock = None;
    }
    ctx.machine_coordinates_valid
        .store(false, Ordering::Release);
    ctx.active_jog.store(false, Ordering::Release);
    let mut job_lock = ctx.job.lock().map_err(|e| lock_err("job", e))?;
    *job_lock = None;
    drop(job_lock);
    remember_job_progress(ctx, None)?;
    ctx.emit_event(
        "machine.disconnected",
        json!({ "stop_warning": disconnect_warning }),
    );
    Ok(())
}

pub fn machine_status(ctx: &ServiceContext) -> ServiceResult<MachineStatus> {
    let job_progress = tick_job(ctx)?;
    let active_job = job_is_active(job_progress.as_ref());
    let mut session_lock = ctx.session.lock().map_err(|e| lock_err("session", e))?;
    let session = session_lock
        .as_mut()
        .ok_or_else(|| ServiceError::invalid_state("Not connected"))?;
    if !active_job {
        // Read pending data (including the response from the previous `?` query).
        session.poll();
        recover_ready_if_idle_after_validation(session)?;
        // Send a fresh `?` so the *next* poll picks up an up-to-date status report.
        // GRBL always responds to `?` regardless of alarm/idle/run state.
        session.query_status();
    }
    let status = session.machine_status();
    drop(session_lock);
    if status.run_state != MachineRunState::Idle {
        let _ = force_laser_fire_stop(ctx, "run_state_change");
    }
    Ok(status)
}

pub fn machine_coordinates_valid(ctx: &ServiceContext) -> bool {
    ctx.machine_coordinates_valid.load(Ordering::Acquire)
}

pub fn session_state(ctx: &ServiceContext) -> ServiceResult<SessionState> {
    let job_progress = tick_job(ctx)?;
    let active_job = job_is_active(job_progress.as_ref());
    let mut session_lock = ctx.session.lock().map_err(|e| lock_err("session", e))?;
    match &mut *session_lock {
        Some(session) => {
            if !active_job {
                session.poll();
                recover_ready_if_idle_after_validation(session)?;
            }
            Ok(session.session_state())
        }
        None => Ok(SessionState::Disconnected),
    }
}

pub fn home(ctx: &ServiceContext) -> ServiceResult<()> {
    let mut session_lock = ctx.session.lock().map_err(|e| lock_err("session", e))?;
    let session = session_lock
        .as_mut()
        .ok_or_else(|| ServiceError::invalid_state("Not connected"))?;
    require_ready_for_motion(session, "Home")?;
    match session {
        MachineSessionHandle::Grbl(session) => {
            session
                .home()
                .map_err(|e| ServiceError::machine(e.to_string()))?;
            ctx.machine_coordinates_valid.store(true, Ordering::Release);
        }
        MachineSessionHandle::Marlin(_) | MachineSessionHandle::Smoothieware(_) => {
            return Err(invalid_capability("Home", ControllerFamily::Gcode));
        }
        MachineSessionHandle::Ruida(session) => {
            session.home().map_err(ServiceError::machine)?;
            // The Ruida status contract used by this adapter confirms that the
            // controller returned to idle, but does not report an absolute XY
            // position. Keep absolute machine coordinates invalid even after a
            // successful home command rather than inventing a position that the
            // controller has not reported.
            ctx.machine_coordinates_valid
                .store(false, Ordering::Release);
        }
        MachineSessionHandle::Lihuiyu(session) => {
            session.home().map_err(ServiceError::machine)?;
            ctx.machine_coordinates_valid
                .store(false, Ordering::Release);
        }
        MachineSessionHandle::Dsp(session) => {
            session.home();
            ctx.machine_coordinates_valid.store(true, Ordering::Release);
        }
        MachineSessionHandle::Galvo(_) => {
            return Err(invalid_capability("Home", ControllerFamily::Galvo));
        }
    }
    ctx.emit_event("machine.homed", json!({}));
    Ok(())
}

pub fn unlock(ctx: &ServiceContext) -> ServiceResult<()> {
    let mut session_lock = ctx.session.lock().map_err(|e| lock_err("session", e))?;
    let session = session_lock
        .as_mut()
        .ok_or_else(|| ServiceError::invalid_state("Not connected"))?;
    match session {
        MachineSessionHandle::Grbl(session) => {
            session
                .unlock()
                .map_err(|e| ServiceError::machine(e.to_string()))?;
            let _ = session.poll();
            let _ = session.poll_status();
            std::thread::sleep(std::time::Duration::from_millis(100));
            let _ = session.poll();
            if session.session_state() != SessionState::Ready {
                session
                    .mark_ready()
                    .map_err(|e| ServiceError::machine(e.to_string()))?;
            }
        }
        MachineSessionHandle::Marlin(_) | MachineSessionHandle::Smoothieware(_) => {
            return Err(invalid_capability("Unlock", ControllerFamily::Gcode));
        }
        MachineSessionHandle::Ruida(_) => {
            return Err(invalid_capability("Unlock", ControllerFamily::Dsp));
        }
        MachineSessionHandle::Lihuiyu(session) => {
            session.unlock().map_err(ServiceError::machine)?;
        }
        MachineSessionHandle::Dsp(session) => {
            session.machine_status.run_state = MachineRunState::Idle;
        }
        MachineSessionHandle::Galvo(_) => {
            return Err(invalid_capability("Unlock", ControllerFamily::Galvo));
        }
    }
    ctx.emit_event("machine.unlocked", json!({}));
    Ok(())
}

pub fn jog(ctx: &ServiceContext, input: JogMachineInput) -> ServiceResult<()> {
    require_positive_finite(input.feed_rate, "feed_rate")?;
    require_finite(input.x_mm, "x_mm")?;
    require_finite(input.y_mm, "y_mm")?;
    let profile = active_profile(ctx)?;
    if input.z_mm.is_some_and(|value| !value.is_finite()) {
        return Err(ServiceError::invalid_input("z must be a finite number"));
    }
    let mut session_lock = ctx.session.lock().map_err(|e| lock_err("session", e))?;
    let session = session_lock
        .as_mut()
        .ok_or_else(|| ServiceError::invalid_state("Not connected"))?;
    require_idle_ready_for_motion(session, "Jog")?;
    let mut ruida_table_axis = None;
    match session {
        MachineSessionHandle::Grbl(session) => {
            validate_optional_z(&profile, input.z_mm, "Jog")?;
            let mut x_mm = input.x_mm;
            let mut y_mm = input.y_mm;
            if input.continuous {
                if ctx.active_jog.swap(true, Ordering::AcqRel) {
                    return Err(ServiceError::busy("A continuous jog is already active"));
                }
                let status = session.last_status();
                if x_mm > 0.0 {
                    x_mm = x_mm.min((profile.bed_width_mm - status.work_position.x).max(0.0));
                } else if x_mm < 0.0 {
                    x_mm = -(-x_mm).min(status.work_position.x.max(0.0));
                }
                if y_mm > 0.0 {
                    y_mm = y_mm.min((profile.bed_height_mm - status.work_position.y).max(0.0));
                } else if y_mm < 0.0 {
                    y_mm = -(-y_mm).min(status.work_position.y.max(0.0));
                }
            }
            session
                .jog(x_mm, y_mm, input.z_mm, input.feed_rate)
                .map_err(|e| {
                    if input.continuous {
                        ctx.active_jog.store(false, Ordering::Release);
                    }
                    ServiceError::machine(e.to_string())
                })?;
        }
        MachineSessionHandle::Marlin(_) | MachineSessionHandle::Smoothieware(_) => {
            validate_optional_z(&profile, input.z_mm, "Jog")?;
            return Err(invalid_capability("Jog", ControllerFamily::Gcode));
        }
        MachineSessionHandle::Ruida(session) => {
            if input.continuous {
                return Err(ServiceError::invalid_state(
                    "Continuous press-and-hold jogging is not available for Ruida; use finite jog steps",
                ));
            }
            if let Some(distance_mm) = input.z_mm {
                if input.x_mm != 0.0 || input.y_mm != 0.0 {
                    return Err(ServiceError::invalid_input(
                        "Ruida lift-table jogging cannot be combined with X or Y motion",
                    ));
                }
                let axis = match profile.ruida_table_axis {
                    RuidaTableAxis::Disabled => {
                        return Err(ServiceError::invalid_state(
                            "Select the Ruida lift-table axis in the active machine profile",
                        ));
                    }
                    RuidaTableAxis::Z => RuidaJogAxis::Z,
                    RuidaTableAxis::U => RuidaJogAxis::U,
                };
                ruida_table_axis = Some(profile.ruida_table_axis);
                session
                    .jog_table(axis, distance_mm, input.feed_rate)
                    .map_err(ServiceError::machine)?;
            } else {
                session
                    .jog(input.x_mm, input.y_mm, input.feed_rate)
                    .map_err(ServiceError::machine)?;
            }
        }
        MachineSessionHandle::Lihuiyu(session) => {
            if input.continuous {
                return Err(ServiceError::invalid_state(
                    "Continuous press-and-hold jogging is not available for Lihuiyu; use finite jog steps",
                ));
            }
            if input.z_mm.is_some() {
                return Err(ServiceError::invalid_state(
                    "Lihuiyu Experimental jogging supports X and Y only",
                ));
            }
            session
                .jog(input.x_mm, input.y_mm)
                .map_err(ServiceError::machine)?;
        }
        MachineSessionHandle::Dsp(session) => {
            validate_optional_z(&profile, input.z_mm, "Jog")?;
            session.machine_status.work_position.x += input.x_mm;
            session.machine_status.work_position.y += input.y_mm;
            if let Some(z) = input.z_mm {
                session.machine_status.work_position.z += z;
            }
        }
        MachineSessionHandle::Galvo(_) => {
            return Err(invalid_capability("Jog", ControllerFamily::Galvo));
        }
    }
    ctx.emit_event(
        "machine.jogged",
        json!({
            "x_mm": input.x_mm,
            "y_mm": input.y_mm,
            "z_mm": input.z_mm,
            "feed_rate": input.feed_rate,
            "continuous": input.continuous,
            "ruida_table_axis": ruida_table_axis,
        }),
    );
    Ok(())
}

pub fn jog_cancel(ctx: &ServiceContext) -> ServiceResult<()> {
    let mut session_lock = ctx.session.lock().map_err(|e| lock_err("session", e))?;
    let session = session_lock
        .as_mut()
        .ok_or_else(|| ServiceError::invalid_state("Not connected"))?;
    match session {
        MachineSessionHandle::Grbl(session) => {
            session
                .jog_cancel()
                .map_err(|e| ServiceError::machine(e.to_string()))?;
            ctx.active_jog.store(false, Ordering::Release);
        }
        MachineSessionHandle::Marlin(_) | MachineSessionHandle::Smoothieware(_) => {
            return Err(invalid_capability("Jog cancel", ControllerFamily::Gcode));
        }
        MachineSessionHandle::Ruida(_) => {
            return Err(ServiceError::invalid_state(
                "Ruida uses finite acknowledged jog steps; there is no active continuous jog to cancel",
            ));
        }
        MachineSessionHandle::Lihuiyu(_) => {
            return Err(ServiceError::invalid_state(
                "Lihuiyu uses finite acknowledged jog steps; there is no active continuous jog to cancel",
            ));
        }
        MachineSessionHandle::Dsp(session) => {
            session.machine_status.run_state = MachineRunState::Idle;
            ctx.active_jog.store(false, Ordering::Release);
        }
        MachineSessionHandle::Galvo(_) => {
            return Err(invalid_capability("Jog cancel", ControllerFamily::Galvo));
        }
    }
    ctx.emit_event("machine.jog.cancelled", json!({}));
    Ok(())
}

pub fn run_preflight_check(ctx: &ServiceContext) -> ServiceResult<PreflightReport> {
    run_preflight_check_with_options(ctx, &planning::SessionJobOptions::default())
}

pub fn run_preflight_check_with_options(
    ctx: &ServiceContext,
    job_options: &planning::SessionJobOptions,
) -> ServiceResult<PreflightReport> {
    // Check tool layers before plan generation — this must run even if the
    // plan is empty (e.g. all enabled layers are tool layers).
    let (has_project, tool_layer_check) = {
        let project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
        match project_guard.as_ref() {
            Some(p) => (true, check_tool_layers(p)),
            None => (false, None),
        }
    };

    // Try to generate a plan. If plan generation fails for a loaded project
    // (e.g. all enabled layers are tool layers → EmptyPlan), return a Fail
    // report that still includes the tool-layer informational warning.
    // If no project is loaded, let the error propagate normally.
    let plan = match planning::ensure_current_plan_with_options(ctx, job_options) {
        Ok(p) => p,
        Err(plan_err) if has_project => {
            let mut checks = vec![PreflightCheck {
                category: "plan".to_string(),
                description: "Plan generation".to_string(),
                passed: false,
                message: format!("{plan_err}"),
            }];
            if let Some(tool_check) = tool_layer_check {
                checks.push(tool_check);
            }
            let report = PreflightReport {
                outcome: PreflightOutcome::Fail,
                checks,
            };
            ctx.emit_event(
                "job.preflight.completed",
                json!({
                    "outcome": report.outcome,
                    "check_count": report.checks.len(),
                }),
            );
            return Ok(report);
        }
        Err(e) => return Err(e),
    };

    let project_for_controller = planning::current_project(ctx)?;
    let session_guard = ctx.session.lock().map_err(|e| lock_err("session", e))?;
    let session = session_guard
        .as_ref()
        .ok_or_else(|| ServiceError::invalid_state("Not connected"))?;
    let profile = active_profile(ctx)?;
    let mut report = match session {
        MachineSessionHandle::Grbl(session) => run_preflight(session, &plan, &profile),
        MachineSessionHandle::Ruida(session) => {
            run_ruida_preflight(session, &plan, &project_for_controller, &profile)
        }
        MachineSessionHandle::Lihuiyu(session) => {
            run_lihuiyu_preflight(session, &plan, &project_for_controller, &profile)
        }
        _ => {
            let (mut checks, basics_ok) = generic_preflight_checks(
                session.session_state(),
                session.machine_status().run_state,
                &plan,
                &profile,
            );
            let can_run_job = session.capabilities().can_run_job;
            checks.push(PreflightCheck {
                category: "controller".to_string(),
                description: "Controller supports job start".to_string(),
                passed: can_run_job,
                message: format!("{:?}", session.controller_family()),
            });
            PreflightReport {
                outcome: if basics_ok && can_run_job {
                    PreflightOutcome::Pass
                } else {
                    PreflightOutcome::Fail
                },
                checks,
            }
        }
    };

    // Surface plan warnings as preflight checks so the user sees them before starting.
    if !plan.warnings.is_empty() {
        for w in &plan.warnings {
            report.checks.push(PreflightCheck {
                category: "planner".to_string(),
                description: w.message.clone(),
                passed: false,
                message: w.message.clone(),
            });
        }
        if report.outcome == PreflightOutcome::Pass {
            report.outcome = PreflightOutcome::PassWithWarnings;
        }
    }

    if !plan.failed_entries.is_empty() {
        for failure in &plan.failed_entries {
            let message = match &failure.reason {
                beambench_planner::PlanEntryFailureReason::ImageMaskSkipped {
                    object_id,
                    message,
                } => format!("Image mask issue on object '{}': {}", object_id, message),
                beambench_planner::PlanEntryFailureReason::OffsetFillTimedOut {
                    iterations,
                    elapsed_ms,
                } => format!(
                    "Offset Fill entry was omitted after timing out at {} iterations ({:.1}s)",
                    iterations,
                    *elapsed_ms as f64 / 1000.0
                ),
                beambench_planner::PlanEntryFailureReason::OffsetFillEmergencyCeiling {
                    iterations,
                } => format!(
                    "Offset Fill entry was omitted after hitting the {}-iteration emergency limit",
                    iterations
                ),
            };
            report.checks.push(PreflightCheck {
                category: "planner".to_string(),
                description: format!(
                    "Layer {} entry {} was omitted for safety",
                    failure.layer_id,
                    failure.cut_entry_id.as_deref().unwrap_or("unknown")
                ),
                passed: false,
                message,
            });
        }
        report.outcome = PreflightOutcome::Fail;
    }

    let project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    if let Some(project) = project_guard.as_ref() {
        if let Some(ref snapshot) = project.machine_profile_snapshot {
            if let Some(mismatch_check) = check_profile_mismatch(snapshot, &profile) {
                report.checks.push(mismatch_check);
                if report.outcome == PreflightOutcome::Pass
                    || report.outcome == PreflightOutcome::PassWithWarnings
                {
                    report.outcome = PreflightOutcome::Fail;
                }
            }
        }
    }
    // Tool layer informational check (pre-computed before plan generation)
    if let Some(tool_check) = tool_layer_check {
        report.checks.push(tool_check);
        if report.outcome == PreflightOutcome::Pass {
            report.outcome = PreflightOutcome::PassWithWarnings;
        }
    }
    ctx.emit_event(
        "job.preflight.completed",
        json!({
            "outcome": report.outcome,
            "check_count": report.checks.len(),
        }),
    );
    Ok(report)
}

pub fn start_job(ctx: &ServiceContext) -> ServiceResult<JobProgress> {
    start_job_with_options(ctx, &planning::SessionJobOptions::default())
}

pub fn start_job_with_options(
    ctx: &ServiceContext,
    job_options: &planning::SessionJobOptions,
) -> ServiceResult<JobProgress> {
    force_laser_fire_stop(ctx, "job_start")?;
    let mut job_lock = ctx.job.lock().map_err(|e| lock_err("job", e))?;
    if job_lock.is_some() {
        return Err(ServiceError::conflict(
            "A job is already active. Cancel or wait for it to finish.",
        ));
    }

    let preflight = run_preflight_check_with_options(ctx, job_options)?;
    match preflight.outcome {
        PreflightOutcome::Pass => {}
        PreflightOutcome::PassWithWarnings => {
            return Err(ServiceError::invalid_state(
                "Preflight passed with warnings; resolve warnings before starting the job",
            )
            .with_details(json!({ "preflight": preflight })));
        }
        PreflightOutcome::Fail => {
            return Err(
                ServiceError::invalid_state("Preflight failed; the job cannot be started")
                    .with_details(json!({ "preflight": preflight })),
            );
        }
    }

    // Force fresh plan for job start (ensure_current_plan syncs live position automatically)
    planning::invalidate_plan_cache(ctx)?;
    let plan = planning::ensure_current_plan_with_options(ctx, job_options)?;

    // Build GcodeConfig from the project's persisted optimization + active machine profile.
    // Runtime overlay (current_position) isn't read here — G-code emission
    // only needs the finish-position fields; the planner consumed the
    // runtime overlay upstream via the offset pass.
    let project_for_gcode = planning::current_project(ctx)?;
    let profile = active_profile(ctx)?;
    let mut gcode_config =
        super::output::build_gcode_config(&project_for_gcode.optimization, &profile);
    super::output::apply_project_gcode_metadata(&mut gcode_config, &project_for_gcode);

    let mut session_lock = ctx.session.lock().map_err(|e| lock_err("session", e))?;
    let session = session_lock
        .as_mut()
        .ok_or_else(|| ServiceError::invalid_state("Not connected"))?;
    let job = match session {
        MachineSessionHandle::Grbl(session) => {
            let config = gcode_config;
            let mut job = JobController::prepare(&plan, &config)
                .map_err(|e| ServiceError::machine(format!("Job prepare failed: {e}")))?;
            job.start(session)
                .map_err(|e| ServiceError::machine(format!("Job start failed: {e}")))?;
            ActiveJobHandle::Grbl(job)
        }
        MachineSessionHandle::Marlin(session) => {
            let driver = session.driver();
            let commands = acknowledged_gcode_commands(driver, &plan, gcode_config)
                .map_err(ServiceError::machine)?;
            ActiveJobHandle::Marlin(MarlinRuntimeJob::start(commands, session).map_err(
                |error| ServiceError::machine(format!("{driver:?} job start failed: {error}")),
            )?)
        }
        MachineSessionHandle::Smoothieware(session) => {
            let commands = smoothieware_gcode_commands(session, &plan, gcode_config)
                .map_err(ServiceError::machine)?;
            ActiveJobHandle::Smoothieware(
                SmoothiewareRuntimeJob::start(commands, session).map_err(|error| {
                    ServiceError::machine(format!("Smoothieware job start failed: {error}"))
                })?,
            )
        }
        MachineSessionHandle::Ruida(session) => {
            let compiled = compile_ruida_execution_plan(&plan, &project_for_gcode, &profile)
                .map_err(ServiceError::machine)?;
            ActiveJobHandle::Ruida(session.start_job(&compiled, false).map_err(|error| {
                ServiceError::machine(format!("Ruida job start failed: {error}"))
            })?)
        }
        MachineSessionHandle::Lihuiyu(session) => {
            let compiled = compile_lihuiyu_execution_plan(&plan, &project_for_gcode, &profile)
                .map_err(ServiceError::machine)?;
            ActiveJobHandle::Lihuiyu(session.start_job(&compiled, false).map_err(|error| {
                ServiceError::machine(format!("Lihuiyu job start failed: {error}"))
            })?)
        }
        MachineSessionHandle::Dsp(session) => ActiveJobHandle::Dsp(session.start_job(&plan)),
        MachineSessionHandle::Galvo(session) => ActiveJobHandle::Galvo(session.start_job(&plan)),
    };
    let progress = job.progress();
    clear_retained_terminal_job(ctx)?;
    *job_lock = Some(job);
    drop(session_lock);
    drop(job_lock);
    remember_job_progress(ctx, Some(progress.clone()))?;
    ctx.emit_event("job.started", events::job_summary(&progress));
    Ok(progress)
}

fn drop_stale_machine_session(ctx: &ServiceContext) {
    if let Ok(mut session_lock) = ctx.session.lock() {
        if let Some(MachineSessionHandle::Grbl(session)) = session_lock.as_mut() {
            let _ = session.soft_reset();
            let _ = session.send_command("M5");
        }
        if let Some(session) = session_lock.as_mut() {
            let _ = session.disconnect();
        }
        *session_lock = None;
    }

    ctx.machine_coordinates_valid
        .store(false, Ordering::Release);
    ctx.active_jog.store(false, Ordering::Release);
}

fn handle_fatal_job_tick_error(ctx: &ServiceContext, message: String) {
    drop_stale_machine_session(ctx);

    if let Ok(mut job) = ctx.job.lock() {
        *job = None;
    }
    let _ = remember_job_progress(ctx, None);
    ctx.push_error(message.clone());
    ctx.emit_event(
        "job.tick_failed",
        json!({
            "message": message,
        }),
    );
    ctx.emit_event(
        "machine.disconnected",
        json!({ "reason": "job_tick_failed" }),
    );
}

fn disconnect_failed_running_job(ctx: &ServiceContext, message: String) {
    drop_stale_machine_session(ctx);
    ctx.push_error(message);
    ctx.emit_event(
        "machine.disconnected",
        json!({ "reason": "job_failed_while_running" }),
    );
}

pub fn tick_job(ctx: &ServiceContext) -> ServiceResult<Option<JobProgress>> {
    let mut job_lock = ctx.job.lock().map_err(|e| lock_err("job", e))?;
    let mut session_lock = ctx.session.lock().map_err(|e| lock_err("session", e))?;

    let progress = match (&mut *job_lock, &mut *session_lock) {
        (Some(job), Some(session)) => match job.tick(Some(session)) {
            Ok(progress) => Some(progress),
            Err(err) => {
                let error = ServiceError::machine(err.to_string());
                let message = format!("Job streaming tick failed: {error}");
                retain_terminal_job_diagnostic(
                    ctx,
                    "fatal_tick_error",
                    Some(job.progress()),
                    Some(message.clone()),
                    Some(job),
                    Some(session),
                );
                drop(session_lock);
                drop(job_lock);
                handle_fatal_job_tick_error(ctx, message);
                return Err(error);
            }
        },
        (Some(job), None) => match job.tick(None) {
            Ok(progress) => Some(progress),
            Err(err) => {
                let error = ServiceError::machine(err.to_string());
                let message = format!("Job streaming tick failed: {error}");
                retain_terminal_job_diagnostic(
                    ctx,
                    "fatal_tick_error",
                    Some(job.progress()),
                    Some(message.clone()),
                    Some(job),
                    None,
                );
                drop(session_lock);
                drop(job_lock);
                handle_fatal_job_tick_error(ctx, message);
                return Err(error);
            }
        },
        _ => None,
    };

    let failed_running_message = progress.as_ref().and_then(|progress| {
        let failed_while_session_running = progress.state == JobState::Failed
            && session_lock
                .as_ref()
                .is_some_and(|session| session.session_state() == SessionState::Running);
        failed_while_session_running.then(|| {
            progress.error_message.clone().unwrap_or_else(|| {
                "Job failed while the machine session was still running".to_string()
            })
        })
    });

    if let Some(progress) = progress.as_ref() {
        emit_job_progress_if_changed(ctx, progress)?;
        let terminal = matches!(
            progress.state,
            beambench_common::machine::JobState::Completed
                | beambench_common::machine::JobState::Failed
                | beambench_common::machine::JobState::Cancelled
        );
        if terminal {
            let reason = if failed_running_message.is_some() {
                "failed_while_session_running"
            } else {
                "terminal_progress"
            };
            retain_terminal_job_diagnostic(
                ctx,
                reason,
                Some(progress.clone()),
                progress
                    .error_message
                    .clone()
                    .or_else(|| failed_running_message.clone()),
                job_lock.as_ref(),
                session_lock.as_ref(),
            );
            *job_lock = None;
        }
    }

    let terminal_event = progress.as_ref().and_then(|progress| match progress.state {
        beambench_common::machine::JobState::Completed => Some("job.completed"),
        beambench_common::machine::JobState::Failed => Some("job.failed"),
        beambench_common::machine::JobState::Cancelled => Some("job.cancelled"),
        _ => None,
    });

    drop(session_lock);
    drop(job_lock);
    if terminal_event.is_some() {
        remember_job_progress(ctx, None)?;
    }
    if let (Some(event_type), Some(progress)) = (terminal_event, progress.as_ref()) {
        ctx.emit_event(event_type, events::job_summary(progress));
    }
    if let Some(message) = failed_running_message {
        disconnect_failed_running_job(ctx, message);
    }

    Ok(progress)
}

/// Tick cadence for the active job. Acknowledged-G-code sessions (Marlin,
/// Smoothieware) send at most one line per protocol round, so their
/// throughput is bounded by this cadence; GRBL keeps its own 127-byte RX
/// window full and only needs the slow cadence.
fn job_tick_interval(ctx: &ServiceContext) -> Duration {
    let fast_tick = ctx.job.lock().ok().is_some_and(|guard| {
        matches!(
            guard.as_ref(),
            Some(ActiveJobHandle::Marlin(_) | ActiveJobHandle::Smoothieware(_))
        )
    }) || ctx.session.lock().ok().is_some_and(|guard| {
        // Staged Lihuiyu/Ruida transfers advance one bounded step per tick;
        // their throughput is bounded by this cadence.
        match guard.as_ref() {
            Some(MachineSessionHandle::Lihuiyu(session)) => session.is_transferring(),
            Some(MachineSessionHandle::Ruida(session)) => session.is_uploading(),
            _ => false,
        }
    });
    if fast_tick {
        Duration::from_millis(5)
    } else {
        Duration::from_millis(50)
    }
}

pub fn spawn_job_tick_loop(ctx: Arc<ServiceContext>) {
    if ctx.job_tick_loop_running.swap(true, Ordering::AcqRel) {
        return;
    }

    std::thread::spawn(move || {
        loop {
            match tick_job(&ctx) {
                Ok(Some(progress)) => {
                    if matches!(
                        progress.state,
                        beambench_common::machine::JobState::Completed
                            | beambench_common::machine::JobState::Failed
                            | beambench_common::machine::JobState::Cancelled
                    ) {
                        break;
                    }
                }
                Ok(None) => break,
                Err(err) => {
                    tracing::warn!(error = %err, "tick_job failed");
                    break;
                }
            }

            std::thread::sleep(job_tick_interval(&ctx));
        }

        ctx.job_tick_loop_running.store(false, Ordering::Release);
    });
}

pub fn pause_job(ctx: &ServiceContext) -> ServiceResult<()> {
    let mut job_lock = ctx.job.lock().map_err(|e| lock_err("job", e))?;
    let mut session_lock = ctx.session.lock().map_err(|e| lock_err("session", e))?;
    let job = job_lock
        .as_mut()
        .ok_or_else(|| ServiceError::invalid_state("No active job"))?;
    let session = session_lock
        .as_mut()
        .ok_or_else(|| ServiceError::invalid_state("Not connected"))?;
    let progress = job.pause(session).map_err(ServiceError::invalid_state)?;
    drop(session_lock);
    drop(job_lock);
    remember_job_progress(ctx, Some(progress.clone()))?;
    ctx.emit_event("job.paused", events::job_summary(&progress));
    Ok(())
}

pub fn resume_job(ctx: &ServiceContext) -> ServiceResult<()> {
    let mut job_lock = ctx.job.lock().map_err(|e| lock_err("job", e))?;
    let mut session_lock = ctx.session.lock().map_err(|e| lock_err("session", e))?;
    let job = job_lock
        .as_mut()
        .ok_or_else(|| ServiceError::invalid_state("No active job"))?;
    let session = session_lock
        .as_mut()
        .ok_or_else(|| ServiceError::invalid_state("Not connected"))?;
    let progress = job.resume(session).map_err(ServiceError::invalid_state)?;
    drop(session_lock);
    drop(job_lock);
    remember_job_progress(ctx, Some(progress.clone()))?;
    ctx.emit_event("job.resumed", events::job_summary(&progress));
    Ok(())
}

pub fn cancel_job(ctx: &ServiceContext) -> ServiceResult<()> {
    let mut job_lock = ctx.job.lock().map_err(|e| lock_err("job", e))?;
    let mut session_lock = ctx.session.lock().map_err(|e| lock_err("session", e))?;
    let job = job_lock
        .as_mut()
        .ok_or_else(|| ServiceError::invalid_state("No active job"))?;
    let session = session_lock
        .as_mut()
        .ok_or_else(|| ServiceError::invalid_state("Not connected"))?;
    let progress = match job.cancel(session) {
        Ok(progress) => progress,
        Err(err) => {
            let progress = job.progress();
            ctx.push_error(format!("Job cancel failed while clearing stale job: {err}"));
            progress
        }
    };
    retain_terminal_job_diagnostic(
        ctx,
        "cancelled",
        Some(progress.clone()),
        progress.error_message.clone(),
        Some(job),
        Some(session),
    );
    *job_lock = None;
    drop(session_lock);
    drop(job_lock);
    remember_job_progress(ctx, None)?;
    ctx.emit_event("job.cancelled", events::job_summary(&progress));
    Ok(())
}

fn object_layer_is_tool(
    project: &beambench_core::Project,
    object: &beambench_core::ProjectObject,
) -> bool {
    project
        .find_layer(object.layer_id)
        .is_some_and(|layer| layer.is_tool_layer)
}

/// Which objects to include in frame job bounds and physical frame motion.
fn frame_objects(
    ctx: &ServiceContext,
    selected_object_ids: &[String],
) -> ServiceResult<(
    Vec<beambench_core::ProjectObject>,
    Vec<beambench_core::ProjectObject>,
)> {
    let project = planning::current_project(ctx)?;
    let include_tool_bounds = ctx
        .settings
        .lock()
        .map_err(|e| lock_err("settings", e))?
        .include_tool_layers_in_job_bounds;

    let (bounds_objects, physical_objects): (Vec<_>, Vec<_>) = if selected_object_ids.is_empty() {
        let visible_unlocked: Vec<_> = project
            .objects
            .iter()
            .filter(|o| o.visible && !o.locked)
            .cloned()
            .collect();
        let bounds = visible_unlocked
            .iter()
            .filter(|o| include_tool_bounds || !object_layer_is_tool(&project, o))
            .cloned()
            .collect();
        let physical = visible_unlocked
            .iter()
            .filter(|o| !object_layer_is_tool(&project, o))
            .cloned()
            .collect();
        (bounds, physical)
    } else {
        let selected: Vec<_> = project
            .objects
            .iter()
            .filter(|o| selected_object_ids.contains(&o.id.to_string()))
            .cloned()
            .collect();
        (selected.clone(), selected)
    };
    if physical_objects.is_empty() {
        return Err(ServiceError::invalid_state("No objects to frame"));
    }
    Ok((bounds_objects, physical_objects))
}

fn bounds_of(objects: &[beambench_core::ProjectObject]) -> Bounds {
    let mut min_x = f64::MAX;
    let mut min_y = f64::MAX;
    let mut max_x = f64::MIN;
    let mut max_y = f64::MIN;
    for obj in objects {
        min_x = min_x.min(obj.bounds.min.x);
        min_y = min_y.min(obj.bounds.min.y);
        max_x = max_x.max(obj.bounds.max.x);
        max_y = max_y.max(obj.bounds.max.y);
    }
    Bounds::new(Point2D::new(min_x, min_y), Point2D::new(max_x, max_y))
}

fn frame_power_percent(laser_on_override: bool, profile: &MachineProfile) -> f64 {
    if laser_on_override || profile.laser_on_when_framing {
        1.0
    } else {
        0.0
    }
}

pub fn frame_job(
    ctx: &ServiceContext,
    frame_mode: &str,
    selected_object_ids: &[String],
    laser_on_override: bool,
    feed_rate: Option<f64>,
) -> ServiceResult<JobProgress> {
    force_laser_fire_stop(ctx, "frame_start")?;
    let frame_feed_rate = feed_rate.unwrap_or(DEFAULT_MOVE_FEED_RATE_MM_MIN);
    require_positive_finite(frame_feed_rate, "feed_rate")?;
    let mut job_lock = ctx.job.lock().map_err(|e| lock_err("job", e))?;
    if job_lock.is_some() {
        return Err(ServiceError::conflict(
            "A job is already active. Cancel or wait for it to finish.",
        ));
    }

    // Sync live machine position so CurrentPosition mode uses fresh data
    planning::sync_current_position(ctx)?;

    let (bounds_objects, physical_objects) = frame_objects(ctx, selected_object_ids)?;
    let raw_bounds = if bounds_objects.is_empty() {
        bounds_of(&physical_objects)
    } else {
        bounds_of(&bounds_objects)
    };
    let physical_bounds = bounds_of(&physical_objects);

    let ruida_frame = {
        let session_lock = ctx.session.lock().map_err(|e| lock_err("session", e))?;
        matches!(
            session_lock.as_ref(),
            Some(MachineSessionHandle::Ruida(_) | MachineSessionHandle::Lihuiyu(_))
        )
    };
    let frame_power = if ruida_frame {
        0.0
    } else {
        frame_power_percent(laser_on_override, &active_profile(ctx)?)
    };

    let mut frame_plan = match frame_mode {
        "rubber_band" => {
            let hull = beambench_core::rubber_band_outline(&physical_objects);
            let hull_points: Vec<Point2D> = hull
                .subpaths
                .first()
                .map(|sp| {
                    sp.commands
                        .iter()
                        .map(|cmd| match cmd {
                            beambench_common::path::PathCommand::MoveTo { x, y }
                            | beambench_common::path::PathCommand::LineTo { x, y } => {
                                Point2D::new(*x, *y)
                            }
                            _ => Point2D::zero(),
                        })
                        .collect()
                })
                .unwrap_or_default();
            build_hull_frame_plan(&hull_points, &physical_bounds, frame_power, frame_feed_rate)
        }
        _ => build_frame_plan(&physical_bounds, frame_power, frame_feed_rate),
    };

    // Apply the same coordinate transforms the real plan uses so the frame
    // traces the area the actual job will engrave.
    let project = planning::current_project(ctx)?;

    apply_workspace_origin_transform(
        &mut frame_plan.segments,
        project.workspace.origin,
        project.workspace.bed_height_mm,
    );

    let current_pos = ctx
        .optimization_runtime
        .lock()
        .map_err(|e| lock_err("optimization_runtime", e))?
        .current_position;

    if project.start_from == StartFromMode::UserOrigin && project.user_origin.is_none() {
        // No offset — matches the builder skip-not-fallback behavior
    } else if project.start_from == StartFromMode::CurrentPosition && current_pos.is_none() {
        // No offset — no live machine position available
    } else if project.start_from != StartFromMode::AbsoluteCoords {
        // Anchor the frame exactly like the job: job-origin anchor of the
        // content lands on the start-from target, in machine space.
        let y_flipped =
            project.workspace.origin != beambench_core::workspace::WorkspaceOrigin::TopLeft;
        let content_bounds = if y_flipped {
            flip_bounds_y(&raw_bounds, project.workspace.bed_height_mm)
        } else {
            raw_bounds
        };
        let target = if project.start_from == StartFromMode::CurrentPosition {
            current_pos.unwrap_or((0.0, 0.0))
        } else {
            project.user_origin.unwrap_or((0.0, 0.0))
        };
        anchor_segments_to_target(
            &mut frame_plan.segments,
            project.job_origin,
            &content_bounds,
            y_flipped,
            target,
        );
    }

    // Recompute bounds after transforms
    let bounds = calculate_plan_bounds(&frame_plan.segments);
    frame_plan.bounds = bounds;

    // Frame G-code must honor the active profile: constant-power machines
    // need M3 (M4 dims toward zero at framing speeds) and the profile's
    // s_value_max caps laser-on framing power.
    let profile = active_profile(ctx)?;
    let frame_gcode_config = super::output::build_gcode_config(&project.optimization, &profile);

    let mut session_lock = ctx.session.lock().map_err(|e| lock_err("session", e))?;
    let session = session_lock
        .as_mut()
        .ok_or_else(|| ServiceError::invalid_state("Not connected"))?;
    let job = match session {
        MachineSessionHandle::Grbl(session) => {
            let config = frame_gcode_config;
            let mut job = JobController::prepare(&frame_plan, &config)
                .map_err(|e| ServiceError::machine(format!("Frame prepare failed: {e}")))?;
            job.start(session)
                .map_err(|e| ServiceError::machine(format!("Frame start failed: {e}")))?;
            ActiveJobHandle::Grbl(job)
        }
        MachineSessionHandle::Marlin(session) => {
            let driver = session.driver();
            let commands = acknowledged_gcode_commands(driver, &frame_plan, frame_gcode_config)
                .map_err(ServiceError::machine)?;
            ActiveJobHandle::Marlin(MarlinRuntimeJob::start(commands, session).map_err(
                |error| ServiceError::machine(format!("{driver:?} frame start failed: {error}")),
            )?)
        }
        MachineSessionHandle::Smoothieware(session) => {
            let commands = smoothieware_gcode_commands(session, &frame_plan, frame_gcode_config)
                .map_err(ServiceError::machine)?;
            ActiveJobHandle::Smoothieware(
                SmoothiewareRuntimeJob::start(commands, session).map_err(|error| {
                    ServiceError::machine(format!("Smoothieware frame start failed: {error}"))
                })?,
            )
        }
        MachineSessionHandle::Ruida(session) => {
            let compiled = compile_ruida_execution_plan(&frame_plan, &project, &profile)
                .map_err(ServiceError::machine)?;
            ActiveJobHandle::Ruida(session.start_job(&compiled, true).map_err(|error| {
                ServiceError::machine(format!("Ruida frame start failed: {error}"))
            })?)
        }
        MachineSessionHandle::Lihuiyu(session) => {
            let compiled = compile_lihuiyu_execution_plan(&frame_plan, &project, &profile)
                .map_err(ServiceError::machine)?;
            ActiveJobHandle::Lihuiyu(session.start_job(&compiled, true).map_err(|error| {
                ServiceError::machine(format!("Lihuiyu frame start failed: {error}"))
            })?)
        }
        MachineSessionHandle::Dsp(session) => ActiveJobHandle::Dsp(session.frame_job()),
        MachineSessionHandle::Galvo(session) => ActiveJobHandle::Galvo(session.frame_job()),
    };
    let progress = job.progress();
    clear_retained_terminal_job(ctx)?;
    *job_lock = Some(job);
    drop(session_lock);
    drop(job_lock);
    remember_job_progress(ctx, Some(progress.clone()))?;
    ctx.emit_event(
        "job.frame.started",
        json!({
            "bounds": bounds,
            "progress": events::job_summary(&progress),
        }),
    );
    Ok(progress)
}

pub fn move_laser_to(ctx: &ServiceContext, input: MoveLaserInput) -> ServiceResult<()> {
    require_finite(input.x, "x")?;
    require_finite(input.y, "y")?;
    require_positive_finite(input.feed_rate, "feed_rate")?;
    let profile = active_profile(ctx)?;
    validate_optional_z(&profile, input.z, "Go")?;
    let mut session_lock = ctx.session.lock().map_err(|e| lock_err("session", e))?;
    let session = session_lock
        .as_mut()
        .ok_or_else(|| ServiceError::invalid_state("Not connected"))?;
    require_idle_ready_for_motion(session, "Go")?;
    match session {
        MachineSessionHandle::Grbl(session) => {
            session
                .send_command(&grbl_commands::move_to(
                    input.x,
                    input.y,
                    input.z,
                    input.feed_rate,
                ))
                .map_err(|e| ServiceError::machine(e.to_string()))?;
        }
        MachineSessionHandle::Marlin(_) | MachineSessionHandle::Smoothieware(_) => {
            return Err(invalid_capability("Go", ControllerFamily::Gcode));
        }
        MachineSessionHandle::Ruida(_) => {
            return Err(ServiceError::invalid_state(
                "Absolute Go moves are not yet available for Ruida; use finite X/Y jog steps",
            ));
        }
        MachineSessionHandle::Lihuiyu(_) => {
            return Err(ServiceError::invalid_state(
                "Absolute Go moves are not available for Lihuiyu; use finite X/Y jog steps",
            ));
        }
        MachineSessionHandle::Dsp(session) => {
            session.machine_status.work_position.x = input.x;
            session.machine_status.work_position.y = input.y;
            if let Some(z) = input.z {
                session.machine_status.work_position.z = z;
            }
        }
        MachineSessionHandle::Galvo(_) => {
            return Err(invalid_capability("Go", ControllerFamily::Galvo));
        }
    }
    ctx.emit_event(
        "machine.move.started",
        json!({
            "x": input.x,
            "y": input.y,
            "z": input.z,
            "feed_rate": input.feed_rate,
            "coordinate_system": "work",
        }),
    );
    Ok(())
}

pub fn move_laser_to_machine(ctx: &ServiceContext, input: MoveLaserInput) -> ServiceResult<()> {
    require_finite(input.x, "x")?;
    require_finite(input.y, "y")?;
    require_positive_finite(input.feed_rate, "feed_rate")?;
    let profile = active_profile(ctx)?;
    validate_optional_z(&profile, input.z, "Machine-zero move")?;
    if !ctx.machine_coordinates_valid.load(Ordering::Acquire) {
        return Err(ServiceError::invalid_state(
            "Machine-zero moves require homing in the current session first",
        ));
    }
    let mut session_lock = ctx.session.lock().map_err(|e| lock_err("session", e))?;
    let session = session_lock
        .as_mut()
        .ok_or_else(|| ServiceError::invalid_state("Not connected"))?;
    require_idle_ready_for_motion(session, "Machine-zero move")?;
    let MachineSessionHandle::Grbl(session) = session else {
        return Err(ServiceError::invalid_state(
            "Machine-zero moves are only supported on GCode/GRBL controllers",
        ));
    };
    session
        .send_command(&grbl_commands::move_to_machine(
            input.x,
            input.y,
            input.z,
            input.feed_rate,
        ))
        .map_err(|e| ServiceError::machine(e.to_string()))?;
    ctx.emit_event(
        "machine.move.started",
        json!({
            "x": input.x,
            "y": input.y,
            "z": input.z,
            "feed_rate": input.feed_rate,
            "coordinate_system": "machine",
        }),
    );
    Ok(())
}

fn fire_s_value(profile: &MachineProfile, power_percent: f64) -> u32 {
    let capped_percent = power_percent
        .min(profile.max_power_percent)
        .clamp(0.0, 100.0);
    ((profile.s_value_max as f64 * capped_percent / 100.0).round() as u32)
        .clamp(1, profile.s_value_max.max(1))
}

fn force_laser_fire_stop(ctx: &ServiceContext, reason: &str) -> ServiceResult<()> {
    let stop_needed = {
        let mut guard = ctx
            .active_laser_fire
            .lock()
            .map_err(|e| lock_err("active_laser_fire", e))?;
        let Some(active) = guard.as_mut() else {
            return Ok(());
        };
        active.stop_requested = true;
        active.expires_at = Instant::now();
        true
    };
    if !stop_needed {
        return Ok(());
    }

    {
        let mut session_lock = ctx.session.lock().map_err(|e| lock_err("session", e))?;
        if let Some(MachineSessionHandle::Grbl(session)) = session_lock.as_mut() {
            session
                .send_command(grbl_commands::laser_fire_off())
                .map_err(|e| ServiceError::machine(e.to_string()))?;
        }
    }

    let cleared = {
        let mut guard = ctx
            .active_laser_fire
            .lock()
            .map_err(|e| lock_err("active_laser_fire", e))?;
        guard.take().is_some()
    };
    if !cleared {
        return Ok(());
    }
    ctx.emit_event(
        "machine.fire.stopped",
        json!({
            "reason": reason,
        }),
    );
    Ok(())
}

fn spawn_laser_fire_deadman(ctx: Arc<ServiceContext>, token: String) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(FIRE_DEADMAN_POLL);
            let should_stop = {
                let Ok(guard) = ctx.active_laser_fire.lock() else {
                    return;
                };
                match guard.as_ref() {
                    Some(active) if active.token == token => {
                        let now = Instant::now();
                        active.stop_requested
                            || now >= active.expires_at
                            || now >= active.max_expires_at
                    }
                    _ => return,
                }
            };
            if should_stop {
                if force_laser_fire_stop(&ctx, "deadman_timeout").is_ok() {
                    return;
                }
            }
        }
    });
}

pub fn laser_fire_start(
    ctx: Arc<ServiceContext>,
    power_percent: Option<f64>,
) -> ServiceResult<LaserFireStartResult> {
    let job_lock = ctx.job.lock().map_err(|e| lock_err("job", e))?;
    if job_lock.is_some() {
        return Err(ServiceError::invalid_state("MACHINE_BUSY_JOB_RUNNING"));
    }
    drop(job_lock);

    let profile = active_profile(&ctx)?;
    if !profile.enable_laser_fire_button {
        return Err(ServiceError::invalid_state(
            "Manual fire is disabled in the active machine profile",
        ));
    }
    let requested_percent = power_percent.unwrap_or(profile.default_fire_power_percent);
    if !requested_percent.is_finite() || requested_percent <= 0.0 || requested_percent > 100.0 {
        return Err(ServiceError::invalid_input(
            "fire power must be greater than 0 and no more than 100 percent",
        ));
    }
    let effective_percent = requested_percent.min(profile.max_power_percent).min(100.0);
    if effective_percent <= 0.0 {
        return Err(ServiceError::invalid_input(
            "active profile maximum power prevents manual fire",
        ));
    }
    let s_value = fire_s_value(&profile, effective_percent);
    let token = uuid::Uuid::new_v4().to_string();
    let now = Instant::now();
    {
        let mut active_guard = ctx
            .active_laser_fire
            .lock()
            .map_err(|e| lock_err("active_laser_fire", e))?;
        if active_guard.is_some() {
            return Err(ServiceError::busy("Manual fire is already active"));
        }
        let mut session_lock = ctx.session.lock().map_err(|e| lock_err("session", e))?;
        let session = session_lock
            .as_mut()
            .ok_or_else(|| ServiceError::invalid_state("Not connected"))?;
        require_idle_ready_for_motion(session, "Manual fire")?;
        let MachineSessionHandle::Grbl(session) = session else {
            return Err(ServiceError::invalid_state(
                "Manual fire is only supported on GCode/GRBL controllers",
            ));
        };
        session
            .send_command(&grbl_commands::laser_fire_on(s_value))
            .map_err(|e| ServiceError::machine(e.to_string()))?;
        *active_guard = Some(LaserFireState {
            token: token.clone(),
            expires_at: now + FIRE_KEEPALIVE_GRACE,
            max_expires_at: now + FIRE_MAX_HOLD,
            stop_requested: false,
        });
    }

    spawn_laser_fire_deadman(Arc::clone(&ctx), token.clone());
    ctx.emit_event(
        "machine.fire.started",
        json!({
            "power_percent": effective_percent,
            "s_value": s_value,
            "keepalive_interval_ms": FIRE_KEEPALIVE_INTERVAL_MS,
            "max_duration_ms": FIRE_MAX_HOLD.as_millis() as u64,
        }),
    );
    Ok(LaserFireStartResult {
        token,
        power_percent: effective_percent,
        s_value,
        keepalive_interval_ms: FIRE_KEEPALIVE_INTERVAL_MS,
        max_duration_ms: FIRE_MAX_HOLD.as_millis() as u64,
    })
}

pub fn laser_fire_keepalive(ctx: &ServiceContext, token: &str) -> ServiceResult<()> {
    let now = Instant::now();
    let mut active_guard = ctx
        .active_laser_fire
        .lock()
        .map_err(|e| lock_err("active_laser_fire", e))?;
    let active = active_guard
        .as_mut()
        .ok_or_else(|| ServiceError::invalid_state("Manual fire is not active"))?;
    if active.token != token {
        return Err(ServiceError::invalid_state(
            "Manual fire token does not match",
        ));
    }
    if active.stop_requested {
        return Err(ServiceError::invalid_state("Manual fire is stopping"));
    }
    if now >= active.max_expires_at {
        drop(active_guard);
        force_laser_fire_stop(ctx, "max_duration")?;
        return Err(ServiceError::invalid_state(
            "Manual fire maximum duration elapsed",
        ));
    }
    active.expires_at = (now + FIRE_KEEPALIVE_GRACE).min(active.max_expires_at);
    Ok(())
}

pub fn laser_fire_stop(ctx: &ServiceContext, token: &str) -> ServiceResult<()> {
    let should_stop = {
        let guard = ctx
            .active_laser_fire
            .lock()
            .map_err(|e| lock_err("active_laser_fire", e))?;
        match guard.as_ref() {
            Some(active) => active.token == token,
            None => return Ok(()),
        }
    };
    if !should_stop {
        return Err(ServiceError::invalid_state(
            "Manual fire token does not match",
        ));
    }
    force_laser_fire_stop(ctx, "frontend_release")
}

pub fn set_feed_override(ctx: &ServiceContext, action: &str) -> ServiceResult<()> {
    let bytes = match action {
        "reset" => grbl_commands::feed_override_reset(),
        "increase_10" => grbl_commands::feed_override_increase_10(),
        "decrease_10" => grbl_commands::feed_override_decrease_10(),
        "increase_1" => grbl_commands::feed_override_increase_1(),
        "decrease_1" => grbl_commands::feed_override_decrease_1(),
        _ => {
            return Err(ServiceError::invalid_input(format!(
                "Unknown feed override action: {action}"
            )));
        }
    };
    let mut session_lock = ctx.session.lock().map_err(|e| lock_err("session", e))?;
    let session = session_lock
        .as_mut()
        .ok_or_else(|| ServiceError::invalid_state("Not connected"))?;
    let MachineSessionHandle::Grbl(session) = session else {
        return Err(ServiceError::invalid_state(
            "Feed override is only supported on GCode/GRBL controllers",
        ));
    };
    session
        .send_realtime(bytes)
        .map_err(|e| ServiceError::machine(e.to_string()))?;
    ctx.emit_event(
        "machine.override.feed_changed",
        json!({
            "action": action,
        }),
    );
    Ok(())
}

pub fn set_spindle_override(ctx: &ServiceContext, action: &str) -> ServiceResult<()> {
    let bytes = match action {
        "reset" => grbl_commands::spindle_override_reset(),
        "increase_10" => grbl_commands::spindle_override_increase_10(),
        "decrease_10" => grbl_commands::spindle_override_decrease_10(),
        "increase_1" => grbl_commands::spindle_override_increase_1(),
        "decrease_1" => grbl_commands::spindle_override_decrease_1(),
        _ => {
            return Err(ServiceError::invalid_input(format!(
                "Unknown spindle override action: {action}"
            )));
        }
    };
    let mut session_lock = ctx.session.lock().map_err(|e| lock_err("session", e))?;
    let session = session_lock
        .as_mut()
        .ok_or_else(|| ServiceError::invalid_state("Not connected"))?;
    let MachineSessionHandle::Grbl(session) = session else {
        return Err(ServiceError::invalid_state(
            "Spindle override is only supported on GCode/GRBL controllers",
        ));
    };
    session
        .send_realtime(bytes)
        .map_err(|e| ServiceError::machine(e.to_string()))?;
    ctx.emit_event(
        "machine.override.spindle_changed",
        json!({
            "action": action,
        }),
    );
    Ok(())
}

pub fn reset_all_overrides(ctx: &ServiceContext) -> ServiceResult<()> {
    let mut session_lock = ctx.session.lock().map_err(|e| lock_err("session", e))?;
    let session = session_lock
        .as_mut()
        .ok_or_else(|| ServiceError::invalid_state("Not connected"))?;
    let MachineSessionHandle::Grbl(session) = session else {
        return Err(ServiceError::invalid_state(
            "Overrides reset is only supported on GCode/GRBL controllers",
        ));
    };
    session
        .send_realtime(grbl_commands::feed_override_reset())
        .map_err(|e| ServiceError::machine(e.to_string()))?;
    session
        .send_realtime(grbl_commands::spindle_override_reset())
        .map_err(|e| ServiceError::machine(e.to_string()))?;
    ctx.emit_event("machine.override.reset", json!({}));
    Ok(())
}

pub fn emergency_stop(ctx: &ServiceContext) -> ServiceResult<()> {
    let _ = force_laser_fire_stop(ctx, "emergency_stop");
    ctx.machine_coordinates_valid
        .store(false, Ordering::Release);
    ctx.active_jog.store(false, Ordering::Release);
    let mut session_lock = ctx.session.lock().map_err(|e| lock_err("session", e))?;
    let session = session_lock
        .as_mut()
        .ok_or_else(|| ServiceError::invalid_state("Not connected"))?;
    match session {
        MachineSessionHandle::Grbl(session) => {
            session
                .soft_reset()
                .map_err(|e| ServiceError::machine(format!("Emergency stop failed: {e}")))?;
            let mut banner_received = false;
            for _ in 0..MAX_BANNER_POLLS {
                let responses = session
                    .poll()
                    .map_err(|e| ServiceError::machine(e.to_string()))?;
                if responses
                    .iter()
                    .any(|r| matches!(r, GrblResponse::Banner(_) | GrblResponse::Alarm(_)))
                {
                    banner_received = true;
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            if banner_received && session.session_state() != SessionState::Alarm {
                let _ = session.request_settings();
                std::thread::sleep(std::time::Duration::from_millis(200));
                let _ = session.poll();
                let _ = session.poll_status();
                std::thread::sleep(std::time::Duration::from_millis(100));
                let _ = session.poll();
            }
        }
        MachineSessionHandle::Marlin(session) => {
            session.emergency_shutdown().map_err(|error| {
                ServiceError::machine(format!("Emergency stop failed: {error}"))
            })?;
        }
        MachineSessionHandle::Smoothieware(session) => {
            session.emergency_shutdown().map_err(|error| {
                ServiceError::machine(format!("Emergency stop failed: {error}"))
            })?;
        }
        MachineSessionHandle::Ruida(session) => {
            session.emergency_stop().map_err(|error| {
                ServiceError::machine(format!("Emergency stop failed: {error}"))
            })?;
        }
        MachineSessionHandle::Lihuiyu(session) => {
            session.emergency_stop().map_err(|error| {
                ServiceError::machine(format!("Emergency stop failed: {error}"))
            })?;
        }
        MachineSessionHandle::Dsp(session) => {
            session.machine_status.run_state = MachineRunState::Idle;
        }
        MachineSessionHandle::Galvo(session) => {
            session.machine_status.run_state = MachineRunState::Idle;
        }
    }
    // Lock-order invariant: job is always acquired BEFORE session (see
    // tick_job). Holding session while taking job here deadlocked against a
    // concurrently running tick, so the session lock must drop first.
    drop(session_lock);
    {
        let mut job_lock = ctx.job.lock().map_err(|e| lock_err("job", e))?;
        *job_lock = None;
    }
    remember_job_progress(ctx, None)?;
    ctx.emit_event("machine.emergency_stop", json!({}));
    Ok(())
}

/// Capture the current work position and store it as user_origin on the project.
/// Returns the captured (x, y) coordinates.
pub fn set_work_origin(ctx: &ServiceContext) -> ServiceResult<(f64, f64)> {
    // Read work_position from the session (release lock before touching project)
    let captured = {
        let session_lock = ctx.session.lock().map_err(|e| lock_err("session", e))?;
        let session = session_lock
            .as_ref()
            .ok_or_else(|| ServiceError::invalid_state("Not connected"))?;
        match session {
            MachineSessionHandle::Grbl(session) => {
                let status = session.last_status();
                (status.work_position.x, status.work_position.y)
            }
            MachineSessionHandle::Marlin(_) | MachineSessionHandle::Smoothieware(_) => {
                return Err(invalid_capability(
                    "Set work origin",
                    ControllerFamily::Gcode,
                ));
            }
            MachineSessionHandle::Ruida(_) => {
                return Err(invalid_capability("Set work origin", ControllerFamily::Dsp));
            }
            MachineSessionHandle::Lihuiyu(_) => {
                return Err(invalid_capability("Set work origin", ControllerFamily::Dsp));
            }
            MachineSessionHandle::Dsp(session) => {
                let wp = &session.machine_status.work_position;
                (wp.x, wp.y)
            }
            MachineSessionHandle::Galvo(_) => {
                return Err(invalid_capability(
                    "Set work origin",
                    ControllerFamily::Galvo,
                ));
            }
        }
    };

    // Store as user_origin on the project (with undo snapshot)
    {
        let mut proj_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
        let project = proj_guard
            .as_mut()
            .ok_or_else(|| ServiceError::not_found("No project open"))?;
        ctx.push_project_undo_snapshot(project)
            .map_err(ServiceError::internal)?;
        project.user_origin = Some(captured);
        project.dirty = true;
    }

    ctx.emit_event("machine.origin.set", json!({ "position": captured }));
    Ok(captured)
}

pub fn reset_work_origin(ctx: &ServiceContext) -> ServiceResult<()> {
    // Verify connected (release lock before touching project)
    {
        let session_lock = ctx.session.lock().map_err(|e| lock_err("session", e))?;
        let session = session_lock
            .as_ref()
            .ok_or_else(|| ServiceError::invalid_state("Not connected"))?;
        match session {
            MachineSessionHandle::Grbl(_) | MachineSessionHandle::Dsp(_) => {}
            MachineSessionHandle::Marlin(_) | MachineSessionHandle::Smoothieware(_) => {
                return Err(invalid_capability(
                    "Reset work origin",
                    ControllerFamily::Gcode,
                ));
            }
            MachineSessionHandle::Ruida(_) => {
                return Err(invalid_capability(
                    "Reset work origin",
                    ControllerFamily::Dsp,
                ));
            }
            MachineSessionHandle::Lihuiyu(_) => {
                return Err(invalid_capability(
                    "Reset work origin",
                    ControllerFamily::Dsp,
                ));
            }
            MachineSessionHandle::Galvo(_) => {
                return Err(invalid_capability(
                    "Reset work origin",
                    ControllerFamily::Galvo,
                ));
            }
        }
    }

    // Clear user_origin on the project (with undo snapshot)
    {
        let mut proj_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
        let project = proj_guard
            .as_mut()
            .ok_or_else(|| ServiceError::not_found("No project open"))?;
        ctx.push_project_undo_snapshot(project)
            .map_err(ServiceError::internal)?;
        project.user_origin = None;
        project.dirty = true;
    }

    ctx.emit_event("machine.origin.reset", json!({}));
    Ok(())
}

pub fn test_air_assist(
    ctx: &ServiceContext,
    duration_ms: u64,
) -> ServiceResult<TestAirAssistResult> {
    let job_lock = ctx.job.lock().map_err(|e| lock_err("job", e))?;
    if job_lock.is_some() {
        return Err(ServiceError::invalid_state("MACHINE_BUSY_JOB_RUNNING"));
    }

    let profile = active_profile(ctx)?;
    let on_gcode = profile.air_assist_on_gcode.clone();
    let off_gcode = profile.air_assist_off_gcode.clone();
    if on_gcode.trim().is_empty() || off_gcode.trim().is_empty() {
        return Err(ServiceError::invalid_input(
            "Active profile air-assist on/off G-code must not be empty",
        ));
    }
    let advisory_text = if profile.preset_id.as_deref() == Some("sculpfun_s30_pro_max_20w") {
        crate::ops::profiles::profile_presets()
            .into_iter()
            .find(|preset| preset.id == "sculpfun_s30_pro_max_20w")
            .and_then(|preset| preset.advisory_text.map(str::to_string))
    } else {
        None
    };

    send_air_assist_lines(ctx, &on_gcode)?;
    std::thread::sleep(Duration::from_millis(duration_ms));
    send_air_assist_lines(ctx, &off_gcode)?;
    drop(job_lock);

    Ok(TestAirAssistResult {
        tested: true,
        duration_ms,
        air_assist_on_gcode: on_gcode,
        air_assist_off_gcode: off_gcode,
        advisory_text,
    })
}

fn send_air_assist_lines(ctx: &ServiceContext, gcode: &str) -> ServiceResult<()> {
    let mut session_lock = ctx.session.lock().map_err(|e| lock_err("session", e))?;
    let session = session_lock
        .as_mut()
        .ok_or_else(|| ServiceError::invalid_state("Not connected"))?;
    match session {
        MachineSessionHandle::Grbl(session) => {
            for line in gcode.lines().map(str::trim).filter(|line| !line.is_empty()) {
                session
                    .send_command(line)
                    .map_err(|e| ServiceError::machine(e.to_string()))?;
            }
            Ok(())
        }
        MachineSessionHandle::Marlin(_)
        | MachineSessionHandle::Smoothieware(_)
        | MachineSessionHandle::Ruida(_)
        | MachineSessionHandle::Lihuiyu(_)
        | MachineSessionHandle::Dsp(_)
        | MachineSessionHandle::Galvo(_) => Err(ServiceError::invalid_state(
            "Air-assist G-code diagnostics are only supported for GRBL sessions",
        )),
    }
}

pub fn save_profile(
    ctx: &ServiceContext,
    profile: MachineProfile,
) -> ServiceResult<MachineProfile> {
    crate::ops::profiles::save_profile(
        ctx,
        crate::ops::profiles::SaveProfileInput {
            profile_id: Some(profile.id),
            name: profile.name,
            preset_id: profile.preset_id,
            preset_version: profile.preset_version,
            bed_width_mm: profile.bed_width_mm,
            bed_height_mm: profile.bed_height_mm,
            max_speed_mm_min: profile.max_speed_mm_min,
            max_power_percent: profile.max_power_percent,
            s_value_max: profile.s_value_max,
            homing_enabled: profile.homing_enabled,
            default_baud_rate: profile.default_baud_rate,
            firmware_type: profile.firmware_type,
            notes: profile.notes,
            selected_camera_id: profile.selected_camera_id,
            camera_calibration: profile.camera_calibration,
            camera_alignment: profile.camera_alignment,
            origin: profile.origin,
            laser_offset_x: profile.laser_offset_x,
            laser_offset_y: profile.laser_offset_y,
            enable_laser_offset: profile.enable_laser_offset,
            swap_xy: profile.swap_xy,
            job_checklist: profile.job_checklist,
            frame_continuously: profile.frame_continuously,
            laser_on_when_framing: profile.laser_on_when_framing,
            tab_pulse_width_ms: profile.tab_pulse_width_ms,
            cnc_machine: profile.cnc_machine,
            use_constant_power: profile.use_constant_power,
            emit_s_every_g1: profile.emit_s_every_g1,
            use_g0_for_overscan: profile.use_g0_for_overscan,
            air_assist_on_gcode: profile.air_assist_on_gcode,
            air_assist_off_gcode: profile.air_assist_off_gcode,
            air_assist_on_delay_ms: profile.air_assist_on_delay_ms,
            job_header_gcode: profile.job_header_gcode,
            job_footer_gcode: profile.job_footer_gcode,
            transfer_mode: profile.transfer_mode,
            preferred_default_origin: profile.preferred_default_origin,
            scanning_offsets: profile.scanning_offsets,
            enable_scanning_offset: profile.enable_scanning_offset,
            dot_width_mm: profile.dot_width_mm,
            enable_dot_width: profile.enable_dot_width,
            supports_z_moves: profile.supports_z_moves,
            z_move_feed_mm_min: profile.z_move_feed_mm_min,
            ruida_table_axis: profile.ruida_table_axis,
            enable_laser_fire_button: profile.enable_laser_fire_button,
            default_fire_power_percent: profile.default_fire_power_percent,
            quality_test_settings: profile.quality_test_settings,
        },
    )
}

pub fn delete_profile(ctx: &ServiceContext, profile_id: MachineProfileId) -> ServiceResult<()> {
    crate::ops::profiles::delete_profile(ctx, profile_id)
}

pub fn set_active_profile(
    ctx: &ServiceContext,
    profile_id: Option<MachineProfileId>,
) -> ServiceResult<()> {
    crate::ops::profiles::set_active_profile(ctx, profile_id)
}

pub fn list_profiles(ctx: &ServiceContext) -> ServiceResult<Vec<MachineProfile>> {
    crate::ops::profiles::list_profiles(ctx)
}

pub fn persist_profiles(ctx: &ServiceContext) {
    persist_settings_to_disk(ctx);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::MachineSessionHandle;
    use crate::test_support::PersistTestGuard;
    use beambench_common::ConsoleDirection;
    use beambench_common::feedback::DiagnosticTerminalJob;
    use beambench_common::machine::{DeviceIdentity, DiscoveryCandidate};
    use beambench_core::object::ShapeKind;
    use beambench_core::{AppSettings, ObjectData, Project, ProjectObject};
    use beambench_dsp::DspSession;
    use beambench_grbl::GcodeConfig;
    use beambench_planner::ExecutionPlan;
    use beambench_serial::{MockSerialTransport, SerialError, SerialTransport};
    use chrono::Utc;
    use std::collections::VecDeque;
    use std::io::{ErrorKind, Read, Write};
    use std::net::{TcpListener, UdpSocket};
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, Mutex};
    use uuid::Uuid;

    static RUIDA_UDP_TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn virtual_lihuiyu_uses_the_same_registered_usb_product_path() {
        let ctx = ServiceContext::new();
        let device = LihuiyuUsbDeviceInfo {
            bus_id: "virtual".to_string(),
            device_address: 4,
            port_numbers: vec![1, 2],
            vendor_id: beambench_lihuiyu::USB_VENDOR_ID,
            product_id: beambench_lihuiyu::USB_PRODUCT_ID,
            manufacturer: Some("Lihuiyu Studio Labs".to_string()),
            product: Some("M2 Nano".to_string()),
            serial_number: None,
            has_required_bulk_endpoints: Some(true),
            driver: Some("virtual-usb".to_string()),
        };
        let result = finish_lihuiyu_usb_connection(
            &ctx,
            beambench_lihuiyu::LihuiyuVirtualUsbIo::m2_nano(),
            device,
        )
        .unwrap();
        let ControllerConnectionResult::Connected {
            endpoint, choice, ..
        } = result
        else {
            panic!("explicit Lihuiyu USB selection must connect directly");
        };
        assert!(matches!(
            endpoint,
            ControllerConnectionEndpoint::Usb {
                vendor_id: beambench_lihuiyu::USB_VENDOR_ID,
                product_id: beambench_lihuiyu::USB_PRODUCT_ID,
                ..
            }
        ));
        assert_eq!(choice.driver, ControllerDriverId::Lihuiyu);
        assert!(!choice.requires_experimental_compatibility_handshake);

        let state = runtime_state(&ctx).unwrap();
        assert_eq!(state.controller_model, Some(ControllerModel::LihuiyuM2Nano));
        assert_eq!(state.transport_kind, Some(TransportKind::UsbPacket));
        assert_eq!(
            state.product_tier,
            Some(ControllerProductTier::Experimental)
        );
        assert!(state.capabilities.unwrap().can_run_job);

        home(&ctx).unwrap();
        unlock(&ctx).unwrap();
        jog(
            &ctx,
            JogMachineInput {
                x_mm: 2.54,
                y_mm: -1.27,
                z_mm: None,
                feed_rate: 1_000.0,
                continuous: false,
            },
        )
        .unwrap();
        assert_eq!(machine_status(&ctx).unwrap().machine_position.x, 2.54);
        disconnect_machine(&ctx).unwrap();
    }

    #[test]
    fn emergency_stop_reaches_a_lihuiyu_job_mid_transfer() {
        let profile = MachineProfile {
            bed_width_mm: 500.0,
            bed_height_mm: 500.0,
            ..MachineProfile::default()
        };
        let settings = AppSettings {
            active_profile_id: Some(profile.id),
            machine_profiles: vec![profile],
            ..AppSettings::default()
        };
        let ctx = ServiceContext::with_settings(settings);
        let device = LihuiyuUsbDeviceInfo {
            bus_id: "virtual".to_string(),
            device_address: 4,
            port_numbers: vec![1, 2],
            vendor_id: beambench_lihuiyu::USB_VENDOR_ID,
            product_id: beambench_lihuiyu::USB_PRODUCT_ID,
            manufacturer: Some("Lihuiyu Studio Labs".to_string()),
            product: Some("M2 Nano".to_string()),
            serial_number: None,
            has_required_bulk_endpoints: Some(true),
            driver: Some("virtual-usb".to_string()),
        };
        finish_lihuiyu_usb_connection(
            &ctx,
            beambench_lihuiyu::LihuiyuVirtualUsbIo::m2_nano(),
            device,
        )
        .unwrap();

        let mut project = Project::new("Lihuiyu mid-transfer stop");
        let layer_id = project.ensure_default_layer();
        project.add_object(ProjectObject::new(
            "Rect",
            layer_id,
            Bounds::new(Point2D::new(5.0, 5.0), Point2D::new(15.0, 15.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        ));
        *ctx.project.lock().unwrap() = Some(project);

        // Starting the job stages the transfer without uploading anything;
        // one tick advances it by at most one packet.
        let progress = start_job(&ctx).unwrap();
        assert_eq!(progress.state, JobState::Preparing);
        tick_job(&ctx).unwrap().unwrap();
        {
            let mut session_lock = ctx.session.lock().unwrap();
            let Some(MachineSessionHandle::Lihuiyu(session)) = session_lock.as_mut() else {
                panic!("expected Lihuiyu session");
            };
            assert!(
                session.is_transferring(),
                "one tick must not upload the whole job"
            );
        }

        // Emergency stop between ticks: previously the synchronous upload
        // held the session lock for the whole burn, making this unreachable.
        emergency_stop(&ctx).unwrap();
        let mut session_lock = ctx.session.lock().unwrap();
        let Some(MachineSessionHandle::Lihuiyu(session)) = session_lock.as_mut() else {
            panic!("expected Lihuiyu session");
        };
        assert!(!session.is_transferring());
        assert_eq!(session.machine_status().spindle_speed, 0.0);
    }

    #[test]
    fn emergency_stop_does_not_deadlock_against_a_concurrent_tick_loop() {
        let profile = MachineProfile {
            bed_width_mm: 500.0,
            bed_height_mm: 500.0,
            ..MachineProfile::default()
        };
        let settings = AppSettings {
            active_profile_id: Some(profile.id),
            machine_profiles: vec![profile],
            ..AppSettings::default()
        };
        let ctx = Arc::new(ServiceContext::with_settings(settings));
        let device = LihuiyuUsbDeviceInfo {
            bus_id: "virtual".to_string(),
            device_address: 4,
            port_numbers: vec![1, 2],
            vendor_id: beambench_lihuiyu::USB_VENDOR_ID,
            product_id: beambench_lihuiyu::USB_PRODUCT_ID,
            manufacturer: Some("Lihuiyu Studio Labs".to_string()),
            product: Some("M2 Nano".to_string()),
            serial_number: None,
            has_required_bulk_endpoints: Some(true),
            driver: Some("virtual-usb".to_string()),
        };
        finish_lihuiyu_usb_connection(
            &ctx,
            beambench_lihuiyu::LihuiyuVirtualUsbIo::m2_nano(),
            device,
        )
        .unwrap();

        let mut project = Project::new("Lihuiyu tick vs stop");
        let layer_id = project.ensure_default_layer();
        project.add_object(ProjectObject::new(
            "Rect",
            layer_id,
            Bounds::new(Point2D::new(5.0, 5.0), Point2D::new(15.0, 15.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        ));
        *ctx.project.lock().unwrap() = Some(project);
        start_job(&ctx).unwrap();

        // tick_job locks job -> session; emergency_stop used to hold session
        // while locking job. Hammer both concurrently: an inversion deadlocks
        // within these loops and trips the watchdog below.
        let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
        let ticker = {
            let ctx = Arc::clone(&ctx);
            std::thread::spawn(move || {
                for _ in 0..5_000 {
                    let _ = tick_job(&ctx);
                }
            })
        };
        let stopper = {
            let ctx = Arc::clone(&ctx);
            std::thread::spawn(move || {
                for _ in 0..500 {
                    let _ = emergency_stop(&ctx);
                }
            })
        };
        std::thread::spawn(move || {
            ticker.join().unwrap();
            stopper.join().unwrap();
            let _ = done_tx.send(());
        });
        done_rx
            .recv_timeout(Duration::from_secs(20))
            .expect("deadlock: emergency_stop and tick_job did not finish");
        assert!(ctx.job.lock().unwrap().is_none());
    }

    #[test]
    fn disconnect_always_releases_the_session_even_when_stop_is_unavailable() {
        let profile = MachineProfile {
            bed_width_mm: 500.0,
            bed_height_mm: 500.0,
            ..MachineProfile::default()
        };
        let settings = AppSettings {
            active_profile_id: Some(profile.id),
            machine_profiles: vec![profile],
            ..AppSettings::default()
        };
        let ctx = ServiceContext::with_settings(settings);
        let device = LihuiyuUsbDeviceInfo {
            bus_id: "virtual".to_string(),
            device_address: 4,
            port_numbers: vec![1, 2],
            vendor_id: beambench_lihuiyu::USB_VENDOR_ID,
            product_id: beambench_lihuiyu::USB_PRODUCT_ID,
            manufacturer: Some("Lihuiyu Studio Labs".to_string()),
            product: Some("M2 Nano".to_string()),
            serial_number: None,
            has_required_bulk_endpoints: Some(true),
            driver: Some("virtual-usb".to_string()),
        };
        let mut io = beambench_lihuiyu::LihuiyuVirtualUsbIo::m2_nano();
        io.fail_next_packet_writes(1);
        finish_lihuiyu_usb_connection(&ctx, io, device).unwrap();

        let mut project = Project::new("Lihuiyu stuck disconnect");
        let layer_id = project.ensure_default_layer();
        project.add_object(ProjectObject::new(
            "Rect",
            layer_id,
            Bounds::new(Point2D::new(5.0, 5.0), Point2D::new(15.0, 15.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        ));
        *ctx.project.lock().unwrap() = Some(project);

        // The ambiguous packet write fails the tick-driven transfer and
        // leaves the session in RecoveryRequired, where a software stop is
        // refused. Disconnect must still release the session; otherwise the
        // user is trapped until app restart.
        start_job(&ctx).unwrap();
        let mut guard = 0;
        loop {
            let progress = tick_job(&ctx).unwrap().unwrap();
            if progress.state == JobState::Failed {
                break;
            }
            guard += 1;
            assert!(guard < 10_000, "job did not fail: {progress:?}");
        }
        disconnect_machine(&ctx).unwrap();
        assert!(ctx.session.lock().unwrap().is_none());
    }

    #[test]
    fn silent_default_grbl_connection_retries_lx2b_baud_only() {
        assert_eq!(grbl_fallback_baud_rates(115_200, false), vec![921_600]);
    }

    #[test]
    fn configured_lx2b_baud_is_not_retried_after_silence() {
        assert!(grbl_fallback_baud_rates(921_600, false).is_empty());
    }

    #[test]
    fn explicitly_configured_legacy_baud_is_not_retried_after_silence() {
        assert!(grbl_fallback_baud_rates(57_600, false).is_empty());
    }

    #[test]
    fn unreadable_default_connection_also_retries_legacy_baud_rates() {
        assert_eq!(
            grbl_fallback_baud_rates(115_200, true),
            vec![921_600, 9_600, 57_600]
        );
    }

    #[derive(Debug, Clone, Copy)]
    enum NetworkGrblFixture {
        FluidNc,
        GrblHal,
        LaserPecker,
    }

    struct NetworkRuidaFixture {
        port: u16,
        controller: Arc<Mutex<beambench_ruida::RuidaVirtualController>>,
        stop: Arc<AtomicBool>,
        server: Option<std::thread::JoinHandle<()>>,
    }

    impl Drop for NetworkRuidaFixture {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Release);
            let _ = UdpSocket::bind(("127.0.0.1", 0))
                .and_then(|socket| socket.send_to(&[0], ("127.0.0.1", self.port)));
            if let Some(server) = self.server.take() {
                server.join().unwrap();
            }
        }
    }

    fn spawn_network_ruida_fixture() -> NetworkRuidaFixture {
        let socket = UdpSocket::bind(("127.0.0.1", 0)).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_millis(100)))
            .unwrap();
        let port = socket.local_addr().unwrap().port();
        let controller = Arc::new(Mutex::new(
            beambench_ruida::RuidaVirtualController::rdc6442s(),
        ));
        let controller_for_server = Arc::clone(&controller);
        let stop = Arc::new(AtomicBool::new(false));
        let stop_for_server = Arc::clone(&stop);
        let server = std::thread::spawn(move || {
            let mut buffer = vec![0_u8; beambench_ruida::MAX_UDP_DATAGRAM_SIZE];
            while !stop_for_server.load(Ordering::Acquire) {
                match socket.recv_from(&mut buffer) {
                    Ok((length, peer)) => {
                        if stop_for_server.load(Ordering::Acquire) {
                            break;
                        }
                        let response = controller_for_server
                            .lock()
                            .unwrap()
                            .receive_datagram(&buffer[..length]);
                        for datagram in response.datagrams {
                            socket.send_to(&datagram, peer).unwrap();
                        }
                    }
                    Err(error)
                        if matches!(
                            error.kind(),
                            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                        ) => {}
                    Err(error) => panic!("Ruida UDP fixture read failed: {error}"),
                }
            }
        });
        NetworkRuidaFixture {
            port,
            controller,
            stop,
            server: Some(server),
        }
    }

    fn spawn_network_grbl_fixture(
        fixture: NetworkGrblFixture,
    ) -> (u16, std::thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut commands = Vec::new();
            let mut line = Vec::new();
            let mut byte = [0_u8; 1];
            loop {
                match stream.read(&mut byte) {
                    Ok(0) => break,
                    Ok(_) if byte[0] == b'?' && line.is_empty() => {
                        commands.push("?".to_string());
                        stream
                            .write_all(b"<Idle|MPos:0.000,0.000,0.000|FS:0,0>\n")
                            .unwrap();
                    }
                    Ok(_) if byte[0] == b'\n' => {
                        let command = String::from_utf8(std::mem::take(&mut line)).unwrap();
                        commands.push(command.clone());
                        match (fixture, command.as_str()) {
                            (NetworkGrblFixture::FluidNc, "$I") => stream
                                .write_all(
                                    b"[VER:4.0 FluidNC v4.0.3 (esp32-wifi) :]\n[OPT:PHSW,15,128]\nok\n",
                                )
                                .unwrap(),
                            (NetworkGrblFixture::GrblHal, "$I") => stream
                                .write_all(
                                    b"[VER:1.1f.20260712:Beam Bench grblHAL fixture]\n[OPT:VN,35,1024]\nok\n",
                                )
                                .unwrap(),
                            (NetworkGrblFixture::GrblHal, "$I+") => stream
                                .write_all(
                                    b"[VER:1.1f.20260712:Beam Bench grblHAL fixture]\n[FIRMWARE:grblHAL]\n[COMPATIBILITY LEVEL:1]\nok\n",
                                )
                                .unwrap(),
                            (_, "$$") => stream
                                .write_all(b"$10=3\n$30=1000\n$32=1\nok\n")
                                .unwrap(),
                            _ => stream.write_all(b"error:3\n").unwrap(),
                        }
                    }
                    Ok(_) if byte[0] == b'\r' => {}
                    Ok(_) => line.push(byte[0]),
                    Err(error)
                        if matches!(
                            error.kind(),
                            ErrorKind::ConnectionReset | ErrorKind::ConnectionAborted
                        ) =>
                    {
                        break;
                    }
                    Err(error) => panic!("network fixture read failed: {error}"),
                }
            }
            commands
        });
        (port, server)
    }

    fn assert_network_grbl_connection(
        fixture: NetworkGrblFixture,
        expected_driver: ControllerDriverId,
        expected_model: ControllerModel,
    ) {
        let (port, server) = spawn_network_grbl_fixture(fixture);
        let ctx = ServiceContext::new();
        let result = begin_network_controller_connection(
            &ctx,
            BeginNetworkControllerConnectionInput {
                host: "127.0.0.1".to_string(),
                port,
                selection: ControllerSelection::AutoDetect,
            },
        )
        .unwrap();

        assert!(matches!(
            result,
            ControllerConnectionResult::Connected {
                session_state: SessionState::Ready,
                endpoint: ControllerConnectionEndpoint::Tcp {
                    ref host,
                    port: connected_port,
                },
                ref choice,
            } if host == "127.0.0.1"
                && connected_port == port
                && choice.driver == expected_driver
        ));
        let state = runtime_state(&ctx).unwrap();
        assert_eq!(state.controller_driver, Some(expected_driver));
        assert_eq!(state.controller_model, Some(expected_model));
        assert_eq!(state.transport_kind, Some(TransportKind::Tcp));
        assert!(state.experimental_mode);

        disconnect_machine(&ctx).unwrap();
        let commands = server.join().unwrap();
        assert_eq!(commands.first().map(String::as_str), Some("?"));
        assert!(commands.iter().any(|command| command == "$I"));
        assert!(commands.iter().any(|command| command == "$$"));
    }

    #[test]
    fn fluidnc_tcp_connection_uses_network_adapter_and_runtime() {
        assert_network_grbl_connection(
            NetworkGrblFixture::FluidNc,
            ControllerDriverId::FluidNc,
            ControllerModel::FluidNc,
        );
    }

    #[test]
    fn grblhal_tcp_connection_uses_extended_identity_and_network_runtime() {
        assert_network_grbl_connection(
            NetworkGrblFixture::GrblHal,
            ControllerDriverId::GrblHal,
            ControllerModel::GrblHal,
        );
    }

    #[test]
    fn laserpecker_tcp_connection_uses_status_only_and_skips_settings_queries() {
        let (port, server) = spawn_network_grbl_fixture(NetworkGrblFixture::LaserPecker);
        let ctx = ServiceContext::new();
        let result = begin_network_controller_connection(
            &ctx,
            BeginNetworkControllerConnectionInput {
                host: "127.0.0.1".to_string(),
                port,
                selection: ControllerSelection::KnownDriver {
                    driver: ControllerDriverId::LaserPecker,
                },
            },
        )
        .unwrap();

        assert!(matches!(
            result,
            ControllerConnectionResult::Connected {
                session_state: SessionState::Ready,
                ref choice,
                ..
            } if choice.driver == ControllerDriverId::LaserPecker
        ));
        let state = runtime_state(&ctx).unwrap();
        assert_eq!(
            state.controller_driver,
            Some(ControllerDriverId::LaserPecker)
        );
        assert_eq!(state.controller_model, Some(ControllerModel::LaserPecker));
        assert_eq!(state.transport_kind, Some(TransportKind::Tcp));
        assert!(state.experimental_mode);
        assert!(state.capabilities.unwrap().can_run_job);

        disconnect_machine(&ctx).unwrap();
        let commands = server.join().unwrap();
        assert!(commands.iter().all(|command| command == "?"));
        assert!(commands.len() >= 2);
    }

    #[test]
    fn ruida_udp_connection_uses_exact_identity_and_real_runtime() {
        let _udp_guard = RUIDA_UDP_TEST_LOCK.lock().unwrap();
        let fixture = spawn_network_ruida_fixture();
        let ctx = ServiceContext::new();

        let result = begin_network_controller_connection(
            &ctx,
            BeginNetworkControllerConnectionInput {
                host: "127.0.0.1".to_string(),
                port: fixture.port,
                selection: ControllerSelection::KnownDriver {
                    driver: ControllerDriverId::Ruida,
                },
            },
        )
        .unwrap();

        assert!(matches!(
            result,
            ControllerConnectionResult::Connected {
                session_state: SessionState::Ready,
                endpoint: ControllerConnectionEndpoint::Udp {
                    ref host,
                    port,
                },
                ref choice,
            } if host == "127.0.0.1"
                && port == fixture.port
                && choice.driver == ControllerDriverId::Ruida
                && choice.detected_identity.as_ref().is_some_and(|identity| {
                    identity.family == ControllerFamily::Dsp
                        && identity.model == ControllerModel::Ruida
                        && identity.firmware_identity.as_deref() == Some("RDC6442S")
                })
        ));
        let state = runtime_state(&ctx).unwrap();
        assert_eq!(state.controller_driver, Some(ControllerDriverId::Ruida));
        assert_eq!(state.controller_model, Some(ControllerModel::Ruida));
        assert_eq!(state.transport_kind, Some(TransportKind::Udp));
        assert_eq!(
            state.product_tier,
            Some(ControllerProductTier::Experimental)
        );
        assert_eq!(
            state.evidence_state,
            Some(ControllerEvidenceState::Emulated)
        );
        assert!(state.experimental_mode);

        disconnect_machine(&ctx).unwrap();
    }

    #[test]
    fn ruida_public_job_path_uploads_runs_and_confirms_completion() {
        let _udp_guard = RUIDA_UDP_TEST_LOCK.lock().unwrap();
        let fixture = spawn_network_ruida_fixture();
        let profile = MachineProfile {
            bed_width_mm: 500.0,
            bed_height_mm: 500.0,
            ..MachineProfile::default()
        };
        let settings = AppSettings {
            active_profile_id: Some(profile.id),
            machine_profiles: vec![profile],
            ..AppSettings::default()
        };
        let ctx = ServiceContext::with_settings(settings);
        begin_network_controller_connection(
            &ctx,
            BeginNetworkControllerConnectionInput {
                host: "127.0.0.1".to_string(),
                port: fixture.port,
                selection: ControllerSelection::KnownDriver {
                    driver: ControllerDriverId::Ruida,
                },
            },
        )
        .unwrap();

        let mut project = Project::new("Ruida public path");
        let layer_id = project.ensure_default_layer();
        project.add_object(ProjectObject::new(
            "Rect",
            layer_id,
            Bounds::new(Point2D::new(5.0, 5.0), Point2D::new(15.0, 15.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        ));
        *ctx.project.lock().unwrap() = Some(project);

        let progress = start_job(&ctx).unwrap();
        assert_eq!(progress.state, JobState::Preparing);
        // The upload is tick-driven; pump until the controller is executing.
        let mut guard = 0;
        while !fixture.controller.lock().unwrap().output_active() {
            let progress = tick_job(&ctx).unwrap().unwrap();
            assert_ne!(
                progress.state,
                JobState::Failed,
                "staged upload failed: {:?}",
                progress.error_message
            );
            guard += 1;
            assert!(guard < 10_000, "controller never started executing");
        }
        assert!(fixture.controller.lock().unwrap().selected_file().is_some());

        assert!(fixture.controller.lock().unwrap().complete_execution());
        let verifying = tick_job(&ctx).unwrap().unwrap();
        assert_eq!(verifying.state, JobState::Running);
        assert!(fixture.controller.lock().unwrap().settle_completion());
        let completed = tick_job(&ctx).unwrap().unwrap();
        assert_eq!(completed.state, JobState::Completed);
        assert!(ctx.job.lock().unwrap().is_none());
        assert!(fixture.controller.lock().unwrap().files().is_empty());

        disconnect_machine(&ctx).unwrap();
    }

    #[test]
    fn ruida_mid_upload_cancel_and_pause_have_clean_semantics() {
        let _udp_guard = RUIDA_UDP_TEST_LOCK.lock().unwrap();
        let fixture = spawn_network_ruida_fixture();
        let profile = MachineProfile {
            bed_width_mm: 500.0,
            bed_height_mm: 500.0,
            ..MachineProfile::default()
        };
        let settings = AppSettings {
            active_profile_id: Some(profile.id),
            machine_profiles: vec![profile],
            ..AppSettings::default()
        };
        let ctx = ServiceContext::with_settings(settings);
        begin_network_controller_connection(
            &ctx,
            BeginNetworkControllerConnectionInput {
                host: "127.0.0.1".to_string(),
                port: fixture.port,
                selection: ControllerSelection::KnownDriver {
                    driver: ControllerDriverId::Ruida,
                },
            },
        )
        .unwrap();

        let mut project = Project::new("Ruida mid-upload cancel");
        let layer_id = project.ensure_default_layer();
        project.add_object(ProjectObject::new(
            "Rect",
            layer_id,
            Bounds::new(Point2D::new(5.0, 5.0), Point2D::new(15.0, 15.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        ));
        *ctx.project.lock().unwrap() = Some(project);

        // Advance past the announcement so the controller is mid-upload.
        start_job(&ctx).unwrap();
        tick_job(&ctx).unwrap().unwrap();
        assert!(fixture.controller.lock().unwrap().has_pending_upload());

        {
            let mut job_lock = ctx.job.lock().unwrap();
            let mut session_lock = ctx.session.lock().unwrap();
            let Some(ActiveJobHandle::Ruida(job)) = job_lock.as_mut() else {
                panic!("expected Ruida job");
            };
            let Some(MachineSessionHandle::Ruida(session)) = session_lock.as_mut() else {
                panic!("expected Ruida session");
            };
            // Pause cannot apply to an upload; the error must say so instead
            // of surfacing an invalid-phase failure.
            let error = job.pause(session).unwrap_err();
            assert!(
                error.contains("uploading"),
                "unexpected pause error: {error}"
            );
            // Cancel mid-upload performs a verified abort with no false
            // cleanup-failure diagnostic.
            let cancelled = job.cancel(session).unwrap();
            assert_eq!(cancelled.state, JobState::Cancelled);
            assert_eq!(cancelled.error_message, None);
        }
        {
            let controller = fixture.controller.lock().unwrap();
            assert!(
                !controller.has_pending_upload(),
                "cancel must close the controller's pending upload"
            );
            assert!(controller.files().is_empty());
        }

        // The connection remains fully usable: a fresh job starts executing.
        *ctx.job.lock().unwrap() = None;
        start_job(&ctx).unwrap();
        let mut guard = 0;
        while !fixture.controller.lock().unwrap().output_active() {
            let progress = tick_job(&ctx).unwrap().unwrap();
            assert_ne!(
                progress.state,
                JobState::Failed,
                "post-cancel upload failed: {:?}",
                progress.error_message
            );
            guard += 1;
            assert!(guard < 10_000, "post-cancel job never started");
        }
        cancel_job(&ctx).unwrap();
        disconnect_machine(&ctx).unwrap();
    }

    #[test]
    fn ruida_public_motion_frame_pause_resume_and_cancel_are_native() {
        let _udp_guard = RUIDA_UDP_TEST_LOCK.lock().unwrap();
        let fixture = spawn_network_ruida_fixture();
        let profile = MachineProfile {
            bed_width_mm: 500.0,
            bed_height_mm: 500.0,
            laser_on_when_framing: true,
            ..MachineProfile::default()
        };
        let mut settings = AppSettings::default();
        settings.active_profile_id = Some(profile.id);
        settings.machine_profiles.push(profile.clone());
        let ctx = ServiceContext::with_settings(settings);
        begin_network_controller_connection(
            &ctx,
            BeginNetworkControllerConnectionInput {
                host: "127.0.0.1".to_string(),
                port: fixture.port,
                selection: ControllerSelection::KnownDriver {
                    driver: ControllerDriverId::Ruida,
                },
            },
        )
        .unwrap();

        home(&ctx).unwrap();
        assert!(!ctx.machine_coordinates_valid.load(Ordering::Acquire));
        assert!(fixture.controller.lock().unwrap().settle_manual_motion());
        assert_eq!(session_state(&ctx).unwrap(), SessionState::Ready);
        jog(
            &ctx,
            JogMachineInput {
                x_mm: 2.5,
                y_mm: -1.25,
                z_mm: None,
                feed_rate: 600.0,
                continuous: false,
            },
        )
        .unwrap();
        assert_eq!(
            fixture.controller.lock().unwrap().position_micrometres(),
            (2_500, -1_250)
        );
        assert!(fixture.controller.lock().unwrap().settle_manual_motion());
        assert_eq!(session_state(&ctx).unwrap(), SessionState::Ready);

        let mut project = Project::new("Ruida zero-output frame");
        let layer_id = project.ensure_default_layer();
        let bounds = Bounds::new(Point2D::new(5.0, 5.0), Point2D::new(15.0, 15.0));
        project.add_object(ProjectObject::new(
            "Rect",
            layer_id,
            bounds,
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        ));
        *ctx.project.lock().unwrap() = Some(project.clone());

        let progress = frame_job(&ctx, "rectangular", &[], true, Some(3_000.0)).unwrap();
        assert_eq!(progress.state, JobState::Preparing);
        // The frame upload is tick-driven; pump until the file is selected
        // and executing on the controller.
        let mut guard = 0;
        while fixture.controller.lock().unwrap().selected_file().is_none()
            || fixture.controller.lock().unwrap().execution_state()
                != beambench_ruida::RuidaVirtualExecutionState::Running
        {
            let progress = tick_job(&ctx).unwrap().unwrap();
            assert_ne!(
                progress.state,
                JobState::Failed,
                "staged frame upload failed: {:?}",
                progress.error_message
            );
            guard += 1;
            assert!(guard < 10_000, "frame never started executing");
        }
        let mut expected_plan = build_frame_plan(&bounds, 0.0, 3_000.0);
        apply_workspace_origin_transform(
            &mut expected_plan.segments,
            project.workspace.origin,
            project.workspace.bed_height_mm,
        );
        expected_plan.bounds = calculate_plan_bounds(&expected_plan.segments);
        let expected = compile_ruida_execution_plan(&expected_plan, &project, &profile).unwrap();
        assert_eq!(
            fixture
                .controller
                .lock()
                .unwrap()
                .selected_file()
                .unwrap()
                .clear_bytes,
            expected.clear_bytes
        );

        pause_job(&ctx).unwrap();
        assert_eq!(
            fixture.controller.lock().unwrap().execution_state(),
            beambench_ruida::RuidaVirtualExecutionState::Paused
        );
        resume_job(&ctx).unwrap();
        assert_eq!(
            fixture.controller.lock().unwrap().execution_state(),
            beambench_ruida::RuidaVirtualExecutionState::Running
        );
        cancel_job(&ctx).unwrap();
        assert!(ctx.job.lock().unwrap().is_none());
        assert!(fixture.controller.lock().unwrap().files().is_empty());

        disconnect_machine(&ctx).unwrap();
    }

    #[test]
    fn network_controller_connection_rejects_invalid_endpoints_before_opening_transport() {
        for input in [
            BeginNetworkControllerConnectionInput {
                host: "   ".to_string(),
                port: 23,
                selection: ControllerSelection::AutoDetect,
            },
            BeginNetworkControllerConnectionInput {
                host: "laser.local\nforged".to_string(),
                port: 23,
                selection: ControllerSelection::AutoDetect,
            },
            BeginNetworkControllerConnectionInput {
                host: "laser.local".to_string(),
                port: 0,
                selection: ControllerSelection::AutoDetect,
            },
        ] {
            let error = begin_network_controller_connection(&ServiceContext::new(), input)
                .expect_err("invalid network endpoint must fail before transport open");
            assert_eq!(error.code, crate::error::ServiceErrorCode::InvalidInput);
        }
    }

    #[test]
    fn network_controller_connection_rejects_serial_only_choices_before_opening_transport() {
        for selection in [
            ControllerSelection::KnownDriver {
                driver: ControllerDriverId::Grbl,
            },
            ControllerSelection::KnownDriver {
                driver: ControllerDriverId::Marlin,
            },
            ControllerSelection::KnownDriver {
                driver: ControllerDriverId::Snapmaker,
            },
            ControllerSelection::KnownDriver {
                driver: ControllerDriverId::Smoothieware,
            },
            ControllerSelection::GenericGrblCompatible,
        ] {
            let error = begin_network_controller_connection(
                &ServiceContext::new(),
                BeginNetworkControllerConnectionInput {
                    host: "127.0.0.1".to_string(),
                    port: 23,
                    selection,
                },
            )
            .expect_err("serial-only selection must fail before transport open");
            assert_eq!(error.code, crate::error::ServiceErrorCode::InvalidInput);
        }
    }

    #[test]
    fn controller_connection_challenge_rejects_the_wrong_token_without_losing_attempt() {
        let ctx = ServiceContext::new();
        let pending = PendingControllerConnection {
            attempt_id: "attempt-owned-by-backend".to_string(),
            endpoint: ControllerConnectionEndpoint::Serial {
                port_name: "mock-port".to_string(),
                baud_rate: 115_200,
            },
            device_identity: DeviceIdentity {
                display_name: "Mock controller".to_string(),
                port_name: Some("mock-port".to_string()),
                ..DeviceIdentity::default()
            },
            selection: ControllerSelection::AutoDetect,
            session: PendingControllerSession::Grbl {
                probe: beambench_grbl::GrblFamilyIdentityProbeResult {
                    identity: beambench_common::GrblFamilyIdentity::default(),
                    controller_info: beambench_grbl::GrblFamilyIdentityProbeOutcome::Succeeded,
                    extended_controller_info:
                        beambench_grbl::GrblFamilyIdentityProbeOutcome::Rejected(3),
                },
                session: Box::new(GrblSession::new(Box::new(MockSerialTransport::new(
                    "mock-port",
                )))),
            },
            created_at: Instant::now(),
        };
        *ctx.pending_controller_connection.lock().unwrap() = Some(pending);

        let error = continue_controller_connection(
            &ctx,
            ContinueControllerConnectionInput {
                attempt_id: "frontend-invented-token".to_string(),
                selection: ControllerSelection::KnownDriver {
                    driver: ControllerDriverId::Grbl,
                },
                decision: None,
            },
        )
        .unwrap_err();

        assert_eq!(error.code, crate::error::ServiceErrorCode::InvalidInput);
        assert_eq!(
            ctx.pending_controller_connection
                .lock()
                .unwrap()
                .as_ref()
                .map(|pending| pending.attempt_id.as_str()),
            Some("attempt-owned-by-backend")
        );
        assert!(ctx.session.lock().unwrap().is_none());
    }

    #[test]
    fn expired_controller_connection_challenge_is_closed_and_removed() {
        let ctx = ServiceContext::new();
        *ctx.pending_controller_connection.lock().unwrap() = Some(PendingControllerConnection {
            attempt_id: "expired-attempt".to_string(),
            endpoint: ControllerConnectionEndpoint::Serial {
                port_name: "mock-port".to_string(),
                baud_rate: 115_200,
            },
            device_identity: DeviceIdentity::default(),
            selection: ControllerSelection::AutoDetect,
            session: PendingControllerSession::Grbl {
                probe: beambench_grbl::GrblFamilyIdentityProbeResult {
                    identity: beambench_common::GrblFamilyIdentity::default(),
                    controller_info: beambench_grbl::GrblFamilyIdentityProbeOutcome::TimedOut,
                    extended_controller_info:
                        beambench_grbl::GrblFamilyIdentityProbeOutcome::NotAttempted,
                },
                session: Box::new(GrblSession::new(Box::new(MockSerialTransport::new(
                    "mock-port",
                )))),
            },
            created_at: Instant::now() - CONTROLLER_CONNECTION_CHALLENGE_TTL,
        });

        assert!(expire_controller_connection(&ctx, "expired-attempt").unwrap());
        assert!(ctx.pending_controller_connection.lock().unwrap().is_none());
        assert!(!expire_controller_connection(&ctx, "expired-attempt").unwrap());
    }

    #[test]
    fn exact_fluidnc_resolution_registers_an_experimental_named_session() {
        use beambench_common::controller_choice::{
            ControllerChoiceResolution, ControllerChoiceSource, ControllerOverrideUpdate,
        };
        use beambench_common::{
            GrblFamilyDialect, GrblFamilyIdentity, GrblFamilyIdentityEvidence,
            GrblFamilyIdentityStatus,
        };
        use beambench_grbl::{GrblFamilyIdentityProbeOutcome, GrblFamilyIdentityProbeResult};

        let ctx = ServiceContext::new();
        let mut transport = MockSerialTransport::new("mock-fluidnc");
        transport.enqueue_response("Grbl 4.0 [FluidNC v4.0.3 (esp32-wifi) '$' for help]");
        let handle = transport.handle();
        let mut session = GrblSession::new(Box::new(transport));
        session.connect().unwrap();
        session.poll().unwrap();
        assert_eq!(session.session_state(), SessionState::Validating);
        handle.enqueue_response("<Idle|MPos:0.000,0.000,0.000|FS:0,0>");

        let probe = GrblFamilyIdentityProbeResult {
            identity: GrblFamilyIdentity {
                dialect: GrblFamilyDialect::FluidNc,
                status: GrblFamilyIdentityStatus::Identified,
                firmware_identity: Some("FluidNC".to_string()),
                firmware_version: Some("4.0.3".to_string()),
                evidence: vec![GrblFamilyIdentityEvidence::ControllerInfoVersion],
            },
            controller_info: GrblFamilyIdentityProbeOutcome::Succeeded,
            extended_controller_info: GrblFamilyIdentityProbeOutcome::NotNeeded,
        };
        let choice = ResolvedControllerChoice {
            selection: ExplicitControllerSelection::KnownDriver {
                driver: ControllerDriverId::FluidNc,
            },
            driver: ControllerDriverId::FluidNc,
            source: ControllerChoiceSource::AutoDetected,
            detected_identity: probe.identity.positive_identity(),
            requires_experimental_mode: true,
            mismatch: false,
            override_scope: None,
            requires_experimental_compatibility_handshake: false,
        };
        let pending = PendingControllerConnection {
            attempt_id: "fluidnc-attempt".to_string(),
            endpoint: ControllerConnectionEndpoint::Serial {
                port_name: "mock-fluidnc".to_string(),
                baud_rate: 115_200,
            },
            device_identity: DeviceIdentity::default(),
            selection: ControllerSelection::AutoDetect,
            session: PendingControllerSession::Grbl {
                probe,
                session: Box::new(session),
            },
            created_at: Instant::now(),
        };

        let result = finish_controller_connection_resolution(
            &ctx,
            pending,
            ControllerChoiceResolution {
                outcome: ControllerChoiceOutcome::Resolved {
                    choice: choice.clone(),
                },
                override_update: ControllerOverrideUpdate::Keep,
                replacement_override: None,
            },
        )
        .unwrap();

        assert!(matches!(
            result,
            ControllerConnectionResult::Connected {
                session_state: SessionState::Ready,
                choice: connected_choice,
                ..
            } if connected_choice == choice
        ));
        let state = runtime_state(&ctx).unwrap();
        assert_eq!(state.controller_model, Some(ControllerModel::FluidNc));
        assert_eq!(state.controller_driver, Some(ControllerDriverId::FluidNc));
        assert!(state.experimental_mode);
        assert_eq!(
            state.product_tier,
            Some(ControllerProductTier::Experimental)
        );
        assert_eq!(
            state.evidence_state,
            Some(ControllerEvidenceState::Emulated)
        );
        assert!(state.capabilities.unwrap().can_home);
    }

    #[test]
    fn exact_grblhal_resolution_registers_an_experimental_named_session() {
        use beambench_common::controller_choice::{
            ControllerChoiceResolution, ControllerChoiceSource, ControllerOverrideUpdate,
        };
        use beambench_common::{
            GrblFamilyDialect, GrblFamilyIdentity, GrblFamilyIdentityEvidence,
            GrblFamilyIdentityStatus,
        };
        use beambench_grbl::{GrblFamilyIdentityProbeOutcome, GrblFamilyIdentityProbeResult};

        let ctx = ServiceContext::new();
        let mut transport = MockSerialTransport::new("mock-grblhal");
        transport.enqueue_response("Grbl 1.1f ['$' for help]");
        let handle = transport.handle();
        let mut session = GrblSession::new(Box::new(transport));
        session.connect().unwrap();
        session.poll().unwrap();
        assert_eq!(session.session_state(), SessionState::Validating);
        handle.enqueue_response("<Idle|MPos:0.000,0.000,0.000|FS:0,0|FW:grblHAL>");

        let probe = GrblFamilyIdentityProbeResult {
            identity: GrblFamilyIdentity {
                dialect: GrblFamilyDialect::GrblHal,
                status: GrblFamilyIdentityStatus::Identified,
                firmware_identity: Some("grblHAL".to_string()),
                firmware_version: Some("1.1f.20260712".to_string()),
                evidence: vec![GrblFamilyIdentityEvidence::FirmwareIdentityMessage],
            },
            controller_info: GrblFamilyIdentityProbeOutcome::Succeeded,
            extended_controller_info: GrblFamilyIdentityProbeOutcome::Succeeded,
        };
        let choice = ResolvedControllerChoice {
            selection: ExplicitControllerSelection::KnownDriver {
                driver: ControllerDriverId::GrblHal,
            },
            driver: ControllerDriverId::GrblHal,
            source: ControllerChoiceSource::AutoDetected,
            detected_identity: probe.identity.positive_identity(),
            requires_experimental_mode: true,
            mismatch: false,
            override_scope: None,
            requires_experimental_compatibility_handshake: false,
        };
        let pending = PendingControllerConnection {
            attempt_id: "grblhal-attempt".to_string(),
            endpoint: ControllerConnectionEndpoint::Serial {
                port_name: "mock-grblhal".to_string(),
                baud_rate: 115_200,
            },
            device_identity: DeviceIdentity::default(),
            selection: ControllerSelection::AutoDetect,
            session: PendingControllerSession::Grbl {
                probe,
                session: Box::new(session),
            },
            created_at: Instant::now(),
        };

        let result = finish_controller_connection_resolution(
            &ctx,
            pending,
            ControllerChoiceResolution {
                outcome: ControllerChoiceOutcome::Resolved {
                    choice: choice.clone(),
                },
                override_update: ControllerOverrideUpdate::Keep,
                replacement_override: None,
            },
        )
        .unwrap();

        assert!(matches!(
            result,
            ControllerConnectionResult::Connected {
                session_state: SessionState::Ready,
                choice: connected_choice,
                ..
            } if connected_choice == choice
        ));
        let state = runtime_state(&ctx).unwrap();
        assert_eq!(state.controller_model, Some(ControllerModel::GrblHal));
        assert_eq!(state.controller_driver, Some(ControllerDriverId::GrblHal));
        assert!(state.experimental_mode);
        assert_eq!(
            state.product_tier,
            Some(ControllerProductTier::Experimental)
        );
        assert_eq!(
            state.evidence_state,
            Some(ControllerEvidenceState::Emulated)
        );
        assert!(state.capabilities.unwrap().can_home);
    }

    #[test]
    fn marlin_preflight_reports_a_real_outcome_instead_of_hardcoded_pass() {
        use beambench_common::controller_choice::{
            ControllerChoiceResolution, ControllerChoiceSource, ControllerOverrideUpdate,
        };

        let profile = MachineProfile {
            bed_width_mm: 100.0,
            bed_height_mm: 100.0,
            ..MachineProfile::default()
        };
        let settings = AppSettings {
            active_profile_id: Some(profile.id),
            machine_profiles: vec![profile],
            ..AppSettings::default()
        };
        let ctx = ServiceContext::with_settings(settings);

        let mut transport = MockSerialTransport::new("mock-marlin");
        transport.enqueue_response(
            "FIRMWARE_NAME:Marlin 2.1.3 SOURCE_CODE_URL:github.com/MarlinFirmware/Marlin PROTOCOL_VERSION:1.0 MACHINE_TYPE:Laser Cutter",
        );
        transport.enqueue_response("Cap:EMERGENCY_PARSER:1");
        transport.enqueue_response("ok");
        let mut session = MarlinSerialSession::new(
            Box::new(transport),
            MarlinSerialSessionConfig {
                poll_interval: Duration::ZERO,
                ..MarlinSerialSessionConfig::default()
            },
        );
        session.connect().unwrap();
        let probe = session.probe_identity().unwrap();
        let choice = ResolvedControllerChoice {
            selection: ExplicitControllerSelection::KnownDriver {
                driver: ControllerDriverId::Marlin,
            },
            driver: ControllerDriverId::Marlin,
            source: ControllerChoiceSource::AutoDetected,
            detected_identity: probe.identity.positive_identity(),
            requires_experimental_mode: true,
            mismatch: false,
            override_scope: None,
            requires_experimental_compatibility_handshake: false,
        };
        let pending = PendingControllerConnection {
            attempt_id: "marlin-preflight".to_string(),
            endpoint: ControllerConnectionEndpoint::Serial {
                port_name: "mock-marlin".to_string(),
                baud_rate: 115_200,
            },
            device_identity: DeviceIdentity::default(),
            selection: ControllerSelection::AutoDetect,
            session: PendingControllerSession::Marlin { probe, session },
            created_at: Instant::now(),
        };
        finish_controller_connection_resolution(
            &ctx,
            pending,
            ControllerChoiceResolution {
                outcome: ControllerChoiceOutcome::Resolved { choice },
                override_update: ControllerOverrideUpdate::Keep,
                replacement_override: None,
            },
        )
        .unwrap();

        // Geometry far outside the 100x100 bed must fail preflight; the
        // catch-all controller arm used to hardcode Pass.
        let mut project = Project::new("Marlin preflight");
        let layer_id = project.ensure_default_layer();
        project.add_object(ProjectObject::new(
            "Rect",
            layer_id,
            Bounds::new(Point2D::new(150.0, 150.0), Point2D::new(250.0, 250.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 100.0,
                height: 100.0,
                corner_radius: 0.0,
            },
        ));
        *ctx.project.lock().unwrap() = Some(project);

        let report = run_preflight_check(&ctx).unwrap();
        assert_eq!(report.outcome, PreflightOutcome::Fail);
        assert!(
            report
                .checks
                .iter()
                .any(|check| check.category == "bounds" && !check.passed),
            "expected a failing bed-bounds row, got: {:?}",
            report.checks
        );
    }

    #[test]
    fn exact_marlin_resolution_registers_and_completes_an_acknowledged_job() {
        use beambench_common::controller_choice::{
            ControllerChoiceResolution, ControllerChoiceSource, ControllerOverrideUpdate,
        };

        let ctx = ServiceContext::new();
        let mut transport = MockSerialTransport::new("mock-marlin");
        transport.enqueue_response(
            "FIRMWARE_NAME:Marlin 2.1.3 SOURCE_CODE_URL:github.com/MarlinFirmware/Marlin PROTOCOL_VERSION:1.0 MACHINE_TYPE:Laser Cutter",
        );
        transport.enqueue_response("Cap:EMERGENCY_PARSER:1");
        transport.enqueue_response("ok");
        let transport_handle = transport.handle();
        let mut session = MarlinSerialSession::new(
            Box::new(transport),
            MarlinSerialSessionConfig {
                poll_interval: Duration::ZERO,
                ..MarlinSerialSessionConfig::default()
            },
        );
        session.connect().unwrap();
        let probe = session.probe_identity().unwrap();
        let choice = ResolvedControllerChoice {
            selection: ExplicitControllerSelection::KnownDriver {
                driver: ControllerDriverId::Marlin,
            },
            driver: ControllerDriverId::Marlin,
            source: ControllerChoiceSource::AutoDetected,
            detected_identity: probe.identity.positive_identity(),
            requires_experimental_mode: true,
            mismatch: false,
            override_scope: None,
            requires_experimental_compatibility_handshake: false,
        };
        let pending = PendingControllerConnection {
            attempt_id: "marlin-attempt".to_string(),
            endpoint: ControllerConnectionEndpoint::Serial {
                port_name: "mock-marlin".to_string(),
                baud_rate: 115_200,
            },
            device_identity: DeviceIdentity::default(),
            selection: ControllerSelection::AutoDetect,
            session: PendingControllerSession::Marlin { probe, session },
            created_at: Instant::now(),
        };

        let result = finish_controller_connection_resolution(
            &ctx,
            pending,
            ControllerChoiceResolution {
                outcome: ControllerChoiceOutcome::Resolved {
                    choice: choice.clone(),
                },
                override_update: ControllerOverrideUpdate::Keep,
                replacement_override: None,
            },
        )
        .unwrap();

        assert!(matches!(
            result,
            ControllerConnectionResult::Connected {
                session_state: SessionState::Ready,
                choice: connected_choice,
                ..
            } if connected_choice == choice
        ));
        let state = runtime_state(&ctx).unwrap();
        assert_eq!(state.controller_model, Some(ControllerModel::Marlin));
        assert_eq!(state.controller_driver, Some(ControllerDriverId::Marlin));
        assert!(state.experimental_mode);
        let capabilities = state.capabilities.unwrap();
        assert!(capabilities.can_run_job);
        assert!(capabilities.can_frame);
        assert!(!capabilities.can_jog);

        let mut session_guard = ctx.session.lock().unwrap();
        let MachineSessionHandle::Marlin(session) = session_guard.as_mut().unwrap() else {
            panic!("expected Marlin runtime session");
        };
        let mut job =
            MarlinRuntimeJob::start(vec!["M5".to_string(), "M400".to_string()], session).unwrap();
        transport_handle.enqueue_response("ok");
        assert_eq!(job.tick(session).state, JobState::Running);
        transport_handle.enqueue_response("ok");
        assert_eq!(job.tick(session).state, JobState::Completed);
        assert_eq!(session.session_state(), SessionState::Ready);
    }

    #[test]
    fn exact_snapmaker_resolution_registers_and_completes_an_acknowledged_job() {
        use beambench_common::controller_choice::{
            ControllerChoiceResolution, ControllerChoiceSource, ControllerOverrideUpdate,
        };

        let ctx = ServiceContext::new();
        let mut transport = MockSerialTransport::new("mock-snapmaker");
        transport.enqueue_response(
            "FIRMWARE_NAME:Marlin SM2-4.7.2 SOURCE_CODE_URL:https://github.com/whimsycwd/SnapmakerMarlin PROTOCOL_VERSION:1.0 MACHINE_TYPE:GD32F305VGT6",
        );
        transport.enqueue_response("Cap:EMERGENCY_PARSER:0");
        transport.enqueue_response("ok");
        let transport_handle = transport.handle();
        let mut session = MarlinSerialSession::new(
            Box::new(transport),
            MarlinSerialSessionConfig {
                poll_interval: Duration::ZERO,
                ..MarlinSerialSessionConfig::default()
            },
        );
        session.connect().unwrap();
        let probe = session.probe_identity().unwrap();
        let choice = ResolvedControllerChoice {
            selection: ExplicitControllerSelection::KnownDriver {
                driver: ControllerDriverId::Snapmaker,
            },
            driver: ControllerDriverId::Snapmaker,
            source: ControllerChoiceSource::AutoDetected,
            detected_identity: probe.identity.positive_identity(),
            requires_experimental_mode: true,
            mismatch: false,
            override_scope: None,
            requires_experimental_compatibility_handshake: false,
        };
        let pending = PendingControllerConnection {
            attempt_id: "snapmaker-attempt".to_string(),
            endpoint: ControllerConnectionEndpoint::Serial {
                port_name: "mock-snapmaker".to_string(),
                baud_rate: 115_200,
            },
            device_identity: DeviceIdentity::default(),
            selection: ControllerSelection::AutoDetect,
            session: PendingControllerSession::Marlin { probe, session },
            created_at: Instant::now(),
        };

        let result = finish_controller_connection_resolution(
            &ctx,
            pending,
            ControllerChoiceResolution {
                outcome: ControllerChoiceOutcome::Resolved {
                    choice: choice.clone(),
                },
                override_update: ControllerOverrideUpdate::Keep,
                replacement_override: None,
            },
        )
        .unwrap();

        assert!(matches!(
            result,
            ControllerConnectionResult::Connected {
                session_state: SessionState::Ready,
                choice: connected_choice,
                ..
            } if connected_choice == choice
        ));
        let state = runtime_state(&ctx).unwrap();
        assert_eq!(state.controller_model, Some(ControllerModel::Snapmaker));
        assert_eq!(state.controller_driver, Some(ControllerDriverId::Snapmaker));
        assert!(state.experimental_mode);
        assert_eq!(
            state.product_tier,
            Some(ControllerProductTier::Experimental)
        );
        let capabilities = state.capabilities.unwrap();
        assert!(capabilities.can_run_job);
        assert!(capabilities.can_frame);
        assert!(!capabilities.can_pause_resume);

        let mut session_guard = ctx.session.lock().unwrap();
        let MachineSessionHandle::Marlin(session) = session_guard.as_mut().unwrap() else {
            panic!("expected Snapmaker runtime session");
        };
        assert_eq!(session.driver(), ControllerDriverId::Snapmaker);
        let mut job =
            MarlinRuntimeJob::start(vec!["M5".to_string(), "M400".to_string()], session).unwrap();
        transport_handle.enqueue_response("ok");
        assert_eq!(job.tick(session).state, JobState::Running);
        transport_handle.enqueue_response("ok");
        assert_eq!(job.tick(session).state, JobState::Completed);
        assert_eq!(session.session_state(), SessionState::Ready);
    }

    #[test]
    fn exact_smoothieware_resolution_registers_and_completes_an_acknowledged_job() {
        use beambench_common::controller_choice::{
            ControllerChoiceResolution, ControllerChoiceSource, ControllerOverrideUpdate,
        };

        let ctx = ServiceContext::new();
        let mut transport = MockSerialTransport::new("mock-smoothieware");
        for line in [
            "FIRMWARE_NAME:Smoothieware, FIRMWARE_VERSION:edge-6ce309b, PROTOCOL_VERSION:1.0, X-GRBL_MODE:0, X-ARCS:1",
            "ok",
            "cached: laser_module_enable is set to true",
            "ok",
            "cached: laser_module_maximum_s_value is set to 1.0",
            "ok",
            "cached: laser_module_proportional_power is set to true",
            "ok",
        ] {
            transport.enqueue_response(line);
        }
        let transport_handle = transport.handle();
        let mut session = SmoothiewareSerialSession::new(
            Box::new(transport),
            SmoothiewareSerialSessionConfig {
                poll_interval: Duration::ZERO,
                ..SmoothiewareSerialSessionConfig::default()
            },
        );
        session.connect().unwrap();
        let probe = session.probe_identity().unwrap();
        let choice = ResolvedControllerChoice {
            selection: ExplicitControllerSelection::KnownDriver {
                driver: ControllerDriverId::Smoothieware,
            },
            driver: ControllerDriverId::Smoothieware,
            source: ControllerChoiceSource::AutoDetected,
            detected_identity: probe.identity.positive_identity(),
            requires_experimental_mode: true,
            mismatch: false,
            override_scope: None,
            requires_experimental_compatibility_handshake: false,
        };
        let pending = PendingControllerConnection {
            attempt_id: "smoothieware-attempt".to_string(),
            endpoint: ControllerConnectionEndpoint::Serial {
                port_name: "mock-smoothieware".to_string(),
                baud_rate: 115_200,
            },
            device_identity: DeviceIdentity::default(),
            selection: ControllerSelection::AutoDetect,
            session: PendingControllerSession::Smoothieware { probe, session },
            created_at: Instant::now(),
        };

        let result = finish_controller_connection_resolution(
            &ctx,
            pending,
            ControllerChoiceResolution {
                outcome: ControllerChoiceOutcome::Resolved {
                    choice: choice.clone(),
                },
                override_update: ControllerOverrideUpdate::Keep,
                replacement_override: None,
            },
        )
        .unwrap();

        assert!(matches!(
            result,
            ControllerConnectionResult::Connected {
                session_state: SessionState::Ready,
                choice: connected_choice,
                ..
            } if connected_choice == choice
        ));
        let state = runtime_state(&ctx).unwrap();
        assert_eq!(state.controller_model, Some(ControllerModel::Smoothieware));
        assert_eq!(
            state.controller_driver,
            Some(ControllerDriverId::Smoothieware)
        );
        assert!(state.experimental_mode);
        assert_eq!(
            state.product_tier,
            Some(ControllerProductTier::Experimental)
        );
        assert_eq!(
            state
                .controller_info
                .as_ref()
                .and_then(|info| info.get("laser_maximum_s_value"))
                .map(String::as_str),
            Some("1")
        );
        let capabilities = state.capabilities.unwrap();
        assert!(capabilities.can_run_job);
        assert!(capabilities.can_frame);
        assert!(!capabilities.can_jog);

        let mut session_guard = ctx.session.lock().unwrap();
        let MachineSessionHandle::Smoothieware(session) = session_guard.as_mut().unwrap() else {
            panic!("expected Smoothieware runtime session");
        };
        let mut job = SmoothiewareRuntimeJob::start(
            vec!["G1 X1 S0.5".to_string(), "M400".to_string()],
            session,
        )
        .unwrap();
        transport_handle.enqueue_response("ok");
        assert_eq!(job.tick(session).state, JobState::Running);
        transport_handle.enqueue_response("ok");
        assert_eq!(job.tick(session).state, JobState::Completed);
        assert_eq!(session.session_state(), SessionState::Ready);
    }

    #[derive(Default)]
    struct DelayedAckState {
        open: bool,
        read_calls_after_initial_responses: usize,
        delayed_ok_sent: bool,
        initial_responses: VecDeque<String>,
        tx_log: Vec<String>,
    }

    struct DelayedAckTransport {
        state: Arc<Mutex<DelayedAckState>>,
        delayed_ok_after_empty_reads: usize,
    }

    impl DelayedAckTransport {
        fn new(state: Arc<Mutex<DelayedAckState>>, delayed_ok_after_empty_reads: usize) -> Self {
            Self {
                state,
                delayed_ok_after_empty_reads,
            }
        }
    }

    impl SerialTransport for DelayedAckTransport {
        fn open(&mut self) -> Result<(), SerialError> {
            self.state.lock().unwrap().open = true;
            Ok(())
        }

        fn close(&mut self) -> Result<(), SerialError> {
            self.state.lock().unwrap().open = false;
            Ok(())
        }

        fn is_open(&self) -> bool {
            self.state.lock().unwrap().open
        }

        fn write_bytes(&mut self, data: &[u8]) -> Result<usize, SerialError> {
            Ok(data.len())
        }

        fn write_line(&mut self, line: &str) -> Result<(), SerialError> {
            self.state.lock().unwrap().tx_log.push(line.to_string());
            Ok(())
        }

        fn read_available(&mut self) -> Result<Vec<u8>, SerialError> {
            Ok(Vec::new())
        }

        fn read_line(&mut self) -> Result<Option<String>, SerialError> {
            let mut state = self.state.lock().unwrap();
            if let Some(line) = state.initial_responses.pop_front() {
                return Ok(Some(line));
            }
            state.read_calls_after_initial_responses += 1;
            if state.read_calls_after_initial_responses >= self.delayed_ok_after_empty_reads
                && !state.delayed_ok_sent
            {
                state.delayed_ok_sent = true;
                return Ok(Some("ok".to_string()));
            }
            Ok(None)
        }

        fn flush(&mut self) -> Result<(), SerialError> {
            Ok(())
        }

        fn port_name(&self) -> &str {
            "delayed-ack"
        }
    }

    #[derive(Default)]
    struct RecordingTransportState {
        open: bool,
        rx: VecDeque<String>,
        lines: Vec<String>,
        bytes: Vec<Vec<u8>>,
        fail_m5_writes: usize,
        fail_next_read: bool,
    }

    struct RecordingTransport {
        state: Arc<Mutex<RecordingTransportState>>,
    }

    impl RecordingTransport {
        fn new(state: Arc<Mutex<RecordingTransportState>>) -> Self {
            Self { state }
        }
    }

    impl SerialTransport for RecordingTransport {
        fn open(&mut self) -> Result<(), SerialError> {
            self.state.lock().unwrap().open = true;
            Ok(())
        }

        fn close(&mut self) -> Result<(), SerialError> {
            self.state.lock().unwrap().open = false;
            Ok(())
        }

        fn is_open(&self) -> bool {
            self.state.lock().unwrap().open
        }

        fn write_bytes(&mut self, data: &[u8]) -> Result<usize, SerialError> {
            self.state.lock().unwrap().bytes.push(data.to_vec());
            Ok(data.len())
        }

        fn write_line(&mut self, line: &str) -> Result<(), SerialError> {
            let mut state = self.state.lock().unwrap();
            state.lines.push(line.to_string());
            if line == "M5" && state.fail_m5_writes > 0 {
                state.fail_m5_writes -= 1;
                return Err(SerialError::WriteFailed("injected M5 failure".to_string()));
            }
            Ok(())
        }

        fn read_available(&mut self) -> Result<Vec<u8>, SerialError> {
            Ok(Vec::new())
        }

        fn read_line(&mut self) -> Result<Option<String>, SerialError> {
            let mut state = self.state.lock().unwrap();
            if state.fail_next_read {
                state.fail_next_read = false;
                return Err(SerialError::IoError(std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    "injected read failure",
                )));
            }
            Ok(state.rx.pop_front())
        }

        fn flush(&mut self) -> Result<(), SerialError> {
            Ok(())
        }

        fn port_name(&self) -> &str {
            "recording"
        }
    }

    fn dummy_plan() -> ExecutionPlan {
        ExecutionPlan {
            id: Uuid::new_v4(),
            project_id: Uuid::new_v4(),
            revision_hash: "dummy".to_string(),
            created_at: Utc::now(),
            bounds: Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(1.0, 1.0)),
            total_distance_mm: 0.0,
            estimated_duration_secs: 0.0,
            segments: vec![],
            layer_order: vec![],
            failed_entries: vec![],
            warnings: vec![],
        }
    }

    #[test]
    fn ruida_compilation_rejects_unreported_current_position_placement() {
        let mut project = Project::new("Ruida current-position placement");
        project.start_from = StartFromMode::CurrentPosition;

        let error =
            compile_ruida_execution_plan(&dummy_plan(), &project, &MachineProfile::default())
                .unwrap_err();

        assert!(error.contains("does not report an absolute current position"));
    }

    fn discovery_candidate(
        id: &str,
        family: ControllerFamily,
        model: ControllerModel,
        unsupported_reason: Option<&str>,
    ) -> DiscoveryCandidate {
        let transport_kind = match family {
            ControllerFamily::Unknown => TransportKind::Serial,
            ControllerFamily::Gcode => TransportKind::Serial,
            ControllerFamily::Dsp => TransportKind::Tcp,
            ControllerFamily::Galvo => TransportKind::UsbPacket,
        };
        DiscoveryCandidate {
            id: id.to_string(),
            controller_family: family,
            controller_model: model,
            transport_kind,
            identity: DeviceIdentity {
                display_name: format!("{model:?} test candidate"),
                ..DeviceIdentity::default()
            },
            confidence: 1.0,
            capabilities: if model == ControllerModel::Grbl {
                DeviceCapabilities::legacy_grbl()
            } else {
                DeviceCapabilities::default()
            },
            product_tier: None,
            evidence_state: None,
            status_text: "Test candidate".to_string(),
            unsupported_reason: unsupported_reason.map(str::to_string),
        }
    }

    fn assert_candidate_connection_rejected(candidate: DiscoveryCandidate, expected_message: &str) {
        let candidate_id = candidate.id.clone();
        let ctx = ServiceContext::with_settings(AppSettings::default());
        ctx.discovery_state.lock().unwrap().candidates = vec![candidate];
        let mut events = ctx.events.subscribe();

        let err = connect_machine(
            &ctx,
            ConnectMachineInput {
                target: MachineConnectionTarget::DiscoveryCandidate { candidate_id },
            },
        )
        .unwrap_err();

        assert_eq!(err.code, crate::error::ServiceErrorCode::InvalidState);
        assert!(
            err.message.contains(expected_message),
            "unexpected rejection message: {}",
            err.message
        );
        assert!(ctx.session.lock().unwrap().is_none());
        assert!(ctx.job.lock().unwrap().is_none());
        while let Ok(raw) = events.try_recv() {
            let event: serde_json::Value = serde_json::from_str(&raw).unwrap();
            assert_ne!(
                event["type"].as_str(),
                Some("machine.connected"),
                "rejected candidate must not emit machine.connected"
            );
        }
    }

    #[test]
    fn discovery_candidate_connect_rejects_unavailable_controller_families() {
        for (id, family, model) in [
            ("dsp", ControllerFamily::Dsp, ControllerModel::Ruida),
            ("galvo", ControllerFamily::Galvo, ControllerModel::Ezcad2),
        ] {
            assert_candidate_connection_rejected(
                discovery_candidate(id, family, model, None),
                "Controller support is not available",
            );
        }
    }

    #[test]
    fn discovery_candidate_connect_rejects_unknown_identity() {
        assert_candidate_connection_rejected(
            discovery_candidate(
                "unknown",
                ControllerFamily::Unknown,
                ControllerModel::Unknown,
                None,
            ),
            "Controller support is not available",
        );
    }

    #[test]
    fn discovery_candidate_connect_rejects_explicit_unsupported_reason() {
        assert_candidate_connection_rejected(
            discovery_candidate(
                "unsupported-grbl",
                ControllerFamily::Gcode,
                ControllerModel::Grbl,
                Some("This firmware has not been qualified"),
            ),
            "This firmware has not been qualified",
        );
    }

    #[test]
    fn discovery_candidate_connect_rejects_non_grbl_gcode_tuples() {
        let wrong_model = discovery_candidate(
            "gcode-wrong-model",
            ControllerFamily::Gcode,
            ControllerModel::Ruida,
            None,
        );
        let mut wrong_transport = discovery_candidate(
            "gcode-wrong-transport",
            ControllerFamily::Gcode,
            ControllerModel::Grbl,
            None,
        );
        wrong_transport.transport_kind = TransportKind::Tcp;

        for candidate in [wrong_model, wrong_transport] {
            assert_candidate_connection_rejected(candidate, "Controller support is not available");
        }
    }

    fn ready_grbl_context_with_status(
        profile: MachineProfile,
        status: &str,
    ) -> (Arc<ServiceContext>, Arc<Mutex<RecordingTransportState>>) {
        let settings = AppSettings {
            active_profile_id: Some(profile.id),
            machine_profiles: vec![profile],
            ..AppSettings::default()
        };
        let ctx = Arc::new(ServiceContext::with_settings(settings));
        let state = Arc::new(Mutex::new(RecordingTransportState {
            rx: VecDeque::from(["Grbl 1.1h".to_string(), status.to_string()]),
            ..RecordingTransportState::default()
        }));
        let transport = RecordingTransport::new(Arc::clone(&state));
        let mut session = GrblSession::new(Box::new(transport));
        session.connect().unwrap();
        session.poll().unwrap();
        session.mark_ready().unwrap();
        *ctx.session.lock().unwrap() = Some(MachineSessionHandle::Grbl(session.into()));
        (ctx, state)
    }

    fn ready_grbl_context(
        profile: MachineProfile,
    ) -> (Arc<ServiceContext>, Arc<Mutex<RecordingTransportState>>) {
        ready_grbl_context_with_status(
            profile,
            "<Idle|MPos:10.000,20.000,0.000|WPos:10.000,20.000,0.000|FS:0,0>",
        )
    }

    fn sent_lines(state: &Arc<Mutex<RecordingTransportState>>) -> Vec<String> {
        state.lock().unwrap().lines.clone()
    }

    fn sent_bytes(state: &Arc<Mutex<RecordingTransportState>>) -> Vec<Vec<u8>> {
        state.lock().unwrap().bytes.clone()
    }

    fn wait_for_sent_line(
        state: &Arc<Mutex<RecordingTransportState>>,
        expected: &str,
        timeout: Duration,
    ) -> bool {
        let start = Instant::now();
        while start.elapsed() < timeout {
            if sent_lines(state).iter().any(|line| line == expected) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        false
    }

    fn wait_for_sent_line_count(
        state: &Arc<Mutex<RecordingTransportState>>,
        expected: &str,
        count: usize,
        timeout: Duration,
    ) -> bool {
        let start = Instant::now();
        while start.elapsed() < timeout {
            if sent_lines(state)
                .iter()
                .filter(|line| line.as_str() == expected)
                .count()
                >= count
            {
                return true;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        false
    }

    fn frame_project() -> Project {
        let mut project = Project::new("Frame");
        let layer_id = project.ensure_default_layer();
        project.add_object(ProjectObject::new(
            "Rect",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        ));
        project
    }

    #[test]
    fn frame_power_defaults_off_and_arms_from_override_or_profile() {
        let mut profile = MachineProfile::default();
        assert_eq!(frame_power_percent(false, &profile), 0.0);
        assert_eq!(frame_power_percent(true, &profile), 1.0);

        profile.laser_on_when_framing = true;
        assert_eq!(frame_power_percent(false, &profile), 1.0);
        assert_eq!(frame_power_percent(true, &profile), 1.0);
    }

    #[test]
    fn continuous_jog_clamps_to_travel_and_cancel_sends_realtime_byte() {
        let profile = MachineProfile {
            bed_width_mm: 100.0,
            bed_height_mm: 50.0,
            ..MachineProfile::default()
        };
        let (ctx, transport) = ready_grbl_context(profile);

        jog(
            &ctx,
            JogMachineInput {
                x_mm: 100_000.0,
                y_mm: -100_000.0,
                z_mm: None,
                feed_rate: 1000.0,
                continuous: true,
            },
        )
        .unwrap();

        assert_eq!(
            sent_lines(&transport).last().map(String::as_str),
            Some("$J=G21G91X90.000Y-20.000F1000")
        );
        assert!(ctx.active_jog.load(Ordering::Acquire));
        let err = jog(
            &ctx,
            JogMachineInput {
                x_mm: 1.0,
                y_mm: 0.0,
                z_mm: None,
                feed_rate: 1000.0,
                continuous: true,
            },
        )
        .unwrap_err();
        assert_eq!(err.code, crate::error::ServiceErrorCode::Busy);

        jog_cancel(&ctx).unwrap();

        assert_eq!(
            sent_bytes(&transport).last().map(Vec::as_slice),
            Some(&[0x85][..])
        );
        assert!(!ctx.active_jog.load(Ordering::Acquire));
    }

    #[test]
    fn move_laser_to_requires_idle_ready_state() {
        let (ctx, transport) = ready_grbl_context_with_status(
            MachineProfile::default(),
            "<Run|MPos:10.000,20.000,0.000|WPos:10.000,20.000,0.000|FS:0,0>",
        );

        let err = move_laser_to(
            &ctx,
            MoveLaserInput {
                x: 1.0,
                y: 2.0,
                z: None,
                feed_rate: 1500.0,
            },
        )
        .unwrap_err();

        assert_eq!(err.code, crate::error::ServiceErrorCode::InvalidState);
        assert!(sent_lines(&transport).is_empty());
    }

    #[test]
    fn move_laser_to_sends_optional_z_only_when_profile_supports_z() {
        let profile = MachineProfile {
            supports_z_moves: true,
            ..MachineProfile::default()
        };
        let (ctx, transport) = ready_grbl_context(profile);

        move_laser_to(
            &ctx,
            MoveLaserInput {
                x: 1.0,
                y: 2.0,
                z: Some(3.0),
                feed_rate: 1500.0,
            },
        )
        .unwrap();

        assert_eq!(
            sent_lines(&transport).last().map(String::as_str),
            Some("G1 X1.000 Y2.000 Z3.000 F1500")
        );
    }

    #[test]
    fn move_laser_to_rejects_z_when_profile_does_not_support_z() {
        let (ctx, transport) = ready_grbl_context(MachineProfile::default());

        let err = move_laser_to(
            &ctx,
            MoveLaserInput {
                x: 1.0,
                y: 2.0,
                z: Some(3.0),
                feed_rate: 1500.0,
            },
        )
        .unwrap_err();

        assert_eq!(err.code, crate::error::ServiceErrorCode::InvalidState);
        assert!(sent_lines(&transport).is_empty());
    }

    #[test]
    fn machine_coordinate_move_requires_current_session_homing() {
        let (ctx, transport) = ready_grbl_context(MachineProfile::default());

        let err = move_laser_to_machine(
            &ctx,
            MoveLaserInput {
                x: 0.0,
                y: 0.0,
                z: None,
                feed_rate: 3000.0,
            },
        )
        .unwrap_err();

        assert_eq!(err.code, crate::error::ServiceErrorCode::InvalidState);
        assert!(sent_lines(&transport).is_empty());
    }

    #[test]
    fn machine_coordinate_move_sends_g53_after_current_session_homing() {
        let (ctx, transport) = ready_grbl_context(MachineProfile::default());
        ctx.machine_coordinates_valid.store(true, Ordering::Release);

        move_laser_to_machine(
            &ctx,
            MoveLaserInput {
                x: 0.0,
                y: 0.0,
                z: None,
                feed_rate: 3000.0,
            },
        )
        .unwrap();

        assert_eq!(
            sent_lines(&transport).last().map(String::as_str),
            Some("G53 G0 X0.000 Y0.000 F3000")
        );
    }

    #[test]
    fn runtime_state_reports_current_session_machine_coordinate_validity() {
        let (ctx, _transport) = ready_grbl_context(MachineProfile::default());

        let state = runtime_state(&ctx).unwrap();
        assert!(!state.machine_coordinates_valid);

        ctx.machine_coordinates_valid.store(true, Ordering::Release);
        let state = runtime_state(&ctx).unwrap();
        assert!(state.machine_coordinates_valid);
    }

    #[test]
    fn frame_job_uses_supplied_feed_rate() {
        let (ctx, transport) = ready_grbl_context(MachineProfile::default());
        *ctx.project.lock().unwrap() = Some(frame_project());

        frame_job(&ctx, "rectangular", &[], false, Some(1234.0)).unwrap();

        assert!(
            sent_lines(&transport)
                .iter()
                .any(|line| line.contains("F1234")),
            "frame job should stream commands with the requested move feed"
        );
    }

    #[test]
    fn frame_job_honors_profile_gcode_config() {
        // Constant-power profile with a non-default S scale: laser-on framing
        // must emit M3 (not M4) and never exceed the profile's s_value_max.
        let profile = MachineProfile {
            use_constant_power: true,
            s_value_max: 255,
            laser_on_when_framing: true,
            ..MachineProfile::default()
        };
        let (ctx, transport) = ready_grbl_context(profile);
        *ctx.project.lock().unwrap() = Some(frame_project());

        frame_job(&ctx, "rectangular", &[], false, None).unwrap();

        let lines = sent_lines(&transport);
        assert!(
            lines.iter().any(|line| line.starts_with("M3")),
            "constant-power profile should frame with M3, got: {lines:?}"
        );
        assert!(
            !lines.iter().any(|line| line.starts_with("M4")),
            "constant-power profile must not emit M4: {lines:?}"
        );
        for line in &lines {
            if let Some(idx) = line.find('S') {
                let s_val: u32 = line[idx + 1..]
                    .split_whitespace()
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0);
                assert!(
                    s_val <= 255,
                    "S value must respect profile s_value_max=255, got line: {line}"
                );
            }
        }
    }

    #[test]
    fn laser_fire_scales_percent_and_stops_on_frontend_release() {
        let profile = MachineProfile {
            enable_laser_fire_button: true,
            default_fire_power_percent: 1.0,
            s_value_max: 1000,
            ..MachineProfile::default()
        };
        let (ctx, transport) = ready_grbl_context(profile);

        let result = laser_fire_start(Arc::clone(&ctx), None).unwrap();
        assert_eq!(result.power_percent, 1.0);
        assert_eq!(result.s_value, 10);
        assert_eq!(
            sent_lines(&transport).last().map(String::as_str),
            Some("M3 S10")
        );

        laser_fire_keepalive(&ctx, &result.token).unwrap();
        laser_fire_stop(&ctx, &result.token).unwrap();

        assert_eq!(
            sent_lines(&transport).last().map(String::as_str),
            Some("M5")
        );
        assert!(ctx.active_laser_fire.lock().unwrap().is_none());
    }

    #[test]
    fn laser_fire_deadman_stops_after_missed_keepalive() {
        let profile = MachineProfile {
            enable_laser_fire_button: true,
            default_fire_power_percent: 1.0,
            s_value_max: 1000,
            ..MachineProfile::default()
        };
        let (ctx, transport) = ready_grbl_context(profile);

        laser_fire_start(Arc::clone(&ctx), None).unwrap();

        assert!(
            wait_for_sent_line(&transport, "M5", Duration::from_secs(2)),
            "deadman should emit M5 after the keepalive window expires"
        );
        assert!(ctx.active_laser_fire.lock().unwrap().is_none());
    }

    #[test]
    fn laser_fire_deadman_retries_after_failed_m5() {
        let profile = MachineProfile {
            enable_laser_fire_button: true,
            default_fire_power_percent: 1.0,
            s_value_max: 1000,
            ..MachineProfile::default()
        };
        let (ctx, transport) = ready_grbl_context(profile);
        transport.lock().unwrap().fail_m5_writes = 1;

        let result = laser_fire_start(Arc::clone(&ctx), None).unwrap();
        let stop_result = laser_fire_stop(&ctx, &result.token);

        assert!(stop_result.is_err());
        assert!(ctx.active_laser_fire.lock().unwrap().is_some());
        assert!(
            wait_for_sent_line_count(&transport, "M5", 2, Duration::from_secs(2)),
            "deadman should retry M5 after a failed frontend stop"
        );
        assert!(ctx.active_laser_fire.lock().unwrap().is_none());
    }

    #[test]
    fn runtime_state_degrades_gracefully_on_poisoned_job_mutex() {
        let ctx = ServiceContext::new();
        // Poison the job mutex by panicking inside a lock
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = ctx.job.lock().unwrap();
            panic!("intentional poison");
        }));
        // The job mutex is now poisoned — tick_job will fail.
        // runtime_state should still return Ok with job_progress: None
        // and job_tick_error populated so callers can distinguish the failure.
        let result = runtime_state(&ctx);
        assert!(
            result.is_ok(),
            "runtime_state should not propagate tick_job errors"
        );
        let state = result.unwrap();
        assert!(state.job_progress.is_none());
        assert!(
            state.job_tick_error.is_some(),
            "job_tick_error should surface the failure"
        );
        assert!(state.job_tick_error.as_ref().unwrap().contains("lock"));
        assert_eq!(state.session_state, SessionState::Disconnected);
    }

    #[test]
    fn grbl_settings_sync_updates_active_profile() {
        let _guard = PersistTestGuard::new();
        let mut settings = AppSettings::default();
        let mut profile = MachineProfile {
            name: "Bench Laser".to_string(),
            bed_width_mm: 200.0,
            bed_height_mm: 200.0,
            max_speed_mm_min: 3000.0,
            s_value_max: 500,
            homing_enabled: false,
            ..MachineProfile::default()
        };
        let profile_id = profile.id;
        settings.active_profile_id = Some(profile_id);
        settings.machine_profiles.push(profile.clone());
        let ctx = ServiceContext::with_settings(settings);
        let mut grbl = GrblSettings::new();
        grbl.set(130, 400.0);
        grbl.set(131, 410.0);
        grbl.set(110, 6000.0);
        grbl.set(111, 5000.0);
        grbl.set(30, 1000.0);
        grbl.set(22, 1.0);

        let sync = sync_active_profile_from_grbl_settings(&ctx, &grbl, Some(115200))
            .unwrap()
            .unwrap();
        let stored = ctx.settings.lock().unwrap();
        profile = stored.machine_profiles[0].clone();

        assert_eq!(sync.profile_id, profile_id);
        assert!(!sync.created_profile);
        assert_eq!(sync.bed_width_mm, Some(400.0));
        assert_eq!(sync.bed_height_mm, Some(410.0));
        assert_eq!(sync.max_speed_mm_min, Some(5000.0));
        assert_eq!(sync.s_value_max, Some(1000));
        assert_eq!(sync.homing_enabled, Some(true));
        assert_eq!(profile.bed_width_mm, 400.0);
        assert_eq!(profile.bed_height_mm, 410.0);
        assert_eq!(profile.max_speed_mm_min, 5000.0);
        assert_eq!(profile.s_value_max, 1000);
        assert!(profile.homing_enabled);
        assert_eq!(profile.default_baud_rate, 115200);
        assert_eq!(profile.preset_id, None);
        assert_eq!(profile.preset_version, None);
    }

    #[test]
    fn grbl_settings_sync_preserves_preset_metadata() {
        let _guard = PersistTestGuard::new();
        let mut settings = AppSettings::default();
        let profile = MachineProfile {
            preset_id: Some("sculpfun_s30_pro_max_20w".to_string()),
            preset_version: Some(1),
            bed_width_mm: 370.0,
            bed_height_mm: 360.0,
            ..MachineProfile::default()
        };
        let profile_id = profile.id;
        settings.active_profile_id = Some(profile_id);
        settings.machine_profiles.push(profile);
        let ctx = ServiceContext::with_settings(settings);
        let mut grbl = GrblSettings::new();
        grbl.set(130, 400.0);
        grbl.set(131, 410.0);
        grbl.set(110, 6000.0);
        grbl.set(111, 6000.0);

        let sync = sync_active_profile_from_grbl_settings(&ctx, &grbl, Some(115200))
            .unwrap()
            .unwrap();
        let stored = ctx.settings.lock().unwrap();
        let profile = &stored.machine_profiles[0];

        assert_eq!(sync.bed_width_mm, Some(370.0));
        assert_eq!(sync.bed_height_mm, Some(360.0));
        assert_eq!(
            profile.preset_id.as_deref(),
            Some("sculpfun_s30_pro_max_20w")
        );
        assert_eq!(profile.preset_version, Some(1));
        assert_eq!(profile.bed_width_mm, 370.0);
        assert_eq!(profile.bed_height_mm, 360.0);
        assert_eq!(profile.max_speed_mm_min, 6000.0);
    }

    #[test]
    fn grbl_settings_sync_updates_open_project_bound_to_profile_without_undo() {
        let _guard = PersistTestGuard::new();
        let mut settings = AppSettings::default();
        let profile = MachineProfile {
            name: "Bench Laser".to_string(),
            bed_width_mm: 400.0,
            bed_height_mm: 200.0,
            ..MachineProfile::default()
        };
        let profile_id = profile.id;
        settings.active_profile_id = Some(profile_id);
        settings.machine_profiles.push(profile.clone());
        let ctx = ServiceContext::with_settings(settings);
        let mut project = Project::new("bound");
        project.machine_profile_id = Some(profile_id);
        project.machine_profile_snapshot = Some(profile.snapshot());
        project.workspace = Workspace {
            bed_width_mm: 400.0,
            bed_height_mm: 200.0,
            origin: profile.origin,
        };
        *ctx.project.lock().unwrap() = Some(project);

        let mut grbl = GrblSettings::new();
        grbl.set(130, 400.0);
        grbl.set(131, 410.0);
        let sync = sync_active_profile_from_grbl_settings(&ctx, &grbl, Some(115200))
            .unwrap()
            .unwrap();
        let project = ctx.project.lock().unwrap().as_ref().unwrap().clone();

        assert!(sync.project_workspace_synced);
        assert_eq!(project.workspace.bed_width_mm, 400.0);
        assert_eq!(project.workspace.bed_height_mm, 410.0);
        assert_eq!(
            project.machine_profile_snapshot.unwrap().bed_height_mm,
            410.0
        );
        assert!(project.dirty);
        assert!(!ctx.undo_state().unwrap().can_undo);
    }

    #[test]
    fn grbl_settings_sync_creates_profile_when_none_exists() {
        let _guard = PersistTestGuard::new();
        let ctx = ServiceContext::with_settings(AppSettings::default());
        let mut grbl = GrblSettings::new();
        grbl.set(130, 400.0);
        grbl.set(131, 410.0);

        let sync = sync_active_profile_from_grbl_settings(&ctx, &grbl, Some(115200))
            .unwrap()
            .unwrap();
        let stored = ctx.settings.lock().unwrap();

        assert!(sync.created_profile);
        assert_eq!(stored.machine_profiles.len(), 1);
        assert_eq!(
            stored.active_profile_id,
            Some(stored.machine_profiles[0].id)
        );
        assert_eq!(stored.machine_profiles[0].name, "GRBL Auto-Detected");
        assert_eq!(stored.machine_profiles[0].bed_width_mm, 400.0);
        assert_eq!(stored.machine_profiles[0].bed_height_mm, 410.0);
    }

    #[test]
    fn grbl_settings_sync_creates_active_profile_when_profiles_exist_but_none_active() {
        let _guard = PersistTestGuard::new();
        let existing_profile = MachineProfile {
            name: "Inactive".to_string(),
            bed_width_mm: 300.0,
            bed_height_mm: 200.0,
            ..MachineProfile::default()
        };
        let mut settings = AppSettings::default();
        settings.machine_profiles.push(existing_profile);
        let ctx = ServiceContext::with_settings(settings);
        let mut grbl = GrblSettings::new();
        grbl.set(130, 400.0);
        grbl.set(131, 410.0);

        let sync = sync_active_profile_from_grbl_settings(&ctx, &grbl, Some(115200))
            .unwrap()
            .unwrap();

        assert!(sync.created_profile);
        let stored = ctx.settings.lock().unwrap();
        assert_eq!(stored.machine_profiles.len(), 2);
        let active_id = stored
            .active_profile_id
            .expect("new profile should be active");
        let active = stored
            .machine_profiles
            .iter()
            .find(|profile| profile.id == active_id)
            .unwrap();
        assert_eq!(active.name, "GRBL Auto-Detected");
        assert_eq!(active.bed_width_mm, 400.0);
        assert_eq!(active.bed_height_mm, 410.0);
    }

    #[test]
    fn runtime_state_no_error_when_job_mutex_healthy() {
        let ctx = ServiceContext::new();
        let result = runtime_state(&ctx);
        assert!(result.is_ok());
        let state = result.unwrap();
        assert!(state.job_progress.is_none());
        assert!(
            state.job_tick_error.is_none(),
            "no error when mutex is healthy"
        );
    }

    #[test]
    fn machine_status_does_not_poll_job_acknowledgements_outside_streamer() {
        let ctx = ServiceContext::new();
        let state = Arc::new(Mutex::new(DelayedAckState {
            initial_responses: VecDeque::from(["Grbl 1.1h".to_string()]),
            ..DelayedAckState::default()
        }));
        let transport = DelayedAckTransport::new(Arc::clone(&state), 3);
        let mut session = GrblSession::new(Box::new(transport));
        session.connect().unwrap();
        session.poll().unwrap();
        session.mark_ready().unwrap();

        let mut job = JobController::prepare(&dummy_plan(), &GcodeConfig::default()).unwrap();
        job.start(&mut session).unwrap();
        *ctx.session.lock().unwrap() = Some(MachineSessionHandle::Grbl(session.into()));
        *ctx.job.lock().unwrap() = Some(ActiveJobHandle::Grbl(job));

        machine_status(&ctx).unwrap();

        {
            let state = state.lock().unwrap();
            assert_eq!(state.read_calls_after_initial_responses, 2);
            assert!(!state.delayed_ok_sent);
        }
        let progress_after_status = ctx.job.lock().unwrap().as_ref().unwrap().progress();
        assert_eq!(progress_after_status.acknowledged_lines, 0);

        let progress_after_streamer_tick = tick_job(&ctx).unwrap().unwrap();
        assert_eq!(progress_after_streamer_tick.acknowledged_lines, 1);
    }

    #[test]
    fn fatal_job_tick_error_drops_stale_session_after_clearing_job() {
        let (ctx, transport) = ready_grbl_context(MachineProfile::default());
        let mut job = JobController::prepare(&dummy_plan(), &GcodeConfig::default()).unwrap();
        {
            let mut session_lock = ctx.session.lock().unwrap();
            let MachineSessionHandle::Grbl(session) = session_lock.as_mut().unwrap() else {
                panic!("expected GRBL session");
            };
            job.start(session).unwrap();
        }
        *ctx.job.lock().unwrap() = Some(ActiveJobHandle::Grbl(job));
        transport.lock().unwrap().fail_next_read = true;

        let err = tick_job(&ctx).unwrap_err();

        assert!(err.to_string().contains("injected read failure"));
        assert!(ctx.job.lock().unwrap().is_none());
        assert!(
            ctx.session.lock().unwrap().is_none(),
            "fatal streaming errors should drop the stale machine session so the UI can reconnect"
        );
        assert_eq!(session_state(&ctx).unwrap(), SessionState::Disconnected);
        let retained = ctx.last_terminal_job.lock().unwrap().clone().unwrap();
        assert_eq!(retained.reason, "fatal_tick_error");
        assert!(
            retained
                .error
                .as_deref()
                .is_some_and(|error| error.contains("injected read failure"))
        );
        assert!(retained.progress.is_some());
        assert!(
            retained
                .job_console
                .iter()
                .any(|entry| entry.direction == ConsoleDirection::Sent),
            "fatal teardown should retain job-stream sent command evidence"
        );
    }

    #[test]
    fn grbl_error_failed_job_drops_stale_running_session() {
        let (ctx, transport) = ready_grbl_context(MachineProfile::default());
        let mut job = JobController::prepare(&dummy_plan(), &GcodeConfig::default()).unwrap();
        {
            let mut session_lock = ctx.session.lock().unwrap();
            let MachineSessionHandle::Grbl(session) = session_lock.as_mut().unwrap() else {
                panic!("expected GRBL session");
            };
            job.start(session).unwrap();
        }
        *ctx.job.lock().unwrap() = Some(ActiveJobHandle::Grbl(job));
        transport
            .lock()
            .unwrap()
            .rx
            .push_back("error:2".to_string());

        let progress = tick_job(&ctx).unwrap().unwrap();

        assert_eq!(progress.state, JobState::Failed);
        assert!(ctx.job.lock().unwrap().is_none());
        assert!(
            ctx.session.lock().unwrap().is_none(),
            "controller errors should not leave the session stuck in Running after the job fails"
        );
        assert_eq!(session_state(&ctx).unwrap(), SessionState::Disconnected);
        let retained = ctx.last_terminal_job.lock().unwrap().clone().unwrap();
        assert_eq!(retained.reason, "failed_while_session_running");
        assert_eq!(
            retained.progress.as_ref().map(|progress| progress.state),
            Some(JobState::Failed)
        );
        assert!(
            retained
                .error
                .as_deref()
                .is_some_and(|error| error.to_ascii_lowercase().contains("error"))
        );
    }

    #[test]
    fn starting_new_job_clears_retained_terminal_diagnostic() {
        let profile = MachineProfile {
            bed_width_mm: 500.0,
            bed_height_mm: 500.0,
            ..MachineProfile::default()
        };
        let settings = AppSettings {
            active_profile_id: Some(profile.id),
            machine_profiles: vec![profile],
            ..AppSettings::default()
        };
        let ctx = ServiceContext::with_settings(settings);
        *ctx.last_terminal_job.lock().unwrap() = Some(DiagnosticTerminalJob {
            captured_at: Utc::now().to_rfc3339(),
            reason: "previous_failure".to_string(),
            progress: Some(JobProgress::default()),
            error: Some("old evidence".to_string()),
            job_tick_loop_running: false,
            session_state: Some(SessionState::Disconnected),
            machine_run_state: None,
            job_console: Vec::new(),
            session_console: Vec::new(),
        });

        let mut transport = MockSerialTransport::new("mock");
        transport.enqueue_response("Grbl 1.1h");
        transport.enqueue_response("$32=1");
        transport.enqueue_response("$22=1");
        transport.enqueue_response("<Idle|MPos:0.000,0.000,0.000|FS:0,0>");
        let mut session = GrblSession::new(Box::new(transport));
        session.connect().unwrap();
        session.poll().unwrap();
        session.mark_ready().unwrap();
        *ctx.session.lock().unwrap() = Some(MachineSessionHandle::Grbl(session.into()));

        let mut project = Project::new("Start Clears Diagnostic");
        let layer_id = project.ensure_default_layer();
        project.add_object(ProjectObject::new(
            "Rect",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        ));
        *ctx.project.lock().unwrap() = Some(project);

        let progress = start_job(&ctx).unwrap();

        assert_eq!(progress.state, JobState::Running);
        assert!(
            ctx.last_terminal_job.lock().unwrap().is_none(),
            "successful new job start should clear stale retained diagnostics"
        );
    }

    #[test]
    fn runtime_state_recovers_validating_grbl_session_when_controller_is_idle() {
        let ctx = ServiceContext::new();
        let mut transport = MockSerialTransport::new("mock");
        transport.enqueue_response("Grbl 1.1h");
        transport.enqueue_response("<Idle|MPos:0.000,0.000,0.000|FS:0,0>");
        let mut session = GrblSession::new(Box::new(transport));
        session.connect().unwrap();
        session.poll().unwrap();
        assert_eq!(session.session_state(), SessionState::Validating);
        assert_eq!(session.last_status().run_state, MachineRunState::Idle);
        *ctx.session.lock().unwrap() = Some(MachineSessionHandle::Grbl(session.into()));

        let state = runtime_state(&ctx).unwrap();

        assert_eq!(state.session_state, SessionState::Ready);
        let session_state = ctx
            .session
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .session_state();
        assert_eq!(session_state, SessionState::Ready);
    }

    #[test]
    fn test_air_assist_disconnected_returns_not_connected() {
        let ctx = ServiceContext::new();

        let err = test_air_assist(&ctx, 0).unwrap_err();

        assert_eq!(err.code, crate::error::ServiceErrorCode::InvalidState);
        assert!(err.message.contains("Not connected"));
    }

    #[test]
    fn test_air_assist_rejects_active_job_before_touching_session() {
        let ctx = ServiceContext::new();
        let mut dsp = DspSession::connect(ControllerModel::Ruida, "mock".to_string());
        let job = dsp.start_job(&dummy_plan());
        *ctx.job.lock().unwrap() = Some(ActiveJobHandle::Dsp(job));

        let err = test_air_assist(&ctx, 0).unwrap_err();

        assert_eq!(err.code, crate::error::ServiceErrorCode::InvalidState);
        assert_eq!(err.message, "MACHINE_BUSY_JOB_RUNNING");
    }

    #[test]
    fn send_gcode_line_rejects_active_job() {
        let ctx = ServiceContext::new();
        let mut dsp = DspSession::connect(ControllerModel::Ruida, "mock".to_string());
        let job = dsp.start_job(&dummy_plan());
        *ctx.job.lock().unwrap() = Some(ActiveJobHandle::Dsp(job));

        let err = ctx.send_gcode_line("M5").unwrap_err();
        assert!(
            err.contains("job is active"),
            "console G-code must be blocked while a job is streaming, got: {err}"
        );
        assert!(
            !err.to_ascii_lowercase().contains("pause"),
            "pausing does not remove the active job and must not be suggested: {err}"
        );
        assert!(err.contains("Cancel the job or wait for it to finish"));
    }

    #[test]
    fn test_air_assist_rejects_empty_profile_commands() {
        let mut settings = AppSettings::default();
        let profile = MachineProfile {
            air_assist_on_gcode: String::new(),
            ..MachineProfile::default()
        };
        let profile_id = profile.id;
        settings.active_profile_id = Some(profile_id);
        settings.machine_profiles.push(profile);
        let ctx = ServiceContext::with_settings(settings);

        let err = test_air_assist(&ctx, 0).unwrap_err();

        assert_eq!(err.code, crate::error::ServiceErrorCode::InvalidInput);
        assert!(err.message.contains("must not be empty"));
    }

    #[test]
    fn test_air_assist_rejects_non_grbl_sessions() {
        let ctx = ServiceContext::new();
        *ctx.session.lock().unwrap() = Some(MachineSessionHandle::Dsp(DspSession::connect(
            ControllerModel::Ruida,
            "mock".to_string(),
        )));

        let err = test_air_assist(&ctx, 0).unwrap_err();

        assert_eq!(err.code, crate::error::ServiceErrorCode::InvalidState);
        assert!(err.message.contains("only supported for GRBL"));
    }

    /// When all enabled layers are tool layers, plan generation fails with
    /// EmptyPlan. `run_preflight_check` should still return a report (not
    /// propagate the error) without warning that tool layers are suspicious.
    #[test]
    fn preflight_with_only_tool_layers_returns_report_without_tool_warning() {
        use beambench_core::object::ShapeKind;
        use beambench_core::{Layer, OperationType, Project, ProjectObject};

        let ctx = ServiceContext::new();

        // Create a project with only a tool layer
        let mut project = Project::new("Tool Only");
        let mut tool_layer = Layer::new("T1", OperationType::Line);
        tool_layer.is_tool_layer = true;
        let tool_id = tool_layer.id;
        project.add_layer(tool_layer);
        project.add_object(ProjectObject::new(
            "Rect",
            tool_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
            beambench_core::ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        ));

        *ctx.project.lock().unwrap() = Some(project);

        let result = run_preflight_check(&ctx);
        assert!(
            result.is_ok(),
            "should return Ok(report), not propagate plan error"
        );

        let report = result.unwrap();
        assert_eq!(report.outcome, PreflightOutcome::Fail);

        // Must contain the plan failure check
        assert!(
            report
                .checks
                .iter()
                .any(|c| c.category == "plan" && !c.passed && c.description == "Plan generation"),
            "report should include plan generation failure check"
        );

        assert!(
            !report
                .checks
                .iter()
                .any(|c| c.description == "Tool layers present"),
            "tool layers should not be warned on as suspicious"
        );
    }

    #[test]
    fn frame_objects_uses_tool_layers_for_bounds_not_physical_motion() {
        use beambench_core::{Layer, OperationType};

        let ctx = ServiceContext::new();
        let mut project = Project::new("Frame");
        project.layers.clear();

        let line_layer = Layer::new("Line", OperationType::Line);
        let line_layer_id = line_layer.id;
        let mut tool_layer = Layer::new("T1", OperationType::Line);
        tool_layer.is_tool_layer = true;
        let tool_layer_id = tool_layer.id;
        project.add_layer(line_layer);
        project.add_layer(tool_layer);

        let line_obj = ProjectObject::new(
            "Line",
            line_layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(5.0, 5.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 5.0,
                height: 5.0,
                corner_radius: 0.0,
            },
        );
        let tool_obj = ProjectObject::new(
            "Tool",
            tool_layer_id,
            Bounds::new(Point2D::new(50.0, 50.0), Point2D::new(60.0, 60.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        );
        project.add_object(line_obj);
        project.add_object(tool_obj);
        *ctx.project.lock().unwrap() = Some(project);

        let (bounds_objects, physical_objects) = frame_objects(&ctx, &[]).unwrap();

        assert_eq!(bounds_objects.len(), 2);
        assert_eq!(physical_objects.len(), 1);
        assert_eq!(physical_objects[0].layer_id, line_layer_id);
    }

    #[test]
    fn frame_objects_allows_selected_tool_layer_objects() {
        use beambench_core::{Layer, OperationType};

        let ctx = ServiceContext::new();
        let mut project = Project::new("Selected Tool Frame");
        project.layers.clear();
        let mut tool_layer = Layer::new("T1", OperationType::Line);
        tool_layer.is_tool_layer = true;
        let tool_layer_id = tool_layer.id;
        project.add_layer(tool_layer);
        let tool_obj = ProjectObject::new(
            "Tool",
            tool_layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        );
        let tool_obj_id = tool_obj.id.to_string();
        project.add_object(tool_obj);
        *ctx.project.lock().unwrap() = Some(project);

        let (bounds_objects, physical_objects) = frame_objects(&ctx, &[tool_obj_id]).unwrap();

        assert_eq!(bounds_objects.len(), 1);
        assert_eq!(physical_objects.len(), 1);
        assert_eq!(physical_objects[0].layer_id, tool_layer_id);
    }

    #[test]
    fn frame_objects_tool_only_with_tool_bounds_disabled_fails_cleanly() {
        use beambench_core::{Layer, OperationType};

        let ctx = ServiceContext::new();
        ctx.settings
            .lock()
            .unwrap()
            .include_tool_layers_in_job_bounds = false;
        let mut project = Project::new("Tool Only Frame");
        project.layers.clear();
        let mut tool_layer = Layer::new("T1", OperationType::Line);
        tool_layer.is_tool_layer = true;
        let tool_layer_id = tool_layer.id;
        project.add_layer(tool_layer);
        project.add_object(ProjectObject::new(
            "Tool",
            tool_layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        ));
        *ctx.project.lock().unwrap() = Some(project);

        let err = frame_objects(&ctx, &[]).unwrap_err();

        assert!(err.to_string().contains("No objects to frame"));
    }

    #[test]
    fn start_job_rechecks_preflight_and_blocks_non_passing_preflight_results() {
        let ctx = ServiceContext::new();

        let mut project = Project::new("Warning-tier Start");
        let layer_id = project.ensure_default_layer();
        project.add_object(ProjectObject::new(
            "Rect",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        ));
        *ctx.project.lock().unwrap() = Some(project);

        let mut transport = MockSerialTransport::new("mock");
        transport.enqueue_response("Grbl 1.1h");
        transport.enqueue_response("$32=0");
        transport.enqueue_response("<Idle|MPos:0.000,0.000,0.000|FS:0,0>");
        let mut session = GrblSession::new(Box::new(transport));
        session.connect().unwrap();
        session.poll().unwrap();
        session.mark_ready().unwrap();
        *ctx.session.lock().unwrap() = Some(MachineSessionHandle::Grbl(session.into()));

        let err = start_job(&ctx).unwrap_err();
        assert_eq!(err.code, crate::error::ServiceErrorCode::InvalidState);
        assert!(
            err.message.contains("Preflight"),
            "expected non-passing preflight to block direct start, got: {}",
            err.message
        );
        assert_eq!(ctx.job.lock().unwrap().is_some(), false);
        let details = err.details.expect("preflight details should be attached");
        assert_ne!(details["preflight"]["outcome"], "pass");
    }
}

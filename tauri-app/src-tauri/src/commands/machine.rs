//! Machine connection and job control Tauri commands.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use beambench_common::console::ConsoleEntry;
use beambench_common::controller_choice::{
    ControllerConnectionResult, ControllerMismatchDecision, ControllerSelection,
};
use beambench_common::machine::{
    DiscoveryScanState, DiscoveryTcpTarget, DiscoveryUsbTarget, JobProgress,
    MachineConnectionTarget, MachineStatus, PortInfo, PreflightReport, SessionState,
};
use beambench_core::{CutEntry, MachineProfile, MachineProfileId, MacroDefinition, MaterialPreset};
use beambench_grbl::GrblSettingId;
use beambench_service::ServiceContext;
use beambench_service::ops::planning::SessionJobOptions;
use beambench_service::ops::{discovery, machine, profiles};
use beambench_service::persist;
use tauri::State;
use uuid::Uuid;

use super::project::parse_id;

const CONTROLLER_CONNECTION_CHALLENGE_EXPIRY: Duration = Duration::from_secs(121);

fn schedule_controller_connection_expiry(
    svc: Arc<ServiceContext>,
    result: &ControllerConnectionResult,
) {
    let ControllerConnectionResult::Challenge { attempt_id, .. } = result else {
        return;
    };
    let attempt_id = attempt_id.clone();
    let _ = std::thread::spawn(move || {
        std::thread::sleep(CONTROLLER_CONNECTION_CHALLENGE_EXPIRY);
        if let Err(error) = machine::expire_controller_connection(&svc, &attempt_id) {
            tracing::warn!(%error, "Failed to expire controller connection challenge");
        }
    });
}

fn merge_imported_materials(
    existing: &[MaterialPreset],
    imported: Vec<MaterialPreset>,
) -> Vec<MaterialPreset> {
    let mut merged = existing.to_vec();
    merged.extend(imported.into_iter().map(|mut preset| {
        preset.id = Uuid::new_v4();
        preset
    }));
    merged
}

fn import_materials_atomic<F>(
    svc: &ServiceContext,
    imported: Vec<MaterialPreset>,
    persist_materials: F,
) -> Result<Vec<MaterialPreset>, String>
where
    F: FnOnce(&[MaterialPreset]) -> Result<(), String>,
{
    let existing = svc.get_materials()?;
    let merged = merge_imported_materials(&existing, imported);
    persist_materials(&merged)?;
    svc.replace_materials(merged.clone())?;
    Ok(merged)
}

fn merge_imported_macros(
    existing: &[MacroDefinition],
    imported: Vec<MacroDefinition>,
) -> Vec<MacroDefinition> {
    let mut merged = existing.to_vec();
    merged.extend(imported.into_iter().map(|mut macro_def| {
        macro_def.id = Uuid::new_v4();
        macro_def
    }));
    merged
}

fn import_macros_atomic<F>(
    svc: &ServiceContext,
    imported: Vec<MacroDefinition>,
    persist_macros: F,
) -> Result<Vec<MacroDefinition>, String>
where
    F: FnOnce(&[MacroDefinition]) -> Result<(), String>,
{
    let existing = svc.get_macros()?;
    let merged = merge_imported_macros(&existing, imported);
    persist_macros(&merged)?;
    svc.replace_macros(merged.clone())?;
    Ok(merged)
}

#[tauri::command]
pub fn list_serial_ports(svc: State<'_, Arc<ServiceContext>>) -> Result<Vec<PortInfo>, String> {
    let ports = machine::list_serial_ports_op().map_err(|e| e.to_string())?;
    svc.push_connection_event(
        "port_scan",
        None,
        None,
        Some(format!("Detected {} serial ports", ports.len())),
        None,
    );
    Ok(ports)
}

#[tauri::command]
pub fn connect_machine(
    port_name: String,
    baud_rate: Option<u32>,
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<SessionState, String> {
    machine::connect_machine(
        &svc,
        machine::ConnectMachineInput {
            target: MachineConnectionTarget::GrblSerial {
                port_name,
                baud_rate: baud_rate.unwrap_or(115200),
            },
        },
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn connect_machine_candidate(
    candidate_id: String,
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<SessionState, String> {
    machine::connect_machine(
        &svc,
        machine::ConnectMachineInput {
            target: MachineConnectionTarget::DiscoveryCandidate { candidate_id },
        },
    )
    .map_err(Into::into)
}

#[tauri::command]
pub async fn begin_controller_connection(
    port_name: String,
    baud_rate: Option<u32>,
    selection: ControllerSelection,
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<ControllerConnectionResult, String> {
    let svc = svc.inner().clone();
    let expiry_svc = svc.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        machine::begin_controller_connection(
            &svc,
            machine::BeginControllerConnectionInput {
                port_name,
                baud_rate: baud_rate.unwrap_or(115200),
                selection,
            },
        )
        .map_err(String::from)
    })
    .await
    .map_err(|error| format!("Controller connection task failed: {error}"))??;
    schedule_controller_connection_expiry(expiry_svc, &result);
    Ok(result)
}

#[tauri::command]
pub async fn begin_network_controller_connection(
    host: String,
    port: u16,
    selection: ControllerSelection,
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<ControllerConnectionResult, String> {
    let svc = svc.inner().clone();
    let expiry_svc = svc.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        machine::begin_network_controller_connection(
            &svc,
            machine::BeginNetworkControllerConnectionInput {
                host,
                port,
                selection,
            },
        )
        .map_err(String::from)
    })
    .await
    .map_err(|error| format!("Network controller connection task failed: {error}"))??;
    schedule_controller_connection_expiry(expiry_svc, &result);
    Ok(result)
}

#[tauri::command]
pub async fn list_lihuiyu_usb_devices() -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let devices = machine::list_lihuiyu_usb_devices().map_err(String::from)?;
        serde_json::to_value(devices)
            .map_err(|error| format!("Serializing Lihuiyu USB devices failed: {error}"))
    })
    .await
    .map_err(|error| format!("USB controller enumeration task failed: {error}"))?
}

#[tauri::command]
pub async fn begin_usb_controller_connection(
    bus_id: String,
    device_address: u8,
    port_numbers: Vec<u8>,
    selection: ControllerSelection,
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<ControllerConnectionResult, String> {
    let svc = svc.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        machine::begin_usb_controller_connection(
            &svc,
            machine::BeginUsbControllerConnectionInput {
                bus_id,
                device_address,
                port_numbers,
                selection,
            },
        )
        .map_err(String::from)
    })
    .await
    .map_err(|error| format!("USB controller connection task failed: {error}"))?
}

#[tauri::command]
pub async fn continue_controller_connection(
    attempt_id: String,
    selection: ControllerSelection,
    decision: Option<ControllerMismatchDecision>,
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<ControllerConnectionResult, String> {
    let svc = svc.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        machine::continue_controller_connection(
            &svc,
            machine::ContinueControllerConnectionInput {
                attempt_id,
                selection,
                decision,
            },
        )
        .map_err(String::from)
    })
    .await
    .map_err(|error| format!("Controller connection task failed: {error}"))?
}

#[tauri::command]
pub fn disconnect_machine(svc: State<'_, Arc<ServiceContext>>) -> Result<(), String> {
    machine::disconnect_machine(&svc).map_err(Into::into)
}

#[tauri::command]
pub fn get_machine_status(svc: State<'_, Arc<ServiceContext>>) -> Result<MachineStatus, String> {
    machine::machine_status(&svc).map_err(Into::into)
}

#[tauri::command]
pub fn get_machine_runtime_state(
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<machine::MachineRuntimeState, String> {
    machine::runtime_state(&svc).map_err(Into::into)
}

#[tauri::command]
pub fn get_machine_coordinates_valid(svc: State<'_, Arc<ServiceContext>>) -> bool {
    machine::machine_coordinates_valid(&svc)
}

#[tauri::command]
pub fn get_session_state(svc: State<'_, Arc<ServiceContext>>) -> Result<SessionState, String> {
    machine::session_state(&svc).map_err(Into::into)
}

#[tauri::command]
pub fn machine_home(svc: State<'_, Arc<ServiceContext>>) -> Result<(), String> {
    machine::home(&svc).map_err(Into::into)
}

#[tauri::command]
pub fn machine_unlock(svc: State<'_, Arc<ServiceContext>>) -> Result<(), String> {
    machine::unlock(&svc).map_err(Into::into)
}

#[tauri::command]
pub fn machine_jog(
    x_mm: f64,
    y_mm: f64,
    z_mm: Option<f64>,
    feed_rate: f64,
    continuous: Option<bool>,
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<(), String> {
    machine::jog(
        &svc,
        machine::JogMachineInput {
            x_mm,
            y_mm,
            z_mm,
            feed_rate,
            continuous: continuous.unwrap_or(false),
        },
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn machine_jog_cancel(svc: State<'_, Arc<ServiceContext>>) -> Result<(), String> {
    machine::jog_cancel(&svc).map_err(Into::into)
}

#[tauri::command]
pub fn run_preflight_check(
    svc: State<'_, Arc<ServiceContext>>,
    job_options: Option<SessionJobOptions>,
) -> Result<PreflightReport, String> {
    machine::run_preflight_check_with_options(&svc, &job_options.unwrap_or_default())
        .map_err(Into::into)
}

#[tauri::command]
pub async fn start_job(
    svc: State<'_, Arc<ServiceContext>>,
    job_options: Option<SessionJobOptions>,
) -> Result<JobProgress, String> {
    let svc = svc.inner().clone();
    let tick_svc = svc.clone();
    let options = job_options.unwrap_or_default();
    let progress = tauri::async_runtime::spawn_blocking(move || {
        machine::start_job_with_options(&svc, &options).map_err(String::from)
    })
    .await
    .map_err(|error| format!("Job start task failed: {error}"))??;
    machine::spawn_job_tick_loop(tick_svc);
    Ok(progress)
}

#[tauri::command]
pub fn get_job_progress(
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<Option<JobProgress>, String> {
    machine::tick_job(&svc).map_err(Into::into)
}

#[tauri::command]
pub fn pause_job(svc: State<'_, Arc<ServiceContext>>) -> Result<(), String> {
    machine::pause_job(&svc).map_err(Into::into)
}

#[tauri::command]
pub fn resume_job(svc: State<'_, Arc<ServiceContext>>) -> Result<(), String> {
    machine::resume_job(&svc).map_err(Into::into)
}

#[tauri::command]
pub fn cancel_job(svc: State<'_, Arc<ServiceContext>>) -> Result<(), String> {
    machine::cancel_job(&svc).map_err(Into::into)
}

#[tauri::command]
pub fn get_machine_profiles(
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<Vec<MachineProfile>, String> {
    profiles::list_profiles(&svc).map_err(Into::into)
}

#[tauri::command]
pub fn save_machine_profile(
    profile: MachineProfile,
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<MachineProfile, String> {
    profiles::save_profile(
        &svc,
        profiles::SaveProfileInput {
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
    .map_err(Into::into)
}

#[tauri::command]
pub fn export_machine_profile(
    profile_id: MachineProfileId,
    path: String,
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<(), String> {
    profiles::export_machine_profile_to_path(&svc, profile_id, &PathBuf::from(path))
        .map_err(Into::into)
}

#[tauri::command]
pub fn import_machine_profile(
    path: String,
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<MachineProfile, String> {
    profiles::import_machine_profile_from_path(&svc, &PathBuf::from(path)).map_err(Into::into)
}

#[tauri::command]
pub fn delete_machine_profile(
    profile_id: MachineProfileId,
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<(), String> {
    profiles::delete_profile(&svc, profile_id).map_err(Into::into)
}

#[tauri::command]
pub fn get_machine_profile_presets() -> Vec<profiles::MachineProfilePreset> {
    profiles::profile_presets()
}

#[tauri::command]
pub fn suggest_machine_profile_preset(
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<profiles::PresetSuggestion, String> {
    profiles::suggest_profile_preset(&svc).map_err(Into::into)
}

#[tauri::command]
pub fn get_machine_profile_preset_diff(
    profile_id: MachineProfileId,
    preset_id: String,
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<Vec<profiles::ProfileFieldDiff>, String> {
    profiles::profile_preset_diff(&svc, profile_id, &preset_id).map_err(Into::into)
}

#[tauri::command]
pub fn apply_machine_profile_preset(
    profile_id: MachineProfileId,
    preset_id: String,
    confirm_diff: bool,
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<profiles::ApplyPresetResult, String> {
    if !confirm_diff {
        return Err(
            "Applying this preset changes profile fields and requires explicit confirmation."
                .to_string(),
        );
    }
    profiles::apply_profile_preset(&svc, profile_id, &preset_id).map_err(Into::into)
}

#[tauri::command]
pub fn set_active_profile(
    profile_id: Option<MachineProfileId>,
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<(), String> {
    profiles::set_active_profile(&svc, profile_id).map_err(Into::into)
}

#[tauri::command]
pub fn start_machine_discovery(
    tcp_targets: Option<Vec<DiscoveryTcpTarget>>,
    usb_targets: Option<Vec<DiscoveryUsbTarget>>,
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<DiscoveryScanState, String> {
    discovery::start_discovery(
        &svc,
        discovery::StartDiscoveryInput {
            tcp_targets: tcp_targets.unwrap_or_default(),
            usb_targets: usb_targets.unwrap_or_default(),
        },
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn get_machine_discovery_state(
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<DiscoveryScanState, String> {
    discovery::get_discovery_state(&svc).map_err(Into::into)
}

#[tauri::command]
pub fn cancel_machine_discovery(
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<DiscoveryScanState, String> {
    discovery::cancel_discovery(&svc).map_err(Into::into)
}

#[tauri::command]
pub fn bootstrap_machine_profile(
    candidate_id: String,
    profile_name: Option<String>,
    activate: Option<bool>,
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<MachineProfile, String> {
    discovery::bootstrap_profile(
        &svc,
        discovery::BootstrapProfileInput {
            candidate_id,
            profile_name,
            activate: activate.unwrap_or(true),
        },
    )
    .map_err(Into::into)
}

#[tauri::command]
pub async fn frame_job(
    svc: State<'_, Arc<ServiceContext>>,
    frame_mode: Option<String>,
    selected_object_ids: Option<Vec<String>>,
    laser_on_override: Option<bool>,
    feed_rate: Option<f64>,
) -> Result<JobProgress, String> {
    let svc = svc.inner().clone();
    let tick_svc = svc.clone();
    let mode = frame_mode.unwrap_or_else(|| "rectangular".to_string());
    let ids = selected_object_ids.unwrap_or_default();
    let progress = tauri::async_runtime::spawn_blocking(move || {
        machine::frame_job(
            &svc,
            &mode,
            &ids,
            laser_on_override.unwrap_or(false),
            feed_rate,
        )
        .map_err(String::from)
    })
    .await
    .map_err(|error| format!("Frame start task failed: {error}"))??;
    machine::spawn_job_tick_loop(tick_svc);
    Ok(progress)
}

#[tauri::command]
pub fn set_feed_override(
    action: String,
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<(), String> {
    machine::set_feed_override(&svc, &action).map_err(Into::into)
}

#[tauri::command]
pub fn set_spindle_override(
    action: String,
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<(), String> {
    machine::set_spindle_override(&svc, &action).map_err(Into::into)
}

#[tauri::command]
pub fn reset_all_overrides(svc: State<'_, Arc<ServiceContext>>) -> Result<(), String> {
    machine::reset_all_overrides(&svc).map_err(Into::into)
}

#[tauri::command]
pub fn emergency_stop(svc: State<'_, Arc<ServiceContext>>) -> Result<(), String> {
    machine::emergency_stop(&svc).map_err(Into::into)
}

#[tauri::command]
pub fn set_work_origin(svc: State<'_, Arc<ServiceContext>>) -> Result<(f64, f64), String> {
    machine::set_work_origin(&svc).map_err(Into::into)
}

#[tauri::command]
pub fn reset_work_origin(svc: State<'_, Arc<ServiceContext>>) -> Result<(), String> {
    machine::reset_work_origin(&svc).map_err(Into::into)
}

// ---------------------------------------------------------------------------
// Machine, material, and macro commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn send_gcode_line(svc: State<'_, Arc<ServiceContext>>, line: String) -> Result<(), String> {
    svc.send_gcode_line(&line)
}

#[tauri::command]
pub fn get_console_log(
    svc: State<'_, Arc<ServiceContext>>,
    limit: usize,
) -> Result<Vec<ConsoleEntry>, String> {
    svc.get_console_log(limit)
}

#[tauri::command]
pub fn clear_console_log(svc: State<'_, Arc<ServiceContext>>) -> Result<(), String> {
    svc.clear_console_log()
}

#[tauri::command]
pub fn get_material_presets(
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<Vec<MaterialPreset>, String> {
    svc.get_materials()
}

#[tauri::command]
pub fn save_material_preset(
    svc: State<'_, Arc<ServiceContext>>,
    preset: MaterialPreset,
) -> Result<(), String> {
    svc.save_material(preset)?;
    // Persist to disk
    let presets = svc.get_materials()?;
    persist::save_material_presets(&presets)
        .map_err(|e| format!("Failed to persist materials: {e}"))
}

#[tauri::command]
pub fn delete_material_preset(
    svc: State<'_, Arc<ServiceContext>>,
    preset_id: String,
) -> Result<(), String> {
    let id = Uuid::parse_str(&preset_id).map_err(|e| format!("Invalid UUID: {e}"))?;
    svc.delete_material(id)?;
    let presets = svc.get_materials()?;
    persist::save_material_presets(&presets)
        .map_err(|e| format!("Failed to persist materials: {e}"))
}

#[tauri::command]
pub fn apply_material_preset(
    svc: State<'_, Arc<ServiceContext>>,
    preset_id: String,
    layer_id: String,
) -> Result<beambench_service::MaterialApplyResponse, String> {
    let pid = Uuid::parse_str(&preset_id).map_err(|e| format!("Invalid UUID: {e}"))?;
    let lid = parse_id(&layer_id)?;
    let mut guard = svc.project.lock().map_err(|e| format!("lock: {e}"))?;
    let project = guard.as_mut().ok_or("No project open")?;
    svc.push_project_undo_snapshot(project)?;
    // Capture old raster_settings before apply_material mutates them
    let old_rs = project
        .find_layer(lid)
        .and_then(|l| l.primary_entry().raster_settings.clone());
    let response = svc.apply_material(pid, lid, project)?;
    // Sync pass-through bounds if the preset triggered pass-through
    if let Some(new_rs) = project
        .find_layer(lid)
        .and_then(|l| l.primary_entry().raster_settings.clone())
    {
        beambench_service::ops::project::sync_passthrough_bounds(
            project,
            lid,
            old_rs.as_ref(),
            &new_rs,
        );
    }
    project.dirty = true;
    drop(guard);
    beambench_service::ops::planning::invalidate_plan_cache(&svc).map_err(|e| format!("{e}"))?;
    svc.emit_event(
        "project.layer.updated",
        serde_json::json!({
            "layer_id": layer_id,
        }),
    );
    Ok(response)
}

/// Apply a material preset to a caller-supplied seed entry without touching project state.
///
/// Used by the quality-test dialogs (Material/Focus/Interval) to compose seed values without
/// mutating the active project, plan cache, undo stack, or emitting layer-update events.
#[tauri::command]
pub fn apply_material_preset_to_seed(
    svc: State<'_, Arc<ServiceContext>>,
    preset_id: String,
    seed: CutEntry,
) -> Result<(CutEntry, Vec<beambench_service::MaterialApplyWarning>), String> {
    let pid = Uuid::parse_str(&preset_id).map_err(|e| format!("Invalid UUID: {e}"))?;
    svc.apply_preset_to_seed(pid, seed)
}

#[tauri::command]
pub fn export_material_presets(
    svc: State<'_, Arc<ServiceContext>>,
    path: String,
) -> Result<(), String> {
    let presets = svc.get_materials()?;
    let json =
        serde_json::to_string_pretty(&presets).map_err(|e| format!("Serialize failed: {e}"))?;
    std::fs::write(&path, json).map_err(|e| format!("Write failed: {e}"))
}

#[tauri::command]
pub fn import_material_presets(
    svc: State<'_, Arc<ServiceContext>>,
    path: String,
) -> Result<Vec<MaterialPreset>, String> {
    let data = std::fs::read_to_string(&path).map_err(|e| format!("Read failed: {e}"))?;
    let imported: Vec<MaterialPreset> =
        serde_json::from_str(&data).map_err(|e| format!("Parse failed: {e}"))?;
    import_materials_atomic(&svc, imported, |presets| {
        persist::save_material_presets(presets)
            .map_err(|e| format!("Failed to persist materials: {e}"))
    })
}

#[tauri::command]
pub fn get_macros(svc: State<'_, Arc<ServiceContext>>) -> Result<Vec<MacroDefinition>, String> {
    svc.get_macros()
}

#[tauri::command]
pub fn save_macro(
    svc: State<'_, Arc<ServiceContext>>,
    macro_def: MacroDefinition,
) -> Result<(), String> {
    svc.save_macro(macro_def)?;
    let macros = svc.get_macros()?;
    persist::save_macros(&macros).map_err(|e| format!("Failed to persist macros: {e}"))
}

#[tauri::command]
pub fn delete_macro(svc: State<'_, Arc<ServiceContext>>, macro_id: String) -> Result<(), String> {
    let id = Uuid::parse_str(&macro_id).map_err(|e| format!("Invalid UUID: {e}"))?;
    svc.delete_macro(id)?;
    let macros = svc.get_macros()?;
    persist::save_macros(&macros).map_err(|e| format!("Failed to persist macros: {e}"))
}

#[tauri::command]
pub fn run_macro(svc: State<'_, Arc<ServiceContext>>, macro_id: String) -> Result<(), String> {
    let id = Uuid::parse_str(&macro_id).map_err(|e| format!("Invalid UUID: {e}"))?;
    let macros = svc.get_macros()?;
    let macro_def = macros
        .iter()
        .find(|m| m.id == id)
        .ok_or_else(|| format!("Macro {id} not found"))?;

    for cmd in &macro_def.commands {
        svc.send_gcode_line(cmd)?;
    }
    Ok(())
}

#[tauri::command]
pub fn export_macros(svc: State<'_, Arc<ServiceContext>>, path: String) -> Result<(), String> {
    let macros = svc.get_macros()?;
    let json =
        serde_json::to_string_pretty(&macros).map_err(|e| format!("Serialize failed: {e}"))?;
    std::fs::write(&path, json).map_err(|e| format!("Write failed: {e}"))
}

#[tauri::command]
pub fn import_macros(
    svc: State<'_, Arc<ServiceContext>>,
    path: String,
) -> Result<Vec<MacroDefinition>, String> {
    let data = std::fs::read_to_string(&path).map_err(|e| format!("Read failed: {e}"))?;
    let imported: Vec<MacroDefinition> =
        serde_json::from_str(&data).map_err(|e| format!("Parse failed: {e}"))?;
    import_macros_atomic(&svc, imported, |macros| {
        persist::save_macros(macros).map_err(|e| format!("Failed to persist macros: {e}"))
    })
}

#[tauri::command]
pub fn get_controller_info(
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<HashMap<String, String>, String> {
    let mut guard = svc.session.lock().map_err(|e| format!("lock: {e}"))?;
    let session = guard.as_mut().ok_or("No active machine session")?;
    match session {
        beambench_service::runtime::MachineSessionHandle::Grbl(grbl) => {
            grbl.get_controller_info()
                .map_err(|e| format!("Failed to get controller info: {e}"))?;
            grbl.poll()
                .map_err(|e| format!("Failed to read controller info: {e}"))?;
            Ok(grbl.controller_info().clone())
        }
        _ => Err("Only GRBL sessions support controller info query".to_string()),
    }
}

#[tauri::command]
pub fn move_laser_to(
    svc: State<'_, Arc<ServiceContext>>,
    x: f64,
    y: f64,
    z: Option<f64>,
    feed_rate: Option<f64>,
) -> Result<(), String> {
    machine::move_laser_to(
        &svc,
        machine::MoveLaserInput {
            x,
            y,
            z,
            feed_rate: feed_rate.unwrap_or(3000.0),
        },
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn move_laser_to_machine(
    svc: State<'_, Arc<ServiceContext>>,
    x: f64,
    y: f64,
    z: Option<f64>,
    feed_rate: Option<f64>,
) -> Result<(), String> {
    machine::move_laser_to_machine(
        &svc,
        machine::MoveLaserInput {
            x,
            y,
            z,
            feed_rate: feed_rate.unwrap_or(3000.0),
        },
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn laser_fire_start(
    svc: State<'_, Arc<ServiceContext>>,
    power_percent: Option<f64>,
) -> Result<machine::LaserFireStartResult, String> {
    machine::laser_fire_start(svc.inner().clone(), power_percent).map_err(Into::into)
}

#[tauri::command]
pub fn laser_fire_keepalive(
    svc: State<'_, Arc<ServiceContext>>,
    token: String,
) -> Result<(), String> {
    machine::laser_fire_keepalive(&svc, &token).map_err(Into::into)
}

#[tauri::command]
pub fn laser_fire_stop(svc: State<'_, Arc<ServiceContext>>, token: String) -> Result<(), String> {
    machine::laser_fire_stop(&svc, &token).map_err(Into::into)
}

#[tauri::command]
pub fn get_grbl_settings(
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<HashMap<String, String>, String> {
    let mut guard = svc.session.lock().map_err(|e| format!("lock: {e}"))?;
    let session = guard.as_mut().ok_or("No active machine session")?;
    match session {
        beambench_service::runtime::MachineSessionHandle::Grbl(grbl) => {
            grbl.query_settings()
                .map_err(|e| format!("Failed to query settings: {e}"))?;
            grbl.poll()
                .map_err(|e| format!("Failed to read settings: {e}"))?;
            Ok(grbl.settings().as_string_map())
        }
        _ => Err("Only GRBL sessions support settings query".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use beambench_core::AppSettings;

    fn material_preset(name: &str) -> MaterialPreset {
        MaterialPreset {
            id: Uuid::new_v4(),
            name: name.to_string(),
            material: "Wood".to_string(),
            ..MaterialPreset::default()
        }
    }

    fn macro_def(name: &str) -> MacroDefinition {
        MacroDefinition {
            id: Uuid::new_v4(),
            name: name.to_string(),
            description: format!("{name} description"),
            commands: vec!["G0 X0 Y0".to_string()],
            hotkey: None,
            show_in_toolbar: false,
        }
    }

    #[test]
    fn import_macros_atomic_preserves_existing_macros_when_persist_fails() {
        let svc = ServiceContext::with_settings(AppSettings::default());
        let existing = macro_def("Existing");
        svc.save_macro(existing.clone()).unwrap();

        let imported = vec![macro_def("First"), macro_def("Second")];
        let result = import_macros_atomic(&svc, imported, |_| Err("persist failed".to_string()));

        assert_eq!(result.unwrap_err(), "persist failed");
        assert_eq!(svc.get_macros().unwrap(), vec![existing]);
    }

    #[test]
    fn import_materials_atomic_preserves_existing_presets_when_persist_fails() {
        let svc = ServiceContext::with_settings(AppSettings::default());
        let existing = material_preset("Existing");
        svc.save_material(existing.clone()).unwrap();

        let imported = vec![material_preset("First"), material_preset("Second")];
        let result = import_materials_atomic(&svc, imported, |_| Err("persist failed".to_string()));

        assert_eq!(result.unwrap_err(), "persist failed");
        assert_eq!(svc.get_materials().unwrap(), vec![existing]);
    }

    #[test]
    fn import_materials_atomic_replaces_presets_only_after_successful_persist() {
        let svc = ServiceContext::with_settings(AppSettings::default());
        let existing = material_preset("Existing");
        svc.save_material(existing.clone()).unwrap();

        let imported = vec![material_preset("First"), material_preset("Second")];
        let merged = import_materials_atomic(&svc, imported, |_| Ok(())).unwrap();

        let stored = svc.get_materials().unwrap();
        assert_eq!(stored.len(), 3);
        assert_eq!(stored, merged);
        assert_eq!(stored[0], existing);
        assert!(stored[1].id != stored[2].id);
        assert_ne!(stored[1].id, stored[0].id);
        assert_ne!(stored[2].id, stored[0].id);
    }

    #[test]
    fn import_macros_atomic_replaces_macros_only_after_successful_persist() {
        let svc = ServiceContext::with_settings(AppSettings::default());
        let existing = macro_def("Existing");
        svc.save_macro(existing.clone()).unwrap();

        let imported = vec![macro_def("First"), macro_def("Second")];
        let merged = import_macros_atomic(&svc, imported, |_| Ok(())).unwrap();

        let stored = svc.get_macros().unwrap();
        assert_eq!(stored.len(), 3);
        assert_eq!(stored, merged);
        assert_eq!(stored[0], existing);
        assert!(stored[1].id != stored[2].id);
        assert_ne!(stored[1].id, stored[0].id);
        assert_ne!(stored[2].id, stored[0].id);
    }
}

#[tauri::command]
pub fn set_grbl_setting(
    svc: State<'_, Arc<ServiceContext>>,
    key: GrblSettingId,
    value: f64,
) -> Result<(), String> {
    let mut guard = svc.session.lock().map_err(|e| format!("lock: {e}"))?;
    let session = guard.as_mut().ok_or("No active machine session")?;
    match session {
        beambench_service::runtime::MachineSessionHandle::Grbl(grbl) => grbl
            .send_setting(key, value)
            .map_err(|e| format!("Failed to set GRBL setting: {e}")),
        _ => Err("Only GRBL sessions support setting configuration".to_string()),
    }
}

#[tauri::command]
pub fn get_saved_positions(
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<Vec<beambench_core::SavedPosition>, String> {
    let guard = svc.settings.lock().map_err(|e| format!("lock: {e}"))?;
    Ok(guard.saved_positions.clone())
}

#[tauri::command]
pub fn save_position(
    svc: State<'_, Arc<ServiceContext>>,
    name: String,
    x: f64,
    y: f64,
    z: Option<f64>,
) -> Result<Vec<beambench_core::SavedPosition>, String> {
    let mut guard = svc.settings.lock().map_err(|e| format!("lock: {e}"))?;
    guard.saved_positions.push(beambench_core::SavedPosition {
        id: Uuid::new_v4().to_string(),
        name,
        x,
        y,
        z,
    });
    // Keep max 10 saved positions
    if guard.saved_positions.len() > 10 {
        guard.saved_positions.remove(0);
    }
    let positions = guard.saved_positions.clone();
    let settings = guard.clone();
    drop(guard);
    persist::save_settings(&settings).map_err(|e| format!("Failed to persist settings: {e}"))?;
    Ok(positions)
}

#[tauri::command]
pub fn delete_saved_position(
    svc: State<'_, Arc<ServiceContext>>,
    id: Option<String>,
    index: Option<usize>,
) -> Result<Vec<beambench_core::SavedPosition>, String> {
    let mut guard = svc.settings.lock().map_err(|e| format!("lock: {e}"))?;
    if let Some(id) = id {
        let before = guard.saved_positions.len();
        guard.saved_positions.retain(|position| position.id != id);
        if guard.saved_positions.len() == before {
            return Err("Position not found".to_string());
        }
    } else if let Some(index) = index {
        if index >= guard.saved_positions.len() {
            return Err("Position index out of bounds".to_string());
        }
        guard.saved_positions.remove(index);
    } else {
        return Err("Position id or index is required".to_string());
    }
    let positions = guard.saved_positions.clone();
    let settings = guard.clone();
    drop(guard);
    persist::save_settings(&settings).map_err(|e| format!("Failed to persist settings: {e}"))?;
    Ok(positions)
}

// Optimization is project-scoped. The former machine-scoped compatibility
// commands were removed; use `commands::project::set_optimization`.

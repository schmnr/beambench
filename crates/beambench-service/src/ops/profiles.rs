use std::fs;
use std::path::Path;

use beambench_common::{CameraAlignment, CameraCalibration};
use beambench_core::WorkspaceOrigin;
use beambench_core::{
    MachineProfile, MachineProfileId, QualityTestSettings, RuidaTableAxis, ScanningOffsetEntry,
    TransferMode,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::context::ServiceContext;
use crate::error::{ServiceError, ServiceResult};
use crate::events;
use crate::persist::persist_settings_to_disk;

pub const MACHINE_PROFILE_FILE_EXTENSION: &str = "bbprofile";
pub const MACHINE_PROFILE_FILE_FORMAT: &str = "beam-bench.machine-profile";
pub const MACHINE_PROFILE_FILE_FORMAT_VERSION: &str = "1.0";

/// Portable, versioned wrapper for sharing one machine profile.
///
/// Camera selection and calibration are deliberately excluded from exported
/// and imported profiles because those values describe a specific host and
/// physical camera setup rather than the machine itself.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MachineProfileFile {
    pub format: String,
    pub format_version: String,
    pub app_version: String,
    pub created_at: String,
    pub profile: MachineProfile,
}

#[derive(Debug, Clone)]
pub struct SaveProfileInput {
    pub profile_id: Option<MachineProfileId>,
    pub name: String,
    pub preset_id: Option<String>,
    pub preset_version: Option<u32>,
    pub bed_width_mm: f64,
    pub bed_height_mm: f64,
    pub max_speed_mm_min: f64,
    pub max_power_percent: f64,
    pub s_value_max: u32,
    pub homing_enabled: bool,
    pub default_baud_rate: u32,
    pub firmware_type: String,
    pub notes: String,
    pub selected_camera_id: Option<String>,
    pub camera_calibration: Option<CameraCalibration>,
    pub camera_alignment: Option<CameraAlignment>,
    pub origin: WorkspaceOrigin,
    pub laser_offset_x: f64,
    pub laser_offset_y: f64,
    pub enable_laser_offset: bool,
    pub swap_xy: bool,
    pub job_checklist: bool,
    pub frame_continuously: bool,
    pub laser_on_when_framing: bool,
    pub tab_pulse_width_ms: f64,
    pub cnc_machine: bool,
    pub use_constant_power: bool,
    pub emit_s_every_g1: bool,
    pub use_g0_for_overscan: bool,
    pub air_assist_on_gcode: String,
    pub air_assist_off_gcode: String,
    pub air_assist_on_delay_ms: u32,
    pub job_header_gcode: String,
    pub job_footer_gcode: String,
    pub transfer_mode: TransferMode,
    pub preferred_default_origin: Option<WorkspaceOrigin>,
    pub scanning_offsets: Vec<ScanningOffsetEntry>,
    pub enable_scanning_offset: bool,
    pub dot_width_mm: f64,
    pub enable_dot_width: bool,
    pub supports_z_moves: bool,
    pub z_move_feed_mm_min: f64,
    pub ruida_table_axis: RuidaTableAxis,
    pub enable_laser_fire_button: bool,
    pub default_fire_power_percent: f64,
    pub quality_test_settings: QualityTestSettings,
}

#[derive(Debug, Clone)]
pub enum ProfileLookup {
    Id(MachineProfileId),
    Name(String),
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MachineProfilePreset {
    pub id: &'static str,
    pub version: u32,
    pub name: &'static str,
    pub description: &'static str,
    pub advisory_text: Option<&'static str>,
    pub firmware_type: &'static str,
    pub default_baud_rate: u32,
    pub bed_width_mm: f64,
    pub bed_height_mm: f64,
    pub max_speed_mm_min: f64,
    pub max_power_percent: f64,
    pub s_value_max: u32,
    pub homing_enabled: bool,
    pub origin: WorkspaceOrigin,
    pub use_constant_power: bool,
    pub emit_s_every_g1: bool,
    pub use_g0_for_overscan: bool,
    pub air_assist_on_gcode: &'static str,
    pub air_assist_off_gcode: &'static str,
    pub air_assist_on_delay_ms: u32,
    pub job_header_gcode: &'static str,
    pub job_footer_gcode: &'static str,
    pub transfer_mode: TransferMode,
    pub preferred_default_origin: Option<WorkspaceOrigin>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ProfileFieldDiff {
    pub field: &'static str,
    pub old: serde_json::Value,
    pub new: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PresetSuggestion {
    pub suggestion: Option<&'static str>,
    pub reason: &'static str,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ApplyPresetResult {
    pub applied: bool,
    pub profile: MachineProfile,
    pub diff: Vec<ProfileFieldDiff>,
}

fn lock_err(name: &str, e: impl std::fmt::Display) -> ServiceError {
    ServiceError::internal(format!("Failed to lock {name}: {e}"))
}

fn portable_profile(mut profile: MachineProfile) -> MachineProfile {
    profile.selected_camera_id = None;
    profile.camera_calibration = None;
    profile.camera_alignment = None;
    profile
}

fn validate_imported_profile(profile: &MachineProfile) -> ServiceResult<()> {
    if profile.name.trim().is_empty() {
        return Err(ServiceError::invalid_input(
            "Machine profile name cannot be blank",
        ));
    }

    for (label, value) in [
        ("bed width", profile.bed_width_mm),
        ("bed height", profile.bed_height_mm),
        ("maximum speed", profile.max_speed_mm_min),
        ("Z move feed", profile.z_move_feed_mm_min),
    ] {
        if !value.is_finite() || value <= 0.0 {
            return Err(ServiceError::invalid_input(format!(
                "Machine profile {label} must be a positive finite number"
            )));
        }
    }

    for (label, value) in [
        ("maximum power", profile.max_power_percent),
        ("default fire power", profile.default_fire_power_percent),
    ] {
        if !value.is_finite() || !(0.0..=100.0).contains(&value) {
            return Err(ServiceError::invalid_input(format!(
                "Machine profile {label} must be between 0 and 100"
            )));
        }
    }

    if profile.s_value_max == 0 {
        return Err(ServiceError::invalid_input(
            "Machine profile S-value maximum must be greater than zero",
        ));
    }
    if profile.default_baud_rate == 0 {
        return Err(ServiceError::invalid_input(
            "Machine profile baud rate must be greater than zero",
        ));
    }

    for (label, value) in [
        ("laser X offset", profile.laser_offset_x),
        ("laser Y offset", profile.laser_offset_y),
    ] {
        if !value.is_finite() {
            return Err(ServiceError::invalid_input(format!(
                "Machine profile {label} must be finite"
            )));
        }
    }

    if !profile.dot_width_mm.is_finite() || profile.dot_width_mm < 0.0 {
        return Err(ServiceError::invalid_input(
            "Machine profile dot width must be a non-negative finite number",
        ));
    }
    if !profile.tab_pulse_width_ms.is_finite() || profile.tab_pulse_width_ms < 0.0 {
        return Err(ServiceError::invalid_input(
            "Machine profile tab pulse width must be a non-negative finite number",
        ));
    }

    for (index, entry) in profile.scanning_offsets.iter().enumerate() {
        if !entry.speed_mm_min.is_finite() || entry.speed_mm_min <= 0.0 {
            return Err(ServiceError::invalid_input(format!(
                "Machine profile scanning offset {} has an invalid speed",
                index + 1
            )));
        }
        if !entry.offset_mm.is_finite() {
            return Err(ServiceError::invalid_input(format!(
                "Machine profile scanning offset {} has an invalid offset",
                index + 1
            )));
        }
    }

    Ok(())
}

fn machine_profile_payload(profile: MachineProfile) -> MachineProfileFile {
    MachineProfileFile {
        format: MACHINE_PROFILE_FILE_FORMAT.to_string(),
        format_version: MACHINE_PROFILE_FILE_FORMAT_VERSION.to_string(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        created_at: Utc::now().to_rfc3339(),
        profile: portable_profile(profile),
    }
}

fn parse_machine_profile_payload(path: &Path) -> ServiceResult<MachineProfileFile> {
    let data = fs::read_to_string(path).map_err(|e| {
        ServiceError::persistence(format!("Failed to read machine profile file: {e}"))
    })?;
    let payload: MachineProfileFile = serde_json::from_str(&data).map_err(|e| {
        ServiceError::invalid_input(format!("Failed to parse machine profile file: {e}"))
    })?;
    if payload.format != MACHINE_PROFILE_FILE_FORMAT {
        return Err(ServiceError::invalid_input(format!(
            "Unsupported machine profile format '{}'",
            payload.format
        )));
    }
    if payload.format_version != MACHINE_PROFILE_FILE_FORMAT_VERSION {
        return Err(ServiceError::invalid_input(format!(
            "Unsupported machine profile version '{}'",
            payload.format_version
        )));
    }
    validate_imported_profile(&payload.profile)?;
    Ok(payload)
}

fn write_machine_profile_payload(path: &Path, payload: &MachineProfileFile) -> ServiceResult<()> {
    let json = serde_json::to_string_pretty(payload)
        .map_err(|e| ServiceError::internal(format!("Failed to serialize machine profile: {e}")))?;
    crate::persist::write_json_atomic(path, &json).map_err(|e| {
        ServiceError::persistence(format!("Failed to write machine profile file: {e}"))
    })
}

fn unique_imported_profile_name(name: &str, existing: &[MachineProfile]) -> String {
    let base = name.trim();
    if !existing.iter().any(|profile| profile.name == base) {
        return base.to_string();
    }

    let imported = format!("{base} (Imported)");
    if !existing.iter().any(|profile| profile.name == imported) {
        return imported;
    }

    let mut suffix = 2;
    loop {
        let candidate = format!("{base} (Imported {suffix})");
        if !existing.iter().any(|profile| profile.name == candidate) {
            return candidate;
        }
        suffix += 1;
    }
}

fn fresh_profile_id(existing: &[MachineProfile]) -> MachineProfileId {
    loop {
        let id = MachineProfileId::new();
        if !existing.iter().any(|profile| profile.id == id) {
            return id;
        }
    }
}

pub fn profile_presets() -> Vec<MachineProfilePreset> {
    vec![
        MachineProfilePreset {
            id: "sculpfun_s30_pro_max_20w",
            version: 1,
            name: "Sculpfun S30 Pro Max 20W",
            description: "Sculpfun S30 Pro Max 20W diode GRBL profile with M8 air assist.",
            advisory_text: Some(
                "Sculpfun S30 air assist uses M8. If M8 causes the controller to reset or disconnect, plug the laser directly into a wall outlet and avoid weak power strips or UPS units.",
            ),
            firmware_type: "grbl",
            default_baud_rate: 115200,
            bed_width_mm: 370.0,
            bed_height_mm: 360.0,
            max_speed_mm_min: 6000.0,
            max_power_percent: 100.0,
            s_value_max: 1000,
            homing_enabled: true,
            origin: WorkspaceOrigin::BottomLeft,
            use_constant_power: false,
            emit_s_every_g1: false,
            use_g0_for_overscan: true,
            air_assist_on_gcode: "M8",
            air_assist_off_gcode: "M9",
            air_assist_on_delay_ms: 0,
            job_header_gcode: "",
            job_footer_gcode: "",
            transfer_mode: TransferMode::Buffered,
            preferred_default_origin: Some(WorkspaceOrigin::BottomLeft),
        },
        MachineProfilePreset {
            id: "laserpecker_lx1",
            version: 1,
            name: "LaserPecker LX1",
            description: "Official-profile GRBL defaults for the LaserPecker LX1.",
            advisory_text: None,
            firmware_type: "grbl",
            default_baud_rate: 460800,
            bed_width_mm: 400.0,
            bed_height_mm: 420.0,
            max_speed_mm_min: 12000.0,
            max_power_percent: 100.0,
            s_value_max: 255,
            homing_enabled: true,
            origin: WorkspaceOrigin::TopLeft,
            use_constant_power: false,
            emit_s_every_g1: false,
            use_g0_for_overscan: false,
            air_assist_on_gcode: "",
            air_assist_off_gcode: "",
            air_assist_on_delay_ms: 0,
            job_header_gcode: "",
            job_footer_gcode: "",
            transfer_mode: TransferMode::Buffered,
            preferred_default_origin: Some(WorkspaceOrigin::TopLeft),
        },
        MachineProfilePreset {
            id: "laserpecker_lx1_max",
            version: 1,
            name: "LaserPecker LX1 Max",
            description: "Official-profile GRBL defaults for the extended LaserPecker LX1 Max.",
            advisory_text: None,
            firmware_type: "grbl",
            default_baud_rate: 460800,
            bed_width_mm: 400.0,
            bed_height_mm: 800.0,
            max_speed_mm_min: 12000.0,
            max_power_percent: 100.0,
            s_value_max: 1000,
            homing_enabled: true,
            origin: WorkspaceOrigin::TopLeft,
            use_constant_power: false,
            emit_s_every_g1: false,
            use_g0_for_overscan: false,
            air_assist_on_gcode: "",
            air_assist_off_gcode: "",
            air_assist_on_delay_ms: 0,
            job_header_gcode: "",
            job_footer_gcode: "",
            transfer_mode: TransferMode::Buffered,
            preferred_default_origin: Some(WorkspaceOrigin::TopLeft),
        },
        MachineProfilePreset {
            id: "laserpecker_lx2",
            version: 1,
            name: "LaserPecker LX2",
            description: "Official-profile GRBL/TCP defaults for the LaserPecker LX2.",
            advisory_text: None,
            firmware_type: "grbl",
            default_baud_rate: 115200,
            bed_width_mm: 500.0,
            bed_height_mm: 305.0,
            max_speed_mm_min: 36000.0,
            max_power_percent: 100.0,
            s_value_max: 1000,
            homing_enabled: true,
            origin: WorkspaceOrigin::TopLeft,
            use_constant_power: false,
            emit_s_every_g1: false,
            use_g0_for_overscan: true,
            air_assist_on_gcode: "",
            air_assist_off_gcode: "",
            air_assist_on_delay_ms: 0,
            job_header_gcode: "START_PRINT",
            job_footer_gcode: "",
            transfer_mode: TransferMode::Buffered,
            preferred_default_origin: Some(WorkspaceOrigin::TopLeft),
        },
        MachineProfilePreset {
            id: "laserpecker_lp2_plus",
            version: 1,
            name: "LaserPecker LP2 Plus",
            description: "Official-profile GRBL defaults for LaserPecker LP2 Plus regular mode.",
            advisory_text: None,
            firmware_type: "grbl",
            default_baud_rate: 460800,
            bed_width_mm: 100.0,
            bed_height_mm: 100.0,
            max_speed_mm_min: 24000.0,
            max_power_percent: 100.0,
            s_value_max: 255,
            homing_enabled: false,
            origin: WorkspaceOrigin::TopLeft,
            use_constant_power: false,
            emit_s_every_g1: false,
            use_g0_for_overscan: false,
            air_assist_on_gcode: "",
            air_assist_off_gcode: "",
            air_assist_on_delay_ms: 0,
            job_header_gcode: "M3030",
            job_footer_gcode: "",
            transfer_mode: TransferMode::Buffered,
            preferred_default_origin: Some(WorkspaceOrigin::TopLeft),
        },
        MachineProfilePreset {
            id: "laserpecker_lp4_450nm",
            version: 1,
            name: "LaserPecker LP4 / LP4 Safeguard - 450 nm",
            description: "Official-profile GRBL defaults for LP4 regular mode with the blue diode laser.",
            advisory_text: None,
            firmware_type: "grbl",
            default_baud_rate: 460800,
            bed_width_mm: 160.0,
            bed_height_mm: 120.0,
            max_speed_mm_min: 50000.0,
            max_power_percent: 100.0,
            s_value_max: 255,
            homing_enabled: false,
            origin: WorkspaceOrigin::TopLeft,
            use_constant_power: false,
            emit_s_every_g1: false,
            use_g0_for_overscan: false,
            air_assist_on_gcode: "",
            air_assist_off_gcode: "",
            air_assist_on_delay_ms: 0,
            job_header_gcode: "M3030\nM3010",
            job_footer_gcode: "",
            transfer_mode: TransferMode::Buffered,
            preferred_default_origin: Some(WorkspaceOrigin::TopLeft),
        },
        MachineProfilePreset {
            id: "laserpecker_lp4_1064nm",
            version: 1,
            name: "LaserPecker LP4 / LP4 Safeguard - 1064 nm",
            description: "Official-profile GRBL defaults for LP4 regular mode with the infrared laser.",
            advisory_text: None,
            firmware_type: "grbl",
            default_baud_rate: 460800,
            bed_width_mm: 160.0,
            bed_height_mm: 120.0,
            max_speed_mm_min: 50000.0,
            max_power_percent: 100.0,
            s_value_max: 255,
            homing_enabled: false,
            origin: WorkspaceOrigin::TopLeft,
            use_constant_power: false,
            emit_s_every_g1: false,
            use_g0_for_overscan: false,
            air_assist_on_gcode: "",
            air_assist_off_gcode: "",
            air_assist_on_delay_ms: 0,
            job_header_gcode: "M3030\nM3011",
            job_footer_gcode: "",
            transfer_mode: TransferMode::Buffered,
            preferred_default_origin: Some(WorkspaceOrigin::TopLeft),
        },
        MachineProfilePreset {
            id: "laserpecker_lp5_450nm",
            version: 1,
            name: "LaserPecker LP5 - 450 nm",
            description: "Official-profile GRBL defaults for LP5 regular mode with the blue diode laser.",
            advisory_text: None,
            firmware_type: "grbl",
            default_baud_rate: 460800,
            bed_width_mm: 120.0,
            bed_height_mm: 160.0,
            max_speed_mm_min: 600000.0,
            max_power_percent: 100.0,
            s_value_max: 255,
            homing_enabled: false,
            origin: WorkspaceOrigin::TopLeft,
            use_constant_power: false,
            emit_s_every_g1: false,
            use_g0_for_overscan: false,
            air_assist_on_gcode: "",
            air_assist_off_gcode: "",
            air_assist_on_delay_ms: 0,
            job_header_gcode: "M3030\nM3010",
            job_footer_gcode: "",
            transfer_mode: TransferMode::Buffered,
            preferred_default_origin: Some(WorkspaceOrigin::TopLeft),
        },
        MachineProfilePreset {
            id: "laserpecker_lp5_1064nm",
            version: 1,
            name: "LaserPecker LP5 - 1064 nm",
            description: "Official-profile GRBL defaults for LP5 regular mode with the fiber laser.",
            advisory_text: None,
            firmware_type: "grbl",
            default_baud_rate: 460800,
            bed_width_mm: 120.0,
            bed_height_mm: 160.0,
            max_speed_mm_min: 600000.0,
            max_power_percent: 100.0,
            s_value_max: 255,
            homing_enabled: false,
            origin: WorkspaceOrigin::TopLeft,
            use_constant_power: false,
            emit_s_every_g1: false,
            use_g0_for_overscan: false,
            air_assist_on_gcode: "",
            air_assist_off_gcode: "",
            air_assist_on_delay_ms: 0,
            job_header_gcode: "M3030\nM3011",
            job_footer_gcode: "",
            transfer_mode: TransferMode::Buffered,
            preferred_default_origin: Some(WorkspaceOrigin::TopLeft),
        },
        MachineProfilePreset {
            id: "generic_grbl_diode",
            version: 1,
            name: "Generic GRBL Diode",
            description: "Current-compatible generic GRBL diode defaults.",
            advisory_text: None,
            firmware_type: "grbl",
            default_baud_rate: 115200,
            bed_width_mm: 400.0,
            bed_height_mm: 400.0,
            max_speed_mm_min: 3000.0,
            max_power_percent: 100.0,
            s_value_max: 1000,
            homing_enabled: false,
            origin: WorkspaceOrigin::BottomLeft,
            use_constant_power: false,
            emit_s_every_g1: false,
            use_g0_for_overscan: true,
            air_assist_on_gcode: "M7",
            air_assist_off_gcode: "M9",
            air_assist_on_delay_ms: 0,
            job_header_gcode: "",
            job_footer_gcode: "",
            transfer_mode: TransferMode::Buffered,
            preferred_default_origin: None,
        },
    ]
}

fn find_preset(preset_id: &str) -> ServiceResult<MachineProfilePreset> {
    profile_presets()
        .into_iter()
        .find(|preset| preset.id == preset_id)
        .ok_or_else(|| ServiceError::not_found("Preset not found"))
}

fn push_diff<T: Serialize + PartialEq>(
    diff: &mut Vec<ProfileFieldDiff>,
    field: &'static str,
    old: &T,
    new: &T,
) {
    if old != new {
        diff.push(ProfileFieldDiff {
            field,
            old: serde_json::to_value(old).unwrap_or(serde_json::Value::Null),
            new: serde_json::to_value(new).unwrap_or(serde_json::Value::Null),
        });
    }
}

fn diff_for_profile(
    profile: &MachineProfile,
    preset: &MachineProfilePreset,
) -> Vec<ProfileFieldDiff> {
    let mut diff = Vec::new();
    push_diff(
        &mut diff,
        "preset_id",
        &profile.preset_id,
        &Some(preset.id.to_string()),
    );
    push_diff(
        &mut diff,
        "preset_version",
        &profile.preset_version,
        &Some(preset.version),
    );
    push_diff(
        &mut diff,
        "bed_width_mm",
        &profile.bed_width_mm,
        &preset.bed_width_mm,
    );
    push_diff(
        &mut diff,
        "bed_height_mm",
        &profile.bed_height_mm,
        &preset.bed_height_mm,
    );
    push_diff(
        &mut diff,
        "max_speed_mm_min",
        &profile.max_speed_mm_min,
        &preset.max_speed_mm_min,
    );
    push_diff(
        &mut diff,
        "max_power_percent",
        &profile.max_power_percent,
        &preset.max_power_percent,
    );
    push_diff(
        &mut diff,
        "s_value_max",
        &profile.s_value_max,
        &preset.s_value_max,
    );
    push_diff(
        &mut diff,
        "homing_enabled",
        &profile.homing_enabled,
        &preset.homing_enabled,
    );
    push_diff(
        &mut diff,
        "default_baud_rate",
        &profile.default_baud_rate,
        &preset.default_baud_rate,
    );
    push_diff(&mut diff, "origin", &profile.origin, &preset.origin);
    push_diff(
        &mut diff,
        "firmware_type",
        &profile.firmware_type,
        &preset.firmware_type.to_string(),
    );
    push_diff(
        &mut diff,
        "use_constant_power",
        &profile.use_constant_power,
        &preset.use_constant_power,
    );
    push_diff(
        &mut diff,
        "emit_s_every_g1",
        &profile.emit_s_every_g1,
        &preset.emit_s_every_g1,
    );
    push_diff(
        &mut diff,
        "use_g0_for_overscan",
        &profile.use_g0_for_overscan,
        &preset.use_g0_for_overscan,
    );
    push_diff(
        &mut diff,
        "air_assist_on_gcode",
        &profile.air_assist_on_gcode,
        &preset.air_assist_on_gcode.to_string(),
    );
    push_diff(
        &mut diff,
        "air_assist_off_gcode",
        &profile.air_assist_off_gcode,
        &preset.air_assist_off_gcode.to_string(),
    );
    push_diff(
        &mut diff,
        "air_assist_on_delay_ms",
        &profile.air_assist_on_delay_ms,
        &preset.air_assist_on_delay_ms,
    );
    push_diff(
        &mut diff,
        "job_header_gcode",
        &profile.job_header_gcode,
        &preset.job_header_gcode.to_string(),
    );
    push_diff(
        &mut diff,
        "job_footer_gcode",
        &profile.job_footer_gcode,
        &preset.job_footer_gcode.to_string(),
    );
    push_diff(
        &mut diff,
        "transfer_mode",
        &profile.transfer_mode,
        &preset.transfer_mode,
    );
    push_diff(
        &mut diff,
        "preferred_default_origin",
        &profile.preferred_default_origin,
        &preset.preferred_default_origin,
    );
    diff
}

fn apply_preset_fields(profile: &mut MachineProfile, preset: &MachineProfilePreset) {
    profile.preset_id = Some(preset.id.to_string());
    profile.preset_version = Some(preset.version);
    profile.bed_width_mm = preset.bed_width_mm;
    profile.bed_height_mm = preset.bed_height_mm;
    profile.max_speed_mm_min = preset.max_speed_mm_min;
    profile.max_power_percent = preset.max_power_percent;
    profile.s_value_max = preset.s_value_max;
    profile.homing_enabled = preset.homing_enabled;
    profile.default_baud_rate = preset.default_baud_rate;
    profile.origin = preset.origin;
    profile.firmware_type = preset.firmware_type.to_string();
    profile.use_constant_power = preset.use_constant_power;
    profile.emit_s_every_g1 = preset.emit_s_every_g1;
    profile.use_g0_for_overscan = preset.use_g0_for_overscan;
    profile.air_assist_on_gcode = preset.air_assist_on_gcode.to_string();
    profile.air_assist_off_gcode = preset.air_assist_off_gcode.to_string();
    profile.air_assist_on_delay_ms = preset.air_assist_on_delay_ms;
    profile.job_header_gcode = preset.job_header_gcode.to_string();
    profile.job_footer_gcode = preset.job_footer_gcode.to_string();
    profile.transfer_mode = preset.transfer_mode;
    profile.preferred_default_origin = preset.preferred_default_origin;
}

pub fn profile_preset_diff(
    ctx: &ServiceContext,
    profile_id: MachineProfileId,
    preset_id: &str,
) -> ServiceResult<Vec<ProfileFieldDiff>> {
    let profile = get_profile(ctx, ProfileLookup::Id(profile_id))?;
    let preset = find_preset(preset_id)?;
    Ok(diff_for_profile(&profile, &preset))
}

pub fn apply_profile_preset(
    ctx: &ServiceContext,
    profile_id: MachineProfileId,
    preset_id: &str,
) -> ServiceResult<ApplyPresetResult> {
    let preset = find_preset(preset_id)?;
    let mut settings = ctx.settings.lock().map_err(|e| lock_err("settings", e))?;
    let profile = settings
        .machine_profiles
        .iter_mut()
        .find(|p| p.id == profile_id)
        .ok_or_else(|| ServiceError::not_found("Profile not found"))?;
    let diff = diff_for_profile(profile, &preset);
    apply_preset_fields(profile, &preset);
    let result = profile.clone();
    let should_invalidate = settings.active_profile_id == Some(result.id);
    drop(settings);
    let project_workspace_synced = if should_invalidate {
        super::machine::sync_open_project_workspace_from_profile(ctx, &result, "profile_preset")?
    } else {
        false
    };
    if should_invalidate && !project_workspace_synced {
        super::planning::invalidate_plan_cache(ctx)?;
    }
    persist_settings_to_disk(ctx);
    ctx.emit_event(
        "profile.preset_applied",
        json!({
            "profile": events::profile_summary(&result),
            "preset_id": preset.id,
            "preset_version": preset.version,
            "diff": diff,
            "project_workspace_synced": project_workspace_synced,
        }),
    );
    Ok(ApplyPresetResult {
        applied: true,
        profile: result,
        diff,
    })
}

pub fn suggest_profile_preset(ctx: &ServiceContext) -> ServiceResult<PresetSuggestion> {
    let runtime = super::machine::runtime_state(ctx)?;
    if runtime.session_state == beambench_common::machine::SessionState::Disconnected {
        return Ok(PresetSuggestion {
            suggestion: None,
            reason: "not_connected",
        });
    }
    let Some(info) = runtime.controller_info else {
        return Ok(PresetSuggestion {
            suggestion: None,
            reason: "unknown_firmware",
        });
    };
    let haystack = info
        .values()
        .cloned()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    if haystack.trim().is_empty() {
        return Ok(PresetSuggestion {
            suggestion: None,
            reason: "unknown_firmware",
        });
    }
    let looks_like_sculpfun_s30 =
        haystack.contains("sculpfun") && (haystack.contains("s30") || haystack.contains("s 30"));
    if looks_like_sculpfun_s30 {
        Ok(PresetSuggestion {
            suggestion: Some("sculpfun_s30_pro_max_20w"),
            reason: "matched",
        })
    } else {
        Ok(PresetSuggestion {
            suggestion: None,
            reason: "no_match",
        })
    }
}

pub fn list_profiles(ctx: &ServiceContext) -> ServiceResult<Vec<MachineProfile>> {
    let settings = ctx.settings.lock().map_err(|e| lock_err("settings", e))?;
    Ok(settings.machine_profiles.clone())
}

pub fn get_active_profile_id(ctx: &ServiceContext) -> ServiceResult<Option<MachineProfileId>> {
    let settings = ctx.settings.lock().map_err(|e| lock_err("settings", e))?;
    Ok(settings.active_profile_id)
}

pub fn get_profile(ctx: &ServiceContext, lookup: ProfileLookup) -> ServiceResult<MachineProfile> {
    let settings = ctx.settings.lock().map_err(|e| lock_err("settings", e))?;
    let profile = match lookup {
        ProfileLookup::Id(id) => settings.machine_profiles.iter().find(|p| p.id == id),
        ProfileLookup::Name(name) => settings.machine_profiles.iter().find(|p| p.name == name),
    };
    profile
        .cloned()
        .ok_or_else(|| ServiceError::not_found("Profile not found"))
}

pub fn export_machine_profile_to_path(
    ctx: &ServiceContext,
    profile_id: MachineProfileId,
    path: &Path,
) -> ServiceResult<()> {
    let profile = get_profile(ctx, ProfileLookup::Id(profile_id))?;
    let payload = machine_profile_payload(profile);
    write_machine_profile_payload(path, &payload)?;
    ctx.emit_event(
        "profile.exported",
        json!({
            "profile_id": profile_id,
            "path": path.to_string_lossy().to_string(),
            "format_version": MACHINE_PROFILE_FILE_FORMAT_VERSION,
        }),
    );
    Ok(())
}

pub fn import_machine_profile_from_path(
    ctx: &ServiceContext,
    path: &Path,
) -> ServiceResult<MachineProfile> {
    // Parse and validate the complete untrusted payload before taking the
    // settings lock or changing application state.
    let payload = parse_machine_profile_payload(path)?;
    let mut imported = portable_profile(payload.profile);

    let mut settings = ctx.settings.lock().map_err(|e| lock_err("settings", e))?;
    imported.id = fresh_profile_id(&settings.machine_profiles);
    imported.name = unique_imported_profile_name(&imported.name, &settings.machine_profiles);
    super::output::normalize_scanning_offsets(&mut imported.scanning_offsets);

    let mut candidate = settings.clone();
    candidate.machine_profiles.push(imported.clone());
    // Persist the candidate before publishing it in memory. A disk failure
    // therefore leaves both the active profile and profile list untouched.
    crate::persist::save_settings(&candidate).map_err(|e| {
        ServiceError::persistence(format!("Failed to persist imported machine profile: {e}"))
    })?;
    *settings = candidate;
    drop(settings);

    ctx.emit_event(
        "profile.imported",
        json!({
            "profile": events::profile_summary(&imported),
            "path": path.to_string_lossy().to_string(),
            "source_app_version": payload.app_version,
            "format_version": payload.format_version,
        }),
    );
    Ok(imported)
}

pub fn save_profile(
    ctx: &ServiceContext,
    input: SaveProfileInput,
) -> ServiceResult<MachineProfile> {
    let mut settings = ctx.settings.lock().map_err(|e| lock_err("settings", e))?;
    let mut scanning_offsets = input.scanning_offsets;
    super::output::normalize_scanning_offsets(&mut scanning_offsets);
    let profile = MachineProfile {
        id: input.profile_id.unwrap_or_default(),
        name: input.name,
        preset_id: input.preset_id,
        preset_version: input.preset_version,
        bed_width_mm: input.bed_width_mm,
        bed_height_mm: input.bed_height_mm,
        max_speed_mm_min: input.max_speed_mm_min,
        max_power_percent: input.max_power_percent,
        s_value_max: input.s_value_max,
        homing_enabled: input.homing_enabled,
        default_baud_rate: input.default_baud_rate,
        firmware_type: input.firmware_type,
        notes: input.notes,
        selected_camera_id: input.selected_camera_id,
        camera_calibration: input.camera_calibration,
        camera_alignment: input.camera_alignment,
        origin: input.origin,
        laser_offset_x: input.laser_offset_x,
        laser_offset_y: input.laser_offset_y,
        enable_laser_offset: input.enable_laser_offset,
        swap_xy: input.swap_xy,
        job_checklist: input.job_checklist,
        frame_continuously: input.frame_continuously,
        laser_on_when_framing: input.laser_on_when_framing,
        tab_pulse_width_ms: input.tab_pulse_width_ms,
        cnc_machine: input.cnc_machine,
        use_constant_power: input.use_constant_power,
        emit_s_every_g1: input.emit_s_every_g1,
        use_g0_for_overscan: input.use_g0_for_overscan,
        air_assist_on_gcode: input.air_assist_on_gcode,
        air_assist_off_gcode: input.air_assist_off_gcode,
        air_assist_on_delay_ms: input.air_assist_on_delay_ms,
        job_header_gcode: input.job_header_gcode,
        job_footer_gcode: input.job_footer_gcode,
        transfer_mode: input.transfer_mode,
        preferred_default_origin: input.preferred_default_origin,
        scanning_offsets,
        enable_scanning_offset: input.enable_scanning_offset,
        dot_width_mm: input.dot_width_mm,
        enable_dot_width: input.enable_dot_width,
        supports_z_moves: input.supports_z_moves,
        z_move_feed_mm_min: input.z_move_feed_mm_min,
        ruida_table_axis: input.ruida_table_axis,
        enable_laser_fire_button: input.enable_laser_fire_button,
        default_fire_power_percent: input.default_fire_power_percent,
        quality_test_settings: input.quality_test_settings,
    };
    if let Some(existing) = settings
        .machine_profiles
        .iter_mut()
        .find(|p| p.id == profile.id)
    {
        *existing = profile.clone();
    } else {
        settings.machine_profiles.push(profile.clone());
    }
    let should_invalidate = settings.active_profile_id == Some(profile.id);
    let result = profile;
    drop(settings);
    if should_invalidate {
        super::planning::invalidate_plan_cache(ctx)?;
    }
    persist_settings_to_disk(ctx);
    ctx.emit_event(
        "profile.saved",
        json!({
            "profile": events::profile_summary(&result),
        }),
    );
    Ok(result)
}

pub fn delete_profile(ctx: &ServiceContext, profile_id: MachineProfileId) -> ServiceResult<()> {
    let mut settings = ctx.settings.lock().map_err(|e| lock_err("settings", e))?;
    let initial_len = settings.machine_profiles.len();
    settings.machine_profiles.retain(|p| p.id != profile_id);
    if settings.machine_profiles.len() == initial_len {
        return Err(ServiceError::not_found("Profile not found"));
    }
    let should_invalidate = settings.active_profile_id == Some(profile_id);
    if should_invalidate {
        settings.active_profile_id = None;
    }
    drop(settings);
    if should_invalidate {
        super::planning::invalidate_plan_cache(ctx)?;
    }
    persist_settings_to_disk(ctx);
    ctx.emit_event(
        "profile.deleted",
        json!({
            "profile_id": profile_id,
        }),
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use beambench_common::geometry::{Bounds, Point2D};
    use beambench_common::{CameraAlignmentSource, SimilarityTransform};
    use beambench_core::{AppSettings, Project, Workspace};
    use beambench_grbl::GrblSession;
    use beambench_planner::ExecutionPlan;
    use beambench_serial::MockSerialTransport;
    use chrono::Utc;
    use uuid::Uuid;

    use crate::test_support::PersistTestGuard;

    fn install_grbl_session_with_info(ctx: &ServiceContext, lines: &[&str]) {
        let mut transport = MockSerialTransport::new("mock");
        for line in lines {
            transport.enqueue_response(line);
        }
        let mut session = GrblSession::new(Box::new(transport));
        session.connect().unwrap();
        session.poll().unwrap();
        *ctx.session.lock().unwrap() =
            Some(crate::runtime::MachineSessionHandle::Grbl(session.into()));
    }

    fn sample_input() -> SaveProfileInput {
        SaveProfileInput {
            profile_id: None,
            name: "Alpha".to_string(),
            preset_id: None,
            preset_version: None,
            bed_width_mm: 300.0,
            bed_height_mm: 200.0,
            max_speed_mm_min: 4000.0,
            max_power_percent: 80.0,
            s_value_max: 1000,
            homing_enabled: true,
            default_baud_rate: 115200,
            firmware_type: "grbl".to_string(),
            notes: "notes".to_string(),
            selected_camera_id: None,
            camera_calibration: None,
            camera_alignment: None,
            origin: WorkspaceOrigin::default(),
            laser_offset_x: 0.0,
            laser_offset_y: 0.0,
            enable_laser_offset: false,
            swap_xy: false,
            job_checklist: false,
            frame_continuously: false,
            laser_on_when_framing: false,
            tab_pulse_width_ms: 0.0,
            cnc_machine: false,
            use_constant_power: false,
            emit_s_every_g1: false,
            use_g0_for_overscan: true,
            air_assist_on_gcode: "M7".to_string(),
            air_assist_off_gcode: "M9".to_string(),
            air_assist_on_delay_ms: 0,
            job_header_gcode: String::new(),
            job_footer_gcode: String::new(),
            transfer_mode: TransferMode::Buffered,
            preferred_default_origin: None,
            scanning_offsets: Vec::new(),
            enable_scanning_offset: false,
            dot_width_mm: 0.0,
            enable_dot_width: false,
            supports_z_moves: false,
            z_move_feed_mm_min: 300.0,
            ruida_table_axis: RuidaTableAxis::Disabled,
            enable_laser_fire_button: false,
            default_fire_power_percent: 1.0,
            quality_test_settings: QualityTestSettings::default(),
        }
    }

    fn sample_camera_calibration() -> CameraCalibration {
        CameraCalibration {
            image_width_px: 1920,
            image_height_px: 1080,
            transform: SimilarityTransform::default(),
            rmse_px: 0.5,
            quality_score: 0.9,
            solved_at: "2026-07-12T12:00:00Z".to_string(),
        }
    }

    fn sample_camera_alignment() -> CameraAlignment {
        CameraAlignment {
            transform: SimilarityTransform::default(),
            image_width_px: Some(1920),
            image_height_px: Some(1080),
            rmse_mm: 0.25,
            quality_score: 0.95,
            solved_at: "2026-07-12T12:00:00Z".to_string(),
            source: CameraAlignmentSource::SolvedPoints,
        }
    }

    fn test_profile_payload(profile: MachineProfile) -> MachineProfileFile {
        MachineProfileFile {
            format: MACHINE_PROFILE_FILE_FORMAT.to_string(),
            format_version: MACHINE_PROFILE_FILE_FORMAT_VERSION.to_string(),
            app_version: "test".to_string(),
            created_at: "2026-07-12T12:00:00Z".to_string(),
            profile,
        }
    }

    fn write_test_profile_file(path: &Path, payload: &MachineProfileFile) {
        fs::write(path, serde_json::to_string_pretty(payload).unwrap()).unwrap();
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
            warnings: vec![],
            failed_entries: vec![],
        }
    }

    #[test]
    fn machine_profile_file_roundtrip_is_portable_unique_and_inactive() {
        let _guard = PersistTestGuard::new();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("alpha.bbprofile");
        let mut original = MachineProfile {
            name: "Alpha".to_string(),
            bed_width_mm: 640.0,
            bed_height_mm: 440.0,
            max_speed_mm_min: 12_000.0,
            job_header_gcode: "T10M6".to_string(),
            selected_camera_id: Some("host-camera".to_string()),
            camera_calibration: Some(sample_camera_calibration()),
            camera_alignment: Some(sample_camera_alignment()),
            ..MachineProfile::default()
        };
        original.scanning_offsets = vec![
            ScanningOffsetEntry {
                speed_mm_min: 2000.0,
                offset_mm: 0.2,
            },
            ScanningOffsetEntry {
                speed_mm_min: 1000.0,
                offset_mm: 0.1,
            },
        ];
        let original_id = original.id;
        let settings = AppSettings {
            machine_profiles: vec![original.clone()],
            active_profile_id: Some(original_id),
            ..AppSettings::default()
        };
        let ctx = ServiceContext::with_settings(settings);

        export_machine_profile_to_path(&ctx, original_id, &path).unwrap();
        let mut payload: MachineProfileFile =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(payload.format, MACHINE_PROFILE_FILE_FORMAT);
        assert_eq!(payload.format_version, MACHINE_PROFILE_FILE_FORMAT_VERSION);
        assert!(!payload.app_version.is_empty());
        assert!(!payload.created_at.is_empty());
        assert!(payload.profile.selected_camera_id.is_none());
        assert!(payload.profile.camera_calibration.is_none());
        assert!(payload.profile.camera_alignment.is_none());

        // A hand-edited/shared file may try to carry another host's camera
        // state; import must strip it independently of export sanitization.
        payload.profile.selected_camera_id = Some("other-host-camera".to_string());
        payload.profile.camera_calibration = Some(sample_camera_calibration());
        payload.profile.camera_alignment = Some(sample_camera_alignment());
        write_test_profile_file(&path, &payload);

        let imported = import_machine_profile_from_path(&ctx, &path).unwrap();
        assert_ne!(imported.id, original_id);
        assert_eq!(imported.name, "Alpha (Imported)");
        assert_eq!(imported.job_header_gcode, "T10M6");
        assert!(imported.selected_camera_id.is_none());
        assert!(imported.camera_calibration.is_none());
        assert!(imported.camera_alignment.is_none());
        assert_eq!(imported.scanning_offsets[0].speed_mm_min, 1000.0);
        assert_eq!(imported.scanning_offsets[1].speed_mm_min, 2000.0);
        assert_eq!(get_active_profile_id(&ctx).unwrap(), Some(original_id));

        let imported_again = import_machine_profile_from_path(&ctx, &path).unwrap();
        assert_ne!(imported_again.id, original_id);
        assert_ne!(imported_again.id, imported.id);
        assert_eq!(imported_again.name, "Alpha (Imported 2)");
        assert_eq!(get_active_profile_id(&ctx).unwrap(), Some(original_id));

        let persisted = crate::persist::load_settings();
        assert_eq!(persisted.active_profile_id, Some(original_id));
        assert!(
            persisted
                .machine_profiles
                .iter()
                .any(|profile| profile.id == imported.id)
        );
        assert!(
            persisted
                .machine_profiles
                .iter()
                .any(|profile| profile.id == imported_again.id)
        );
    }

    #[test]
    fn machine_profile_export_overwrites_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("machine.bbprofile");
        fs::write(&path, "old contents").unwrap();
        let profile = MachineProfile {
            name: "Replacement".to_string(),
            ..MachineProfile::default()
        };
        let profile_id = profile.id;
        let ctx = ServiceContext::with_settings(AppSettings {
            machine_profiles: vec![profile],
            ..AppSettings::default()
        });

        export_machine_profile_to_path(&ctx, profile_id, &path).unwrap();

        let payload: MachineProfileFile =
            serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(payload.profile.name, "Replacement");
    }

    #[test]
    fn machine_profile_import_rejects_wrong_wrapper_before_mutating_settings() {
        let _guard = PersistTestGuard::new();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("invalid.bbprofile");
        let active = MachineProfile {
            name: "Active".to_string(),
            ..MachineProfile::default()
        };
        let active_id = active.id;
        let ctx = ServiceContext::with_settings(AppSettings {
            machine_profiles: vec![active],
            active_profile_id: Some(active_id),
            ..AppSettings::default()
        });

        let mut payload = test_profile_payload(MachineProfile::default());
        payload.format = "something-else".to_string();
        write_test_profile_file(&path, &payload);
        let format_error = import_machine_profile_from_path(&ctx, &path).unwrap_err();
        assert!(
            format_error
                .message
                .contains("Unsupported machine profile format")
        );

        payload.format = MACHINE_PROFILE_FILE_FORMAT.to_string();
        payload.format_version = "99.0".to_string();
        write_test_profile_file(&path, &payload);
        let version_error = import_machine_profile_from_path(&ctx, &path).unwrap_err();
        assert!(
            version_error
                .message
                .contains("Unsupported machine profile version")
        );

        payload.format_version = MACHINE_PROFILE_FILE_FORMAT_VERSION.to_string();
        payload.profile.bed_width_mm = 0.0;
        write_test_profile_file(&path, &payload);
        let profile_error = import_machine_profile_from_path(&ctx, &path).unwrap_err();
        assert!(profile_error.message.contains("bed width"));

        assert_eq!(list_profiles(&ctx).unwrap().len(), 1);
        assert_eq!(get_active_profile_id(&ctx).unwrap(), Some(active_id));
    }

    #[test]
    fn imported_machine_profile_validation_rejects_unsafe_values() {
        let invalid_profiles = vec![
            MachineProfile {
                name: "   ".to_string(),
                ..MachineProfile::default()
            },
            MachineProfile {
                bed_height_mm: -1.0,
                ..MachineProfile::default()
            },
            MachineProfile {
                max_speed_mm_min: f64::NAN,
                ..MachineProfile::default()
            },
            MachineProfile {
                max_power_percent: 100.1,
                ..MachineProfile::default()
            },
            MachineProfile {
                s_value_max: 0,
                ..MachineProfile::default()
            },
            MachineProfile {
                default_baud_rate: 0,
                ..MachineProfile::default()
            },
            MachineProfile {
                laser_offset_x: f64::INFINITY,
                ..MachineProfile::default()
            },
            MachineProfile {
                dot_width_mm: -0.1,
                ..MachineProfile::default()
            },
            MachineProfile {
                z_move_feed_mm_min: f64::NEG_INFINITY,
                ..MachineProfile::default()
            },
            MachineProfile {
                default_fire_power_percent: -0.1,
                ..MachineProfile::default()
            },
            MachineProfile {
                scanning_offsets: vec![ScanningOffsetEntry {
                    speed_mm_min: 0.0,
                    offset_mm: 0.1,
                }],
                ..MachineProfile::default()
            },
            MachineProfile {
                scanning_offsets: vec![ScanningOffsetEntry {
                    speed_mm_min: 1000.0,
                    offset_mm: f64::NAN,
                }],
                ..MachineProfile::default()
            },
        ];

        for profile in invalid_profiles {
            assert!(validate_imported_profile(&profile).is_err());
        }
    }

    #[test]
    fn create_update_roundtrip_by_id() {
        let ctx = ServiceContext::with_settings(AppSettings::default());
        let created = save_profile(&ctx, sample_input()).unwrap();
        let loaded = get_profile(&ctx, ProfileLookup::Id(created.id)).unwrap();
        assert_eq!(loaded.name, "Alpha");

        let mut update = sample_input();
        update.profile_id = Some(created.id);
        update.name = "Beta".to_string();
        let updated = save_profile(&ctx, update).unwrap();
        assert_eq!(updated.id, created.id);
        assert_eq!(updated.name, "Beta");

        let by_name = get_profile(&ctx, ProfileLookup::Name("Beta".to_string())).unwrap();
        assert_eq!(by_name.id, created.id);
    }

    #[test]
    fn preset_diff_and_apply_records_audit_metadata() {
        let ctx = ServiceContext::with_settings(AppSettings::default());
        let created = save_profile(&ctx, sample_input()).unwrap();

        let diff = profile_preset_diff(&ctx, created.id, "sculpfun_s30_pro_max_20w").unwrap();
        assert!(
            diff.iter()
                .any(|entry| entry.field == "air_assist_on_gcode" && entry.new == "M8")
        );

        let result = apply_profile_preset(&ctx, created.id, "sculpfun_s30_pro_max_20w").unwrap();
        assert!(result.applied);
        assert_eq!(
            result.profile.preset_id.as_deref(),
            Some("sculpfun_s30_pro_max_20w")
        );
        assert_eq!(result.profile.preset_version, Some(1));
        assert_eq!(result.profile.air_assist_on_gcode, "M8");

        let second_diff =
            profile_preset_diff(&ctx, created.id, "sculpfun_s30_pro_max_20w").unwrap();
        assert!(second_diff.is_empty());
    }

    #[test]
    fn laserpecker_presets_match_the_published_device_profiles() {
        let presets = profile_presets();
        let cases = [
            ("laserpecker_lx1", 460_800, 400.0, 420.0, 255, ""),
            ("laserpecker_lx1_max", 460_800, 400.0, 800.0, 1000, ""),
            (
                "laserpecker_lx2",
                115_200,
                500.0,
                305.0,
                1000,
                "START_PRINT",
            ),
            ("laserpecker_lp2_plus", 460_800, 100.0, 100.0, 255, "M3030"),
            (
                "laserpecker_lp4_450nm",
                460_800,
                160.0,
                120.0,
                255,
                "M3030\nM3010",
            ),
            (
                "laserpecker_lp4_1064nm",
                460_800,
                160.0,
                120.0,
                255,
                "M3030\nM3011",
            ),
            (
                "laserpecker_lp5_450nm",
                460_800,
                120.0,
                160.0,
                255,
                "M3030\nM3010",
            ),
            (
                "laserpecker_lp5_1064nm",
                460_800,
                120.0,
                160.0,
                255,
                "M3030\nM3011",
            ),
        ];

        for (id, baud, width, height, s_max, header) in cases {
            let preset = presets.iter().find(|preset| preset.id == id).unwrap();
            assert_eq!(preset.default_baud_rate, baud, "{id}");
            assert_eq!(preset.bed_width_mm, width, "{id}");
            assert_eq!(preset.bed_height_mm, height, "{id}");
            assert_eq!(preset.s_value_max, s_max, "{id}");
            assert_eq!(preset.job_header_gcode, header, "{id}");
            assert_eq!(preset.origin, WorkspaceOrigin::TopLeft, "{id}");
            assert_eq!(
                preset.preferred_default_origin,
                Some(WorkspaceOrigin::TopLeft),
                "{id}"
            );
        }
    }

    #[test]
    fn applying_laserpecker_preset_sets_top_left_origin_and_job_commands() {
        let ctx = ServiceContext::with_settings(AppSettings::default());
        let created = save_profile(&ctx, sample_input()).unwrap();

        let result = apply_profile_preset(&ctx, created.id, "laserpecker_lx2").unwrap();

        assert_eq!(result.profile.origin, WorkspaceOrigin::TopLeft);
        assert_eq!(result.profile.default_baud_rate, 115_200);
        assert_eq!(result.profile.job_header_gcode, "START_PRINT");
        assert_eq!(result.profile.s_value_max, 1000);
    }

    #[test]
    fn apply_active_preset_syncs_bound_project_bed_without_origin_hint() {
        let ctx = ServiceContext::with_settings(AppSettings::default());
        let created = save_profile(&ctx, sample_input()).unwrap();
        set_active_profile(&ctx, Some(created.id)).unwrap();
        let mut project = Project::new("bound");
        project.machine_profile_id = Some(created.id);
        project.machine_profile_snapshot = Some(created.snapshot());
        project.workspace = Workspace {
            bed_width_mm: created.bed_width_mm,
            bed_height_mm: created.bed_height_mm,
            origin: created.origin,
        };
        *ctx.project.lock().unwrap() = Some(project);

        apply_profile_preset(&ctx, created.id, "sculpfun_s30_pro_max_20w").unwrap();
        let project = ctx.project.lock().unwrap().as_ref().unwrap().clone();

        assert_eq!(project.workspace.bed_width_mm, 370.0);
        assert_eq!(project.workspace.bed_height_mm, 360.0);
        assert_eq!(project.workspace.origin, created.origin);
        assert!(project.dirty);
    }

    #[test]
    fn suggest_profile_preset_reports_not_connected_without_session() {
        let ctx = ServiceContext::with_settings(AppSettings::default());

        let suggestion = suggest_profile_preset(&ctx).unwrap();

        assert_eq!(
            suggestion,
            PresetSuggestion {
                suggestion: None,
                reason: "not_connected",
            }
        );
    }

    #[test]
    fn suggest_profile_preset_reports_unknown_firmware_without_build_info() {
        let ctx = ServiceContext::with_settings(AppSettings::default());
        install_grbl_session_with_info(&ctx, &[]);

        let suggestion = suggest_profile_preset(&ctx).unwrap();

        assert_eq!(
            suggestion,
            PresetSuggestion {
                suggestion: None,
                reason: "unknown_firmware",
            }
        );
    }

    #[test]
    fn suggest_profile_preset_matches_sculpfun_s30_build_info_only() {
        let ctx = ServiceContext::with_settings(AppSettings::default());
        install_grbl_session_with_info(
            &ctx,
            &[
                "Grbl 1.1h ['$' for help]",
                "[VER:Sculpfun S30 Pro Max 20W:]",
            ],
        );

        let suggestion = suggest_profile_preset(&ctx).unwrap();

        assert_eq!(
            suggestion,
            PresetSuggestion {
                suggestion: Some("sculpfun_s30_pro_max_20w"),
                reason: "matched",
            }
        );
    }

    #[test]
    fn suggest_profile_preset_does_not_match_generic_or_other_firmware() {
        for lines in [
            vec!["Grbl 1.1h ['$' for help]"],
            vec!["Grbl 1.1h ['$' for help]", "[VER:FluidNC v3.7.0:]"],
            vec!["Grbl 1.1h ['$' for help]", "[VER:Smoothie compatible:]"],
            vec!["Grbl 1.1h ['$' for help]", "[VER:garbage:]"],
        ] {
            let ctx = ServiceContext::with_settings(AppSettings::default());
            install_grbl_session_with_info(&ctx, &lines);

            let suggestion = suggest_profile_preset(&ctx).unwrap();

            assert_eq!(
                suggestion,
                PresetSuggestion {
                    suggestion: None,
                    reason: "no_match",
                }
            );
        }
    }

    #[test]
    fn unknown_preset_returns_not_found() {
        let ctx = ServiceContext::with_settings(AppSettings::default());
        let created = save_profile(&ctx, sample_input()).unwrap();
        let err = profile_preset_diff(&ctx, created.id, "missing-preset").unwrap_err();
        assert_eq!(err.code, crate::error::ServiceErrorCode::NotFound);
    }

    #[test]
    fn duplicate_id_updates_existing_profile() {
        let ctx = ServiceContext::with_settings(AppSettings::default());
        let created = save_profile(&ctx, sample_input()).unwrap();

        let mut update = sample_input();
        update.profile_id = Some(created.id);
        update.name = "Updated".to_string();
        save_profile(&ctx, update).unwrap();

        let profiles = list_profiles(&ctx).unwrap();
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].name, "Updated");
    }

    #[test]
    fn delete_active_profile_clears_activation() {
        let ctx = ServiceContext::with_settings(AppSettings::default());
        let created = save_profile(&ctx, sample_input()).unwrap();
        set_active_profile(&ctx, Some(created.id)).unwrap();

        delete_profile(&ctx, created.id).unwrap();

        assert!(get_active_profile_id(&ctx).unwrap().is_none());
    }

    #[test]
    fn activate_missing_profile_fails() {
        let ctx = ServiceContext::with_settings(AppSettings::default());
        let err = set_active_profile(&ctx, Some(MachineProfileId::new())).unwrap_err();
        assert_eq!(err.code, crate::error::ServiceErrorCode::NotFound);
    }

    #[test]
    fn save_profile_invalidates_plan_cache() {
        let ctx = ServiceContext::with_settings(AppSettings::default());
        let created = save_profile(&ctx, sample_input()).unwrap();
        set_active_profile(&ctx, Some(created.id)).unwrap();
        *ctx.plan_cache.lock().unwrap() = Some(dummy_plan());

        let mut update = sample_input();
        update.profile_id = Some(created.id);
        update.name = "Renamed".to_string();
        save_profile(&ctx, update).unwrap();

        assert!(ctx.plan_cache.lock().unwrap().is_none());
    }

    #[test]
    fn save_inactive_profile_does_not_invalidate_plan_cache() {
        let ctx = ServiceContext::with_settings(AppSettings::default());
        let active = save_profile(&ctx, sample_input()).unwrap();
        set_active_profile(&ctx, Some(active.id)).unwrap();
        *ctx.plan_cache.lock().unwrap() = Some(dummy_plan());

        let mut inactive = sample_input();
        inactive.name = "Inactive".to_string();
        save_profile(&ctx, inactive).unwrap();

        assert!(ctx.plan_cache.lock().unwrap().is_some());
    }

    #[test]
    fn set_active_profile_invalidates_plan_cache() {
        let ctx = ServiceContext::with_settings(AppSettings::default());
        let created = save_profile(&ctx, sample_input()).unwrap();
        *ctx.plan_cache.lock().unwrap() = Some(dummy_plan());

        set_active_profile(&ctx, Some(created.id)).unwrap();

        assert!(ctx.plan_cache.lock().unwrap().is_none());
    }

    #[test]
    fn delete_inactive_profile_does_not_invalidate_plan_cache() {
        let ctx = ServiceContext::with_settings(AppSettings::default());
        let active = save_profile(&ctx, sample_input()).unwrap();
        let mut inactive_input = sample_input();
        inactive_input.name = "Inactive".to_string();
        let inactive = save_profile(&ctx, inactive_input).unwrap();
        set_active_profile(&ctx, Some(active.id)).unwrap();
        *ctx.plan_cache.lock().unwrap() = Some(dummy_plan());

        delete_profile(&ctx, inactive.id).unwrap();

        assert!(ctx.plan_cache.lock().unwrap().is_some());
    }
}

pub fn set_active_profile(
    ctx: &ServiceContext,
    profile_id: Option<MachineProfileId>,
) -> ServiceResult<()> {
    let mut settings = ctx.settings.lock().map_err(|e| lock_err("settings", e))?;
    if let Some(id) = profile_id
        && !settings.machine_profiles.iter().any(|p| p.id == id)
    {
        return Err(ServiceError::not_found("Profile not found"));
    }
    settings.active_profile_id = profile_id;
    drop(settings);
    super::planning::invalidate_plan_cache(ctx)?;
    persist_settings_to_disk(ctx);
    match profile_id {
        Some(profile_id) => ctx.emit_event(
            "profile.activated",
            json!({
                "profile_id": profile_id,
            }),
        ),
        None => ctx.emit_event("profile.deactivated", json!({})),
    }
    Ok(())
}

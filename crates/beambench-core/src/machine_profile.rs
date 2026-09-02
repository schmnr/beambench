//! Machine profile definitions for laser engravers.

use crate::quality_test::QualityTestSettings;
use crate::workspace::WorkspaceOrigin;
use beambench_common::id::Id;
use beambench_common::markers::MachineProfileMarker;
use beambench_common::{CameraAlignment, CameraCalibration, ControllerSelection};
use serde::{Deserialize, Serialize};

/// Typed ID for machine profiles.
pub type MachineProfileId = Id<MachineProfileMarker>;

fn default_air_assist_on_gcode() -> String {
    "M7".to_string()
}

fn default_air_assist_off_gcode() -> String {
    "M9".to_string()
}

/// How GRBL G-code should be streamed to the controller.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferMode {
    /// Use GRBL's RX buffer window and keep the controller fed.
    #[default]
    Buffered,
    /// Send one line per acknowledgement. Much slower, intended for diagnostics.
    Synchronous,
}

/// Controller channel used for manual lift-table motion on Ruida machines.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuidaTableAxis {
    /// Do not expose lift-table controls until the operator selects the
    /// channel used by their machine.
    #[default]
    Disabled,
    /// The lift table is connected to the controller's Z channel.
    Z,
    /// The lift table is connected to the controller's U channel.
    U,
}

/// Mechanical style of a rotary attachment.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RotaryType {
    /// The workpiece rests on driven rollers. Output scaling is based on the
    /// driven roller diameter; object diameter only defines the usable wrap.
    #[default]
    Roller,
    /// The workpiece is held by a chuck. Output scaling is based on the
    /// workpiece diameter.
    Chuck,
}

/// Controller axis used for rotary motion on a G-code machine.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RotaryAxis {
    X,
    #[default]
    Y,
    /// Dedicated rotary interfaces, including the Genmitsu Kiosk, commonly
    /// expose the attachment as the controller's Z axis.
    Z,
}

/// Entry in the scanning-offset calibration table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScanningOffsetEntry {
    pub speed_mm_min: f64,
    pub offset_mm: f64,
}

fn default_use_g0_for_overscan() -> bool {
    true
}

fn default_z_move_feed_mm_min() -> f64 {
    300.0
}

fn default_fire_power_percent() -> f64 {
    1.0
}

fn default_rotary_mm_per_rotation() -> f64 {
    50.0
}

fn default_rotary_roller_diameter_mm() -> f64 {
    16.0
}

fn default_rotary_object_diameter_mm() -> f64 {
    75.0
}

/// Local connection details remembered for a machine profile.
///
/// USB bus addresses are intentionally excluded because they are not stable
/// device identities across reconnects or hosts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MachineConnectionPreference {
    Serial {
        port_name: String,
        baud_rate: u32,
        controller_selection: ControllerSelection,
    },
    Network {
        host: String,
        port: u16,
        controller_selection: ControllerSelection,
    },
}

/// A machine profile describing a laser engraver's capabilities.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MachineProfile {
    pub id: MachineProfileId,
    pub name: String,
    #[serde(default)]
    pub preset_id: Option<String>,
    #[serde(default)]
    pub preset_version: Option<u32>,
    pub bed_width_mm: f64,
    pub bed_height_mm: f64,
    pub max_speed_mm_min: f64,
    /// Effective XY acceleration used for duration estimates, in mm/s².
    /// Zero means unknown. GRBL profiles are synchronized from $120/$121.
    #[serde(default)]
    pub acceleration_mm_s2: f64,
    pub max_power_percent: f64,
    pub s_value_max: u32,
    pub homing_enabled: bool,
    pub default_baud_rate: u32,
    pub firmware_type: String,
    pub notes: String,
    /// Last successful serial or network endpoint for this profile.
    #[serde(default)]
    pub connection_preference: Option<MachineConnectionPreference>,
    #[serde(default)]
    pub selected_camera_id: Option<String>,
    #[serde(default)]
    pub camera_calibration: Option<CameraCalibration>,
    #[serde(default)]
    pub camera_alignment: Option<CameraAlignment>,
    #[serde(default)]
    pub origin: WorkspaceOrigin,
    #[serde(default)]
    pub laser_offset_x: f64,
    #[serde(default)]
    pub laser_offset_y: f64,
    #[serde(default)]
    pub enable_laser_offset: bool,
    #[serde(default)]
    pub swap_xy: bool,
    /// When enabled, show a preflight checklist before running jobs.
    #[serde(default)]
    pub job_checklist: bool,
    /// When enabled, frame operation repeats continuously.
    #[serde(default)]
    pub frame_continuously: bool,
    /// When enabled, framing runs with low laser power instead of motion-only.
    #[serde(default)]
    pub laser_on_when_framing: bool,
    /// Pulse width for tab generation in milliseconds.
    #[serde(default)]
    pub tab_pulse_width_ms: f64,
    /// Flag indicating this is a CNC machine (non-laser).
    #[serde(default)]
    pub cnc_machine: bool,
    /// Use constant power (M3) instead of dynamic power (M4).
    #[serde(default)]
    pub use_constant_power: bool,
    /// Repeat S value on every G1 line (for GRBL clones).
    #[serde(default)]
    pub emit_s_every_g1: bool,
    /// Use rapid G0 moves for overscan approach moves.
    #[serde(default = "default_use_g0_for_overscan")]
    pub use_g0_for_overscan: bool,
    /// G-code emitted when an air-enabled cut entry starts on GRBL output.
    #[serde(default = "default_air_assist_on_gcode")]
    pub air_assist_on_gcode: String,
    /// G-code emitted when air-enabled output ends on GRBL output.
    #[serde(default = "default_air_assist_off_gcode")]
    pub air_assist_off_gcode: String,
    /// Optional delay after every air-on emission, in milliseconds.
    #[serde(default)]
    pub air_assist_on_delay_ms: u32,
    /// G-code job header injected before planned job commands.
    #[serde(default)]
    pub job_header_gcode: String,
    /// G-code job footer injected after planned job commands.
    #[serde(default)]
    pub job_footer_gcode: String,
    /// Streaming mode for GRBL sessions.
    #[serde(default)]
    pub transfer_mode: TransferMode,
    /// Preferred default origin for new projects made with this profile.
    #[serde(default)]
    pub preferred_default_origin: Option<WorkspaceOrigin>,
    /// Speed-keyed scanning-offset calibration table.
    #[serde(default)]
    pub scanning_offsets: Vec<ScanningOffsetEntry>,
    /// Whether scanning-offset compensation is active.
    #[serde(default)]
    pub enable_scanning_offset: bool,
    /// Beam diameter in mm for dot-width correction.
    #[serde(default)]
    pub dot_width_mm: f64,
    /// Whether dot-width correction is active.
    #[serde(default)]
    pub enable_dot_width: bool,
    /// Whether the machine supports Z-axis motions (gates Focus Test live actions).
    #[serde(default)]
    pub supports_z_moves: bool,
    /// Controlled Z move feed rate for GRBL Z offsets and Focus Test Z sweeps.
    #[serde(default = "default_z_move_feed_mm_min")]
    pub z_move_feed_mm_min: f64,
    /// Ruida controller channel used for bounded manual lift-table jogging.
    /// This does not enable automated Z moves in jobs or Focus Test actions.
    #[serde(default)]
    pub ruida_table_axis: RuidaTableAxis,
    /// Whether the operator has explicitly enabled the manual fire button.
    #[serde(default)]
    pub enable_laser_fire_button: bool,
    /// Manual fire button power in percent (0-100). 1.0 means 1%.
    #[serde(default = "default_fire_power_percent")]
    pub default_fire_power_percent: f64,
    /// Persisted dialog state for Material/Focus/Interval quality-test tools.
    #[serde(default)]
    pub quality_test_settings: QualityTestSettings,
    /// Whether G-code output is currently mapped to a rotary attachment.
    #[serde(default)]
    pub rotary_enabled: bool,
    /// Roller or chuck mechanics.
    #[serde(default)]
    pub rotary_type: RotaryType,
    /// Controller axis receiving the rotary motion.
    #[serde(default)]
    pub rotary_axis: RotaryAxis,
    /// Commanded controller millimetres that rotate the attachment once.
    #[serde(default = "default_rotary_mm_per_rotation")]
    pub rotary_mm_per_rotation: f64,
    /// Diameter of the driven roller. Used only for roller scaling.
    #[serde(default = "default_rotary_roller_diameter_mm")]
    pub rotary_roller_diameter_mm: f64,
    /// Diameter of the workpiece. Used for chuck scaling and wrap dimensions.
    #[serde(default = "default_rotary_object_diameter_mm")]
    pub rotary_object_diameter_mm: f64,
    /// Invert rotary motion without mirroring the artwork itself.
    #[serde(default)]
    pub rotary_reverse_direction: bool,
}

impl Default for MachineProfile {
    fn default() -> Self {
        Self {
            id: MachineProfileId::new(),
            name: "Default Machine".to_string(),
            preset_id: None,
            preset_version: None,
            bed_width_mm: 200.0,
            bed_height_mm: 200.0,
            max_speed_mm_min: 3000.0,
            acceleration_mm_s2: 0.0,
            max_power_percent: 100.0,
            s_value_max: 1000,
            homing_enabled: false,
            default_baud_rate: 115200,
            firmware_type: "grbl".to_string(),
            notes: String::new(),
            connection_preference: None,
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
            air_assist_on_gcode: default_air_assist_on_gcode(),
            air_assist_off_gcode: default_air_assist_off_gcode(),
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
            z_move_feed_mm_min: default_z_move_feed_mm_min(),
            ruida_table_axis: RuidaTableAxis::Disabled,
            enable_laser_fire_button: false,
            default_fire_power_percent: default_fire_power_percent(),
            quality_test_settings: QualityTestSettings::default(),
            rotary_enabled: false,
            rotary_type: RotaryType::Roller,
            rotary_axis: RotaryAxis::Y,
            rotary_mm_per_rotation: default_rotary_mm_per_rotation(),
            rotary_roller_diameter_mm: default_rotary_roller_diameter_mm(),
            rotary_object_diameter_mm: default_rotary_object_diameter_mm(),
            rotary_reverse_direction: false,
        }
    }
}

/// Lightweight snapshot of a profile for embedding in plans/reports.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MachineProfileSnapshot {
    pub profile_id: MachineProfileId,
    pub profile_name: String,
    pub bed_width_mm: f64,
    pub bed_height_mm: f64,
    pub max_speed_mm_min: f64,
}

impl MachineProfile {
    /// Circumference represented by the rotary workspace.
    pub fn rotary_object_circumference_mm(&self) -> f64 {
        std::f64::consts::PI * self.rotary_object_diameter_mm
    }

    /// Controller-coordinate distance per millimetre of workpiece surface.
    pub fn rotary_command_scale(&self) -> Option<f64> {
        if !self.rotary_enabled || !self.rotary_mm_per_rotation.is_finite() {
            return None;
        }
        let reference_diameter = match self.rotary_type {
            RotaryType::Roller => self.rotary_roller_diameter_mm,
            RotaryType::Chuck => self.rotary_object_diameter_mm,
        };
        if !reference_diameter.is_finite()
            || reference_diameter <= 0.0
            || self.rotary_mm_per_rotation <= 0.0
        {
            return None;
        }
        let direction = if self.rotary_reverse_direction {
            -1.0
        } else {
            1.0
        };
        Some(direction * self.rotary_mm_per_rotation / (std::f64::consts::PI * reference_diameter))
    }

    /// Effective design surface while rotary mode is enabled. The physical
    /// gantry dimension remains unchanged on the linear axis, while the rotary
    /// dimension becomes one workpiece circumference.
    pub fn workspace_dimensions_mm(&self) -> (f64, f64) {
        if !self.rotary_enabled {
            return (self.bed_width_mm, self.bed_height_mm);
        }
        let circumference = self.rotary_object_circumference_mm();
        if !circumference.is_finite() || circumference <= 0.0 {
            return (self.bed_width_mm, self.bed_height_mm);
        }
        match self.rotary_axis {
            RotaryAxis::X => (circumference, self.bed_height_mm),
            RotaryAxis::Y | RotaryAxis::Z => (self.bed_width_mm, circumference),
        }
    }

    pub fn snapshot(&self) -> MachineProfileSnapshot {
        let (bed_width_mm, bed_height_mm) = self.workspace_dimensions_mm();
        MachineProfileSnapshot {
            profile_id: self.id,
            profile_name: self.name.clone(),
            bed_width_mm,
            bed_height_mm,
            max_speed_mm_min: self.max_speed_mm_min,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_profile_is_sensible() {
        let p = MachineProfile::default();
        assert_eq!(p.name, "Default Machine");
        assert_eq!(p.bed_width_mm, 200.0);
        assert_eq!(p.default_baud_rate, 115200);
        assert_eq!(p.firmware_type, "grbl");
        assert!(p.connection_preference.is_none());
        assert!(p.selected_camera_id.is_none());
        assert_eq!(p.z_move_feed_mm_min, 300.0);
        assert_eq!(p.ruida_table_axis, RuidaTableAxis::Disabled);
        assert!(!p.enable_laser_fire_button);
        assert_eq!(p.default_fire_power_percent, 1.0);
        assert!(!p.rotary_enabled);
        assert_eq!(p.rotary_axis, RotaryAxis::Y);
    }

    #[test]
    fn profile_serialization_roundtrip() {
        let p = MachineProfile {
            connection_preference: Some(MachineConnectionPreference::Network {
                host: "10.0.1.155".to_string(),
                port: 8080,
                controller_selection: ControllerSelection::KnownDriver {
                    driver: beambench_common::ControllerDriverId::GrblHal,
                },
            }),
            ..MachineProfile::default()
        };
        let json = serde_json::to_string(&p).unwrap();
        let restored: MachineProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(p, restored);
    }

    #[test]
    fn legacy_profile_without_connection_preference_still_loads() {
        let profile = MachineProfile::default();
        let mut json = serde_json::to_value(profile).unwrap();
        json.as_object_mut()
            .unwrap()
            .remove("connection_preference");

        let restored: MachineProfile = serde_json::from_value(json).unwrap();

        assert!(restored.connection_preference.is_none());
    }

    #[test]
    fn snapshot_captures_key_fields() {
        let p = MachineProfile {
            name: "Test Laser".to_string(),
            bed_width_mm: 300.0,
            bed_height_mm: 400.0,
            max_speed_mm_min: 5000.0,
            ..Default::default()
        };
        let snap = p.snapshot();
        assert_eq!(snap.profile_name, "Test Laser");
        assert_eq!(snap.bed_width_mm, 300.0);
        assert_eq!(snap.bed_height_mm, 400.0);
        assert_eq!(snap.max_speed_mm_min, 5000.0);
        assert_eq!(snap.profile_id, p.id);
    }

    #[test]
    fn roller_rotary_uses_roller_for_scale_and_object_for_workspace() {
        let p = MachineProfile {
            rotary_enabled: true,
            rotary_type: RotaryType::Roller,
            rotary_axis: RotaryAxis::Z,
            rotary_mm_per_rotation: 80.0,
            rotary_roller_diameter_mm: 20.0,
            rotary_object_diameter_mm: 100.0,
            ..Default::default()
        };

        assert!(
            (p.rotary_command_scale().unwrap() - 80.0 / (20.0 * std::f64::consts::PI)).abs() < 1e-9
        );
        let (width, height) = p.workspace_dimensions_mm();
        assert_eq!(width, p.bed_width_mm);
        assert!((height - 100.0 * std::f64::consts::PI).abs() < 1e-9);
    }

    #[test]
    fn chuck_rotary_reverse_produces_negative_scale() {
        let p = MachineProfile {
            rotary_enabled: true,
            rotary_type: RotaryType::Chuck,
            rotary_axis: RotaryAxis::X,
            rotary_mm_per_rotation: 40.0,
            rotary_object_diameter_mm: 50.0,
            rotary_reverse_direction: true,
            ..Default::default()
        };

        assert!(p.rotary_command_scale().unwrap() < 0.0);
        assert!((p.workspace_dimensions_mm().0 - 50.0 * std::f64::consts::PI).abs() < 1e-9);
    }

    // --- MachineProfile field tests ---

    #[test]
    fn machine_profile_p1_fields_roundtrip() {
        let mut p = MachineProfile::default();
        p.origin = WorkspaceOrigin::BottomLeft;
        p.laser_offset_x = 10.0;
        p.laser_offset_y = 5.0;
        p.enable_laser_offset = true;
        p.swap_xy = true;

        let json = serde_json::to_string(&p).unwrap();
        let restored: MachineProfile = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.origin, WorkspaceOrigin::BottomLeft);
        assert_eq!(restored.laser_offset_x, 10.0);
        assert_eq!(restored.laser_offset_y, 5.0);
        assert!(restored.enable_laser_offset);
        assert!(restored.swap_xy);
    }

    #[test]
    fn profile_with_current_required_fields_deserializes_optional_p1_defaults() {
        let json = r#"{
            "id": "00000000-0000-0000-0000-000000000001",
            "name": "Current Profile",
            "bed_width_mm": 200.0,
            "bed_height_mm": 200.0,
            "max_speed_mm_min": 3000.0,
            "max_power_percent": 100.0,
            "s_value_max": 1000,
            "homing_enabled": false,
            "default_baud_rate": 115200,
            "firmware_type": "grbl",
            "notes": "",
            "use_g0_for_overscan": true
        }"#;
        let restored: MachineProfile = serde_json::from_str(json).unwrap();
        assert_eq!(restored.origin, WorkspaceOrigin::BottomLeft);
        assert_eq!(restored.laser_offset_x, 0.0);
        assert_eq!(restored.laser_offset_y, 0.0);
        assert!(!restored.enable_laser_offset);
        assert!(!restored.swap_xy);
        assert_eq!(restored.air_assist_on_gcode, "M7");
        assert_eq!(restored.air_assist_off_gcode, "M9");
        assert_eq!(restored.transfer_mode, TransferMode::Buffered);
        assert!(restored.preset_id.is_none());
        assert!(!restored.enable_laser_fire_button);
        assert_eq!(restored.default_fire_power_percent, 1.0);
        assert_eq!(restored.ruida_table_axis, RuidaTableAxis::Disabled);
    }

    #[test]
    fn machine_profile_p1_field_defaults() {
        let p = MachineProfile::default();
        assert_eq!(p.origin, WorkspaceOrigin::BottomLeft);
        assert_eq!(p.laser_offset_x, 0.0);
        assert_eq!(p.laser_offset_y, 0.0);
        assert!(!p.enable_laser_offset);
        assert!(!p.swap_xy);
        assert_eq!(p.air_assist_on_gcode, "M7");
        assert_eq!(p.air_assist_off_gcode, "M9");
        assert_eq!(p.air_assist_on_delay_ms, 0);
        assert_eq!(p.transfer_mode, TransferMode::Buffered);
    }

    #[test]
    fn machine_profile_origin_serializes() {
        let mut p = MachineProfile::default();
        p.origin = WorkspaceOrigin::BottomLeft;
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("\"origin\":\"bottom_left\""));
    }

    #[test]
    fn machine_profile_laser_offset_persists() {
        let mut p = MachineProfile::default();
        p.laser_offset_x = 15.5;
        p.laser_offset_y = -7.2;
        p.enable_laser_offset = true;

        let json = serde_json::to_string(&p).unwrap();
        let restored: MachineProfile = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.laser_offset_x, 15.5);
        assert_eq!(restored.laser_offset_y, -7.2);
        assert!(restored.enable_laser_offset);
    }

    // --- Output-policy and calibration tests ---

    #[test]
    fn output_policy_fields_default_disabled() {
        let p = MachineProfile::default();
        assert!(!p.use_constant_power);
        assert!(!p.emit_s_every_g1);
        assert_eq!(p.s_value_max, 1000);
        assert!(p.use_g0_for_overscan);
        assert!(p.scanning_offsets.is_empty());
        assert!(!p.enable_scanning_offset);
        assert_eq!(p.dot_width_mm, 0.0);
        assert!(!p.enable_dot_width);
    }

    #[test]
    fn output_policy_fields_roundtrip() {
        let mut p = MachineProfile::default();
        p.use_constant_power = true;
        p.emit_s_every_g1 = true;
        p.s_value_max = 500;
        p.use_g0_for_overscan = false;
        p.scanning_offsets = vec![
            ScanningOffsetEntry {
                speed_mm_min: 1000.0,
                offset_mm: 0.05,
            },
            ScanningOffsetEntry {
                speed_mm_min: 3000.0,
                offset_mm: 0.15,
            },
        ];
        p.enable_scanning_offset = true;
        p.dot_width_mm = 0.1;
        p.enable_dot_width = true;

        let json = serde_json::to_string(&p).unwrap();
        let restored: MachineProfile = serde_json::from_str(&json).unwrap();

        assert!(restored.use_constant_power);
        assert!(restored.emit_s_every_g1);
        assert_eq!(restored.s_value_max, 500);
        assert!(!restored.use_g0_for_overscan);
        assert_eq!(restored.scanning_offsets.len(), 2);
        assert_eq!(restored.scanning_offsets[0].speed_mm_min, 1000.0);
        assert_eq!(restored.scanning_offsets[1].offset_mm, 0.15);
        assert!(restored.enable_scanning_offset);
        assert_eq!(restored.dot_width_mm, 0.1);
        assert!(restored.enable_dot_width);
    }

    #[test]
    fn missing_overscan_policy_defaults_to_g0() {
        let json = r#"{
            "id": "00000000-0000-0000-0000-000000000001",
            "name": "Missing Overscan",
            "bed_width_mm": 200.0,
            "bed_height_mm": 200.0,
            "max_speed_mm_min": 3000.0,
            "max_power_percent": 100.0,
            "s_value_max": 1000,
            "homing_enabled": false,
            "default_baud_rate": 115200,
            "firmware_type": "grbl",
            "notes": ""
        }"#;
        let restored: MachineProfile = serde_json::from_str(json).unwrap();
        assert!(restored.use_g0_for_overscan);
    }

    #[test]
    fn missing_current_output_policy_fields_fail() {
        let json = r#"{
            "id": "00000000-0000-0000-0000-000000000001",
            "name": "Incomplete Profile",
            "bed_width_mm": 200.0,
            "bed_height_mm": 200.0,
            "max_speed_mm_min": 3000.0,
            "max_power_percent": 100.0,
            "homing_enabled": false,
            "default_baud_rate": 115200,
            "firmware_type": "grbl",
            "notes": ""
        }"#;
        let err = serde_json::from_str::<MachineProfile>(json).unwrap_err();
        assert!(err.to_string().contains("s_value_max"));
    }

    #[test]
    fn machine_profile_swap_xy_persists() {
        let mut p = MachineProfile::default();
        p.swap_xy = true;

        let json = serde_json::to_string(&p).unwrap();
        let restored: MachineProfile = serde_json::from_str(&json).unwrap();

        assert!(restored.swap_xy);
    }
}

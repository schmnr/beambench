//! Shared machine-related types used across serial, GRBL, and streamer crates.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Supported controller families.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControllerFamily {
    #[default]
    Unknown,
    Gcode,
    Dsp,
    Galvo,
}

/// Supported controller models across families.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControllerModel {
    #[default]
    Unknown,
    Grbl,
    FluidNc,
    GrblHal,
    LaserPecker,
    Marlin,
    Snapmaker,
    Smoothieware,
    Ruida,
    LihuiyuM2Nano,
    Trocen,
    Topwisdom,
    Ezcad2,
    Ezcad2Lite,
    Bsl,
}

/// Product availability for a controller implementation or exact compatibility row.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControllerProductTier {
    #[default]
    Unavailable,
    Internal,
    Experimental,
    Beta,
    Supported,
}

/// Evidence maturity for a controller implementation or exact compatibility row.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControllerEvidenceState {
    #[default]
    Emulated,
    HardwareObserved,
    CommunityValidated,
    LabVendorVerified,
}

/// Transport kind used to reach a machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportKind {
    Serial,
    Tcp,
    Udp,
    UsbPacket,
}

/// Display and identity metadata for a discovered device.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DeviceIdentity {
    pub display_name: String,
    pub manufacturer: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub product: Option<String>,
    pub serial_number: Option<String>,
    #[serde(default)]
    pub vendor_id: Option<u16>,
    #[serde(default)]
    pub product_id: Option<u16>,
    pub port_name: Option<String>,
    pub host: Option<String>,
    #[serde(default)]
    pub tcp_port: Option<u16>,
    #[serde(default)]
    pub udp_port: Option<u16>,
    pub usb_path: Option<String>,
}

/// Capability summary surfaced across UI/API/CLI.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DeviceCapabilities {
    pub can_home: bool,
    pub can_jog: bool,
    /// Press-and-hold (continuous) jogging with a cancel command. Controllers
    /// without it fall back to finite jog steps in the UI.
    #[serde(default)]
    pub can_jog_continuous: bool,
    pub can_unlock: bool,
    pub can_pause_resume: bool,
    pub can_set_origin: bool,
    pub can_frame: bool,
    pub can_run_job: bool,
    /// The controller reports a trustworthy absolute position; when false the
    /// UI must not present dead-reckoned placeholders as measured positions.
    #[serde(default)]
    pub reports_absolute_position: bool,
    /// Manual laser fire pulses (GCode/GRBL protocol surface only).
    #[serde(default)]
    pub can_manual_fire: bool,
    /// Realtime feed/spindle/rapid override commands during a job.
    #[serde(default)]
    pub can_adjust_overrides: bool,
    pub supports_rotary: bool,
    pub supports_cylinder: bool,
    pub supports_camera_alignment: bool,
}

impl DeviceCapabilities {
    /// A fail-closed capability set for an unknown or unavailable controller.
    pub const fn disabled() -> Self {
        Self {
            can_home: false,
            can_jog_continuous: false,
            can_jog: false,
            can_unlock: false,
            can_pause_resume: false,
            can_set_origin: false,
            can_frame: false,
            can_run_job: false,
            reports_absolute_position: false,
            can_manual_fire: false,
            can_adjust_overrides: false,
            supports_rotary: false,
            supports_cylinder: false,
            supports_camera_alignment: false,
        }
    }

    /// The capabilities exposed by Beam Bench's existing GRBL integration.
    ///
    /// This is intentionally explicit so adding another controller never inherits
    /// GRBL's live actions through `Default`.
    pub const fn legacy_grbl() -> Self {
        Self {
            can_home: true,
            can_jog_continuous: true,
            can_jog: true,
            can_unlock: true,
            can_pause_resume: true,
            can_set_origin: true,
            can_frame: true,
            can_run_job: true,
            reports_absolute_position: true,
            can_manual_fire: true,
            can_adjust_overrides: true,
            supports_rotary: false,
            supports_cylinder: false,
            supports_camera_alignment: false,
        }
    }

    /// Shared GRBL protocol capabilities exposed after an explicit Experimental
    /// compatibility choice. Homing stays disabled until a named driver can
    /// verify the controller-specific homing contract; the remaining actions
    /// use the common streamed-G-code protocol surface.
    pub const fn experimental_grbl_compatible() -> Self {
        Self {
            can_home: false,
            can_jog_continuous: true,
            can_jog: true,
            can_unlock: true,
            can_pause_resume: true,
            can_set_origin: true,
            can_frame: true,
            can_run_job: true,
            reports_absolute_position: true,
            can_manual_fire: true,
            can_adjust_overrides: true,
            supports_rotary: false,
            supports_cylinder: false,
            supports_camera_alignment: false,
        }
    }

    /// Full shared GRBL-family actions for an exactly identified named adapter.
    ///
    /// Product tier and hardware evidence are reported separately. Experimental
    /// status therefore does not remove normal protocol actions merely because
    /// beta hardware has not exercised every machine configuration yet.
    pub const fn experimental_named_grbl_family() -> Self {
        Self::legacy_grbl()
    }

    /// Conservative actions implemented by acknowledgement-based G-code
    /// adapters before controller-specific manual motion is added.
    pub const fn experimental_acknowledged_gcode() -> Self {
        Self {
            can_home: false,
            can_jog_continuous: false,
            can_jog: false,
            can_unlock: false,
            can_pause_resume: false,
            can_set_origin: false,
            can_frame: true,
            can_run_job: true,
            reports_absolute_position: false,
            can_manual_fire: false,
            can_adjust_overrides: false,
            supports_rotary: false,
            supports_cylinder: false,
            supports_camera_alignment: false,
        }
    }
}

/// Request-scoped manual discovery target for TCP probing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiscoveryTcpTarget {
    pub host: String,
    pub port: u16,
    pub label: Option<String>,
}

/// Request-scoped manual discovery target for USB packet probing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiscoveryUsbTarget {
    pub device_path: String,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
}

/// Candidate discovered by the backend discovery system.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiscoveryCandidate {
    pub id: String,
    pub controller_family: ControllerFamily,
    pub controller_model: ControllerModel,
    pub transport_kind: TransportKind,
    pub identity: DeviceIdentity,
    pub confidence: f64,
    pub capabilities: DeviceCapabilities,
    /// `None` represents legacy availability that has not yet been normalized.
    #[serde(default)]
    pub product_tier: Option<ControllerProductTier>,
    /// `None` represents legacy evidence that has not yet been normalized.
    #[serde(default)]
    pub evidence_state: Option<ControllerEvidenceState>,
    pub status_text: String,
    pub unsupported_reason: Option<String>,
}

/// Coarse discovery state for UI/API polling and event snapshots.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscoveryPhase {
    Idle,
    Scanning,
    Completed,
    Cancelled,
}

/// In-memory discovery scan state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiscoveryScanState {
    pub phase: DiscoveryPhase,
    pub status_text: String,
    pub candidates: Vec<DiscoveryCandidate>,
    pub scanned_serial_count: usize,
    pub scanned_tcp_count: usize,
    pub scanned_usb_count: usize,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

impl Default for DiscoveryScanState {
    fn default() -> Self {
        Self {
            phase: DiscoveryPhase::Idle,
            status_text: "Waiting for connection...".to_string(),
            candidates: Vec::new(),
            scanned_serial_count: 0,
            scanned_tcp_count: 0,
            scanned_usb_count: 0,
            started_at: None,
            completed_at: None,
        }
    }
}

/// Connection target accepted by the controller-neutral runtime.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MachineConnectionTarget {
    GrblSerial { port_name: String, baud_rate: u32 },
    DiscoveryCandidate { candidate_id: String },
}

/// Stable ID helper for discovery scans and candidate generation.
pub fn new_discovery_id() -> String {
    Uuid::new_v4().to_string()
}

/// Overall session lifecycle state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    #[default]
    Disconnected,
    Connecting,
    TransportOpen,
    WaitingForBanner,
    Validating,
    Ready,
    Running,
    Paused,
    Alarm,
    Error,
}

/// GRBL machine run state parsed from status reports.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MachineRunState {
    #[default]
    Idle,
    Run,
    Hold,
    Jog,
    Home,
    Alarm,
    Door,
    Sleep,
    Check,
    Unknown,
}

/// 3-axis position in machine or work coordinates.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct MachinePosition {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

/// Full machine status parsed from a GRBL `?` status report.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MachineStatus {
    pub run_state: MachineRunState,
    pub machine_position: MachinePosition,
    pub work_position: MachinePosition,
    pub feed_rate: f64,
    pub spindle_speed: f64,
    pub feed_override: u8,
    pub spindle_override: u8,
    pub rapid_override: u8,
    pub pin_states: String,
}

/// Job lifecycle state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    #[default]
    Idle,
    Preparing,
    ReadyToRun,
    Running,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

/// Snapshot of job streaming progress.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct JobProgress {
    pub state: JobState,
    pub total_lines: usize,
    pub queued_lines: usize,
    pub sent_lines: usize,
    pub acknowledged_lines: usize,
    pub elapsed_secs: f64,
    pub estimated_remaining_secs: f64,
    pub buffer_fill_bytes: usize,
    #[serde(default)]
    pub error_message: Option<String>,
    #[serde(default)]
    pub buckets: Vec<JobProgressBucket>,
}

/// Per-layer / per-cut-entry progress bucket metadata for multi-entry jobs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobProgressBucket {
    pub layer_id: String,
    pub cut_entry_id: String,
    pub segment_count: usize,
}

/// Information about an available serial port.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortInfo {
    pub port_name: String,
    pub description: String,
    pub manufacturer: String,
    pub vid: Option<u16>,
    pub pid: Option<u16>,
}

/// Overall outcome of a preflight check.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreflightOutcome {
    #[default]
    Pass,
    PassWithWarnings,
    Fail,
}

/// A single preflight check result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreflightCheck {
    pub category: String,
    pub description: String,
    pub passed: bool,
    pub message: String,
}

/// Complete preflight report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreflightReport {
    pub outcome: PreflightOutcome,
    pub checks: Vec<PreflightCheck>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_controller_identity_is_the_default() {
        assert_eq!(ControllerFamily::default(), ControllerFamily::Unknown);
        assert_eq!(ControllerModel::default(), ControllerModel::Unknown);
    }

    #[test]
    fn marlin_controller_model_has_a_stable_wire_name() {
        assert_eq!(
            serde_json::to_string(&ControllerModel::Marlin).unwrap(),
            r#""marlin""#
        );
    }

    #[test]
    fn snapmaker_controller_model_has_a_stable_wire_name() {
        assert_eq!(
            serde_json::to_string(&ControllerModel::Snapmaker).unwrap(),
            r#""snapmaker""#
        );
    }

    #[test]
    fn smoothieware_controller_model_has_a_stable_wire_name() {
        assert_eq!(
            serde_json::to_string(&ControllerModel::Smoothieware).unwrap(),
            r#""smoothieware""#
        );
    }

    #[test]
    fn laserpecker_controller_model_has_a_stable_wire_name() {
        assert_eq!(
            serde_json::to_string(&ControllerModel::LaserPecker).unwrap(),
            r#""laser_pecker""#
        );
    }

    #[test]
    fn controller_product_and_evidence_vocabulary_uses_snake_case() {
        assert_eq!(
            serde_json::to_string(&ControllerProductTier::Experimental).unwrap(),
            r#""experimental""#
        );
        assert_eq!(
            serde_json::to_string(&ControllerEvidenceState::HardwareObserved).unwrap(),
            r#""hardware_observed""#
        );
        assert_eq!(
            serde_json::to_string(&ControllerEvidenceState::LabVendorVerified).unwrap(),
            r#""lab_vendor_verified""#
        );
    }

    #[test]
    fn legacy_discovery_candidate_defaults_new_metadata_to_unnormalized() {
        let candidate: DiscoveryCandidate = serde_json::from_value(serde_json::json!({
            "id": "legacy-grbl",
            "controller_family": "gcode",
            "controller_model": "grbl",
            "transport_kind": "serial",
            "identity": {
                "display_name": "Legacy GRBL",
                "manufacturer": null,
                "serial_number": null,
                "port_name": "/dev/ttyUSB0",
                "host": null,
                "usb_path": null
            },
            "confidence": 0.75,
            "capabilities": DeviceCapabilities::legacy_grbl(),
            "status_text": "Found /dev/ttyUSB0",
            "unsupported_reason": null
        }))
        .unwrap();

        assert_eq!(candidate.product_tier, None);
        assert_eq!(candidate.evidence_state, None);
        assert_eq!(candidate.identity.description, None);
        assert_eq!(candidate.identity.product, None);
        assert_eq!(candidate.identity.vendor_id, None);
        assert_eq!(candidate.identity.product_id, None);
        assert_eq!(candidate.identity.tcp_port, None);

        let serialized = serde_json::to_value(candidate).unwrap();
        assert_eq!(serialized["product_tier"], serde_json::Value::Null);
        assert_eq!(serialized["evidence_state"], serde_json::Value::Null);
    }

    #[test]
    fn device_capabilities_default_is_disabled() {
        let capabilities = DeviceCapabilities::disabled();
        assert_eq!(DeviceCapabilities::default(), capabilities);
        assert!(!capabilities.can_home);
        assert!(!capabilities.can_jog);
        assert!(!capabilities.can_unlock);
        assert!(!capabilities.can_pause_resume);
        assert!(!capabilities.can_set_origin);
        assert!(!capabilities.can_frame);
        assert!(!capabilities.can_run_job);
        assert!(!capabilities.supports_rotary);
        assert!(!capabilities.supports_cylinder);
        assert!(!capabilities.supports_camera_alignment);
    }

    #[test]
    fn legacy_grbl_capabilities_are_explicitly_enabled() {
        let capabilities = DeviceCapabilities::legacy_grbl();
        assert!(capabilities.can_home);
        assert!(capabilities.can_jog);
        assert!(capabilities.can_unlock);
        assert!(capabilities.can_pause_resume);
        assert!(capabilities.can_set_origin);
        assert!(capabilities.can_frame);
        assert!(capabilities.can_run_job);
        assert!(!capabilities.supports_rotary);
        assert!(!capabilities.supports_cylinder);
        assert!(!capabilities.supports_camera_alignment);
    }

    #[test]
    fn experimental_grbl_compatible_capabilities_keep_homing_disabled() {
        let capabilities = DeviceCapabilities::experimental_grbl_compatible();
        assert!(!capabilities.can_home);
        assert!(capabilities.can_jog);
        assert!(capabilities.can_unlock);
        assert!(capabilities.can_pause_resume);
        assert!(capabilities.can_set_origin);
        assert!(capabilities.can_frame);
        assert!(capabilities.can_run_job);
        assert!(!capabilities.supports_rotary);
        assert!(!capabilities.supports_cylinder);
        assert!(!capabilities.supports_camera_alignment);
    }

    #[test]
    fn exact_experimental_grbl_family_exposes_the_shared_protocol_actions() {
        let capabilities = DeviceCapabilities::experimental_named_grbl_family();
        assert!(capabilities.can_home);
        assert!(capabilities.can_jog);
        assert!(capabilities.can_unlock);
        assert!(capabilities.can_pause_resume);
        assert!(capabilities.can_set_origin);
        assert!(capabilities.can_frame);
        assert!(capabilities.can_run_job);
    }

    #[test]
    fn legacy_grbl_capability_wire_shape_is_stable() {
        assert_eq!(
            serde_json::to_value(DeviceCapabilities::legacy_grbl()).unwrap(),
            serde_json::json!({
                "can_home": true,
                "can_jog": true,
                "can_jog_continuous": true,
                "can_unlock": true,
                "can_pause_resume": true,
                "can_set_origin": true,
                "can_frame": true,
                "can_run_job": true,
                "reports_absolute_position": true,
                "can_manual_fire": true,
                "can_adjust_overrides": true,
                "supports_rotary": false,
                "supports_cylinder": false,
                "supports_camera_alignment": false
            })
        );
    }

    #[test]
    fn session_state_default_is_disconnected() {
        assert_eq!(SessionState::default(), SessionState::Disconnected);
    }

    #[test]
    fn machine_run_state_default_is_idle() {
        assert_eq!(MachineRunState::default(), MachineRunState::Idle);
    }

    #[test]
    fn job_state_default_is_idle() {
        assert_eq!(JobState::default(), JobState::Idle);
    }

    #[test]
    fn machine_status_serialization_roundtrip() {
        let status = MachineStatus {
            run_state: MachineRunState::Run,
            machine_position: MachinePosition {
                x: 10.0,
                y: 20.0,
                z: 0.0,
            },
            work_position: MachinePosition {
                x: 5.0,
                y: 15.0,
                z: 0.0,
            },
            feed_rate: 1000.0,
            spindle_speed: 500.0,
            feed_override: 100,
            spindle_override: 100,
            rapid_override: 100,
            pin_states: String::new(),
        };
        let json = serde_json::to_string(&status).unwrap();
        let restored: MachineStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(status, restored);
    }

    #[test]
    fn job_progress_serialization_roundtrip() {
        let progress = JobProgress {
            state: JobState::Running,
            total_lines: 100,
            queued_lines: 50,
            sent_lines: 30,
            acknowledged_lines: 25,
            elapsed_secs: 10.5,
            estimated_remaining_secs: 30.0,
            buffer_fill_bytes: 64,
            error_message: Some("test failure".to_string()),
            buckets: vec![JobProgressBucket {
                layer_id: "layer-1".to_string(),
                cut_entry_id: "entry-1".to_string(),
                segment_count: 3,
            }],
        };
        let json = serde_json::to_string(&progress).unwrap();
        let restored: JobProgress = serde_json::from_str(&json).unwrap();
        assert_eq!(progress, restored);
    }

    #[test]
    fn port_info_serialization_roundtrip() {
        let port = PortInfo {
            port_name: "/dev/ttyUSB0".to_string(),
            description: "USB Serial".to_string(),
            manufacturer: "FTDI".to_string(),
            vid: Some(0x0403),
            pid: Some(0x6001),
        };
        let json = serde_json::to_string(&port).unwrap();
        let restored: PortInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(port, restored);
    }

    #[test]
    fn preflight_report_serialization_roundtrip() {
        let report = PreflightReport {
            outcome: PreflightOutcome::PassWithWarnings,
            checks: vec![PreflightCheck {
                category: "connection".to_string(),
                description: "Session is ready".to_string(),
                passed: true,
                message: "Connected".to_string(),
            }],
        };
        let json = serde_json::to_string(&report).unwrap();
        let restored: PreflightReport = serde_json::from_str(&json).unwrap();
        assert_eq!(report, restored);
    }

    #[test]
    fn session_state_serde_snake_case() {
        let json = serde_json::to_string(&SessionState::WaitingForBanner).unwrap();
        assert_eq!(json, r#""waiting_for_banner""#);
    }

    #[test]
    fn discovery_state_defaults_to_idle() {
        let state = DiscoveryScanState::default();
        assert_eq!(state.phase, DiscoveryPhase::Idle);
        assert!(state.candidates.is_empty());
    }
}

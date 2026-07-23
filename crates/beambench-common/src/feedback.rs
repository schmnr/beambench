use std::sync::LazyLock;

use crate::console::ConsoleEntry;
use crate::machine::{JobProgress, MachineRunState, SessionState};
use regex::Regex;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

pub const FEEDBACK_SCHEMA_VERSION: u32 = 1;
pub const MAX_FEEDBACK_TITLE_CHARS: usize = 200;
pub const MAX_FEEDBACK_DESCRIPTION_CHARS: usize = 10_000;
pub const MAX_FEEDBACK_REPLY_TO_EMAIL_CHARS: usize = 320;
pub const MAX_PROJECT_ATTACHMENT_RAW_BYTES: usize = 4_718_592; // 4.5 MiB
pub const MAX_SUBMIT_BODY_BYTES: usize = 7_340_032; // 7 MiB, exact nginx/axum cap
pub const FEEDBACK_HISTORY_MAX_ENTRIES: usize = 1_000;

pub const SUBMIT_FEEDBACK_DISCLOSURE_FIELDS: &[&str] = &[
    "schema_version",
    "kind",
    "title",
    "description",
    "notes",
    "reply_to_email",
    "bundle",
    "project_file_attached",
    "project_file_blob",
];

pub const DIAGNOSTIC_BUNDLE_DISCLOSURE_FIELDS: &[&str] = &[
    "schema_version",
    "kind",
    "created_at",
    "client",
    "system",
    "machine",
    "ports_detected",
    "connection_events",
    "recent_serial",
    "recent_logs",
    "recent_panics",
    "terminal_job",
    "project_metadata",
    "known_issues",
    "project_file_attached",
    "source_context",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeedbackKind {
    Bug,
    Connectivity,
    Crash,
}

impl FeedbackKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Bug => "bug",
            Self::Connectivity => "connectivity",
            Self::Crash => "crash",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubmitFeedbackRequest {
    pub schema_version: u32,
    pub kind: FeedbackKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to_email: Option<String>,
    pub bundle: DiagnosticBundleV1,
    pub project_file_attached: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_file_blob: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticBundleV1 {
    pub schema_version: u32,
    pub kind: FeedbackKind,
    pub created_at: String,
    pub client: DiagnosticClient,
    pub system: DiagnosticSystem,
    pub machine: DiagnosticMachine,
    #[serde(default)]
    pub ports_detected: Vec<DiagnosticPort>,
    #[serde(default)]
    pub connection_events: Vec<DiagnosticConnectionEvent>,
    pub recent_serial: DiagnosticSerialTraffic,
    #[serde(default)]
    pub recent_logs: Vec<DiagnosticLogEntry>,
    #[serde(default)]
    pub recent_panics: Vec<DiagnosticPanic>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_job: Option<DiagnosticTerminalJob>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_metadata: Option<DiagnosticProjectMetadata>,
    #[serde(default)]
    pub known_issues: Vec<KnownIssueWarning>,
    pub project_file_attached: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_context: Option<FeedbackSourceContext>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticTerminalJob {
    pub captured_at: String,
    pub reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<JobProgress>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub job_tick_loop_running: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_state: Option<SessionState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub machine_run_state: Option<MachineRunState>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub job_console: Vec<ConsoleEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub session_console: Vec<ConsoleEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticClient {
    pub app_version: String,
    pub tauri_version: Option<String>,
    pub rust_version: Option<String>,
    pub build_target: String,
    pub git_sha: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticSystem {
    pub os: String,
    pub os_version: Option<String>,
    pub arch: String,
    pub locale: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticSessionState {
    Connected,
    Disconnected,
    Connecting,
    HandshakeFailed,
    Streaming,
    Error,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticMachine {
    pub connected: bool,
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_preset_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_preset_version: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub firmware_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub controller_family: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub controller_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transport_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transfer_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub s_value_max: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homing_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub use_constant_power: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emit_s_every_g1: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub use_g0_for_overscan: Option<bool>,
    pub firmware_version: Option<String>,
    pub baud_rate: Option<u32>,
    pub port_name: Option<String>,
    pub port_vendor_id: Option<String>,
    pub port_product_id: Option<String>,
    pub session_state: DiagnosticSessionState,
    pub handshake_message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticPort {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manufacturer: Option<String>,
    pub vendor_id: Option<String>,
    pub product_id: Option<String>,
    pub in_use_by_beambench: bool,
    pub available: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticConnectionEvent {
    pub ts: String,
    pub stage: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baud_rate: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticSerialTraffic {
    pub tx_hex: String,
    pub tx_ascii: String,
    pub rx_hex: String,
    pub rx_ascii: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticLogEntry {
    pub ts: String,
    pub level: String,
    pub target: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticPanic {
    pub ts: String,
    pub thread: Option<String>,
    pub message: String,
    pub location: Option<String>,
    pub backtrace: Option<String>,
    pub app_version: String,
    pub os: String,
    pub build_target: String,
    pub git_sha: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticProjectMetadata {
    pub object_count: usize,
    pub size_bytes: Option<u64>,
    pub has_raster: bool,
    pub has_vector: bool,
    pub has_text: bool,
    pub project_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnownIssueWarning {
    pub code: String,
    pub severity: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeedbackSourceContext {
    pub source: String,
    pub error_message: Option<String>,
    pub stack: Option<String>,
    pub feature: Option<String>,
    pub correlation_ts: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeedbackReportInput {
    pub kind: FeedbackKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to_email: Option<String>,
    #[serde(default)]
    pub include_project_file: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_context: Option<FeedbackSourceContext>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedReport {
    pub path: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubmitFeedbackResponse {
    pub report_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeedbackHistoryEntry {
    pub recorded_at: String,
    pub kind: FeedbackKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub saved_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectionDiagnosticsSnapshot {
    pub captured_at: String,
    #[serde(default)]
    pub ports_detected: Vec<DiagnosticPort>,
    pub machine: DiagnosticMachine,
    #[serde(default)]
    pub connection_events: Vec<DiagnosticConnectionEvent>,
    pub recent_serial: DiagnosticSerialTraffic,
    #[serde(default)]
    pub known_issues: Vec<KnownIssueWarning>,
}

#[derive(Debug, Error)]
pub enum FeedbackSerializationError {
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

pub fn scrub_string(input: &str) -> String {
    static MAC_HOME: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"/Users/[^/]+/").expect("valid mac home regex"));
    static LINUX_HOME: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"/home/[^/]+/").expect("valid linux home regex"));
    static WINDOWS_HOME: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)[a-z]:[\\/]users[\\/][^\\/]+[\\/]").expect("valid windows home regex")
    });

    let input = MAC_HOME.replace_all(input, "<userhome>/");
    let input = LINUX_HOME.replace_all(&input, "<userhome>/");
    WINDOWS_HOME.replace_all(&input, "<userhome>/").into_owned()
}

pub fn scrub_json_value(value: &mut Value) {
    match value {
        Value::String(text) => {
            *text = scrub_string(text);
        }
        Value::Array(items) => {
            for item in items {
                scrub_json_value(item);
            }
        }
        Value::Object(map) => {
            for value in map.values_mut() {
                scrub_json_value(value);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

pub fn scrubbed_value<T: Serialize>(value: &T) -> Result<Value, FeedbackSerializationError> {
    let mut value = serde_json::to_value(value)?;
    scrub_json_value(&mut value);
    Ok(value)
}

pub fn scrub_deserialized<T>(value: &T) -> Result<T, FeedbackSerializationError>
where
    T: Serialize + DeserializeOwned,
{
    let value = scrubbed_value(value)?;
    Ok(serde_json::from_value(value)?)
}

pub fn scrub_and_serialize<T: Serialize>(value: &T) -> Result<Vec<u8>, FeedbackSerializationError> {
    let value = scrubbed_value(value)?;
    Ok(serde_json::to_vec_pretty(&value)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_bundle() -> DiagnosticBundleV1 {
        DiagnosticBundleV1 {
            schema_version: FEEDBACK_SCHEMA_VERSION,
            kind: FeedbackKind::Bug,
            created_at: "2026-05-14T12:00:00Z".to_owned(),
            client: DiagnosticClient {
                app_version: "0.1.0".to_owned(),
                tauri_version: Some("2.x".to_owned()),
                rust_version: Some("1.x".to_owned()),
                build_target: "aarch64-apple-darwin".to_owned(),
                git_sha: "abc123".to_owned(),
            },
            system: DiagnosticSystem {
                os: "macOS".to_owned(),
                os_version: Some("14.6.0".to_owned()),
                arch: "aarch64".to_owned(),
                locale: Some("en-US".to_owned()),
            },
            machine: DiagnosticMachine {
                connected: false,
                model: None,
                profile_id: None,
                profile_name: None,
                profile_preset_id: None,
                profile_preset_version: None,
                firmware_type: None,
                controller_family: None,
                controller_model: None,
                transport_kind: None,
                transfer_mode: None,
                s_value_max: None,
                homing_enabled: None,
                use_constant_power: None,
                emit_s_every_g1: None,
                use_g0_for_overscan: None,
                firmware_version: None,
                baud_rate: None,
                port_name: None,
                port_vendor_id: None,
                port_product_id: None,
                session_state: DiagnosticSessionState::Disconnected,
                handshake_message: Some("no response".to_owned()),
            },
            ports_detected: vec![DiagnosticPort {
                name: "/dev/cu.usbserial".to_owned(),
                description: Some("USB Serial".to_owned()),
                manufacturer: Some("Silicon Labs".to_owned()),
                vendor_id: Some("0x10c4".to_owned()),
                product_id: Some("0xea60".to_owned()),
                in_use_by_beambench: false,
                available: true,
            }],
            connection_events: vec![DiagnosticConnectionEvent {
                ts: "2026-05-14T12:00:00Z".to_owned(),
                stage: "open_attempt".to_owned(),
                error_code: None,
                port_name: Some("/Users/alice/dev/cu.usbserial".to_owned()),
                baud_rate: Some(115200),
                message: Some("Opening serial port".to_owned()),
                error: None,
            }],
            recent_serial: DiagnosticSerialTraffic::default(),
            recent_logs: vec![DiagnosticLogEntry {
                ts: "2026-05-14T12:00:00Z".to_owned(),
                level: "warn".to_owned(),
                target: "test".to_owned(),
                message: "/Users/alice/Documents/file.lzrproj failed".to_owned(),
            }],
            recent_panics: vec![DiagnosticPanic {
                ts: "2026-05-14T12:00:00Z".to_owned(),
                thread: Some("main".to_owned()),
                message: "panic at C:\\Users\\alice\\Desktop\\x".to_owned(),
                location: Some("/home/alice/src/main.rs:1".to_owned()),
                backtrace: None,
                app_version: "0.1.0".to_owned(),
                os: "macOS".to_owned(),
                build_target: "aarch64-apple-darwin".to_owned(),
                git_sha: "abc123".to_owned(),
            }],
            terminal_job: Some(DiagnosticTerminalJob {
                captured_at: "2026-05-14T12:00:01Z".to_owned(),
                reason: "terminal_progress".to_owned(),
                progress: None,
                error: Some("job failed".to_owned()),
                job_tick_loop_running: false,
                session_state: None,
                machine_run_state: None,
                job_console: Vec::new(),
                session_console: Vec::new(),
            }),
            project_metadata: Some(DiagnosticProjectMetadata {
                object_count: 1,
                size_bytes: Some(10),
                has_raster: false,
                has_vector: true,
                has_text: false,
                project_path: Some("/Users/alice/Documents/project.lzrproj".to_owned()),
            }),
            known_issues: Vec::new(),
            project_file_attached: false,
            source_context: Some(FeedbackSourceContext {
                source: "help_menu".to_owned(),
                error_message: None,
                stack: None,
                feature: None,
                correlation_ts: None,
            }),
        }
    }

    fn sample_request() -> SubmitFeedbackRequest {
        SubmitFeedbackRequest {
            schema_version: FEEDBACK_SCHEMA_VERSION,
            kind: FeedbackKind::Bug,
            title: Some("Title".to_owned()),
            description: Some("Description".to_owned()),
            notes: Some("Note".to_owned()),
            reply_to_email: Some("user@example.com".to_owned()),
            bundle: sample_bundle(),
            project_file_attached: true,
            project_file_blob: Some("YWJj".to_owned()),
        }
    }

    fn object_keys(value: Value) -> Vec<String> {
        let mut keys: Vec<String> = value
            .as_object()
            .expect("sample serializes to object")
            .keys()
            .cloned()
            .collect();
        keys.sort();
        keys
    }

    #[test]
    fn disclosure_fields_match_submit_request_serialized_fields() {
        let mut expected: Vec<String> = SUBMIT_FEEDBACK_DISCLOSURE_FIELDS
            .iter()
            .map(|field| (*field).to_owned())
            .collect();
        expected.sort();

        assert_eq!(
            object_keys(serde_json::to_value(sample_request()).unwrap()),
            expected
        );
    }

    #[test]
    fn disclosure_fields_match_bundle_serialized_fields() {
        let mut expected: Vec<String> = DIAGNOSTIC_BUNDLE_DISCLOSURE_FIELDS
            .iter()
            .map(|field| (*field).to_owned())
            .collect();
        expected.sort();

        assert_eq!(
            object_keys(serde_json::to_value(sample_bundle()).unwrap()),
            expected
        );
    }

    #[test]
    fn scrubber_removes_usernames_from_paths_everywhere() {
        let mut value = json!({
            "project_path": "/Users/alice/Documents/foo.lzrproj",
            "log_line": "opened /home/alice/projects/foo.lzrproj",
            "panic_message": "failed at D:\\Users\\alice\\Desktop\\foo.lzrproj"
        });

        scrub_json_value(&mut value);
        let serialized = serde_json::to_string(&value).unwrap();

        assert!(!serialized.contains("alice"));
        assert!(serialized.contains("<userhome>/Documents/foo.lzrproj"));
        assert!(serialized.contains("<userhome>/projects/foo.lzrproj"));
        assert!(serialized.contains("<userhome>/Desktop"));
    }
}

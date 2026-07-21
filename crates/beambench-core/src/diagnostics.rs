//! Diagnostics export for support / debugging.
//!
//! Collects a snapshot of application state into a JSON bundle that the user
//! can attach to bug reports. No sensitive user data is included.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// A snapshot of application state for diagnostics / support.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticsBundle {
    /// Application version string.
    pub app_version: String,
    /// ISO-8601 timestamp when the bundle was created.
    pub timestamp: String,
    /// Serialized summary of the current `AppSettings`.
    pub settings_summary: serde_json::Value,
    /// Serialized summary of the active machine profile, if any.
    pub machine_profile_summary: Option<serde_json::Value>,
    /// Serialized summary of the current project metadata, if a project is open.
    pub project_metadata_summary: Option<serde_json::Value>,
    /// Any active error messages at the time of export.
    pub active_errors: Vec<String>,
    /// Excerpt of recent log output (may be empty if not available).
    pub log_excerpt: String,
}

impl DiagnosticsBundle {
    /// Serialize the bundle to pretty-printed JSON.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

/// Write a diagnostics bundle as a JSON file at the given path.
pub fn export_diagnostics(bundle: &DiagnosticsBundle, path: &Path) -> Result<(), ExportError> {
    let json = bundle.to_json().map_err(ExportError::Serialize)?;
    fs::write(path, json).map_err(ExportError::Io)?;
    Ok(())
}

/// Errors that can occur during diagnostics export.
#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("failed to serialize diagnostics: {0}")]
    Serialize(serde_json::Error),
    #[error("failed to write diagnostics file: {0}")]
    Io(std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_bundle() -> DiagnosticsBundle {
        DiagnosticsBundle {
            app_version: "0.1.0".to_string(),
            timestamp: "2026-03-18T12:00:00Z".to_string(),
            settings_summary: serde_json::json!({
                "display_unit": "mm",
                "autosave_enabled": true,
            }),
            machine_profile_summary: Some(serde_json::json!({
                "name": "My Laser",
                "bed_width_mm": 300.0,
            })),
            project_metadata_summary: Some(serde_json::json!({
                "project_name": "Test Project",
                "layers": 3,
                "objects": 5,
            })),
            active_errors: vec!["Example error".to_string()],
            log_excerpt: "INFO beam_bench: started\n".to_string(),
        }
    }

    #[test]
    fn bundle_serializes_to_valid_json() {
        let bundle = sample_bundle();
        let json = bundle.to_json().expect("serialization should succeed");
        // Verify it parses back as valid JSON
        let parsed: serde_json::Value =
            serde_json::from_str(&json).expect("output should be valid JSON");
        assert_eq!(parsed["app_version"], "0.1.0");
        assert_eq!(parsed["timestamp"], "2026-03-18T12:00:00Z");
        assert!(
            parsed["settings_summary"]["autosave_enabled"]
                .as_bool()
                .unwrap()
        );
        assert_eq!(parsed["active_errors"][0], "Example error");
    }

    #[test]
    fn bundle_roundtrips_through_json() {
        let bundle = sample_bundle();
        let json = bundle.to_json().unwrap();
        let restored: DiagnosticsBundle = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.app_version, bundle.app_version);
        assert_eq!(restored.timestamp, bundle.timestamp);
        assert_eq!(restored.active_errors, bundle.active_errors);
        assert_eq!(restored.log_excerpt, bundle.log_excerpt);
    }

    #[test]
    fn bundle_with_none_optionals_serializes() {
        let bundle = DiagnosticsBundle {
            app_version: "0.1.0".to_string(),
            timestamp: "2026-03-18T00:00:00Z".to_string(),
            settings_summary: serde_json::json!({}),
            machine_profile_summary: None,
            project_metadata_summary: None,
            active_errors: vec![],
            log_excerpt: String::new(),
        };
        let json = bundle.to_json().expect("serialization should succeed");
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed["machine_profile_summary"].is_null());
        assert!(parsed["project_metadata_summary"].is_null());
    }

    #[test]
    fn export_writes_json_file() {
        let bundle = sample_bundle();
        let dir = std::env::temp_dir().join("beambench_diag_test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test_diagnostics.json");

        export_diagnostics(&bundle, &path).expect("export should succeed");

        let contents = std::fs::read_to_string(&path).expect("file should exist");
        let parsed: serde_json::Value =
            serde_json::from_str(&contents).expect("file should contain valid JSON");
        assert_eq!(parsed["app_version"], "0.1.0");

        // Cleanup
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }
}

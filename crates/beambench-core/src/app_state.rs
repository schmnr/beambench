use std::path::PathBuf;

use crate::project::Project;
use crate::settings::AppSettings;
use serde::Serialize;
use std::sync::Mutex;

/// Application-level status information.
#[derive(Debug, Clone, Serialize)]
pub struct AppStatus {
    pub version: String,
    pub state: String,
}

/// Build metadata surfaced to the UI (About dialog, bug reports).
#[derive(Debug, Clone, Serialize)]
pub struct BuildInfo {
    pub version: String,
    pub git_sha: String,
    pub build_timestamp: String,
    pub target_triple: String,
    pub rustc_version: String,
    pub channel: String,
}

/// Top-level application state managed by Tauri.
/// Wrapped in Mutex for thread-safe access from command handlers.
pub struct AppState {
    pub settings: Mutex<AppSettings>,
    pub project: Mutex<Option<Project>>,
    pub project_path: Mutex<Option<PathBuf>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            settings: Mutex::new(AppSettings::default()),
            project: Mutex::new(None),
            project_path: Mutex::new(None),
        }
    }

    pub fn status(&self) -> AppStatus {
        AppStatus {
            version: env!("CARGO_PKG_VERSION").to_string(),
            state: "ready".to_string(),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_state_returns_version() {
        let state = AppState::new();
        let status = state.status();
        assert!(!status.version.is_empty());
        assert_eq!(status.state, "ready");
    }

    #[test]
    fn settings_are_accessible() {
        let state = AppState::new();
        let settings = state.settings.lock().unwrap();
        assert!(settings.autosave_enabled);
    }

    #[test]
    fn project_starts_as_none() {
        let state = AppState::new();
        let project = state.project.lock().unwrap();
        assert!(project.is_none());
    }
}

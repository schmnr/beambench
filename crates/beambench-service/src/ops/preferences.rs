use std::fs;
use std::path::{Path, PathBuf};

use beambench_core::AppSettings;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::context::ServiceContext;
use crate::error::{ServiceError, ServiceResult};
use crate::ops::app::validate_app_settings;
use crate::persist;

pub const PREFERENCES_FORMAT: &str = "beam-bench.preferences";
pub const PREFERENCES_FORMAT_VERSION: &str = "1";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PreferenceFile {
    pub format: String,
    pub format_version: String,
    pub app_version: String,
    pub created_at: String,
    pub settings: AppSettings,
}

#[derive(Debug, Clone, Copy)]
pub struct SettingsSanitizerExclusions {
    pub machine_state: bool,
    pub recent_files: bool,
    pub saved_positions: bool,
    pub api_settings: bool,
    pub export_dialog_paths: bool,
    pub image_presets: bool,
}

pub const PREFERENCE_FILE_EXCLUSIONS: SettingsSanitizerExclusions = SettingsSanitizerExclusions {
    machine_state: true,
    recent_files: true,
    saved_positions: true,
    api_settings: true,
    export_dialog_paths: true,
    image_presets: false,
};

fn lock_err(name: &str, e: impl std::fmt::Display) -> ServiceError {
    ServiceError::internal(format!("Failed to lock {name}: {e}"))
}

pub fn sanitize_app_settings(
    settings: &AppSettings,
    exclusions: SettingsSanitizerExclusions,
) -> AppSettings {
    let mut sanitized = settings.clone();
    let defaults = AppSettings::default();

    if exclusions.machine_state {
        sanitized.machine_profiles.clear();
        sanitized.active_profile_id = None;
    }
    if exclusions.recent_files {
        sanitized.recent_files.clear();
    }
    if exclusions.saved_positions {
        sanitized.saved_positions.clear();
    }
    if exclusions.api_settings {
        sanitized.api_enabled = defaults.api_enabled;
        sanitized.api_port = defaults.api_port;
        sanitized.api_localhost_only = defaults.api_localhost_only;
    }
    if exclusions.export_dialog_paths {
        sanitized.export_settings.last_directory = None;
        sanitized.export_settings.filename_stem = None;
    }
    if exclusions.image_presets {
        sanitized.image_presets.clear();
    }

    sanitized
}

pub fn sanitize_preferences_settings(settings: &AppSettings) -> AppSettings {
    sanitize_app_settings(settings, PREFERENCE_FILE_EXCLUSIONS)
}

pub fn merge_preferences_settings(current: &AppSettings, imported: AppSettings) -> AppSettings {
    let mut merged = imported;
    merged.machine_profiles = current.machine_profiles.clone();
    merged.active_profile_id = current.active_profile_id;
    merged.recent_files = current.recent_files.clone();
    merged.saved_positions = current.saved_positions.clone();
    merged.api_enabled = current.api_enabled;
    merged.api_port = current.api_port;
    merged.api_localhost_only = current.api_localhost_only;
    merged.export_settings.last_directory = current.export_settings.last_directory.clone();
    merged.export_settings.filename_stem = current.export_settings.filename_stem.clone();
    merged
}

fn preference_payload(settings: &AppSettings) -> PreferenceFile {
    PreferenceFile {
        format: PREFERENCES_FORMAT.to_string(),
        format_version: PREFERENCES_FORMAT_VERSION.to_string(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        created_at: Utc::now().to_rfc3339(),
        settings: sanitize_preferences_settings(settings),
    }
}

fn parse_preference_payload(path: &Path) -> ServiceResult<PreferenceFile> {
    let data = fs::read_to_string(path)
        .map_err(|e| ServiceError::persistence(format!("Failed to read preferences file: {e}")))?;
    let payload: PreferenceFile = serde_json::from_str(&data).map_err(|e| {
        ServiceError::invalid_input(format!("Failed to parse preferences file: {e}"))
    })?;
    if payload.format != PREFERENCES_FORMAT {
        return Err(ServiceError::invalid_input(format!(
            "Unsupported preferences format '{}'",
            payload.format
        )));
    }
    if payload.format_version != PREFERENCES_FORMAT_VERSION {
        return Err(ServiceError::invalid_input(format!(
            "Unsupported preferences version '{}'",
            payload.format_version
        )));
    }
    Ok(payload)
}

fn write_preference_payload(path: &Path, payload: &PreferenceFile) -> ServiceResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| ServiceError::persistence("Invalid preferences path"))?;
    fs::create_dir_all(parent).map_err(|e| {
        ServiceError::persistence(format!("Failed to create preferences directory: {e}"))
    })?;
    let json = serde_json::to_string_pretty(payload)
        .map_err(|e| ServiceError::internal(format!("Failed to serialize preferences: {e}")))?;
    let tmp = path.with_extension(format!(
        "{}.tmp",
        path.extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("bbprefs")
    ));
    fs::write(&tmp, json).map_err(|e| {
        ServiceError::persistence(format!("Failed to write preferences temp file: {e}"))
    })?;
    fs::rename(&tmp, path).map_err(|e| {
        ServiceError::persistence(format!("Failed to finalize preferences file: {e}"))
    })?;
    Ok(())
}

fn save_current_backup(current: &AppSettings) -> ServiceResult<PathBuf> {
    let payload = preference_payload(current);
    let json = serde_json::to_string_pretty(&payload).map_err(|e| {
        ServiceError::internal(format!("Failed to serialize preference backup: {e}"))
    })?;
    persist::save_preferences_backup(&json)
        .map_err(|e| ServiceError::persistence(format!("Failed to save preference backup: {e}")))
}

fn activate_candidate(
    ctx: &ServiceContext,
    mut candidate: AppSettings,
    current: AppSettings,
    event_type: &'static str,
    path: &Path,
) -> ServiceResult<AppSettings> {
    beambench_core::settings::normalize_display_language(&mut candidate);
    validate_app_settings(&candidate)?;
    persist::save_settings(&candidate).map_err(|e| {
        ServiceError::persistence(format!("Failed to persist imported preferences: {e}"))
    })?;
    save_current_backup(&current)?;
    ctx.apply_settings_side_effects(&candidate)
        .map_err(ServiceError::internal)?;
    {
        let mut settings = ctx.settings.lock().map_err(|e| lock_err("settings", e))?;
        *settings = candidate.clone();
    }
    ctx.emit_event(
        event_type,
        json!({
            "path": path.to_string_lossy().to_string(),
            "settings": candidate.clone(),
        }),
    );
    Ok(candidate)
}

pub fn export_preferences_to_path(ctx: &ServiceContext, path: &Path) -> ServiceResult<String> {
    let settings = ctx
        .settings
        .lock()
        .map_err(|e| lock_err("settings", e))?
        .clone();
    let payload = preference_payload(&settings);
    write_preference_payload(path, &payload)?;
    ctx.emit_event(
        "app.preferences.exported",
        json!({
            "path": path.to_string_lossy().to_string(),
        }),
    );
    Ok(path.to_string_lossy().to_string())
}

pub fn import_preferences_from_path(
    ctx: &ServiceContext,
    path: &Path,
) -> ServiceResult<AppSettings> {
    let payload = parse_preference_payload(path)?;
    let current = ctx
        .settings
        .lock()
        .map_err(|e| lock_err("settings", e))?
        .clone();
    let candidate = merge_preferences_settings(&current, payload.settings);
    activate_candidate(ctx, candidate, current, "app.preferences.imported", path)
}

pub fn reset_preferences_to_defaults(ctx: &ServiceContext) -> ServiceResult<AppSettings> {
    let current = ctx
        .settings
        .lock()
        .map_err(|e| lock_err("settings", e))?
        .clone();
    let candidate = merge_preferences_settings(&current, AppSettings::default());
    activate_candidate(
        ctx,
        candidate,
        current,
        "app.preferences.reset",
        Path::new("defaults"),
    )
}

#[cfg(test)]
mod tests {
    use beambench_core::settings::ArtworkExportFormat;
    use beambench_core::{DisplayUnit, MachineProfile, SpeedTimeUnit, UiTheme};

    use super::*;
    use crate::test_support::PersistTestGuard;

    #[test]
    fn preference_payload_sanitizes_local_state_but_keeps_user_preferences() {
        let mut settings = AppSettings::default();
        settings.machine_profiles.push(MachineProfile::default());
        settings.active_profile_id = Some(settings.machine_profiles[0].id);
        settings.push_recent_file("/tmp/job.bbp", "job");
        settings
            .saved_positions
            .push(beambench_core::SavedPosition {
                id: "home".to_string(),
                name: "Home".to_string(),
                x: 0.0,
                y: 0.0,
                z: None,
            });
        settings.api_enabled = true;
        settings.api_port = 7000;
        settings.api_localhost_only = false;
        settings.ui_theme = UiTheme::Light;
        settings.image_presets.push(beambench_core::ImagePreset {
            name: "Dither".to_string(),
            adjustments: Default::default(),
        });
        settings
            .custom_hotkeys
            .insert("tools.rectangle".to_string(), "Ctrl+Shift+r".to_string());
        settings.export_settings.last_directory = Some("/tmp".to_string());
        settings.export_settings.filename_stem = Some("job".to_string());
        settings.export_settings.last_format = ArtworkExportFormat::Dxf;

        let sanitized = sanitize_preferences_settings(&settings);

        assert!(sanitized.machine_profiles.is_empty());
        assert!(sanitized.active_profile_id.is_none());
        assert!(sanitized.recent_files.is_empty());
        assert!(sanitized.saved_positions.is_empty());
        // API fields reset to the locked-down defaults: an exported
        // preferences file must not carry the network API on-switch.
        assert!(!sanitized.api_enabled);
        assert_eq!(sanitized.api_port, 5900);
        assert!(sanitized.api_localhost_only);
        assert_eq!(sanitized.ui_theme, UiTheme::Light);
        assert_eq!(sanitized.image_presets.len(), 1);
        assert_eq!(
            sanitized
                .custom_hotkeys
                .get("tools.rectangle")
                .map(String::as_str),
            Some("Ctrl+Shift+r")
        );
        assert!(sanitized.export_settings.last_directory.is_none());
        assert!(sanitized.export_settings.filename_stem.is_none());
        assert_eq!(
            sanitized.export_settings.last_format,
            ArtworkExportFormat::Dxf
        );
    }

    #[test]
    fn preference_import_preserves_excluded_state_and_writes_backup() {
        let _guard = PersistTestGuard::new();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prefs.bbprefs");

        let mut imported = AppSettings::default();
        imported.autosave_enabled = false;
        imported.ui_theme = UiTheme::System;
        imported.api_enabled = true;
        imported.api_port = 7000;

        let payload = PreferenceFile {
            format: PREFERENCES_FORMAT.to_string(),
            format_version: PREFERENCES_FORMAT_VERSION.to_string(),
            app_version: "test".to_string(),
            created_at: Utc::now().to_rfc3339(),
            settings: imported,
        };
        fs::write(&path, serde_json::to_string_pretty(&payload).unwrap()).unwrap();

        let mut current = AppSettings::default();
        current.api_port = 4567;
        current.machine_profiles.push(MachineProfile::default());
        current.active_profile_id = Some(current.machine_profiles[0].id);
        let ctx = ServiceContext::with_settings(current.clone());
        let mut events = ctx.events.subscribe();

        let updated = import_preferences_from_path(&ctx, &path).unwrap();

        assert!(!updated.autosave_enabled);
        assert_eq!(updated.ui_theme, UiTheme::System);
        assert_eq!(updated.api_port, 4567);
        assert_eq!(updated.machine_profiles.len(), 1);
        assert_eq!(updated.active_profile_id, current.active_profile_id);
        assert_eq!(persist::list_preferences_backups().unwrap().len(), 1);
        let event: serde_json::Value = serde_json::from_str(&events.try_recv().unwrap()).unwrap();
        assert_eq!(event["type"], "app.preferences.imported");
        assert_eq!(event["payload"]["settings"]["ui_theme"], "system");
    }

    #[test]
    fn reset_preferences_to_defaults_preserves_excluded_state_and_writes_backup() {
        let _guard = PersistTestGuard::new();
        let mut current = AppSettings::default();
        current.display_unit = DisplayUnit::Inches;
        current.speed_time_unit = SpeedTimeUnit::Seconds;
        current.autosave_enabled = false;
        current.ui_theme = UiTheme::Light;
        current.api_port = 4567;
        current.machine_profiles.push(MachineProfile::default());
        current.active_profile_id = Some(current.machine_profiles[0].id);
        current.image_presets.push(beambench_core::ImagePreset {
            name: "Dither".to_string(),
            adjustments: Default::default(),
        });
        current.export_settings.last_directory = Some("/tmp".to_string());
        current.export_settings.filename_stem = Some("job".to_string());
        let ctx = ServiceContext::with_settings(current.clone());
        let mut events = ctx.events.subscribe();

        let reset = reset_preferences_to_defaults(&ctx).unwrap();

        assert_eq!(reset.display_unit, DisplayUnit::Mm);
        assert_eq!(reset.speed_time_unit, SpeedTimeUnit::Minutes);
        assert!(reset.autosave_enabled);
        assert_eq!(reset.ui_theme, UiTheme::Dark);
        assert_eq!(reset.api_port, 4567);
        assert_eq!(reset.machine_profiles.len(), 1);
        assert_eq!(reset.active_profile_id, current.active_profile_id);
        assert!(reset.image_presets.is_empty());
        assert_eq!(
            reset.export_settings.last_directory.as_deref(),
            Some("/tmp")
        );
        assert_eq!(reset.export_settings.filename_stem.as_deref(), Some("job"));
        assert_eq!(persist::list_preferences_backups().unwrap().len(), 1);
        let event: serde_json::Value = serde_json::from_str(&events.try_recv().unwrap()).unwrap();
        assert_eq!(event["type"], "app.preferences.reset");
        assert_eq!(event["payload"]["settings"]["ui_theme"], "dark");
    }

    #[test]
    fn invalid_preferences_do_not_create_backup_or_mutate_settings() {
        let _guard = PersistTestGuard::new();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prefs.bbprefs");
        fs::write(&path, "{ not json").unwrap();
        let ctx = ServiceContext::with_settings(AppSettings::default());

        let err = import_preferences_from_path(&ctx, &path).unwrap_err();

        assert!(err.to_string().contains("parse"));
        assert!(persist::list_preferences_backups().unwrap().is_empty());
        assert!(ctx.settings.lock().unwrap().autosave_enabled);
    }
}

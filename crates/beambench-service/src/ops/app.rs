use std::collections::HashMap;
use std::path::Path;

use beambench_core::diagnostics::{self, DiagnosticsBundle};
use beambench_core::settings::{ExportSettings, PanelLayout};
use beambench_core::{
    AppSettings, AppStatus, BuildInfo, CursorSize, DisplayUnit, IconSize, SpeedTimeUnit, UiTheme,
};
use serde::Deserialize;
use serde_json::json;

use crate::context::ServiceContext;
use crate::error::{ServiceError, ServiceResult};
use crate::events;
use crate::persist::persist_settings_to_disk;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct UpdateAppSettingsInput {
    pub display_unit: Option<String>,
    pub speed_time_unit: Option<SpeedTimeUnit>,
    pub autosave_enabled: Option<bool>,
    pub autosave_interval_secs: Option<u32>,
    pub api_enabled: Option<bool>,
    pub api_port: Option<u16>,
    pub api_localhost_only: Option<bool>,
    pub ui_theme: Option<UiTheme>,
    pub dark_mode: Option<bool>,
    pub antialiasing: Option<bool>,
    pub filled_rendering: Option<bool>,
    pub reduce_motion: Option<bool>,
    pub show_palette_labels: Option<bool>,
    pub cursor_size: Option<CursorSize>,
    pub toolbar_icon_size: Option<IconSize>,
    pub click_tolerance_px: Option<f64>,
    pub snap_threshold_px: Option<f64>,
    pub grid_spacing_mm: Option<f64>,
    pub nudge_step_mm: Option<f64>,
    pub nudge_step_fine_mm: Option<f64>,
    pub nudge_step_coarse_mm: Option<f64>,
    pub scroll_zoom: Option<bool>,
    pub debug_log_enabled: Option<bool>,
    pub panel_layout: Option<PanelLayout>,
    pub last_radius_mm: Option<f64>,
    pub export_settings: Option<ExportSettings>,
    pub allow_importing_to_tool_layers: Option<bool>,
    pub include_tool_layers_in_job_bounds: Option<bool>,
    pub check_for_updates_on_startup: Option<bool>,
    pub update_snoozed_until: Option<String>,
    pub skipped_update_version: Option<String>,
    pub custom_hotkeys: Option<HashMap<String, String>>,
    pub display_language: Option<String>,
}

fn lock_err(name: &str, e: impl std::fmt::Display) -> ServiceError {
    ServiceError::internal(format!("Failed to lock {name}: {e}"))
}

pub fn get_app_status(ctx: &ServiceContext) -> ServiceResult<AppStatus> {
    let _settings = ctx.settings.lock().map_err(|e| lock_err("settings", e))?;
    Ok(AppStatus {
        version: env!("CARGO_PKG_VERSION").to_string(),
        state: "ready".to_string(),
    })
}

pub fn get_build_info() -> BuildInfo {
    BuildInfo {
        version: beambench_buildinfo::APP_VERSION.to_string(),
        git_sha: beambench_buildinfo::GIT_SHA.to_string(),
        build_timestamp: beambench_buildinfo::BUILD_TIMESTAMP.to_string(),
        target_triple: beambench_buildinfo::TARGET_TRIPLE.to_string(),
        rustc_version: beambench_buildinfo::RUSTC_VERSION.to_string(),
        channel: "beta".to_string(),
    }
}

pub fn get_app_settings(ctx: &ServiceContext) -> ServiceResult<AppSettings> {
    let settings = ctx.settings.lock().map_err(|e| lock_err("settings", e))?;
    Ok(settings.clone())
}

pub fn apply_app_settings_update(
    current: &AppSettings,
    input: UpdateAppSettingsInput,
) -> ServiceResult<AppSettings> {
    let mut next_settings = current.clone();
    if let Some(unit) = input.display_unit {
        next_settings.display_unit = match unit.as_str() {
            "mm" => DisplayUnit::Mm,
            "inches" => DisplayUnit::Inches,
            _ => {
                return Err(ServiceError::invalid_input(format!(
                    "Invalid display unit: {unit}"
                )));
            }
        };
    }
    if let Some(lang) = input.display_language {
        let canonical = beambench_core::settings::validate_locale_code(&lang)
            .map_err(|e| ServiceError::invalid_input(e.to_string()))?;
        next_settings.display_language = canonical.to_string();
    }
    if let Some(unit) = input.speed_time_unit {
        next_settings.speed_time_unit = unit;
    }
    if let Some(enabled) = input.autosave_enabled {
        next_settings.autosave_enabled = enabled;
    }
    if let Some(interval) = input.autosave_interval_secs {
        next_settings.autosave_interval_secs = interval;
    }
    if let Some(enabled) = input.api_enabled {
        next_settings.api_enabled = enabled;
    }
    if let Some(port) = input.api_port {
        next_settings.api_port = port;
    }
    if let Some(localhost_only) = input.api_localhost_only {
        next_settings.api_localhost_only = localhost_only;
    }
    if let Some(ui_theme) = input.ui_theme {
        next_settings.ui_theme = ui_theme;
    }
    if let Some(v) = input.dark_mode {
        next_settings.dark_mode = v;
    }
    if let Some(v) = input.antialiasing {
        next_settings.antialiasing = v;
    }
    if let Some(v) = input.filled_rendering {
        next_settings.filled_rendering = v;
    }
    if let Some(v) = input.reduce_motion {
        next_settings.reduce_motion = v;
    }
    if let Some(v) = input.show_palette_labels {
        next_settings.show_palette_labels = v;
    }
    if let Some(v) = input.cursor_size {
        next_settings.cursor_size = v;
    }
    if let Some(v) = input.toolbar_icon_size {
        next_settings.toolbar_icon_size = v;
    }
    if let Some(v) = input.click_tolerance_px {
        next_settings.click_tolerance_px = v;
    }
    if let Some(snap) = input.snap_threshold_px {
        next_settings.snap_threshold_px = snap;
    }
    if let Some(v) = input.grid_spacing_mm {
        next_settings.grid_spacing_mm = v;
    }
    if let Some(v) = input.nudge_step_mm {
        next_settings.nudge_step_mm = v;
    }
    if let Some(v) = input.nudge_step_fine_mm {
        next_settings.nudge_step_fine_mm = v;
    }
    if let Some(v) = input.nudge_step_coarse_mm {
        next_settings.nudge_step_coarse_mm = v;
    }
    if let Some(v) = input.scroll_zoom {
        next_settings.scroll_zoom = v;
    }
    if let Some(v) = input.debug_log_enabled {
        next_settings.debug_log_enabled = v;
    }
    if let Some(layout) = input.panel_layout {
        next_settings.panel_layout = Some(layout);
    }
    if let Some(radius) = input.last_radius_mm {
        next_settings.last_radius_mm = radius;
    }
    if let Some(export_settings) = input.export_settings {
        next_settings.export_settings = export_settings;
    }
    if let Some(v) = input.allow_importing_to_tool_layers {
        next_settings.allow_importing_to_tool_layers = v;
    }
    if let Some(v) = input.include_tool_layers_in_job_bounds {
        next_settings.include_tool_layers_in_job_bounds = v;
    }
    if let Some(v) = input.check_for_updates_on_startup {
        next_settings.check_for_updates_on_startup = v;
    }
    if let Some(v) = input.update_snoozed_until {
        next_settings.update_snoozed_until = v;
    }
    if let Some(v) = input.skipped_update_version {
        next_settings.skipped_update_version = v;
    }
    if let Some(v) = input.custom_hotkeys {
        next_settings.custom_hotkeys = v;
    }

    validate_app_settings(&next_settings)?;
    Ok(next_settings)
}

pub fn validate_app_settings(settings: &AppSettings) -> ServiceResult<()> {
    if !(30..=3600).contains(&settings.autosave_interval_secs) {
        return Err(ServiceError::invalid_input(format!(
            "Invalid autosave interval: {} (expected 30-3600 seconds)",
            settings.autosave_interval_secs
        )));
    }
    if settings.api_port == 0 {
        return Err(ServiceError::invalid_input("Invalid API port: 0"));
    }
    if !settings.click_tolerance_px.is_finite()
        || !(0.0..=1000.0).contains(&settings.click_tolerance_px)
    {
        return Err(ServiceError::invalid_input(format!(
            "Invalid click tolerance: {}",
            settings.click_tolerance_px
        )));
    }
    if !settings.snap_threshold_px.is_finite()
        || !(0.0..=1000.0).contains(&settings.snap_threshold_px)
    {
        return Err(ServiceError::invalid_input(format!(
            "Invalid snap threshold: {}",
            settings.snap_threshold_px
        )));
    }
    if !settings.grid_spacing_mm.is_finite()
        || !(0.1..=1_000_000.0).contains(&settings.grid_spacing_mm)
    {
        return Err(ServiceError::invalid_input(format!(
            "Invalid grid spacing: {}",
            settings.grid_spacing_mm
        )));
    }
    for (label, value) in [
        ("nudge step", settings.nudge_step_mm),
        ("fine nudge step", settings.nudge_step_fine_mm),
        ("coarse nudge step", settings.nudge_step_coarse_mm),
    ] {
        if !value.is_finite() || !(0.0..=1_000_000.0).contains(&value) {
            return Err(ServiceError::invalid_input(format!(
                "Invalid {label}: {value}"
            )));
        }
    }
    if !settings.last_radius_mm.is_finite()
        || !(0.0..=1_000_000.0).contains(&settings.last_radius_mm)
    {
        return Err(ServiceError::invalid_input(format!(
            "Invalid radius: {}",
            settings.last_radius_mm
        )));
    }
    Ok(())
}

pub fn update_app_settings(
    ctx: &ServiceContext,
    input: UpdateAppSettingsInput,
) -> ServiceResult<AppSettings> {
    let current = ctx
        .settings
        .lock()
        .map_err(|e| lock_err("settings", e))?
        .clone();
    let next_settings = apply_app_settings_update(&current, input)?;

    ctx.apply_settings_side_effects(&next_settings)
        .map_err(ServiceError::internal)?;
    {
        let mut settings = ctx.settings.lock().map_err(|e| lock_err("settings", e))?;
        *settings = next_settings.clone();
    }
    persist_settings_to_disk(ctx);
    ctx.emit_event(
        "app.settings.updated",
        json!({
            "settings": next_settings.clone(),
        }),
    );
    Ok(next_settings)
}

pub fn export_diagnostics_to_path(ctx: &ServiceContext, path: &Path) -> ServiceResult<String> {
    let settings_summary = {
        let settings = ctx.settings.lock().map_err(|e| lock_err("settings", e))?;
        serde_json::to_value(&*settings)
            .map_err(|e| ServiceError::internal(format!("Failed to serialize settings: {e}")))?
    };

    let machine_profile_summary = {
        let settings = ctx.settings.lock().map_err(|e| lock_err("settings", e))?;
        settings
            .active_profile_id
            .and_then(|id| {
                settings
                    .machine_profiles
                    .iter()
                    .find(|p| p.id == id)
                    .cloned()
            })
            .and_then(|profile| serde_json::to_value(&profile).ok())
    };

    let project_metadata_summary = {
        let project = ctx.project.lock().map_err(|e| lock_err("project", e))?;
        project.as_ref().map(|p| {
            serde_json::json!({
                "project_name": p.metadata.project_name,
                "format_version": p.metadata.format_version,
                "created_at": p.metadata.created_at,
                "modified_at": p.metadata.modified_at,
                "layer_count": p.layers.len(),
                "object_count": p.objects.len(),
                "asset_count": p.assets.len(),
            })
        })
    };

    let active_errors: Vec<String> = ctx
        .active_errors
        .lock()
        .map_err(|e| lock_err("active_errors", e))?
        .iter()
        .map(|entry| entry.line.clone())
        .collect();

    let log_excerpt = ctx
        .log_buffer
        .lock()
        .map_err(|e| lock_err("log_buffer", e))?
        .iter()
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");

    let bundle = DiagnosticsBundle {
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        settings_summary,
        machine_profile_summary,
        project_metadata_summary,
        active_errors,
        log_excerpt,
    };

    diagnostics::export_diagnostics(&bundle, path)
        .map_err(|e| ServiceError::persistence(format!("Failed to export diagnostics: {e}")))?;
    ctx.emit_event(
        "app.diagnostics.exported",
        json!({
            "path": path.to_string_lossy().to_string(),
            "timestamp": events::timestamp(),
        }),
    );
    Ok(path.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ServiceContext;
    use crate::test_support::PersistTestGuard;
    use beambench_core::settings::ZoneState;

    #[test]
    fn update_app_settings_emits_event() {
        let _guard = PersistTestGuard::new();
        let ctx = ServiceContext::new();
        let mut rx = ctx.events.subscribe();
        let applied_theme = std::sync::Arc::new(std::sync::Mutex::new(None));
        let applied_theme_for_hook = std::sync::Arc::clone(&applied_theme);
        ctx.set_settings_applier(move |settings| {
            *applied_theme_for_hook.lock().unwrap() = Some(settings.ui_theme);
            Ok(())
        })
        .unwrap();

        update_app_settings(
            &ctx,
            UpdateAppSettingsInput {
                autosave_enabled: Some(false),
                ui_theme: Some(UiTheme::Light),
                ..Default::default()
            },
        )
        .unwrap();

        let msg = rx.try_recv().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&msg).unwrap();
        assert_eq!(parsed["type"], "app.settings.updated");
        assert_eq!(parsed["payload"]["settings"]["autosave_enabled"], false);
        assert_eq!(parsed["payload"]["settings"]["ui_theme"], "light");
        assert_eq!(*applied_theme.lock().unwrap(), Some(UiTheme::Light));
    }

    #[test]
    fn ui_theme_update_does_not_change_workspace_background() {
        let current = AppSettings {
            dark_mode: true,
            ..Default::default()
        };

        let updated = apply_app_settings_update(
            &current,
            UpdateAppSettingsInput {
                ui_theme: Some(UiTheme::System),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(updated.ui_theme, UiTheme::System);
        assert!(updated.dark_mode);
    }

    #[test]
    fn update_app_settings_does_not_partially_apply_on_error() {
        let _guard = PersistTestGuard::new();
        let ctx = ServiceContext::new();

        let err = update_app_settings(
            &ctx,
            UpdateAppSettingsInput {
                display_unit: Some("inches".to_string()),
                autosave_interval_secs: Some(5),
                ..Default::default()
            },
        )
        .unwrap_err();

        assert!(err.to_string().contains("Invalid autosave interval"));
        let settings = get_app_settings(&ctx).unwrap();
        assert_eq!(settings.display_unit, DisplayUnit::Mm);
        assert_eq!(settings.autosave_interval_secs, 120);
    }

    #[test]
    fn update_app_settings_persists_panel_layout() {
        let _guard = PersistTestGuard::new();
        let ctx = ServiceContext::new();

        // Verify no layout initially
        let settings = get_app_settings(&ctx).unwrap();
        assert!(settings.panel_layout.is_none());

        // Write a panel layout through update_app_settings
        let mut zones = std::collections::HashMap::new();
        zones.insert(
            "upper-right".to_string(),
            ZoneState {
                panel_ids: vec!["cuts_layers".to_string(), "move".to_string()],
                active_tab: "cuts_layers".to_string(),
            },
        );
        zones.insert(
            "lower-right".to_string(),
            ZoneState {
                panel_ids: vec!["laser".to_string()],
                active_tab: "laser".to_string(),
            },
        );

        let layout = PanelLayout {
            zones,
            hidden_panel_ids: vec!["macros".to_string()],
            upper_split_ratio: 0.65,
            right_panel_width: 400.0,
            floating_panels: vec![],
            left_panel_width: 280.0,
            bottom_panel_height: 200.0,
            side_panels_visible: true,
            toolbar_visibility: std::collections::HashMap::new(),
        };

        update_app_settings(
            &ctx,
            UpdateAppSettingsInput {
                panel_layout: Some(layout),
                ..Default::default()
            },
        )
        .unwrap();

        // Read back and verify round-trip
        let restored = get_app_settings(&ctx).unwrap();
        let pl = restored.panel_layout.expect("panel_layout should be Some");
        assert_eq!(pl.zones.len(), 2);
        assert_eq!(pl.hidden_panel_ids, vec!["macros"]);
        assert!((pl.upper_split_ratio - 0.65).abs() < f64::EPSILON);
        assert!((pl.right_panel_width - 400.0).abs() < f64::EPSILON);
        let upper = pl.zones.get("upper-right").unwrap();
        assert_eq!(upper.panel_ids, vec!["cuts_layers", "move"]);
        assert_eq!(upper.active_tab, "cuts_layers");
    }
}

use crate::machine_profile::{MachineProfile, MachineProfileId};
use beambench_common::RasterAdjustments;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// User-configurable display unit preference.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DisplayUnit {
    #[default]
    Mm,
    Inches,
}

/// User-configurable speed time unit preference.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SpeedTimeUnit {
    #[default]
    Minutes,
    Seconds,
}

/// Cursor size preference.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CursorSize {
    Small,
    #[default]
    Normal,
    Large,
}

/// Icon size preference.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IconSize {
    Small,
    #[default]
    Normal,
    Large,
}

/// User-configurable theme for the application chrome.
///
/// This is intentionally separate from [`AppSettings::dark_mode`], which
/// controls only the drawing workspace background.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UiTheme {
    System,
    Light,
    #[default]
    Dark,
}

/// State of a single dock zone (list of panel IDs and active tab).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZoneState {
    pub panel_ids: Vec<String>,
    pub active_tab: String,
}

/// State of a single floating panel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FloatingPanelState {
    pub panel_id: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub z_index: u32,
    /// The dock zone this panel was floated from, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin_zone: Option<String>,
    /// The tab index within the origin zone, for restoring position.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin_index: Option<u32>,
}

/// Persistent panel layout configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PanelLayout {
    pub zones: HashMap<String, ZoneState>,
    pub hidden_panel_ids: Vec<String>,
    pub upper_split_ratio: f64,
    pub right_panel_width: f64,
    #[serde(default)]
    pub floating_panels: Vec<FloatingPanelState>,
    #[serde(default = "default_left_panel_width")]
    pub left_panel_width: f64,
    #[serde(default = "default_bottom_panel_height")]
    pub bottom_panel_height: f64,
    #[serde(default = "default_side_panels_visible")]
    pub side_panels_visible: bool,
    #[serde(default = "default_toolbar_visibility")]
    pub toolbar_visibility: HashMap<String, bool>,
}

/// A saved machine position (name + coordinates).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedPosition {
    #[serde(default = "new_saved_position_id")]
    pub id: String,
    pub name: String,
    pub x: f64,
    pub y: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub z: Option<f64>,
}

fn new_saved_position_id() -> String {
    Uuid::new_v4().to_string()
}

fn default_left_panel_width() -> f64 {
    280.0
}

fn default_bottom_panel_height() -> f64 {
    36.0
}

fn default_side_panels_visible() -> bool {
    true
}

fn default_toolbar_visibility() -> HashMap<String, bool> {
    HashMap::from([
        ("main".to_string(), true),
        ("arrange".to_string(), true),
        ("arrangeLong".to_string(), false),
        ("modifiers".to_string(), true),
        ("docking".to_string(), true),
        ("numericEdits".to_string(), true),
        ("textOptions".to_string(), true),
        ("tools".to_string(), true),
    ])
}

fn default_click_tolerance() -> f64 {
    5.0
}

fn default_snap_threshold() -> f64 {
    5.0
}

fn default_grid_spacing_mm() -> f64 {
    10.0
}

fn default_nudge_step_mm() -> f64 {
    5.0
}

fn default_nudge_step_fine_mm() -> f64 {
    1.0
}

fn default_nudge_step_coarse_mm() -> f64 {
    20.0
}

fn default_true_settings() -> bool {
    true
}

/// A recently opened/saved project file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentFile {
    pub path: String,
    pub name: String,
    pub opened_at: String,
}

/// Named image adjustment preset, persisted in app settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImagePreset {
    pub name: String,
    pub adjustments: RasterAdjustments,
}

/// Persisted defaults for artwork export dialogs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportSettings {
    #[serde(default)]
    pub last_directory: Option<String>,
    #[serde(default)]
    pub last_format: ArtworkExportFormat,
    #[serde(default)]
    pub filename_stem: Option<String>,
}

impl Default for ExportSettings {
    fn default() -> Self {
        Self {
            last_directory: None,
            last_format: ArtworkExportFormat::Svg,
            filename_stem: None,
        }
    }
}

/// Artwork export formats exposed in the unified export dialog.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ArtworkExportFormat {
    #[default]
    Svg,
    Dxf,
    Pdf,
    Eps,
    Ai,
    Png,
    Jpg,
    Bmp,
}

/// Bump when a one-time settings migration is needed; see `migrate_settings`.
pub const SETTINGS_SCHEMA_VERSION: u32 = 1;

/// Application-level settings stored locally outside project files.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    /// Schema version last written by the app (0 = pre-versioning files).
    #[serde(default)]
    pub settings_schema_version: u32,
    pub display_unit: DisplayUnit,
    #[serde(default)]
    pub speed_time_unit: SpeedTimeUnit,
    pub autosave_enabled: bool,
    pub autosave_interval_secs: u32,
    #[serde(default)]
    pub machine_profiles: Vec<MachineProfile>,
    #[serde(default)]
    pub active_profile_id: Option<MachineProfileId>,
    #[serde(default)]
    pub recent_files: Vec<RecentFile>,
    #[serde(default)]
    pub api_enabled: bool,
    #[serde(default = "default_api_port")]
    pub api_port: u16,
    #[serde(default = "default_api_localhost_only")]
    pub api_localhost_only: bool,
    #[serde(default)]
    pub ui_theme: UiTheme,
    #[serde(default)]
    pub dark_mode: bool,
    #[serde(default)]
    pub antialiasing: bool,
    #[serde(default)]
    pub filled_rendering: bool,
    #[serde(default)]
    pub reduce_motion: bool,
    #[serde(default)]
    pub show_palette_labels: bool,
    #[serde(default)]
    pub cursor_size: CursorSize,
    #[serde(default)]
    pub toolbar_icon_size: IconSize,
    #[serde(default = "default_click_tolerance")]
    pub click_tolerance_px: f64,
    #[serde(default = "default_snap_threshold")]
    pub snap_threshold_px: f64,
    #[serde(default = "default_grid_spacing_mm")]
    pub grid_spacing_mm: f64,
    #[serde(default = "default_nudge_step_mm")]
    pub nudge_step_mm: f64,
    #[serde(default = "default_nudge_step_fine_mm")]
    pub nudge_step_fine_mm: f64,
    #[serde(default = "default_nudge_step_coarse_mm")]
    pub nudge_step_coarse_mm: f64,
    #[serde(default = "default_true_settings")]
    pub scroll_zoom: bool,
    #[serde(default)]
    pub debug_log_enabled: bool,
    #[serde(default)]
    pub panel_layout: Option<PanelLayout>,
    #[serde(default)]
    pub saved_positions: Vec<SavedPosition>,
    #[serde(default = "default_radius")]
    pub last_radius_mm: f64,
    #[serde(default)]
    pub image_presets: Vec<ImagePreset>,
    #[serde(default)]
    pub custom_hotkeys: HashMap<String, String>,
    #[serde(default)]
    pub export_settings: ExportSettings,
    #[serde(default)]
    pub allow_importing_to_tool_layers: bool,
    #[serde(default = "default_true_settings")]
    pub include_tool_layers_in_job_bounds: bool,
    #[serde(default = "default_true_settings")]
    pub check_for_updates_on_startup: bool,
    #[serde(default)]
    pub update_snoozed_until: String,
    #[serde(default)]
    pub skipped_update_version: String,
    #[serde(default = "default_display_language")]
    pub display_language: String,
}

/// Locale codes the app ships translations for. This runtime allowlist is
/// parity-tested against the canonical frontend locale manifest.
pub const SUPPORTED_LOCALES: &[&str] = &[
    "en", "de", "es-ES", "es-419", "fr", "it", "pt-BR", "nl", "pl", "cs", "sv", "nb", "da", "fi",
    "hu", "tr", "el", "ru", "sl", "ja", "ko", "zh-CN", "zh-TW",
];

fn default_display_language() -> String {
    "en".to_string()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidLocaleError(pub String);

impl std::fmt::Display for InvalidLocaleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unsupported locale code: {}", self.0)
    }
}

impl std::error::Error for InvalidLocaleError {}

/// Returns the canonical locale string for a supported code, or an error
/// for any unknown / malformed input. Used on the update-API path where
/// rejection is the right policy.
pub fn validate_locale_code(value: &str) -> Result<&'static str, InvalidLocaleError> {
    SUPPORTED_LOCALES
        .iter()
        .find(|c| **c == value)
        .copied()
        .ok_or_else(|| InvalidLocaleError(value.to_string()))
}

/// Coerce an out-of-range `display_language` back to `"en"`. Used on
/// load / import paths where rejecting would brick the app.
pub fn normalize_display_language(settings: &mut AppSettings) {
    if validate_locale_code(&settings.display_language).is_err() {
        settings.display_language = "en".to_string();
    }
}

/// One-time migrations for settings files written by older app versions.
/// Runs on every load; versions only move forward.
pub fn migrate_settings(settings: &mut AppSettings) {
    if settings.settings_schema_version < 1 {
        // v0 (0.1.0) shipped with the network API enabled and bound to all
        // interfaces by default, and serialized those values into every
        // settings file. Reset to the locked-down defaults once; users who
        // genuinely want the API on can re-enable it (which stamps the new
        // schema version so this never runs again).
        settings.api_enabled = false;
        settings.api_localhost_only = true;
    }
    settings.settings_schema_version = SETTINGS_SCHEMA_VERSION;
}

fn default_radius() -> f64 {
    2.0
}

fn default_api_port() -> u16 {
    5900
}

// The API can start jobs and move the laser, so it ships locked down:
// disabled until the user opts in, and bound to the local machine only.
fn default_api_localhost_only() -> bool {
    true
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            settings_schema_version: SETTINGS_SCHEMA_VERSION,
            display_unit: DisplayUnit::Mm,
            speed_time_unit: SpeedTimeUnit::Minutes,
            autosave_enabled: true,
            autosave_interval_secs: 120,
            machine_profiles: Vec::new(),
            active_profile_id: None,
            recent_files: Vec::new(),
            api_enabled: false,
            api_port: 5900,
            api_localhost_only: true,
            ui_theme: UiTheme::Dark,
            dark_mode: false,
            antialiasing: false,
            filled_rendering: false,
            reduce_motion: false,
            show_palette_labels: false,
            cursor_size: CursorSize::default(),
            toolbar_icon_size: IconSize::default(),
            click_tolerance_px: 5.0,
            snap_threshold_px: 5.0,
            grid_spacing_mm: 10.0,
            nudge_step_mm: 5.0,
            nudge_step_fine_mm: 1.0,
            nudge_step_coarse_mm: 20.0,
            scroll_zoom: true,
            debug_log_enabled: false,
            panel_layout: None,
            saved_positions: Vec::new(),
            last_radius_mm: 5.0,
            image_presets: Vec::new(),
            custom_hotkeys: HashMap::new(),
            export_settings: ExportSettings::default(),
            allow_importing_to_tool_layers: false,
            include_tool_layers_in_job_bounds: true,
            check_for_updates_on_startup: true,
            update_snoozed_until: String::new(),
            skipped_update_version: String::new(),
            display_language: default_display_language(),
        }
    }
}

impl AppSettings {
    /// Push a recently opened/saved file to the front of the list.
    /// Removes any existing entry with the same path and keeps max 24 items.
    pub fn push_recent_file(&mut self, path: &str, name: &str) {
        // Remove any existing entry with same path
        self.recent_files.retain(|r| r.path != path);
        // Add to front
        self.recent_files.insert(
            0,
            RecentFile {
                path: path.to_string(),
                name: name.to_string(),
                opened_at: chrono::Utc::now().to_rfc3339(),
            },
        );
        // Keep max 24
        self.recent_files.truncate(24);
    }

    /// Get the list of recent files.
    pub fn get_recent_files(&self) -> &[RecentFile] {
        &self.recent_files
    }

    /// Clear all recent files.
    pub fn clear_recent_files(&mut self) {
        self.recent_files.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct LocaleManifestFixture {
        schema_version: u32,
        default_locale: String,
        locales: Vec<LocaleManifestEntryFixture>,
    }

    #[derive(serde::Deserialize)]
    struct LocaleManifestEntryFixture {
        code: String,
    }

    #[test]
    fn locale_manifest_matches_runtime_allowlist() {
        let manifest: LocaleManifestFixture = serde_json::from_str(include_str!(
            "../../../tauri-app/src/i18n/locale-manifest.json"
        ))
        .expect("locale manifest should be valid JSON");
        let manifest_codes: Vec<&str> = manifest
            .locales
            .iter()
            .map(|locale| locale.code.as_str())
            .collect();

        assert_eq!(manifest.schema_version, 1);
        assert_eq!(manifest.default_locale, "en");
        assert_eq!(manifest_codes, SUPPORTED_LOCALES);
    }

    #[test]
    fn locale_validation_rejects_unknown_codes_and_normalizes_persisted_values() {
        assert_eq!(validate_locale_code("de").unwrap(), "de");
        assert!(validate_locale_code("en-XA").is_err());
        assert!(validate_locale_code("de-DE").is_err());

        let mut settings = AppSettings {
            display_language: "unknown".to_string(),
            ..Default::default()
        };
        normalize_display_language(&mut settings);
        assert_eq!(settings.display_language, "en");
    }

    #[test]
    fn default_settings_are_sensible() {
        let s = AppSettings::default();
        assert_eq!(s.display_unit, DisplayUnit::Mm);
        assert_eq!(s.speed_time_unit, SpeedTimeUnit::Minutes);
        assert!(s.autosave_enabled);
        assert_eq!(s.autosave_interval_secs, 120);
        assert!(!s.api_enabled, "API must ship disabled by default");
        assert!(
            s.api_localhost_only,
            "API must ship bound to localhost by default"
        );
        assert!(s.check_for_updates_on_startup);
        assert!(s.update_snoozed_until.is_empty());
        assert!(s.skipped_update_version.is_empty());
        assert_eq!(s.ui_theme, UiTheme::Dark);
        assert!(!s.dark_mode, "workspace background remains independent");
    }

    #[test]
    fn settings_roundtrip_json() {
        let mut s = AppSettings::default();
        s.speed_time_unit = SpeedTimeUnit::Seconds;
        let json = serde_json::to_string(&s).unwrap();
        let restored: AppSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.display_unit, DisplayUnit::Mm);
        assert_eq!(restored.speed_time_unit, SpeedTimeUnit::Seconds);
        assert_eq!(
            restored.export_settings.last_format,
            ArtworkExportFormat::Svg
        );
        assert!(restored.check_for_updates_on_startup);
        assert_eq!(restored.ui_theme, UiTheme::Dark);
    }

    #[test]
    fn old_settings_json_without_machine_profiles_deserializes() {
        // Backward compat: old JSON without machine_profiles/active_profile_id
        let json = r#"{"display_unit":"mm","autosave_enabled":true,"autosave_interval_secs":120}"#;
        let restored: AppSettings = serde_json::from_str(json).unwrap();
        assert!(restored.machine_profiles.is_empty());
        assert!(restored.active_profile_id.is_none());
        assert_eq!(restored.speed_time_unit, SpeedTimeUnit::Minutes);
        assert!(restored.check_for_updates_on_startup);
        assert!(restored.update_snoozed_until.is_empty());
        assert!(restored.skipped_update_version.is_empty());
        assert_eq!(restored.ui_theme, UiTheme::Dark);
    }

    #[test]
    fn ui_theme_roundtrips_all_variants() {
        for ui_theme in [UiTheme::System, UiTheme::Light, UiTheme::Dark] {
            let settings = AppSettings {
                ui_theme,
                ..Default::default()
            };
            let json = serde_json::to_string(&settings).unwrap();
            let restored: AppSettings = serde_json::from_str(&json).unwrap();
            assert_eq!(restored.ui_theme, ui_theme);
        }
    }

    #[test]
    fn settings_with_machine_profiles_roundtrip() {
        use crate::machine_profile::MachineProfile;
        let mut s = AppSettings::default();
        let profile = MachineProfile::default();
        s.active_profile_id = Some(profile.id);
        s.machine_profiles.push(profile);
        let json = serde_json::to_string(&s).unwrap();
        let restored: AppSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.machine_profiles.len(), 1);
        assert!(restored.active_profile_id.is_some());
    }

    #[test]
    fn old_settings_without_recent_files_deserializes() {
        let json = r#"{"display_unit":"mm","autosave_enabled":true,"autosave_interval_secs":120}"#;
        let restored: AppSettings = serde_json::from_str(json).unwrap();
        assert!(restored.recent_files.is_empty());
    }

    #[test]
    fn export_settings_roundtrip_and_default() {
        let old_json =
            r#"{"display_unit":"mm","autosave_enabled":true,"autosave_interval_secs":120}"#;
        let restored: AppSettings = serde_json::from_str(old_json).unwrap();
        assert_eq!(restored.export_settings, ExportSettings::default());

        let mut s = AppSettings::default();
        s.export_settings = ExportSettings {
            last_directory: Some("/tmp/exports".to_string()),
            last_format: ArtworkExportFormat::Dxf,
            filename_stem: Some("fixture".to_string()),
        };
        let json = serde_json::to_string(&s).unwrap();
        let restored: AppSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(
            restored.export_settings.last_directory.as_deref(),
            Some("/tmp/exports")
        );
        assert_eq!(
            restored.export_settings.last_format,
            ArtworkExportFormat::Dxf
        );
        assert_eq!(
            restored.export_settings.filename_stem.as_deref(),
            Some("fixture")
        );
    }

    #[test]
    fn push_recent_file_adds_to_front() {
        let mut s = AppSettings::default();
        s.push_recent_file("/path/to/project.lzrproj", "project");
        assert_eq!(s.recent_files.len(), 1);
        assert_eq!(s.recent_files[0].path, "/path/to/project.lzrproj");
        assert_eq!(s.recent_files[0].name, "project");
    }

    #[test]
    fn push_recent_file_removes_duplicates() {
        let mut s = AppSettings::default();
        s.push_recent_file("/path/a.lzrproj", "a");
        s.push_recent_file("/path/b.lzrproj", "b");
        s.push_recent_file("/path/a.lzrproj", "a");
        assert_eq!(s.recent_files.len(), 2);
        assert_eq!(s.recent_files[0].path, "/path/a.lzrproj");
        assert_eq!(s.recent_files[1].path, "/path/b.lzrproj");
    }

    #[test]
    fn push_recent_file_truncates_at_24() {
        let mut s = AppSettings::default();
        for i in 0..30 {
            s.push_recent_file(&format!("/path/{i}.lzrproj"), &format!("file{i}"));
        }
        assert_eq!(s.recent_files.len(), 24);
        assert_eq!(s.recent_files[0].path, "/path/29.lzrproj");
        assert_eq!(s.recent_files[23].path, "/path/6.lzrproj");
    }

    #[test]
    fn migration_locks_down_api_from_v0_settings() {
        // A settings file written by 0.1.0: no schema version, API enabled
        // and bound to all interfaces (the old serialized defaults).
        let json = r#"{"display_unit":"mm","autosave_enabled":true,"autosave_interval_secs":120,"api_enabled":true,"api_port":5900,"api_localhost_only":false}"#;
        let mut settings: AppSettings = serde_json::from_str(json).unwrap();
        assert_eq!(settings.settings_schema_version, 0);

        migrate_settings(&mut settings);

        assert!(!settings.api_enabled, "v0 API on-switch must be reset");
        assert!(settings.api_localhost_only, "v0 binding must be localhost");
        assert_eq!(settings.settings_schema_version, SETTINGS_SCHEMA_VERSION);
    }

    #[test]
    fn migration_respects_deliberate_api_choice_after_v1() {
        // Once stamped with the current schema version, a user's explicit
        // re-enable of the API must survive future loads.
        let mut settings = AppSettings::default();
        settings.api_enabled = true;
        settings.api_localhost_only = false;

        migrate_settings(&mut settings);

        assert!(settings.api_enabled);
        assert!(!settings.api_localhost_only);
    }

    #[test]
    fn old_settings_without_api_fields_deserializes() {
        let json = r#"{"display_unit":"mm","autosave_enabled":true,"autosave_interval_secs":120}"#;
        let restored: AppSettings = serde_json::from_str(json).unwrap();
        assert!(!restored.api_enabled);
        assert_eq!(restored.api_port, 5900);
        assert!(restored.api_localhost_only);
    }

    #[test]
    fn api_settings_roundtrip() {
        let mut s = AppSettings::default();
        s.api_enabled = true;
        s.api_port = 8080;
        s.api_localhost_only = false;
        let json = serde_json::to_string(&s).unwrap();
        let restored: AppSettings = serde_json::from_str(&json).unwrap();
        assert!(restored.api_enabled);
        assert_eq!(restored.api_port, 8080);
        assert!(!restored.api_localhost_only);
    }

    #[test]
    fn custom_hotkeys_roundtrip_and_default() {
        let old_json =
            r#"{"display_unit":"mm","autosave_enabled":true,"autosave_interval_secs":120}"#;
        let restored: AppSettings = serde_json::from_str(old_json).unwrap();
        assert!(restored.custom_hotkeys.is_empty());

        let mut s = AppSettings::default();
        s.custom_hotkeys
            .insert("tools.rectangle".to_string(), "Ctrl+Shift+r".to_string());
        let json = serde_json::to_string(&s).unwrap();
        let restored: AppSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(
            restored
                .custom_hotkeys
                .get("tools.rectangle")
                .map(String::as_str),
            Some("Ctrl+Shift+r")
        );
    }

    // --- AppSettings field tests ---

    #[test]
    fn app_settings_p1_fields_roundtrip() {
        let mut s = AppSettings::default();
        s.dark_mode = true;
        s.antialiasing = true;
        s.filled_rendering = true;
        s.reduce_motion = true;
        s.show_palette_labels = true;
        s.cursor_size = CursorSize::Large;
        s.toolbar_icon_size = IconSize::Small;
        s.click_tolerance_px = 10.0;
        s.scroll_zoom = false;
        s.debug_log_enabled = true;

        let json = serde_json::to_string(&s).unwrap();
        let restored: AppSettings = serde_json::from_str(&json).unwrap();

        assert!(restored.dark_mode);
        assert!(restored.antialiasing);
        assert!(restored.filled_rendering);
        assert!(restored.reduce_motion);
        assert!(restored.show_palette_labels);
        assert_eq!(restored.cursor_size, CursorSize::Large);
        assert_eq!(restored.toolbar_icon_size, IconSize::Small);
        assert_eq!(restored.click_tolerance_px, 10.0);
        assert!(!restored.scroll_zoom);
        assert!(restored.debug_log_enabled);
    }

    #[test]
    fn old_app_settings_without_p1_fields_deserializes() {
        let json = r#"{"display_unit":"mm","autosave_enabled":true,"autosave_interval_secs":120}"#;
        let restored: AppSettings = serde_json::from_str(json).unwrap();
        assert!(!restored.dark_mode);
        assert!(!restored.antialiasing);
        assert!(!restored.filled_rendering);
        assert!(!restored.reduce_motion);
        assert!(!restored.show_palette_labels);
        assert_eq!(restored.cursor_size, CursorSize::Normal);
        assert_eq!(restored.toolbar_icon_size, IconSize::Normal);
        assert_eq!(restored.click_tolerance_px, 5.0);
        assert!(restored.scroll_zoom);
        assert!(!restored.debug_log_enabled);
    }

    #[test]
    fn cursor_size_enum_roundtrips() {
        let sizes = vec![CursorSize::Small, CursorSize::Normal, CursorSize::Large];
        for size in sizes {
            let json = serde_json::to_string(&size).unwrap();
            let restored: CursorSize = serde_json::from_str(&json).unwrap();
            assert_eq!(size, restored);
        }
    }

    #[test]
    fn icon_size_enum_roundtrips() {
        let sizes = vec![IconSize::Small, IconSize::Normal, IconSize::Large];
        for size in sizes {
            let json = serde_json::to_string(&size).unwrap();
            let restored: IconSize = serde_json::from_str(&json).unwrap();
            assert_eq!(size, restored);
        }
    }

    #[test]
    fn cursor_size_default_is_normal() {
        assert_eq!(CursorSize::default(), CursorSize::Normal);
    }

    #[test]
    fn icon_size_default_is_normal() {
        assert_eq!(IconSize::default(), IconSize::Normal);
    }

    #[test]
    fn old_settings_without_panel_layout_deserializes() {
        let json = r#"{"display_unit":"mm","autosave_enabled":true,"autosave_interval_secs":120}"#;
        let restored: AppSettings = serde_json::from_str(json).unwrap();
        assert!(restored.panel_layout.is_none());
    }

    #[test]
    fn panel_layout_roundtrip() {
        let mut s = AppSettings::default();
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
        s.panel_layout = Some(PanelLayout {
            zones,
            hidden_panel_ids: vec!["macros".to_string()],
            upper_split_ratio: 0.7,
            right_panel_width: 400.0,
            floating_panels: vec![],
            left_panel_width: 280.0,
            bottom_panel_height: 200.0,
            side_panels_visible: true,
            toolbar_visibility: default_toolbar_visibility(),
        });
        let json = serde_json::to_string(&s).unwrap();
        let restored: AppSettings = serde_json::from_str(&json).unwrap();
        let layout = restored.panel_layout.unwrap();
        assert_eq!(layout.zones.len(), 2);
        assert_eq!(layout.hidden_panel_ids, vec!["macros"]);
        assert!((layout.upper_split_ratio - 0.7).abs() < f64::EPSILON);
        assert!((layout.right_panel_width - 400.0).abs() < f64::EPSILON);
        assert!((layout.left_panel_width - 280.0).abs() < f64::EPSILON);
        assert!((layout.bottom_panel_height - 200.0).abs() < f64::EPSILON);
        let upper = layout.zones.get("upper-right").unwrap();
        assert_eq!(upper.panel_ids, vec!["cuts_layers", "move"]);
        assert_eq!(upper.active_tab, "cuts_layers");
    }

    // --- Recent-files tests ---

    #[test]
    fn get_recent_files_returns_list() {
        let mut s = AppSettings::default();
        s.push_recent_file("/path/a.lzrproj", "a");
        s.push_recent_file("/path/b.lzrproj", "b");
        let recent = s.get_recent_files();
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].path, "/path/b.lzrproj");
        assert_eq!(recent[1].path, "/path/a.lzrproj");
    }

    #[test]
    fn panel_layout_with_floating_roundtrip() {
        let mut s = AppSettings::default();
        let mut zones = std::collections::HashMap::new();
        zones.insert(
            "upper-right".to_string(),
            ZoneState {
                panel_ids: vec!["cuts_layers".to_string()],
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
        s.panel_layout = Some(PanelLayout {
            zones,
            hidden_panel_ids: vec![],
            upper_split_ratio: 0.6,
            right_panel_width: 384.0,
            floating_panels: vec![
                FloatingPanelState {
                    panel_id: "console".to_string(),
                    x: 100.0,
                    y: 200.0,
                    width: 420.0,
                    height: 300.0,
                    z_index: 1,
                    origin_zone: None,
                    origin_index: None,
                },
                FloatingPanelState {
                    panel_id: "macros".to_string(),
                    x: 150.0,
                    y: 250.0,
                    width: 320.0,
                    height: 260.0,
                    z_index: 2,
                    origin_zone: None,
                    origin_index: None,
                },
            ],
            left_panel_width: 280.0,
            bottom_panel_height: 200.0,
            side_panels_visible: true,
            toolbar_visibility: default_toolbar_visibility(),
        });
        let json = serde_json::to_string(&s).unwrap();
        let restored: AppSettings = serde_json::from_str(&json).unwrap();
        let layout = restored.panel_layout.unwrap();
        assert_eq!(layout.floating_panels.len(), 2);
        assert_eq!(layout.floating_panels[0].panel_id, "console");
        assert_eq!(layout.floating_panels[0].z_index, 1);
        assert!((layout.floating_panels[0].x - 100.0).abs() < f64::EPSILON);
        assert_eq!(layout.floating_panels[1].panel_id, "macros");
        assert_eq!(layout.floating_panels[1].z_index, 2);
    }

    #[test]
    fn old_panel_layout_without_floating_deserializes() {
        // Backward compat: old panel_layout JSON without floating_panels field
        let json = r#"{
            "display_unit":"mm",
            "autosave_enabled":true,
            "autosave_interval_secs":120,
            "panel_layout":{
                "zones":{"upper-right":{"panel_ids":["cuts_layers"],"active_tab":"cuts_layers"},"lower-right":{"panel_ids":["laser"],"active_tab":"laser"}},
                "hidden_panel_ids":[],
                "upper_split_ratio":0.6,
                "right_panel_width":384.0
            }
        }"#;
        let restored: AppSettings = serde_json::from_str(json).unwrap();
        let layout = restored.panel_layout.unwrap();
        assert!(layout.floating_panels.is_empty());
        assert_eq!(layout.zones.len(), 2);
        assert_eq!(layout.toolbar_visibility, default_toolbar_visibility());
    }

    #[test]
    fn saved_positions_roundtrip() {
        let mut s = AppSettings::default();
        s.saved_positions.push(SavedPosition {
            id: "home".to_string(),
            name: "Home".to_string(),
            x: 0.0,
            y: 0.0,
            z: None,
        });
        s.saved_positions.push(SavedPosition {
            id: "center".to_string(),
            name: "Center".to_string(),
            x: 150.0,
            y: 100.0,
            z: Some(5.0),
        });
        let json = serde_json::to_string(&s).unwrap();
        let restored: AppSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.saved_positions.len(), 2);
        assert_eq!(restored.saved_positions[0].name, "Home");
        assert_eq!(restored.saved_positions[1].x, 150.0);
        assert_eq!(restored.saved_positions[1].z, Some(5.0));
    }

    #[test]
    fn old_saved_positions_without_id_or_z_deserialize() {
        let json = r#"{
            "display_unit":"mm",
            "autosave_enabled":true,
            "autosave_interval_secs":120,
            "saved_positions":[{"name":"Old","x":10.0,"y":20.0}]
        }"#;
        let restored: AppSettings = serde_json::from_str(json).unwrap();
        assert_eq!(restored.saved_positions.len(), 1);
        assert!(!restored.saved_positions[0].id.is_empty());
        assert_eq!(restored.saved_positions[0].name, "Old");
        assert_eq!(restored.saved_positions[0].z, None);
    }

    #[test]
    fn old_settings_without_saved_positions_deserializes() {
        let json = r#"{"display_unit":"mm","autosave_enabled":true,"autosave_interval_secs":120}"#;
        let restored: AppSettings = serde_json::from_str(json).unwrap();
        assert!(restored.saved_positions.is_empty());
    }

    #[test]
    fn snap_threshold_defaults_to_five() {
        let s = AppSettings::default();
        assert_eq!(s.snap_threshold_px, 5.0);
    }

    #[test]
    fn nudge_steps_use_expected_defaults() {
        let s = AppSettings::default();
        assert_eq!(s.nudge_step_mm, 5.0);
        assert_eq!(s.nudge_step_fine_mm, 1.0);
        assert_eq!(s.nudge_step_coarse_mm, 20.0);
    }

    #[test]
    fn snap_threshold_roundtrips_serde() {
        // New settings with custom snap_threshold
        let mut s = AppSettings::default();
        s.snap_threshold_px = 10.0;
        let json = serde_json::to_string(&s).unwrap();
        let restored: AppSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.snap_threshold_px, 10.0);

        // Backward compat: old settings without snap_threshold_px → 5.0
        let old_json =
            r#"{"display_unit":"mm","autosave_enabled":true,"autosave_interval_secs":120}"#;
        let restored: AppSettings = serde_json::from_str(old_json).unwrap();
        assert_eq!(restored.snap_threshold_px, 5.0);
    }

    #[test]
    fn clear_recent_files_empties_list() {
        let mut s = AppSettings::default();
        s.push_recent_file("/path/a.lzrproj", "a");
        s.push_recent_file("/path/b.lzrproj", "b");
        assert_eq!(s.recent_files.len(), 2);

        s.clear_recent_files();
        assert_eq!(s.recent_files.len(), 0);
        assert!(s.get_recent_files().is_empty());
    }
}

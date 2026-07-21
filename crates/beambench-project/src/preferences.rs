//! Preferences export/import for AppSettings + MaterialPresets + Macros.

use beambench_core::{AppSettings, MacroDefinition, MaterialPreset};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Preferences bundle for export/import.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreferencesBundle {
    pub settings: AppSettings,
    #[serde(default)]
    pub material_presets: Vec<MaterialPreset>,
    #[serde(default)]
    pub macros: Vec<MacroDefinition>,
}

/// Export preferences to a `.prefs` JSON file.
pub fn export_preferences(
    settings: &AppSettings,
    material_presets: &[MaterialPreset],
    macros: &[MacroDefinition],
    path: &Path,
) -> Result<(), String> {
    let bundle = PreferencesBundle {
        settings: settings.clone(),
        material_presets: material_presets.to_vec(),
        macros: macros.to_vec(),
    };

    let json = serde_json::to_string_pretty(&bundle)
        .map_err(|e| format!("Failed to serialize preferences: {e}"))?;

    std::fs::write(path, json).map_err(|e| format!("Failed to write preferences file: {e}"))?;
    Ok(())
}

/// Import preferences from a `.prefs` JSON file.
pub fn import_preferences(path: &Path) -> Result<PreferencesBundle, String> {
    let json = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read preferences file: {e}"))?;

    let bundle: PreferencesBundle = serde_json::from_str(&json)
        .map_err(|e| format!("Failed to deserialize preferences: {e}"))?;

    Ok(bundle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use beambench_core::{DisplayUnit, OperationType, UiTheme};
    use tempfile::TempDir;
    use uuid::Uuid;

    #[test]
    fn export_and_import_preferences_roundtrip() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.prefs");

        let settings = AppSettings {
            display_unit: DisplayUnit::Inches,
            ui_theme: UiTheme::Light,
            autosave_enabled: false,
            ..Default::default()
        };

        let material_presets = vec![MaterialPreset {
            id: Uuid::new_v4(),
            name: "Plywood".to_string(),
            material: "Plywood".to_string(),
            thickness_mm: 3.0,
            operation: OperationType::Cut,
            speed_mm_min: 800.0,
            power_percent: 80.0,
            passes: 2,
            dpi: None,
            raster_mode: None,
            line_interval_mm: None,
            scan_angle: None,
            bidirectional: None,
            overscan_mm: None,
            flood_fill: None,
            angle_passes: None,
            angle_increment_deg: None,
            pass_through: None,
            halftone_cells_per_inch: None,
            halftone_angle_deg: None,
            newsprint_angle_deg: None,
            newsprint_frequency: None,
            invert: None,
            dot_width_correction_mm: None,
            ramp_length_mm: None,
            notes: String::new(),
            category: String::new(),
            device_profile: None,
        }];

        let macros = vec![MacroDefinition {
            id: Uuid::new_v4(),
            name: "Home".to_string(),
            description: "Home the machine".to_string(),
            commands: vec!["$H".to_string()],
            hotkey: None,
            show_in_toolbar: false,
        }];

        export_preferences(&settings, &material_presets, &macros, &path).unwrap();

        let imported = import_preferences(&path).unwrap();

        assert_eq!(imported.settings.display_unit, DisplayUnit::Inches);
        assert_eq!(imported.settings.ui_theme, UiTheme::Light);
        assert!(!imported.settings.autosave_enabled);
        assert_eq!(imported.material_presets.len(), 1);
        assert_eq!(imported.material_presets[0].name, "Plywood");
        assert_eq!(imported.macros.len(), 1);
        assert_eq!(imported.macros[0].name, "Home");
    }

    #[test]
    fn import_nonexistent_file_returns_error() {
        let result = import_preferences(Path::new("/nonexistent/test.prefs"));
        assert!(result.is_err());
    }

    #[test]
    fn import_invalid_json_returns_error() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("bad.prefs");
        std::fs::write(&path, "NOT JSON!!!").unwrap();

        let result = import_preferences(&path);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("deserialize"));
    }

    #[test]
    fn export_creates_valid_json() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.prefs");

        let settings = AppSettings::default();
        let material_presets = Vec::new();
        let macros = Vec::new();

        export_preferences(&settings, &material_presets, &macros, &path).unwrap();

        let json = std::fs::read_to_string(&path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert!(parsed["settings"].is_object());
        assert!(parsed["material_presets"].is_array());
        assert!(parsed["macros"].is_array());
    }

    #[test]
    fn preferences_bundle_backward_compat() {
        // Old preferences files without material_presets or macros should still load
        let json = r#"{
            "settings": {
                "display_unit": "mm",
                "autosave_enabled": true,
                "autosave_interval_secs": 120
            }
        }"#;

        let bundle: PreferencesBundle = serde_json::from_str(json).unwrap();
        assert_eq!(bundle.settings.display_unit, DisplayUnit::Mm);
        assert_eq!(bundle.settings.ui_theme, UiTheme::Dark);
        assert!(bundle.material_presets.is_empty());
        assert!(bundle.macros.is_empty());
    }
}

//! Material presets and cut settings.

use crate::layer::OperationType;
use beambench_common::RasterMode;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Material preset with operation settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MaterialPreset {
    pub id: Uuid,
    pub name: String,
    pub material: String,
    pub thickness_mm: f64,
    pub operation: OperationType,
    pub speed_mm_min: f64,
    pub power_percent: f64,
    pub passes: u32,
    #[serde(default)]
    pub dpi: Option<u32>,
    #[serde(default)]
    pub raster_mode: Option<RasterMode>,
    #[serde(default)]
    pub line_interval_mm: Option<f64>,
    #[serde(default)]
    pub scan_angle: Option<f64>,
    #[serde(default)]
    pub bidirectional: Option<bool>,
    #[serde(default)]
    pub overscan_mm: Option<f64>,
    #[serde(default)]
    pub flood_fill: Option<bool>,
    #[serde(default)]
    pub angle_passes: Option<u32>,
    #[serde(default)]
    pub angle_increment_deg: Option<f64>,
    #[serde(default)]
    pub pass_through: Option<bool>,
    #[serde(default)]
    pub halftone_cells_per_inch: Option<u32>,
    #[serde(default)]
    pub halftone_angle_deg: Option<f64>,
    #[serde(default)]
    pub newsprint_angle_deg: Option<f64>,
    #[serde(default)]
    pub newsprint_frequency: Option<f64>,
    /// Invert the image (negative). Image-only.
    #[serde(default)]
    pub invert: Option<bool>,
    /// Layer-level dot-width correction in mm. Image-only.
    #[serde(default)]
    pub dot_width_correction_mm: Option<f64>,
    /// Ramp length in mm. Image-only.
    #[serde(default)]
    pub ramp_length_mm: Option<f64>,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub category: String,
    /// Optional device profile name for per-device library filtering.
    #[serde(default)]
    pub device_profile: Option<String>,
}

impl Default for MaterialPreset {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            name: String::new(),
            material: String::new(),
            thickness_mm: 3.0,
            operation: OperationType::Line,
            speed_mm_min: 1000.0,
            power_percent: 50.0,
            passes: 1,
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
        }
    }
}

/// Aggregate cut settings for layer updates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct CutSettings {
    #[serde(default)]
    pub speed_mm_min: f64,
    #[serde(default)]
    pub power_max_percent: f64,
    #[serde(default)]
    pub power_min_percent: f64,
    #[serde(default)]
    pub passes: u32,
    #[serde(default)]
    pub air_assist: bool,
    #[serde(default)]
    pub z_offset_mm: f64,
    #[serde(default)]
    pub gcode_prefix: String,
    #[serde(default)]
    pub gcode_suffix: String,
    // Fill/Image specific
    #[serde(default)]
    pub line_interval_mm: Option<f64>,
    #[serde(default)]
    pub scan_angle: Option<f64>,
    #[serde(default)]
    pub angle_passes: Option<u32>,
    #[serde(default)]
    pub angle_increment_deg: Option<f64>,
    #[serde(default)]
    pub crosshatch: Option<bool>,
    #[serde(default)]
    pub bidirectional: Option<bool>,
    #[serde(default)]
    pub overscan_mm: Option<f64>,
    #[serde(default)]
    pub flood_fill: Option<bool>,
    #[serde(default)]
    pub dpi: Option<u32>,
    #[serde(default)]
    pub raster_mode: Option<RasterMode>,
    #[serde(default)]
    pub brightness: Option<f64>,
    #[serde(default)]
    pub contrast: Option<f64>,
    #[serde(default)]
    pub gamma: Option<f64>,
    // Vector specific
    #[serde(default)]
    pub kerf_offset_mm: Option<f64>,
    #[serde(default)]
    pub perforation_enabled: Option<bool>,
    #[serde(default)]
    pub perforation_on_ms: Option<f64>,
    #[serde(default)]
    pub perforation_off_ms: Option<f64>,
    #[serde(default)]
    pub tab_count: Option<u32>,
    #[serde(default)]
    pub tab_width_mm: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn material_preset_default_values() {
        let preset = MaterialPreset::default();
        assert_eq!(preset.name, "");
        assert_eq!(preset.material, "");
        assert_eq!(preset.thickness_mm, 3.0);
        assert_eq!(preset.operation, OperationType::Line);
        assert_eq!(preset.speed_mm_min, 1000.0);
        assert_eq!(preset.power_percent, 50.0);
        assert_eq!(preset.passes, 1);
        assert!(preset.dpi.is_none());
        assert_eq!(preset.notes, "");
    }

    #[test]
    fn material_preset_roundtrips() {
        let preset = MaterialPreset {
            id: Uuid::new_v4(),
            name: "Plywood Cut".to_string(),
            material: "Plywood".to_string(),
            thickness_mm: 6.0,
            operation: OperationType::Cut,
            speed_mm_min: 800.0,
            power_percent: 80.0,
            passes: 2,
            dpi: Some(300),
            raster_mode: Some(RasterMode::Grayscale),
            line_interval_mm: Some(0.1),
            scan_angle: Some(45.0),
            bidirectional: Some(true),
            overscan_mm: Some(2.5),
            flood_fill: Some(false),
            angle_passes: Some(2),
            angle_increment_deg: Some(90.0),
            pass_through: Some(true),
            halftone_cells_per_inch: Some(15),
            halftone_angle_deg: Some(22.5),
            newsprint_angle_deg: Some(45.0),
            newsprint_frequency: Some(20.0),
            invert: None,
            dot_width_correction_mm: None,
            ramp_length_mm: None,
            notes: "Use air assist".to_string(),
            category: String::new(),
            device_profile: None,
        };
        let json = serde_json::to_string(&preset).unwrap();
        let restored: MaterialPreset = serde_json::from_str(&json).unwrap();
        assert_eq!(preset, restored);
    }

    #[test]
    fn material_preset_roundtrips_new_image_fields() {
        let preset = MaterialPreset {
            id: Uuid::new_v4(),
            name: "Photo Engrave".to_string(),
            material: "Anodized Aluminum".to_string(),
            thickness_mm: 1.5,
            operation: OperationType::Image,
            speed_mm_min: 4000.0,
            power_percent: 70.0,
            passes: 1,
            dpi: Some(508),
            raster_mode: Some(RasterMode::FloydSteinberg),
            line_interval_mm: Some(0.05),
            scan_angle: Some(0.0),
            bidirectional: Some(true),
            overscan_mm: Some(2.5),
            flood_fill: None,
            angle_passes: None,
            angle_increment_deg: None,
            pass_through: Some(false),
            halftone_cells_per_inch: None,
            halftone_angle_deg: None,
            newsprint_angle_deg: None,
            newsprint_frequency: None,
            invert: Some(true),
            dot_width_correction_mm: Some(0.08),
            ramp_length_mm: Some(0.5),
            notes: String::new(),
            category: "Image".to_string(),
            device_profile: None,
        };
        let json = serde_json::to_string(&preset).unwrap();
        let restored: MaterialPreset = serde_json::from_str(&json).unwrap();
        assert_eq!(preset.invert, restored.invert);
        assert_eq!(
            preset.dot_width_correction_mm,
            restored.dot_width_correction_mm
        );
        assert_eq!(preset.ramp_length_mm, restored.ramp_length_mm);
        assert_eq!(preset, restored);
    }

    #[test]
    fn material_preset_ids_are_unique() {
        let p1 = MaterialPreset::default();
        let p2 = MaterialPreset::default();
        assert_ne!(p1.id, p2.id);
    }

    #[test]
    fn cut_settings_default_is_empty() {
        let settings = CutSettings::default();
        assert_eq!(settings.speed_mm_min, 0.0);
        assert_eq!(settings.power_max_percent, 0.0);
        assert_eq!(settings.power_min_percent, 0.0);
        assert_eq!(settings.passes, 0);
        assert!(!settings.air_assist);
        assert!(settings.line_interval_mm.is_none());
        assert!(settings.kerf_offset_mm.is_none());
    }

    #[test]
    fn cut_settings_roundtrips() {
        let settings = CutSettings {
            speed_mm_min: 1000.0,
            power_max_percent: 80.0,
            power_min_percent: 10.0,
            passes: 2,
            air_assist: true,
            z_offset_mm: 5.0,
            gcode_prefix: "G0 Z5".to_string(),
            gcode_suffix: "G0 Z0".to_string(),
            line_interval_mm: Some(0.2),
            scan_angle: Some(45.0),
            angle_passes: Some(3),
            angle_increment_deg: Some(60.0),
            crosshatch: Some(true),
            bidirectional: Some(false),
            overscan_mm: Some(3.0),
            flood_fill: Some(true),
            dpi: Some(300),
            raster_mode: Some(RasterMode::FloydSteinberg),
            brightness: Some(0.1),
            contrast: Some(-0.2),
            gamma: Some(1.5),
            kerf_offset_mm: Some(0.1),
            perforation_enabled: Some(true),
            perforation_on_ms: Some(5.0),
            perforation_off_ms: Some(10.0),
            tab_count: Some(4),
            tab_width_mm: Some(5.0),
        };
        let json = serde_json::to_string(&settings).unwrap();
        let restored: CutSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(settings, restored);
    }
}

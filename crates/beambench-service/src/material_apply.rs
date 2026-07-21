use beambench_core::{
    CutEntry, CutEntryId, LayerId, MaterialPreset, OperationType, VectorSettings,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaterialApplyWarningCode {
    MultiEntryLayerTargetedPrimary,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterialApplyWarning {
    pub code: MaterialApplyWarningCode,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterialApplyResponse {
    pub applied_layer_id: LayerId,
    pub targeted_entry_id: CutEntryId,
    #[serde(default)]
    pub warnings: Vec<MaterialApplyWarning>,
}

/// Pure preset-to-entry transform.
///
/// Returns a new `CutEntry` with the preset applied plus any entry-level warnings. Has no side
/// effects: does not touch project state, plan cache, events, dirty flags, or pass-through bounds.
/// Layer-context warnings (e.g. `MultiEntryLayerTargetedPrimary`) are added by the caller, not here.
pub fn apply_material_to_entry(
    preset: &MaterialPreset,
    seed: &CutEntry,
) -> (CutEntry, Vec<MaterialApplyWarning>) {
    let mut entry = seed.clone();
    let warnings: Vec<MaterialApplyWarning> = Vec::new();

    entry.speed_mm_min = preset.speed_mm_min;
    entry.power_percent = preset.power_percent;

    let is_image = entry.operation == OperationType::Image;

    // Ensure the per-operation settings bag exists before applying preset fields. Without this,
    // a seed with `raster_settings: None` (e.g. a frontend stub) would silently drop the preset's
    // interval/DPI/scan-angle and the dialog defaults derived from the result would be wrong.
    match entry.operation {
        OperationType::Image | OperationType::Fill => {
            entry.ensure_raster_settings();
        }
        _ => {
            if entry.vector_settings.is_none() {
                entry.vector_settings = Some(VectorSettings::default());
            }
        }
    }

    match entry.operation {
        OperationType::Image | OperationType::Fill => {
            if let Some(ref mut raster_settings) = entry.raster_settings {
                raster_settings.passes = preset.passes;
                if let Some(li) = preset.line_interval_mm {
                    if li > 0.0 {
                        raster_settings.line_interval_mm = li;
                        raster_settings.dpi = (25.4 / li).round() as u32;
                    }
                } else if let Some(dpi) = preset.dpi {
                    if dpi > 0 {
                        raster_settings.dpi = dpi;
                        raster_settings.line_interval_mm = 25.4 / dpi as f64;
                    }
                }
                if is_image {
                    if let Some(raster_mode) = preset.raster_mode {
                        raster_settings.mode = raster_mode;
                    }
                    if let Some(pass_through) = preset.pass_through {
                        raster_settings.pass_through = pass_through;
                    }
                    if let Some(halftone_cells_per_inch) = preset.halftone_cells_per_inch {
                        raster_settings.halftone_cells_per_inch = halftone_cells_per_inch;
                    }
                    if let Some(halftone_angle_deg) = preset.halftone_angle_deg {
                        raster_settings.halftone_angle_deg = halftone_angle_deg;
                    }
                    if let Some(newsprint_angle_deg) = preset.newsprint_angle_deg {
                        raster_settings.newsprint_angle_deg = newsprint_angle_deg;
                    }
                    if let Some(newsprint_frequency) = preset.newsprint_frequency {
                        raster_settings.newsprint_frequency = newsprint_frequency;
                    }
                    if let Some(invert) = preset.invert {
                        raster_settings.invert = invert;
                    }
                    if let Some(dwc) = preset.dot_width_correction_mm {
                        raster_settings.dot_width_correction_mm = dwc;
                    }
                    if let Some(ramp) = preset.ramp_length_mm {
                        raster_settings.ramp_length_mm = ramp;
                    }
                }
                if let Some(scan_angle) = preset.scan_angle {
                    raster_settings.scan_angle = scan_angle;
                }
                if let Some(bidirectional) = preset.bidirectional {
                    raster_settings.bidirectional = bidirectional;
                }
                if let Some(overscan_mm) = preset.overscan_mm {
                    raster_settings.overscan_mm = overscan_mm;
                }
                if let Some(flood_fill) = preset.flood_fill {
                    raster_settings.flood_fill = flood_fill;
                }
                if let Some(angle_passes) = preset.angle_passes {
                    raster_settings.angle_passes = angle_passes;
                }
                if let Some(angle_increment_deg) = preset.angle_increment_deg {
                    raster_settings.angle_increment_deg = angle_increment_deg;
                }
            }
        }
        _ => {
            if let Some(ref mut vector_settings) = entry.vector_settings {
                vector_settings.passes = preset.passes;
            }
        }
    }

    (entry, warnings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use beambench_core::{CutEntry, MaterialPreset, OperationType, RasterSettings};
    use uuid::Uuid;

    fn fill_preset(line_interval: f64) -> MaterialPreset {
        MaterialPreset {
            id: Uuid::new_v4(),
            name: "test".to_string(),
            material: "test".to_string(),
            thickness_mm: 0.0,
            operation: OperationType::Fill,
            speed_mm_min: 1500.0,
            power_percent: 70.0,
            passes: 2,
            line_interval_mm: Some(line_interval),
            ..Default::default()
        }
    }

    #[test]
    fn fill_seed_without_raster_settings_still_picks_up_preset_interval() {
        // Regression: a frontend stub seed with `raster_settings: None` for a Fill operation
        // would previously silently drop the preset's interval/DPI/scan-angle because the
        // mutator's `if let Some(ref mut raster_settings)` block was skipped entirely.
        let seed = CutEntry {
            id: Default::default(),
            operation: OperationType::Fill,
            speed_mm_min: 0.0,
            power_percent: 0.0,
            raster_settings: None,
            vector_settings: None,
            air_assist: false,
            power_min_percent: 0.0,
            z_offset_mm: 0.0,
            gcode_prefix: String::new(),
            gcode_suffix: String::new(),
            output_enabled: true,
        };
        let preset = fill_preset(0.05);
        let (updated, _w) = apply_material_to_entry(&preset, &seed);
        assert_eq!(updated.speed_mm_min, 1500.0);
        assert_eq!(updated.power_percent, 70.0);
        let rs = updated
            .raster_settings
            .expect("raster_settings must be populated for Fill operation");
        assert_eq!(rs.line_interval_mm, 0.05);
        assert_eq!(rs.dpi, (25.4_f64 / 0.05).round() as u32);
        assert_eq!(rs.passes, 2);
    }

    #[test]
    fn fill_seed_with_raster_settings_keeps_existing_unchanged_fields() {
        // Pre-existing path: a seed with raster_settings: Some(...) preserves any fields the
        // preset doesn't override (e.g., scan_angle stays at the seed's value if preset omits it).
        let mut rs = RasterSettings::default();
        rs.scan_angle = 45.0;
        let seed = CutEntry {
            id: Default::default(),
            operation: OperationType::Fill,
            speed_mm_min: 0.0,
            power_percent: 0.0,
            raster_settings: Some(rs),
            vector_settings: None,
            air_assist: false,
            power_min_percent: 0.0,
            z_offset_mm: 0.0,
            gcode_prefix: String::new(),
            gcode_suffix: String::new(),
            output_enabled: true,
        };
        let preset = fill_preset(0.1);
        let (updated, _w) = apply_material_to_entry(&preset, &seed);
        assert_eq!(updated.raster_settings.as_ref().unwrap().scan_angle, 45.0);
        assert_eq!(
            updated.raster_settings.as_ref().unwrap().line_interval_mm,
            0.1
        );
    }

    #[test]
    fn cut_seed_without_vector_settings_gets_default_then_passes_applied() {
        let seed = CutEntry {
            id: Default::default(),
            operation: OperationType::Cut,
            speed_mm_min: 0.0,
            power_percent: 0.0,
            raster_settings: None,
            vector_settings: None,
            air_assist: false,
            power_min_percent: 0.0,
            z_offset_mm: 0.0,
            gcode_prefix: String::new(),
            gcode_suffix: String::new(),
            output_enabled: true,
        };
        let mut preset = fill_preset(0.0);
        preset.operation = OperationType::Cut;
        preset.passes = 3;
        let (updated, _w) = apply_material_to_entry(&preset, &seed);
        assert_eq!(
            updated
                .vector_settings
                .expect("vector_settings populated")
                .passes,
            3
        );
    }
}

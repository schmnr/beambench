use std::io::{Cursor, Write};

use beambench_grbl::{GcodeConfig, generate_gcode};
use beambench_planner::ExecutionPlan;
use thiserror::Error;
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

use crate::M1_POWER_MAXIMUM;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct M1CompileConfig {
    pub material_thickness_mm: f64,
    pub focus_reference_mm: f64,
}

impl Default for M1CompileConfig {
    fn default() -> Self {
        Self {
            material_thickness_mm: 0.0,
            focus_reference_mm: 17.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct M1CompiledJob {
    pub gcode: String,
    pub archive: Vec<u8>,
    pub line_count: usize,
}

#[derive(Debug, Error)]
pub enum M1CompileError {
    #[error("xTool M1 material thickness must be a finite value from 0 to 16 mm")]
    InvalidMaterialThickness,
    #[error("xTool M1 focus reference must be a finite value from 1 to 35 mm")]
    InvalidFocusReference,
    #[error("xTool M1 focus target {0:.3} mm is outside the safe 0 to 35 mm range")]
    UnsafeFocusTarget(f64),
    #[error("xTool M1 does not accept custom job header or footer G-code")]
    CustomGcodeUnsupported,
    #[error("xTool M1 job generation failed: {0}")]
    Generation(String),
    #[error("xTool M1 compiler received unsupported generated command: {0}")]
    UnsupportedCommand(String),
    #[error("xTool M1 ZIP packaging failed: {0}")]
    Packaging(String),
}

pub fn compile_m1_job(
    plan: &ExecutionPlan,
    base_config: &GcodeConfig,
    config: M1CompileConfig,
) -> Result<M1CompiledJob, M1CompileError> {
    if !config.material_thickness_mm.is_finite()
        || !(0.0..=16.0).contains(&config.material_thickness_mm)
    {
        return Err(M1CompileError::InvalidMaterialThickness);
    }
    if !config.focus_reference_mm.is_finite() || !(1.0..=35.0).contains(&config.focus_reference_mm)
    {
        return Err(M1CompileError::InvalidFocusReference);
    }
    let focus_target = config.focus_reference_mm - config.material_thickness_mm;
    if !(0.0..=35.0).contains(&focus_target) {
        return Err(M1CompileError::UnsafeFocusTarget(focus_target));
    }
    if !base_config.gcode_prefix.trim().is_empty() || !base_config.gcode_suffix.trim().is_empty() {
        return Err(M1CompileError::CustomGcodeUnsupported);
    }

    let mut generated_config = base_config.clone();
    generated_config.gcode_prefix.clear();
    generated_config.gcode_suffix.clear();
    generated_config.s_value_max = M1_POWER_MAXIMUM;
    generated_config.emit_s_every_g1 = true;
    generated_config.use_constant_power = false;
    generated_config.z_moves_enabled = false;
    generated_config.air_assist_cut_entry_ids.clear();
    generated_config.air_assist_on_gcode.clear();
    generated_config.air_assist_off_gcode.clear();

    let generated = generate_gcode(plan, &generated_config)
        .map_err(|error| M1CompileError::Generation(error.to_string()))?;
    let mut lines = vec![
        format!("G0 Z{focus_target:.2}"),
        "M4 S0".to_string(),
        "G1 F1000".to_string(),
        "M19 S1".to_string(),
        "M18 S0".to_string(),
        "G0 F9600".to_string(),
        "G4 P0.1".to_string(),
        "M104 X0".to_string(),
    ];

    for line in generated {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed == "G21" {
            continue;
        }
        if let Some(power) = trimmed
            .strip_prefix("M3 S")
            .or_else(|| trimmed.strip_prefix("M4 S"))
        {
            lines.push(format!("G1 S{power}"));
            continue;
        }
        if trimmed == "M5" {
            lines.push("G1 S0".to_string());
            continue;
        }
        let command = trimmed.split_ascii_whitespace().next().unwrap_or_default();
        if matches!(command, "G0" | "G1" | "G4" | "G90" | "G91") {
            lines.push(trimmed.to_string());
        } else {
            return Err(M1CompileError::UnsupportedCommand(trimmed.to_string()));
        }
    }
    if lines.last().is_none_or(|line| line != "G1 S0") {
        lines.push("G1 S0".to_string());
    }
    lines.push("G4 P0.1".to_string());
    lines.push("M6 P1".to_string());
    let gcode = format!("{}\n", lines.join("\n"));
    let archive = package_m1_gcode(gcode.as_bytes())?;
    Ok(M1CompiledJob {
        line_count: lines.len(),
        gcode,
        archive,
    })
}

pub fn package_m1_gcode(gcode: &[u8]) -> Result<Vec<u8>, M1CompileError> {
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut zip = ZipWriter::new(&mut cursor);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        zip.start_file("gcodes.txt", options)
            .map_err(|error| M1CompileError::Packaging(error.to_string()))?;
        zip.write_all(gcode)
            .map_err(|error| M1CompileError::Packaging(error.to_string()))?;
        zip.finish()
            .map_err(|error| M1CompileError::Packaging(error.to_string()))?;
    }
    Ok(cursor.into_inner())
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use beambench_common::geometry::{Bounds, Point2D};
    use beambench_planner::{
        DirectionMode, ExecutionPlan, PlanSegment, PowerMode, ScanAxis, ScanDirection, ScanRun,
        Scanline,
    };
    use chrono::Utc;
    use uuid::Uuid;

    use super::*;

    fn vector_plan() -> ExecutionPlan {
        ExecutionPlan {
            id: Uuid::new_v4(),
            project_id: Uuid::new_v4(),
            revision_hash: "xtool-test".to_string(),
            created_at: Utc::now(),
            bounds: Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(40.0, 40.0)),
            total_distance_mm: 20.0,
            segments: vec![PlanSegment::Vector {
                polyline: vec![Point2D::new(10.0, 20.0), Point2D::new(30.0, 20.0)],
                closed: false,
                speed_mm_min: 1200.0,
                power_percent: 50.0,
                cut_entry_id: "cut".to_string(),
                layer_id: "layer".to_string(),
                perforation_enabled: false,
                perforation_on_ms: 0.0,
                perforation_off_ms: 0.0,
                source_object_id: None,
                source_subpath_index: None,
            }],
            estimated_duration_secs: 1.0,
            layer_order: vec!["layer".to_string()],
            warnings: Vec::new(),
            failed_entries: Vec::new(),
        }
    }

    #[test]
    fn compiles_native_m1_job_and_collapses_spindle_commands() {
        let compiled = compile_m1_job(
            &vector_plan(),
            &GcodeConfig::default(),
            M1CompileConfig {
                material_thickness_mm: 3.0,
                ..M1CompileConfig::default()
            },
        )
        .unwrap();
        assert!(compiled.gcode.starts_with("G0 Z14.00\nM4 S0\n"));
        assert!(compiled.gcode.contains("G1 S500"));
        assert!(compiled.gcode.contains("G1 X30.000 Y20.000"));
        assert_eq!(compiled.gcode.matches("M4").count(), 1);
        assert!(!compiled.gcode.contains("M5"));
        assert!(compiled.gcode.ends_with("G4 P0.1\nM6 P1\n"));
    }

    #[test]
    fn packages_one_stored_gcodes_member() {
        let bytes = package_m1_gcode(b"G0 X1\n").unwrap();
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        assert_eq!(archive.len(), 1);
        let mut member = archive.by_name("gcodes.txt").unwrap();
        assert_eq!(member.compression(), CompressionMethod::Stored);
        let mut text = String::new();
        member.read_to_string(&mut text).unwrap();
        assert_eq!(text, "G0 X1\n");
    }

    #[test]
    fn compiles_raster_motion_with_inline_m1_power() {
        let mut plan = vector_plan();
        plan.segments = vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 10.0,
                runs: vec![ScanRun {
                    start_x_mm: 5.0,
                    end_x_mm: 25.0,
                    power_values: Vec::new(),
                }],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Binary,
            speed_mm_min: 2_000.0,
            layer_id: "layer".to_string(),
            cut_entry_id: "raster".to_string(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::zero(),
            overscan_mm: 0.0,
            outlines: Vec::new(),
            scan_axis: ScanAxis::default(),
            power_max_percent: 60.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }];

        let compiled =
            compile_m1_job(&plan, &GcodeConfig::default(), M1CompileConfig::default()).unwrap();

        assert!(compiled.gcode.contains("G0 X5.000 Y10.000"));
        assert!(compiled.gcode.contains("G1 X25.000 F2000 S600"));
    }

    #[test]
    fn rejects_unsafe_material_thickness() {
        let error = compile_m1_job(
            &vector_plan(),
            &GcodeConfig::default(),
            M1CompileConfig {
                material_thickness_mm: 17.0,
                ..M1CompileConfig::default()
            },
        )
        .unwrap_err();
        assert!(matches!(error, M1CompileError::InvalidMaterialThickness));
    }
}

//! Snapmaker 2.0 laser G-code generation from Beam Bench execution plans.
//!
//! Snapmaker 2.0 is Marlin-derived, but its documented laser contract is a
//! distinct dialect: `M3` selects constant power, `M4` selects dynamic power,
//! and both accept PWM `S` values in the inclusive 0-255 range without
//! Marlin's inline-mode `I` parameter.

use beambench_grbl::{GcodeConfig, GrblError, generate_gcode};
use beambench_planner::ExecutionPlan;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::MARLIN_FINISH_MOVES_COMMAND;

/// Snapmaker 2.0's documented constant or speed-adjusted laser behavior.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapmakerLaserMode {
    #[default]
    Constant,
    Dynamic,
}

/// Snapmaker-specific output settings layered over the established plan emitter.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SnapmakerGcodeConfig {
    pub gcode: GcodeConfig,
    #[serde(default)]
    pub laser_mode: SnapmakerLaserMode,
}

#[derive(Debug, Error)]
pub enum SnapmakerGcodeError {
    #[error("execution-plan G-code generation failed: {0}")]
    PlanGeneration(#[from] GrblError),
    #[error("execution-plan emitter returned an unexpected layout: {0}")]
    UnexpectedEmitterLayout(&'static str),
}

/// Emit a complete Snapmaker 2.0 job from an execution plan.
///
/// The generated dialect uses the vendor-documented PWM range, keeps custom
/// Start/End G-code untouched, forces explicit laser-off boundaries, and makes
/// the final `M400` acknowledgement the completion barrier.
pub fn generate_snapmaker_gcode(
    plan: &ExecutionPlan,
    config: &SnapmakerGcodeConfig,
) -> Result<Vec<String>, SnapmakerGcodeError> {
    let mut base = config.gcode.clone();
    let prefix = std::mem::take(&mut base.gcode_prefix);
    let suffix = std::mem::take(&mut base.gcode_suffix);

    base.s_value_max = 255;
    base.use_constant_power = config.laser_mode == SnapmakerLaserMode::Constant;
    base.emit_s_every_g1 = config.laser_mode == SnapmakerLaserMode::Dynamic;

    let generated = generate_gcode(plan, &base)?;
    validate_base_layout(&generated)?;
    let final_off_index = generated.iter().rposition(|line| line == "M5").ok_or(
        SnapmakerGcodeError::UnexpectedEmitterLayout("missing final laser-off command"),
    )?;
    if final_off_index <= 2 {
        return Err(SnapmakerGcodeError::UnexpectedEmitterLayout(
            "final laser-off command overlaps the preamble",
        ));
    }

    let mut lines = Vec::with_capacity(generated.len() + 5);
    for (index, line) in generated.into_iter().enumerate() {
        if index == 2 {
            lines.push("M5".to_string());
            lines.push(MARLIN_FINISH_MOVES_COMMAND.to_string());
            push_custom_gcode(&mut lines, &prefix);
        } else if index == final_off_index {
            push_custom_gcode(&mut lines, &suffix);
            lines.push("M5".to_string());
        } else {
            lines.push(line);
        }
    }
    lines.push(MARLIN_FINISH_MOVES_COMMAND.to_string());

    Ok(lines)
}

fn validate_base_layout(lines: &[String]) -> Result<(), SnapmakerGcodeError> {
    let expected = ["G90", "G21", "M5"];
    if lines.len() < expected.len()
        || lines
            .iter()
            .take(expected.len())
            .map(String::as_str)
            .ne(expected)
    {
        return Err(SnapmakerGcodeError::UnexpectedEmitterLayout(
            "expected G90, G21, M5 preamble",
        ));
    }
    Ok(())
}

fn push_custom_gcode(lines: &mut Vec<String>, block: &str) {
    lines.extend(
        block
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string),
    );
}

#[cfg(test)]
mod tests {
    use beambench_common::geometry::{Bounds, Point2D};
    use beambench_planner::{
        DirectionMode, PlanSegment, PowerMode, ScanAxis, ScanDirection, ScanRun, Scanline,
    };

    use super::*;

    fn plan(segments: Vec<PlanSegment>) -> ExecutionPlan {
        ExecutionPlan {
            id: Default::default(),
            project_id: Default::default(),
            revision_hash: "snapmaker-test".to_string(),
            created_at: Default::default(),
            bounds: Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
            total_distance_mm: 0.0,
            estimated_duration_secs: 0.0,
            segments,
            layer_order: vec![],
            warnings: vec![],
            failed_entries: vec![],
        }
    }

    #[test]
    fn empty_job_has_explicit_off_and_completion_boundaries() {
        let lines =
            generate_snapmaker_gcode(&plan(vec![]), &SnapmakerGcodeConfig::default()).unwrap();
        assert_eq!(
            lines,
            ["G90", "G21", "M5", "M400", "M5", "G0 X0 Y0", "M400"]
        );
    }

    #[test]
    fn constant_vector_uses_documented_pwm255_contract() {
        let segment = PlanSegment::Vector {
            polyline: vec![Point2D::new(1.0, 2.0), Point2D::new(3.0, 4.0)],
            closed: false,
            power_percent: 50.0,
            speed_mm_min: 1_200.0,
            layer_id: "layer".to_string(),
            cut_entry_id: "entry".to_string(),
            perforation_enabled: false,
            perforation_on_ms: 0.0,
            perforation_off_ms: 0.0,
            source_object_id: None,
            source_subpath_index: None,
        };

        let lines =
            generate_snapmaker_gcode(&plan(vec![segment]), &SnapmakerGcodeConfig::default())
                .unwrap();

        assert!(lines.iter().any(|line| line == "M3 S128"));
        assert!(!lines.iter().any(|line| line.contains(" I")));
    }

    #[test]
    fn dynamic_raster_uses_m4_without_marlin_inline_parameter() {
        let segment = PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 10.0,
                runs: vec![ScanRun {
                    start_x_mm: 0.0,
                    end_x_mm: 2.0,
                    power_values: vec![64, 192],
                }],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Grayscale,
            speed_mm_min: 2_000.0,
            layer_id: "raster".to_string(),
            cut_entry_id: "raster".to_string(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(1.0, 10.0),
            overscan_mm: 0.0,
            outlines: vec![],
            scan_axis: ScanAxis::Horizontal,
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 1.0,
        };
        let config = SnapmakerGcodeConfig {
            laser_mode: SnapmakerLaserMode::Dynamic,
            ..SnapmakerGcodeConfig::default()
        };

        let lines = generate_snapmaker_gcode(&plan(vec![segment]), &config).unwrap();

        assert!(lines.iter().any(|line| line == "M4 S64"));
        assert!(lines.iter().any(|line| line == "G1 X1.000 F2000 S64"));
        assert!(lines.iter().any(|line| line == "G1 X2.000 S192"));
        assert!(!lines.iter().any(|line| line.contains(" I")));
    }

    #[test]
    fn custom_gcode_is_preserved_verbatim() {
        let mut config = SnapmakerGcodeConfig::default();
        config.gcode.gcode_prefix = "M3 P80\nG4 P0.100".to_string();
        config.gcode.gcode_suffix = "M2000 L30 P1".to_string();

        let lines = generate_snapmaker_gcode(&plan(vec![]), &config).unwrap();

        assert!(lines.iter().any(|line| line == "M3 P80"));
        assert!(lines.iter().any(|line| line == "M2000 L30 P1"));
    }
}

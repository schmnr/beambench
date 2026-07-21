//! Marlin G-code generation from Beam Bench execution plans.
//!
//! Geometry emission stays on the established execution-plan path. This
//! adapter changes only Beam Bench-generated laser commands and job boundaries;
//! operator-provided Start/End G-code is inserted separately and never rewritten.

use beambench_grbl::{GcodeConfig, GrblError, generate_gcode};
use beambench_planner::ExecutionPlan;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    MARLIN_FINISH_MOVES_COMMAND, MarlinLaserCommandError, MarlinLaserCommands, MarlinLaserMode,
    MarlinPowerScale,
};

/// Marlin-specific output settings layered over the established plan emitter.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MarlinGcodeConfig {
    pub gcode: GcodeConfig,
    #[serde(default)]
    pub laser_mode: MarlinLaserMode,
    #[serde(default)]
    pub power_scale: MarlinPowerScale,
}

#[derive(Debug, Error)]
pub enum MarlinGcodeError {
    #[error(transparent)]
    LaserCommand(#[from] MarlinLaserCommandError),
    #[error("execution-plan G-code generation failed: {0}")]
    PlanGeneration(#[from] GrblError),
    #[error("execution-plan emitter returned an unexpected layout: {0}")]
    UnexpectedEmitterLayout(&'static str),
}

/// Emit a complete Marlin job from an execution plan.
///
/// Standard mode uses synchronized `M3 S…` changes. Continuous and dynamic
/// inline modes select `M3 I S…` or `M4 I S…` once, then repeat `S` values on
/// burn moves so power remains attached to Marlin's motion blocks without
/// synchronizing the planner for every power change.
pub fn generate_marlin_gcode(
    plan: &ExecutionPlan,
    config: &MarlinGcodeConfig,
) -> Result<Vec<String>, MarlinGcodeError> {
    let laser = MarlinLaserCommands::new(config.laser_mode, config.power_scale)?;
    let mut base = config.gcode.clone();
    let prefix = std::mem::take(&mut base.gcode_prefix);
    let suffix = std::mem::take(&mut base.gcode_suffix);

    base.s_value_max = laser.power_scale().maximum();
    base.use_constant_power = !matches!(config.laser_mode, MarlinLaserMode::DynamicInline);
    if !matches!(config.laser_mode, MarlinLaserMode::Standard) {
        base.emit_s_every_g1 = true;
    }

    let generated = generate_gcode(plan, &base)?;
    validate_base_layout(&generated)?;
    let final_off_index = generated.iter().rposition(|line| line == "M5").ok_or(
        MarlinGcodeError::UnexpectedEmitterLayout("missing final laser-off command"),
    )?;
    if final_off_index <= 2 {
        return Err(MarlinGcodeError::UnexpectedEmitterLayout(
            "final laser-off command overlaps the preamble",
        ));
    }

    let boundary = laser.boundary_commands();
    let mut lines = Vec::with_capacity(generated.len() + 5);
    let mut inline_mode_selected = false;
    for (index, line) in generated.into_iter().enumerate() {
        if index == 2 {
            lines.extend(boundary.map(str::to_string));
            push_custom_gcode(&mut lines, &prefix);
        } else if index == final_off_index {
            push_custom_gcode(&mut lines, &suffix);
            lines.push(boundary[0].to_string());
        } else {
            if let Some(line) =
                adapt_generated_laser_command(line, config.laser_mode, &mut inline_mode_selected)
            {
                lines.push(line);
            }
        }
    }
    // The established emitter may place a configured finish-position move
    // after its final M5. Keep the laser-off command before that travel, then
    // make M400 the actual final command so its acknowledgement proves every
    // queued move has completed.
    lines.push(MARLIN_FINISH_MOVES_COMMAND.to_string());

    Ok(lines)
}

fn validate_base_layout(lines: &[String]) -> Result<(), MarlinGcodeError> {
    let expected = ["G90", "G21", "M5"];
    if lines.len() < expected.len()
        || lines
            .iter()
            .take(expected.len())
            .map(String::as_str)
            .ne(expected)
    {
        return Err(MarlinGcodeError::UnexpectedEmitterLayout(
            "expected G90, G21, M5 preamble",
        ));
    }
    Ok(())
}

fn adapt_generated_laser_command(
    line: String,
    mode: MarlinLaserMode,
    inline_mode_selected: &mut bool,
) -> Option<String> {
    let (source, replacement) = match mode {
        MarlinLaserMode::Standard => return Some(line),
        MarlinLaserMode::ContinuousInline => ("M3 S", "M3 I S"),
        MarlinLaserMode::DynamicInline => ("M4 S", "M4 I S"),
    };

    let Some(power) = line.strip_prefix(source) else {
        return Some(line);
    };
    if *inline_mode_selected {
        return None;
    }
    *inline_mode_selected = true;
    Some(format!("{replacement}{power}"))
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
            revision_hash: "marlin-test".to_string(),
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
    fn empty_standard_job_has_explicit_boundaries() {
        let lines = generate_marlin_gcode(&plan(vec![]), &MarlinGcodeConfig::default()).unwrap();
        assert_eq!(
            lines,
            ["G90", "G21", "M5", "M400", "M5", "G0 X0 Y0", "M400"]
        );
    }

    #[test]
    fn custom_gcode_is_not_rewritten_as_generated_marlin_commands() {
        let mut config = MarlinGcodeConfig {
            laser_mode: MarlinLaserMode::DynamicInline,
            ..MarlinGcodeConfig::default()
        };
        config.gcode.gcode_prefix = "M4 S77\nG4 P0.1".to_string();
        config.gcode.gcode_suffix = "M3 S12".to_string();

        let lines = generate_marlin_gcode(&plan(vec![]), &config).unwrap();
        assert!(lines.iter().any(|line| line == "M4 S77"));
        assert!(lines.iter().any(|line| line == "M3 S12"));
        assert!(!lines.iter().any(|line| line == "M4 I S77"));
    }

    #[test]
    fn invalid_power_scale_fails_before_plan_emission() {
        let config = MarlinGcodeConfig {
            power_scale: MarlinPowerScale::Custom { maximum: 0 },
            ..MarlinGcodeConfig::default()
        };
        assert!(matches!(
            generate_marlin_gcode(&plan(vec![]), &config),
            Err(MarlinGcodeError::LaserCommand(
                MarlinLaserCommandError::ZeroPowerMaximum
            ))
        ));
    }

    #[test]
    fn established_air_assist_z_and_finish_features_survive_adaptation() {
        let segment = PlanSegment::Vector {
            polyline: vec![Point2D::new(1.0, 2.0), Point2D::new(3.0, 4.0)],
            closed: false,
            power_percent: 50.0,
            speed_mm_min: 1_000.0,
            layer_id: "layer".to_string(),
            cut_entry_id: "air-z".to_string(),
            perforation_enabled: false,
            perforation_on_ms: 0.0,
            perforation_off_ms: 0.0,
            source_object_id: None,
            source_subpath_index: None,
        };
        let mut config = MarlinGcodeConfig {
            laser_mode: MarlinLaserMode::ContinuousInline,
            ..MarlinGcodeConfig::default()
        };
        config.gcode.air_assist_cut_entry_ids = vec!["air-z".to_string()];
        config.gcode.air_assist_on_gcode = "M8".to_string();
        config.gcode.air_assist_off_gcode = "M9".to_string();
        config.gcode.air_assist_on_delay_ms = 250;
        config.gcode.z_moves_enabled = true;
        config.gcode.z_base_mm = 1.0;
        config.gcode.z_move_feed_mm_min = 300.0;
        config.gcode.z_offset_cut_entry_ids = vec![("air-z".to_string(), 2.0)];
        config.gcode.finish_position = beambench_core::FinishPosition::CustomXY;
        config.gcode.finish_x = Some(9.0);
        config.gcode.finish_y = Some(8.0);

        let lines = generate_marlin_gcode(&plan(vec![segment]), &config).unwrap();
        for expected in [
            "M8",
            "G4 P0.250",
            "G1 Z3.000 F300",
            "M3 I S128",
            "M9",
            "G1 Z1.000 F300",
            "G0 X9.000 Y8.000",
        ] {
            assert!(lines.iter().any(|line| line == expected), "{expected}");
        }
    }

    #[test]
    fn inline_raster_selects_mode_once_and_changes_power_on_motion() {
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
        let config = MarlinGcodeConfig {
            laser_mode: MarlinLaserMode::DynamicInline,
            ..MarlinGcodeConfig::default()
        };

        let lines = generate_marlin_gcode(&plan(vec![segment]), &config).unwrap();
        assert_eq!(
            lines
                .iter()
                .filter(|line| line.starts_with("M4 I S"))
                .count(),
            1
        );
        assert!(lines.iter().any(|line| line == "G1 X1.000 F2000 S64"));
        assert!(lines.iter().any(|line| line == "G1 X2.000 S192"));
    }
}

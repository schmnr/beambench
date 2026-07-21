//! Smoothieware G-code generation from Beam Bench execution plans.
//!
//! Geometry emission stays on the established execution-plan path. This
//! adapter removes generated GRBL spindle commands, attaches an explicit `S`
//! value to every generated feed move, and preserves operator-provided
//! Start/End G-code without rewriting it.

use beambench_grbl::{GcodeConfig, GrblError, generate_gcode};
use beambench_planner::ExecutionPlan;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    SMOOTHIEWARE_FINISH_MOVES_COMMAND, SmoothiewareLaserCommandError, SmoothiewareLaserCommands,
    SmoothiewarePowerMode, SmoothiewarePowerScale,
};

const INTERNAL_POWER_MAXIMUM: u32 = 1_000;

/// Smoothieware-specific output settings layered over the plan emitter.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SmoothiewareGcodeConfig {
    pub gcode: GcodeConfig,
    #[serde(default)]
    pub power_mode: SmoothiewarePowerMode,
    #[serde(default)]
    pub power_scale: SmoothiewarePowerScale,
}

#[derive(Debug, Error)]
pub enum SmoothiewareGcodeError {
    #[error(transparent)]
    LaserCommand(#[from] SmoothiewareLaserCommandError),
    #[error("execution-plan G-code generation failed: {0}")]
    PlanGeneration(#[from] GrblError),
    #[error("execution-plan emitter returned an unexpected layout: {0}")]
    UnexpectedEmitterLayout(&'static str),
    #[error("execution-plan emitter produced an invalid power word: {0}")]
    InvalidGeneratedPower(String),
}

/// Emit a complete Smoothieware job from an execution plan.
///
/// Smoothieware stores `S` modally and laser builds default that modal value to
/// a nonzero number. Every Beam Bench-generated `G1`/`G2`/`G3` therefore gets
/// an explicit `S`, including `S0` for motion-only frames and Z moves. Generated
/// `M5` boundaries become `M400`: once queued motion is idle, the firmware's
/// laser module has no active cutting block and turns its output off.
pub fn generate_smoothieware_gcode(
    plan: &ExecutionPlan,
    config: &SmoothiewareGcodeConfig,
) -> Result<Vec<String>, SmoothiewareGcodeError> {
    let laser = SmoothiewareLaserCommands::new(config.power_mode, config.power_scale)?;
    let mut base = config.gcode.clone();
    let prefix = std::mem::take(&mut base.gcode_prefix);
    let suffix = std::mem::take(&mut base.gcode_suffix);
    base.s_value_max = INTERNAL_POWER_MAXIMUM;
    base.emit_s_every_g1 = true;
    base.use_constant_power = true;

    let generated = generate_gcode(plan, &base)?;
    validate_base_layout(&generated)?;
    let final_off_index = generated.iter().rposition(|line| line == "M5").ok_or(
        SmoothiewareGcodeError::UnexpectedEmitterLayout("missing final laser-off command"),
    )?;
    if final_off_index <= 2 {
        return Err(SmoothiewareGcodeError::UnexpectedEmitterLayout(
            "final laser-off command overlaps the preamble",
        ));
    }

    let mut lines = Vec::with_capacity(generated.len() + 4);
    for (index, line) in generated.into_iter().enumerate() {
        if index == 2 {
            push_distinct(&mut lines, SMOOTHIEWARE_FINISH_MOVES_COMMAND);
            lines.push(laser.mode_command().to_string());
            push_custom_gcode(&mut lines, &prefix);
            continue;
        }
        if index == final_off_index {
            push_distinct(&mut lines, SMOOTHIEWARE_FINISH_MOVES_COMMAND);
            push_custom_gcode(&mut lines, &suffix);
            continue;
        }

        if is_generated_laser_activation(&line) {
            continue;
        }
        if line == "M5" {
            push_distinct(&mut lines, SMOOTHIEWARE_FINISH_MOVES_COMMAND);
            continue;
        }
        lines.push(adapt_generated_motion(line, &laser)?);
    }
    push_distinct(&mut lines, SMOOTHIEWARE_FINISH_MOVES_COMMAND);

    Ok(lines)
}

fn validate_base_layout(lines: &[String]) -> Result<(), SmoothiewareGcodeError> {
    let expected = ["G90", "G21", "M5"];
    if lines.len() < expected.len()
        || lines
            .iter()
            .take(expected.len())
            .map(String::as_str)
            .ne(expected)
    {
        return Err(SmoothiewareGcodeError::UnexpectedEmitterLayout(
            "expected G90, G21, M5 preamble",
        ));
    }
    Ok(())
}

fn is_generated_laser_activation(line: &str) -> bool {
    line.starts_with("M3 S") || line.starts_with("M4 S")
}

fn adapt_generated_motion(
    line: String,
    laser: &SmoothiewareLaserCommands,
) -> Result<String, SmoothiewareGcodeError> {
    if !is_laser_motion(&line) {
        return Ok(line);
    }

    let mut saw_power = false;
    let mut tokens = Vec::new();
    for token in line.split_ascii_whitespace() {
        if let Some(raw_power) = token.strip_prefix('S') {
            let internal_power = raw_power
                .parse::<u32>()
                .map_err(|_| SmoothiewareGcodeError::InvalidGeneratedPower(token.to_string()))?;
            if internal_power > INTERNAL_POWER_MAXIMUM {
                return Err(SmoothiewareGcodeError::InvalidGeneratedPower(
                    token.to_string(),
                ));
            }
            let percent = f64::from(internal_power) / f64::from(INTERNAL_POWER_MAXIMUM) * 100.0;
            tokens.push(laser.motion_power_word(percent)?);
            saw_power = true;
        } else {
            tokens.push(token.to_string());
        }
    }
    if !saw_power {
        tokens.push("S0".to_string());
    }
    Ok(tokens.join(" "))
}

fn is_laser_motion(line: &str) -> bool {
    ["G1", "G2", "G3"]
        .iter()
        .any(|command| line == *command || line.starts_with(&format!("{command} ")))
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

fn push_distinct(lines: &mut Vec<String>, line: &str) {
    if lines.last().is_none_or(|last| last != line) {
        lines.push(line.to_string());
    }
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
            revision_hash: "smoothieware-test".to_string(),
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
    fn empty_job_has_explicit_mode_and_completion_boundaries() {
        let lines = generate_smoothieware_gcode(&plan(vec![]), &SmoothiewareGcodeConfig::default())
            .unwrap();
        assert_eq!(
            lines,
            [
                "G90",
                "G21",
                "M400",
                "M221 S100 P0",
                "M400",
                "G0 X0 Y0",
                "M400"
            ]
        );
    }

    #[test]
    fn every_generated_feed_move_has_explicit_power() {
        let segments = vec![
            PlanSegment::Frame {
                path: vec![Point2D::new(1.0, 2.0), Point2D::new(3.0, 4.0)],
                power_percent: 0.0,
                speed_mm_min: 1_000.0,
            },
            PlanSegment::Vector {
                polyline: vec![Point2D::new(4.0, 5.0), Point2D::new(6.0, 7.0)],
                closed: false,
                power_percent: 50.0,
                speed_mm_min: 1_200.0,
                layer_id: "vector".to_string(),
                cut_entry_id: "vector".to_string(),
                perforation_enabled: false,
                perforation_on_ms: 0.0,
                perforation_off_ms: 0.0,
                source_object_id: None,
                source_subpath_index: None,
            },
        ];
        let lines =
            generate_smoothieware_gcode(&plan(segments), &SmoothiewareGcodeConfig::default())
                .unwrap();
        let feeds: Vec<_> = lines
            .iter()
            .filter(|line| line.starts_with("G1 "))
            .collect();
        assert!(!feeds.is_empty());
        assert!(feeds.iter().all(|line| line.contains(" S")), "{feeds:?}");
        assert!(lines.iter().any(|line| line == "G1 X3.000 Y4.000 F1000 S0"));
        assert!(
            lines
                .iter()
                .any(|line| line == "G1 X6.000 Y7.000 F1200 S0.5")
        );
    }

    #[test]
    fn raster_power_is_scaled_on_each_motion_block() {
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
        let lines =
            generate_smoothieware_gcode(&plan(vec![segment]), &SmoothiewareGcodeConfig::default())
                .unwrap();
        assert!(lines.iter().any(|line| line == "G1 X1.000 F2000 S0.251"));
        assert!(lines.iter().any(|line| line == "G1 X2.000 S0.753"));
        assert!(!lines.iter().any(|line| line.starts_with("M3 S")));
        assert!(!lines.iter().any(|line| line.starts_with("M4 S")));
        assert!(!lines.iter().any(|line| line == "M5"));
    }

    #[test]
    fn perforation_uses_powered_feed_moves_and_unpowered_rapid_gaps() {
        let segment = PlanSegment::Vector {
            polyline: vec![Point2D::new(0.0, 0.0), Point2D::new(10.0, 0.0)],
            closed: false,
            power_percent: 50.0,
            speed_mm_min: 6_000.0,
            layer_id: "perforation".to_string(),
            cut_entry_id: "perforation".to_string(),
            perforation_enabled: true,
            perforation_on_ms: 20.0,
            perforation_off_ms: 10.0,
            source_object_id: None,
            source_subpath_index: None,
        };
        let lines =
            generate_smoothieware_gcode(&plan(vec![segment]), &SmoothiewareGcodeConfig::default())
                .unwrap();
        assert!(lines.iter().any(|line| line == "G1 X2.000 Y0.000 S0.5"));
        assert!(lines.iter().any(|line| line == "G0 X3.000 Y0.000"));
        assert!(lines.iter().any(|line| line == "G1 X5.000 Y0.000 S0.5"));
        assert!(!lines.iter().any(|line| line.starts_with("M3 S")));
        assert!(!lines.iter().any(|line| line == "M5"));
    }

    #[test]
    fn operator_start_and_end_gcode_are_preserved_verbatim() {
        let mut config = SmoothiewareGcodeConfig::default();
        config.gcode.gcode_prefix = "M4 S77\nG1 X9 S0.25".to_string();
        config.gcode.gcode_suffix = "M5\nfire off".to_string();
        let lines = generate_smoothieware_gcode(&plan(vec![]), &config).unwrap();
        for expected in ["M4 S77", "G1 X9 S0.25", "M5", "fire off"] {
            assert!(lines.iter().any(|line| line == expected), "{expected}");
        }
    }
}

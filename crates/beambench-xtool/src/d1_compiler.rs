use beambench_grbl::{GcodeConfig, GrblError, generate_gcode_to};
use beambench_planner::{ExecutionPlan, calculate_plan_bounds, translate_segments};

pub const D1_MAX_JOB_BYTES: usize = 32 * 1024 * 1024;

#[derive(Debug, Clone, Copy)]
pub struct D1CompileConfig {
    pub width_mm: f64,
    pub height_mm: f64,
    pub frame: bool,
}

#[derive(Debug, Clone)]
pub struct D1CompiledJob {
    pub gcode: String,
    pub line_count: usize,
    pub frame: bool,
}

/// Native D-series file output uses inline G1 power, not the GRBL serial
/// compatibility handshake. Its G92 origin is the current laser spot.
pub fn compile_d1_job(
    plan: &ExecutionPlan,
    base: &GcodeConfig,
    config: D1CompileConfig,
) -> Result<D1CompiledJob, String> {
    if !config.width_mm.is_finite()
        || !config.height_mm.is_finite()
        || config.width_mm <= 0.0
        || config.height_mm <= 0.0
        || config.width_mm > 432.0
        || config.height_mm > 406.0
    {
        return Err("xTool D1 requires a valid stock-frame workspace".into());
    }
    if !base.gcode_prefix.trim().is_empty()
        || !base.gcode_suffix.trim().is_empty()
        || !base.air_assist_cut_entry_ids.is_empty()
        || base.z_moves_enabled
        || base.rotary.is_some()
        || base.use_relative_coords
        || base
            .z_offset_cut_entry_ids
            .iter()
            .any(|(_, z)| !z.is_finite() || z.abs() > 1e-9)
        || base.s_value_max != 1000
    {
        return Err(
            "xTool D1 requires S1000 and absolute file coordinates; custom G-code, automatic air, Z moves and rotary output are unavailable"
                .into(),
        );
    }
    let bounds = calculate_plan_bounds(&plan.segments);
    if ![bounds.min.x, bounds.min.y, bounds.max.x, bounds.max.y]
        .iter()
        .all(|v| v.is_finite())
        || bounds.max.x.max(0.0) - bounds.min.x.min(0.0) > config.width_mm + 1e-6
        || bounds.max.y.max(0.0) - bounds.min.y.min(0.0) > config.height_mm + 1e-6
    {
        return Err("xTool D1 output exceeds the stock workspace".into());
    }
    // Keep the job in nonnegative file coordinates without changing its
    // physical position relative to the current laser spot.
    let dx = (-bounds.min.x).max(0.0);
    let dy = (-bounds.min.y).max(0.0);
    let mut shifted;
    let plan = if dx > 0.0 || dy > 0.0 {
        shifted = plan.clone();
        translate_segments(&mut shifted.segments, dx, dy);
        shifted.bounds = calculate_plan_bounds(&shifted.segments);
        &shifted
    } else {
        plan
    };
    let mut config_gcode = base.clone();
    config_gcode.s_value_max = 1000;
    config_gcode.emit_s_every_g1 = true;
    config_gcode.use_g0_for_overscan = false;
    config_gcode.finish_position = beambench_core::FinishPosition::DontMove;
    let mut gcode = format!(
        "M17 S1\nM106 S0\nM205 X{:.3} Y{:.3}\nM101\nG92 X{dx:.3} Y{dy:.3}\nG90\nG0 F3000\nG1 F1000\nG1 S0\n",
        config.width_mm, config.height_mm
    );
    let mut count = 9;
    let mut push = |line: String| -> Result<(), GrblError> {
        if gcode.len() + line.len() + 1 > D1_MAX_JOB_BYTES {
            return Err(GrblError::GcodeError(
                "xTool D1 job exceeds the 32 MiB limit; reduce image detail".into(),
            ));
        }
        gcode.push_str(&line);
        gcode.push('\n');
        count += 1;
        Ok(())
    };
    generate_gcode_to(plan, &config_gcode, &mut |line| {
        let line = line.trim();
        if line == "G21" || line == "G90" {
            return Ok(());
        }
        if let Some(power) = line
            .strip_prefix("M3 S")
            .or_else(|| line.strip_prefix("M4 S"))
        {
            return push(format!("G1 S{}", if config.frame { "0" } else { power }));
        }
        if line == "M5" {
            return push("G1 S0".into());
        }
        let command = line.split_ascii_whitespace().next().unwrap_or_default();
        if !matches!(command, "G0" | "G1") {
            return Err(GrblError::GcodeError(format!(
                "xTool D1 does not support generated command {command}"
            )));
        }
        let output = if config.frame {
            line.split_ascii_whitespace()
                .map(|word| if word.starts_with('S') { "S0" } else { word })
                .collect::<Vec<_>>()
                .join(" ")
        } else {
            line.to_string()
        };
        push(output)
    })
    .map_err(|error| error.to_string())?;
    push("G1 S0".into()).map_err(|error| error.to_string())?;
    if config.frame {
        push(format!("G0 X{dx:.3} Y{dy:.3}")).map_err(|error| error.to_string())?;
    }
    push("M18".into()).map_err(|error| error.to_string())?;
    Ok(D1CompiledJob {
        gcode,
        line_count: count,
        frame: config.frame,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use beambench_common::geometry::{Bounds, Point2D};
    use beambench_planner::{
        DirectionMode, PlanSegment, PowerMode, ScanAxis, ScanDirection, ScanRun, Scanline,
    };
    use chrono::Utc;
    use uuid::Uuid;
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

    fn config(frame: bool) -> D1CompileConfig {
        D1CompileConfig {
            width_mm: 430.0,
            height_mm: 400.0,
            frame,
        }
    }
    #[test]
    fn native_vector_power_orientation_and_finish_are_preserved() {
        let out = compile_d1_job(&vector_plan(), &GcodeConfig::default(), config(false)).unwrap();
        assert!(
            out.gcode
                .starts_with("M17 S1\nM106 S0\nM205 X430.000 Y400.000\nM101\nG92 X0.000 Y0.000\n")
        );
        assert!(out.gcode.contains("G1 X30.000 Y20.000"));
        assert!(out.gcode.contains("S500"));
        assert!(!out.gcode.contains("M3 "));
        assert!(!out.gcode.contains("M4 "));
        assert!(!out.gcode.contains("M5"));
        assert!(out.gcode.ends_with("G1 S0\nM18\n"));
        assert!(!out.gcode.contains("G0 X0 Y0"));
        assert_eq!(out.line_count, out.gcode.lines().count());
    }
    #[test]
    fn centered_job_origin_stays_at_the_same_physical_laser_spot() {
        let mut plan = vector_plan();
        translate_segments(&mut plan.segments, -20.0, -30.0);
        let out = compile_d1_job(&plan, &GcodeConfig::default(), config(false)).unwrap();
        assert!(out.gcode.contains("G92 X10.000 Y10.000"));
        assert!(out.gcode.contains("G1 X20.000 Y0.000"));
        let frame = compile_d1_job(&plan, &GcodeConfig::default(), config(true)).unwrap();
        assert!(frame.gcode.ends_with("G1 S0\nG0 X10.000 Y10.000\nM18\n"));
        for line in frame.gcode.lines().filter(|line| line.starts_with("G1")) {
            assert!(
                line.split_ascii_whitespace()
                    .filter_map(|word| word.strip_prefix('S'))
                    .all(|s| s == "0")
            );
        }
    }
    #[test]
    fn unsupported_settings_and_dwell_output_fail_before_transfer() {
        let plan = vector_plan();
        for base in [
            GcodeConfig {
                s_value_max: 255,
                ..Default::default()
            },
            GcodeConfig {
                gcode_prefix: "M8".into(),
                ..Default::default()
            },
            GcodeConfig {
                air_assist_cut_entry_ids: vec!["cut".into()],
                ..Default::default()
            },
            GcodeConfig {
                z_offset_cut_entry_ids: vec![("cut".into(), 1.0)],
                ..Default::default()
            },
        ] {
            assert!(compile_d1_job(&plan, &base, config(false)).is_err());
        }
        let mut plan = plan;
        if let PlanSegment::Vector {
            perforation_enabled,
            perforation_on_ms,
            perforation_off_ms,
            ..
        } = &mut plan.segments[0]
        {
            *perforation_enabled = true;
            *perforation_on_ms = 10.0;
            *perforation_off_ms = 10.0;
        }
        assert!(compile_d1_job(&plan, &GcodeConfig::default(), config(false)).is_err());
    }
    #[test]
    fn workspace_limit_includes_travel_from_the_anchor() {
        let mut plan = vector_plan();
        translate_segments(&mut plan.segments, 500.0, 0.0);
        assert!(compile_d1_job(&plan, &GcodeConfig::default(), config(false)).is_err());
    }
    #[test]
    fn raster_retains_motion_and_inline_power() {
        let mut plan = vector_plan();
        plan.segments = vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 10.0,
                runs: vec![ScanRun {
                    start_x_mm: 5.0,
                    end_x_mm: 25.0,
                    power_values: Vec::new(),
                }]
                .into(),
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

        let out = compile_d1_job(&plan, &GcodeConfig::default(), config(false)).unwrap();
        assert!(out.gcode.contains("G0 X5.000 Y10.000"));
        assert!(out.gcode.contains("G1 X25.000 F2000 S600"));
    }
}

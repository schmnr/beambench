//! G-code generation from ExecutionPlan segments.

use crate::error::GrblError;
use beambench_common::geometry::Point2D;
use beambench_core::{FinishPosition, FocusTestZMode, TransferMode};
use beambench_planner::{
    ExecutionPlan, PlanSegment, PowerMode, ScanAxis, ScanDirection, Scanline, z_offset_command,
};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

/// Configuration for G-code generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GcodeConfig {
    pub laser_mode: bool,
    pub use_relative_coords: bool,
    /// Custom G-code lines injected after the preamble (before any segments).
    #[serde(default)]
    pub gcode_prefix: String,
    /// Custom G-code lines injected before the postamble (after all segments).
    #[serde(default)]
    pub gcode_suffix: String,
    /// Where the laser moves after the job completes.
    #[serde(default)]
    pub finish_position: FinishPosition,
    /// Custom X coordinate for FinishPosition::CustomXY.
    #[serde(default)]
    pub finish_x: Option<f64>,
    /// Custom Y coordinate for FinishPosition::CustomXY.
    #[serde(default)]
    pub finish_y: Option<f64>,
    /// Use constant power (M3) instead of dynamic power (M4).
    #[serde(default)]
    pub use_constant_power: bool,
    /// Repeat S value on every G1 line.
    #[serde(default)]
    pub emit_s_every_g1: bool,
    /// Maximum controller S-value that represents 100% power.
    pub s_value_max: u32,
    /// Use rapid G0 moves for overscan approach moves.
    pub use_g0_for_overscan: bool,
    /// Flattened scanning-offset table: (speed_mm_min, offset_mm) pairs, sorted by speed.
    #[serde(default)]
    pub scanning_offsets: Vec<(f64, f64)>,
    /// Whether scanning-offset compensation is active.
    #[serde(default)]
    pub enable_scanning_offset: bool,
    /// Cut-entry IDs whose generated segments should run with air assist enabled.
    #[serde(default)]
    pub air_assist_cut_entry_ids: Vec<String>,
    /// Air-assist command emitted when entering air-enabled GRBL output.
    #[serde(default = "default_air_assist_on_gcode")]
    pub air_assist_on_gcode: String,
    /// Air-assist command emitted when leaving air-enabled GRBL output.
    #[serde(default = "default_air_assist_off_gcode")]
    pub air_assist_off_gcode: String,
    /// Delay after every air-on command, in milliseconds.
    #[serde(default)]
    pub air_assist_on_delay_ms: u32,
    /// GRBL streaming mode.
    #[serde(default)]
    pub transfer_mode: TransferMode,
    /// Whether generated GRBL should emit Z moves for cut-entry offsets.
    #[serde(default)]
    pub z_moves_enabled: bool,
    /// Controlled feed rate for Z moves, in mm/min.
    #[serde(default = "default_z_move_feed_mm_min")]
    pub z_move_feed_mm_min: f64,
    /// Base absolute Z target before per-entry offsets are applied.
    #[serde(default)]
    pub z_base_mm: f64,
    /// Z movement interpretation for per-entry offsets.
    #[serde(default)]
    pub z_mode: FocusTestZMode,
    /// Cut-entry IDs and their configured Z offsets in mm.
    #[serde(default)]
    pub z_offset_cut_entry_ids: Vec<(String, f64)>,
}

fn default_air_assist_on_gcode() -> String {
    "M7".to_string()
}

fn default_air_assist_off_gcode() -> String {
    "M9".to_string()
}

fn default_z_move_feed_mm_min() -> f64 {
    300.0
}

impl Default for GcodeConfig {
    fn default() -> Self {
        Self {
            laser_mode: true,
            use_relative_coords: false,
            gcode_prefix: String::new(),
            gcode_suffix: String::new(),
            finish_position: FinishPosition::Origin,
            finish_x: None,
            finish_y: None,
            use_constant_power: false,
            emit_s_every_g1: false,
            s_value_max: 1000,
            use_g0_for_overscan: true,
            scanning_offsets: Vec::new(),
            enable_scanning_offset: false,
            air_assist_cut_entry_ids: Vec::new(),
            air_assist_on_gcode: default_air_assist_on_gcode(),
            air_assist_off_gcode: default_air_assist_off_gcode(),
            air_assist_on_delay_ms: 0,
            transfer_mode: TransferMode::Buffered,
            z_moves_enabled: false,
            z_move_feed_mm_min: default_z_move_feed_mm_min(),
            z_base_mm: 0.0,
            z_mode: FocusTestZMode::AbsoluteWorkCoord,
            z_offset_cut_entry_ids: Vec::new(),
        }
    }
}

/// Convert an ExecutionPlan into a sequence of G-code lines.
pub fn generate_gcode(
    plan: &ExecutionPlan,
    config: &GcodeConfig,
) -> Result<Vec<String>, GrblError> {
    let mut lines = vec![
        "G90".to_string(), // Absolute positioning
        "G21".to_string(), // Metric units
        "M5".to_string(),  // Laser off
    ];

    // Inject user prefix
    if !config.gcode_prefix.is_empty() {
        for line in config.gcode_prefix.lines() {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                lines.push(trimmed.to_string());
            }
        }
    }

    let air_assist_cut_entry_ids: HashSet<&str> = config
        .air_assist_cut_entry_ids
        .iter()
        .map(String::as_str)
        .collect();
    let z_offsets: HashMap<&str, f64> = config
        .z_offset_cut_entry_ids
        .iter()
        .map(|(id, z)| (id.as_str(), *z))
        .collect();
    let mut air_assist_active = false;
    let mut current_absolute_z: Option<f64> = None;
    let mut current_relative_z_offset = 0.0;

    for segment in &plan.segments {
        let wants_air_assist =
            segment_cut_entry_id(segment).is_some_and(|id| air_assist_cut_entry_ids.contains(id));
        if wants_air_assist && !air_assist_active {
            push_custom_gcode(&mut lines, &config.air_assist_on_gcode);
            if config.air_assist_on_delay_ms > 0 {
                lines.push(format!(
                    "G4 P{:.3}",
                    config.air_assist_on_delay_ms as f64 / 1000.0
                ));
            }
            air_assist_active = true;
        } else if !wants_air_assist && air_assist_active {
            push_custom_gcode(&mut lines, &config.air_assist_off_gcode);
            air_assist_active = false;
        }
        emit_z_if_needed(
            &mut lines,
            segment,
            config,
            &z_offsets,
            &mut current_absolute_z,
            &mut current_relative_z_offset,
        );
        generate_segment(&mut lines, segment, config)?;
    }

    if air_assist_active {
        push_custom_gcode(&mut lines, &config.air_assist_off_gcode);
    }

    emit_final_z_return(
        &mut lines,
        config,
        current_absolute_z,
        current_relative_z_offset,
    );

    // Inject user suffix
    if !config.gcode_suffix.is_empty() {
        for line in config.gcode_suffix.lines() {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                lines.push(trimmed.to_string());
            }
        }
    }

    // Postamble
    lines.push("M5".to_string()); // Laser off
    match config.finish_position {
        FinishPosition::Origin => lines.push("G0 X0 Y0".to_string()),
        FinishPosition::DontMove => { /* no movement command */ }
        FinishPosition::CustomXY => {
            let x = config.finish_x.unwrap_or(0.0);
            let y = config.finish_y.unwrap_or(0.0);
            lines.push(format!("G0 X{x:.3} Y{y:.3}"));
        }
    }

    Ok(lines)
}

fn push_custom_gcode(lines: &mut Vec<String>, block: &str) {
    for line in block.lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            lines.push(trimmed.to_string());
        }
    }
}

fn z_values_equal(a: f64, b: f64) -> bool {
    (a - b).abs() <= 0.000_5
}

fn emit_z_if_needed(
    lines: &mut Vec<String>,
    segment: &PlanSegment,
    config: &GcodeConfig,
    z_offsets: &HashMap<&str, f64>,
    current_absolute_z: &mut Option<f64>,
    current_relative_z_offset: &mut f64,
) {
    if !config.z_moves_enabled {
        return;
    }
    let Some(cut_entry_id) = segment_cut_entry_id(segment) else {
        return;
    };
    let target_offset = z_offsets.get(cut_entry_id).copied().unwrap_or(0.0);
    match config.z_mode {
        FocusTestZMode::AbsoluteWorkCoord => {
            if current_absolute_z.is_none() && z_values_equal(target_offset, 0.0) {
                return;
            }
            let target_z = config.z_base_mm + target_offset;
            if current_absolute_z.is_none_or(|current| !z_values_equal(current, target_z)) {
                push_custom_gcode(
                    lines,
                    &z_offset_command(
                        target_z,
                        FocusTestZMode::AbsoluteWorkCoord,
                        config.z_move_feed_mm_min,
                    ),
                );
                *current_absolute_z = Some(target_z);
            }
        }
        FocusTestZMode::RelativeTemporary => {
            let delta = target_offset - *current_relative_z_offset;
            if !z_values_equal(delta, 0.0) {
                push_custom_gcode(
                    lines,
                    &z_offset_command(
                        delta,
                        FocusTestZMode::RelativeTemporary,
                        config.z_move_feed_mm_min,
                    ),
                );
                *current_relative_z_offset = target_offset;
            }
        }
    }
}

fn emit_final_z_return(
    lines: &mut Vec<String>,
    config: &GcodeConfig,
    current_absolute_z: Option<f64>,
    current_relative_z_offset: f64,
) {
    if !config.z_moves_enabled {
        return;
    }
    match config.z_mode {
        FocusTestZMode::AbsoluteWorkCoord => {
            if current_absolute_z.is_some_and(|current| !z_values_equal(current, config.z_base_mm))
            {
                push_custom_gcode(
                    lines,
                    &z_offset_command(
                        config.z_base_mm,
                        FocusTestZMode::AbsoluteWorkCoord,
                        config.z_move_feed_mm_min,
                    ),
                );
            }
        }
        FocusTestZMode::RelativeTemporary => {
            if !z_values_equal(current_relative_z_offset, 0.0) {
                push_custom_gcode(
                    lines,
                    &z_offset_command(
                        -current_relative_z_offset,
                        FocusTestZMode::RelativeTemporary,
                        config.z_move_feed_mm_min,
                    ),
                );
            }
        }
    }
}

fn segment_cut_entry_id(segment: &PlanSegment) -> Option<&str> {
    match segment {
        PlanSegment::Vector { cut_entry_id, .. } | PlanSegment::Raster { cut_entry_id, .. } => {
            Some(cut_entry_id.as_str())
        }
        _ => None,
    }
}

/// Rotate a local-space point to world-space around scan_origin.
fn rotate_to_world(x: f64, y: f64, origin: &Point2D, cos_a: f64, sin_a: f64) -> (f64, f64) {
    (
        origin.x + x * cos_a - y * sin_a,
        origin.y + x * sin_a + y * cos_a,
    )
}

/// Return the laser-on command string: M3 or M4 based on config.
fn laser_on_command(config: &GcodeConfig, s_value: u32) -> String {
    if config.use_constant_power {
        format!("M3 S{s_value}")
    } else {
        format!("M4 S{s_value}")
    }
}

/// Return the laser-on-at-zero command: "M3 S0" or "M4 S0".
fn laser_on_zero(config: &GcodeConfig) -> String {
    if config.use_constant_power {
        "M3 S0".to_string()
    } else {
        "M4 S0".to_string()
    }
}

fn power_suffix(config: &GcodeConfig, s_value: u32) -> String {
    if config.emit_s_every_g1 {
        format!(" S{s_value}")
    } else {
        String::new()
    }
}

fn same_point(a: Point2D, b: Point2D) -> bool {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    dx * dx + dy * dy <= 1e-18
}

fn vector_points_for_emission(polyline: &[Point2D], closed: bool) -> Cow<'_, [Point2D]> {
    if !closed || polyline.len() < 2 {
        return Cow::Borrowed(polyline);
    }

    let first = polyline[0];
    let last = polyline[polyline.len() - 1];
    if same_point(first, last) {
        Cow::Borrowed(polyline)
    } else {
        let mut points = polyline.to_vec();
        points.push(first);
        Cow::Owned(points)
    }
}

fn percent_to_s_value(percent: f64, config: &GcodeConfig) -> u32 {
    let max = config.s_value_max as f64;
    (percent.clamp(0.0, 100.0) / 100.0 * max).round() as u32
}

fn axis_burn_move(
    axis: char,
    position: f64,
    speed_mm_min: Option<f64>,
    s_value: u32,
    config: &GcodeConfig,
) -> String {
    let suffix = power_suffix(config, s_value);
    match speed_mm_min {
        Some(speed) => format!("G1 {axis}{position:.3} F{speed:.0}{suffix}"),
        None => format!("G1 {axis}{position:.3}{suffix}"),
    }
}

fn xy_burn_move(
    x: f64,
    y: f64,
    speed_mm_min: Option<f64>,
    s_value: u32,
    config: &GcodeConfig,
) -> String {
    let suffix = power_suffix(config, s_value);
    match speed_mm_min {
        Some(speed) => format!("G1 X{x:.3} Y{y:.3} F{speed:.0}{suffix}"),
        None => format!("G1 X{x:.3} Y{y:.3}{suffix}"),
    }
}

/// Linearly interpolate scanning offset from a sorted (speed, offset) table.
/// Empty table → 0.0. Clamps to min/max for out-of-range speeds.
/// Interpolate the scanning-offset table at a given speed (mm/min).
/// Public so preflight can validate worst-case raster motion bounds.
pub fn interpolate_scanning_offset(offsets: &[(f64, f64)], speed: f64) -> f64 {
    if offsets.is_empty() {
        return 0.0;
    }
    if offsets.len() == 1 || speed <= offsets[0].0 {
        return offsets[0].1;
    }
    if speed >= offsets[offsets.len() - 1].0 {
        return offsets[offsets.len() - 1].1;
    }
    // Find bracketing entries
    for window in offsets.windows(2) {
        let (s0, o0) = window[0];
        let (s1, o1) = window[1];
        if speed >= s0 && speed <= s1 {
            if (s1 - s0).abs() < f64::EPSILON {
                return o0; // Identical speeds — use first entry's offset
            }
            let t = (speed - s0) / (s1 - s0);
            return o0 + t * (o1 - o0);
        }
    }
    offsets[offsets.len() - 1].1
}

/// Map a raw pixel power (0-255) to an S-value within [min_s, max_s].
fn power_to_s_value(pixel_power: u8, min_s: u32, max_s: u32) -> u32 {
    if min_s >= max_s {
        return max_s;
    }
    let t = pixel_power as f64 / 255.0;
    (min_s as f64 + t * (max_s - min_s) as f64).round() as u32
}

/// Interpolate a ramp power fraction (0.0-1.0) to an S-value within [min_s, max_s].
fn ramp_fraction_to_s(fraction: f64, min_s: u32, max_s: u32) -> u32 {
    if min_s >= max_s {
        return max_s;
    }
    let t = fraction.clamp(0.0, 1.0);
    (min_s as f64 + t * (max_s - min_s) as f64).round() as u32
}

#[derive(Debug, Clone, Copy)]
enum RasterLineGeometry {
    Orthogonal {
        axis: ScanAxis,
        cross_pos: f64,
    },
    Rotated {
        cross_pos: f64,
        cos_a: f64,
        sin_a: f64,
        origin: Point2D,
    },
}

impl RasterLineGeometry {
    fn rapid_move(self, run_pos: f64) -> String {
        match self {
            Self::Orthogonal {
                axis: ScanAxis::Horizontal,
                cross_pos,
            } => format!("G0 X{run_pos:.3} Y{cross_pos:.3}"),
            Self::Orthogonal {
                axis: ScanAxis::Vertical,
                cross_pos,
            } => format!("G0 X{cross_pos:.3} Y{run_pos:.3}"),
            Self::Rotated {
                cross_pos,
                cos_a,
                sin_a,
                origin,
            } => {
                let (x, y) = rotate_to_world(run_pos, cross_pos, &origin, cos_a, sin_a);
                format!("G0 X{x:.3} Y{y:.3}")
            }
        }
    }

    fn approach_move(self, run_pos: f64, speed_mm_min: f64) -> String {
        match self {
            Self::Orthogonal {
                axis: ScanAxis::Horizontal,
                cross_pos,
            } => format!("G1 X{run_pos:.3} Y{cross_pos:.3} S0 F{speed_mm_min:.0}"),
            Self::Orthogonal {
                axis: ScanAxis::Vertical,
                cross_pos,
            } => format!("G1 X{cross_pos:.3} Y{run_pos:.3} S0 F{speed_mm_min:.0}"),
            Self::Rotated {
                cross_pos,
                cos_a,
                sin_a,
                origin,
            } => {
                let (x, y) = rotate_to_world(run_pos, cross_pos, &origin, cos_a, sin_a);
                format!("G1 X{x:.3} Y{y:.3} S0 F{speed_mm_min:.0}")
            }
        }
    }

    fn feed_move(
        self,
        run_pos: f64,
        speed_mm_min: Option<f64>,
        s_value: u32,
        config: &GcodeConfig,
    ) -> String {
        match self {
            Self::Orthogonal {
                axis: ScanAxis::Horizontal,
                ..
            } => axis_burn_move('X', run_pos, speed_mm_min, s_value, config),
            Self::Orthogonal {
                axis: ScanAxis::Vertical,
                ..
            } => axis_burn_move('Y', run_pos, speed_mm_min, s_value, config),
            Self::Rotated {
                cross_pos,
                cos_a,
                sin_a,
                origin,
            } => {
                let (x, y) = rotate_to_world(run_pos, cross_pos, &origin, cos_a, sin_a);
                xy_burn_move(x, y, speed_mm_min, s_value, config)
            }
        }
    }
}

fn emit_raster_power_move(
    lines: &mut Vec<String>,
    geometry: RasterLineGeometry,
    end_pos: f64,
    speed_mm_min: f64,
    need_feed: &mut bool,
    s_value: u32,
    config: &GcodeConfig,
) {
    lines.push(laser_on_command(config, s_value));
    let feed = need_feed.then_some(speed_mm_min);
    lines.push(geometry.feed_move(end_pos, feed, s_value, config));
    *need_feed = false;
}

#[allow(clippy::too_many_arguments)]
fn emit_raster_scanline(
    lines: &mut Vec<String>,
    scanline: &Scanline,
    geometry: RasterLineGeometry,
    power_mode: PowerMode,
    speed_mm_min: f64,
    min_s: u32,
    max_s: u32,
    overscan_mm: f64,
    ramp_length_mm: f64,
    scan_offset: f64,
    config: &GcodeConfig,
) {
    if scanline.runs.is_empty() {
        return;
    }

    let is_ltr = matches!(scanline.direction, ScanDirection::LeftToRight);
    let direction = if is_ltr { 1.0 } else { -1.0 };
    let offset = if is_ltr { -scan_offset } else { scan_offset };
    let mut runs: Vec<_> = scanline.runs.iter().collect();
    runs.sort_by(|a, b| {
        let a_min = a.start_x_mm.min(a.end_x_mm);
        let b_min = b.start_x_mm.min(b.end_x_mm);
        if is_ltr {
            a_min.total_cmp(&b_min)
        } else {
            b_min.total_cmp(&a_min)
        }
    });

    let directed_bounds = |run: &beambench_planner::ScanRun| {
        let min = run.start_x_mm.min(run.end_x_mm) + offset;
        let max = run.start_x_mm.max(run.end_x_mm) + offset;
        if is_ltr { (min, max) } else { (max, min) }
    };

    let (row_start, _) = directed_bounds(runs[0]);
    let (_, row_end) = directed_bounds(runs[runs.len() - 1]);
    let overscan_start = row_start - direction * overscan_mm;
    let overscan_end = row_end + direction * overscan_mm;

    let mut need_feed = overscan_mm <= 0.0;
    if overscan_mm > 0.0 {
        if config.use_g0_for_overscan {
            lines.push(geometry.rapid_move(overscan_start));
        } else {
            lines.push(geometry.approach_move(overscan_start, speed_mm_min));
        }
        lines.push(laser_on_zero(config));
        lines.push(geometry.feed_move(row_start, Some(speed_mm_min), 0, config));
    } else {
        lines.push(geometry.rapid_move(row_start));
    }

    let mut current_pos = row_start;
    for run in runs {
        let (run_start, run_end) = directed_bounds(run);
        if (run_start - current_pos).abs() > 1e-9 {
            lines.push(laser_on_zero(config));
            let feed = need_feed.then_some(speed_mm_min);
            lines.push(geometry.feed_move(run_start, feed, 0, config));
            need_feed = false;
        }

        match power_mode {
            PowerMode::Binary => {
                for segment in beambench_planner::ramp::expand_threshold_run(
                    run_start,
                    run_end,
                    ramp_length_mm,
                ) {
                    let s_value = ramp_fraction_to_s(segment.power_fraction, min_s, max_s);
                    emit_raster_power_move(
                        lines,
                        geometry,
                        segment.end_x_mm,
                        speed_mm_min,
                        &mut need_feed,
                        s_value,
                        config,
                    );
                }
            }
            PowerMode::Grayscale if run.power_values.is_empty() => {
                emit_raster_power_move(
                    lines,
                    geometry,
                    run_end,
                    speed_mm_min,
                    &mut need_feed,
                    max_s,
                    config,
                );
            }
            PowerMode::Grayscale => {
                let mut powers = run.power_values.clone();
                if !is_ltr {
                    powers.reverse();
                }
                let pixel_width = (run_end - run_start) / powers.len() as f64;
                let mut group_power = powers[0];
                for index in 1..=powers.len() {
                    if index == powers.len() || powers[index] != group_power {
                        let end_pos = run_start + index as f64 * pixel_width;
                        let s_value = power_to_s_value(group_power, min_s, max_s);
                        emit_raster_power_move(
                            lines,
                            geometry,
                            end_pos,
                            speed_mm_min,
                            &mut need_feed,
                            s_value,
                            config,
                        );
                        if index < powers.len() {
                            group_power = powers[index];
                        }
                    }
                }
            }
        }
        current_pos = run_end;
    }

    if overscan_mm > 0.0 {
        lines.push(laser_on_zero(config));
        lines.push(geometry.feed_move(overscan_end, Some(speed_mm_min), 0, config));
    }
    lines.push("M5".to_string());
}

fn generate_segment(
    lines: &mut Vec<String>,
    segment: &PlanSegment,
    config: &GcodeConfig,
) -> Result<(), GrblError> {
    match segment {
        PlanSegment::Travel { end, .. } => {
            lines.push(format!("G0 X{:.3} Y{:.3}", end.x, end.y));
        }
        PlanSegment::Vector {
            polyline,
            closed,
            power_percent,
            speed_mm_min,
            perforation_enabled,
            perforation_on_ms,
            perforation_off_ms,
            ..
        } => {
            if polyline.is_empty() {
                return Ok(());
            }
            let emission_polyline = vector_points_for_emission(polyline, *closed);

            // Rapid move to start
            lines.push(format!(
                "G0 X{:.3} Y{:.3}",
                emission_polyline[0].x, emission_polyline[0].y
            ));

            // S-value: power_percent scaled to configured max
            let s_value = percent_to_s_value(*power_percent, config);

            if *perforation_enabled && *perforation_on_ms > 0.0 && *perforation_off_ms > 0.0 {
                // Perforation mode: alternating laser on/off pulses along each segment.
                // Convert timing to distance: dist = speed * time
                // speed is mm/min, time is ms, so dist_mm = speed_mm_min / 60_000 * ms
                let on_dist = speed_mm_min / 60_000.0 * perforation_on_ms;
                let off_dist = speed_mm_min / 60_000.0 * perforation_off_ms;

                lines.push(format!("F{speed_mm_min:.0}"));

                let mut laser_on = false;
                // Track remaining phase distance across segment boundaries
                let mut remaining_on = on_dist;
                let mut remaining_off = 0.0;
                let mut in_on_phase = true;

                for pair in emission_polyline.windows(2) {
                    let (p0, p1) = (&pair[0], &pair[1]);
                    let dx = p1.x - p0.x;
                    let dy = p1.y - p0.y;
                    let seg_len = (dx * dx + dy * dy).sqrt();
                    if seg_len < 1e-9 {
                        continue;
                    }
                    let ux = dx / seg_len;
                    let uy = dy / seg_len;

                    let mut traveled = 0.0;
                    while traveled < seg_len - 1e-9 {
                        if in_on_phase {
                            if !laser_on {
                                lines.push(laser_on_command(config, s_value));
                                laser_on = true;
                            }
                            let advance = (seg_len - traveled).min(remaining_on);
                            traveled += advance;
                            remaining_on -= advance;
                            let nx = p0.x + ux * traveled;
                            let ny = p0.y + uy * traveled;
                            if config.emit_s_every_g1 {
                                lines.push(format!("G1 X{nx:.3} Y{ny:.3} S{s_value}"));
                            } else {
                                lines.push(format!("G1 X{nx:.3} Y{ny:.3}"));
                            }

                            if remaining_on < 1e-9 {
                                // Transition to off-phase. The gap is purely
                                // geometric: the head traverses `off_dist`
                                // with the laser off. A G4 dwell here would
                                // add stationary time ON TOP of the traverse,
                                // stretching every gap beyond the configured
                                // off-time. The M5 must fire even when the
                                // phase flips exactly at a polyline corner,
                                // or the laser stays on through the gap.
                                if laser_on {
                                    lines.push("M5".to_string());
                                    laser_on = false;
                                }
                                in_on_phase = false;
                                remaining_off = off_dist;
                            }
                        } else {
                            let advance = (seg_len - traveled).min(remaining_off);
                            traveled += advance;
                            remaining_off -= advance;
                            let nx = p0.x + ux * traveled;
                            let ny = p0.y + uy * traveled;
                            lines.push(format!("G0 X{nx:.3} Y{ny:.3}"));

                            if remaining_off < 1e-9 {
                                // Transition to on-phase
                                in_on_phase = true;
                                remaining_on = on_dist;
                            }
                        }
                    }
                }

                if laser_on {
                    lines.push("M5".to_string());
                }
            } else {
                // Continuous cutting mode
                lines.push(laser_on_command(config, s_value));

                for point in emission_polyline.iter().skip(1) {
                    if config.emit_s_every_g1 {
                        lines.push(format!(
                            "G1 X{:.3} Y{:.3} F{:.0} S{s_value}",
                            point.x, point.y, speed_mm_min
                        ));
                    } else {
                        lines.push(format!(
                            "G1 X{:.3} Y{:.3} F{:.0}",
                            point.x, point.y, speed_mm_min
                        ));
                    }
                }

                lines.push("M5".to_string());
            }
        }
        PlanSegment::Frame {
            path,
            power_percent,
            speed_mm_min,
            ..
        } => {
            if path.is_empty() {
                return Ok(());
            }

            // Rapid move to start
            lines.push(format!("G0 X{:.3} Y{:.3}", path[0].x, path[0].y));

            let laser_on = *power_percent > 0.0;
            let s_value = percent_to_s_value(*power_percent, config);
            if laser_on {
                lines.push(laser_on_command(config, s_value));
            }

            for point in path.iter().skip(1) {
                if laser_on && config.emit_s_every_g1 {
                    lines.push(format!(
                        "G1 X{:.3} Y{:.3} F{:.0} S{s_value}",
                        point.x, point.y, speed_mm_min
                    ));
                } else {
                    lines.push(format!(
                        "G1 X{:.3} Y{:.3} F{:.0}",
                        point.x, point.y, speed_mm_min
                    ));
                }
            }

            lines.push("M5".to_string());
        }
        PlanSegment::OffsetFill { .. } => {
            tracing::warn!(
                "OffsetFill segment reached G-code generation — should have been resolved earlier"
            );
        }
        PlanSegment::Raster {
            scanlines,
            speed_mm_min,
            power_mode,
            scan_angle_deg,
            scan_origin,
            scan_axis,
            power_max_percent,
            power_min_percent,
            overscan_mm,
            ramp_length_mm,
            ..
        } => {
            let is_orthogonal = scan_angle_deg.abs() < 0.5
                || (scan_angle_deg.abs() - 90.0).abs() < 0.5
                || (scan_angle_deg.abs() - 180.0).abs() < 0.5
                || (scan_angle_deg.abs() - 270.0).abs() < 0.5
                || (scan_angle_deg.abs() - 360.0).abs() < 0.5;
            let max_s = percent_to_s_value(*power_max_percent, config);
            let min_s = percent_to_s_value(*power_min_percent, config);
            let scan_offset = if config.enable_scanning_offset {
                interpolate_scanning_offset(&config.scanning_offsets, *speed_mm_min)
            } else {
                0.0
            };
            let rotated = if is_orthogonal {
                None
            } else {
                let radians = scan_angle_deg.to_radians();
                Some((radians.cos(), radians.sin()))
            };

            for scanline in scanlines {
                let geometry = match rotated {
                    None => RasterLineGeometry::Orthogonal {
                        axis: *scan_axis,
                        cross_pos: scanline.y_mm,
                    },
                    Some((cos_a, sin_a)) => RasterLineGeometry::Rotated {
                        cross_pos: scanline.y_mm,
                        cos_a,
                        sin_a,
                        origin: *scan_origin,
                    },
                };
                emit_raster_scanline(
                    lines,
                    scanline,
                    geometry,
                    *power_mode,
                    *speed_mm_min,
                    min_s,
                    max_s,
                    *overscan_mm,
                    *ramp_length_mm,
                    scan_offset,
                    config,
                );
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use beambench_common::geometry::{Bounds, Point2D};
    use beambench_planner::*;
    use chrono::Utc;
    use uuid::Uuid;

    fn make_plan(segments: Vec<PlanSegment>) -> ExecutionPlan {
        ExecutionPlan {
            id: Uuid::new_v4(),
            project_id: Uuid::new_v4(),
            revision_hash: "test".to_string(),
            created_at: Utc::now(),
            bounds: Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(100.0, 100.0)),
            total_distance_mm: 0.0,
            estimated_duration_secs: 0.0,
            segments,
            layer_order: vec![],
            warnings: vec![],
            failed_entries: vec![],
        }
    }

    fn raster_segment_with_runs(
        runs: Vec<ScanRun>,
        direction: ScanDirection,
        power_mode: PowerMode,
        scan_axis: ScanAxis,
        scan_angle_deg: f64,
        overscan_mm: f64,
    ) -> PlanSegment {
        PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 10.0,
                runs,
                direction,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode,
            speed_mm_min: 2000.0,
            layer_id: "l1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm,
            outlines: vec![],
            scan_axis,
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 1.0,
        }
    }

    fn axis_feed_positions(gcode: &[String], axis: char) -> Vec<f64> {
        let prefix = format!("G1 {axis}");
        gcode
            .iter()
            .filter_map(|line| {
                line.strip_prefix(&prefix)
                    .and_then(|rest| rest.split_whitespace().next())
                    .and_then(|value| value.parse().ok())
            })
            .collect()
    }

    fn z_test_vector(entry_id: &str, y: f64) -> PlanSegment {
        PlanSegment::Vector {
            polyline: vec![Point2D::new(0.0, y), Point2D::new(10.0, y)],
            closed: false,
            power_percent: 50.0,
            speed_mm_min: 1000.0,
            layer_id: format!("layer-{entry_id}"),
            cut_entry_id: entry_id.to_string(),
            perforation_enabled: false,
            perforation_on_ms: 0.0,
            perforation_off_ms: 0.0,
            source_object_id: None,
            source_subpath_index: None,
        }
    }

    #[test]
    fn empty_plan_generates_preamble_postamble() {
        let plan = make_plan(vec![]);
        let config = GcodeConfig::default();
        let gcode = generate_gcode(&plan, &config).unwrap();
        assert_eq!(gcode[0], "G90");
        assert_eq!(gcode[1], "G21");
        assert_eq!(gcode[2], "M5");
        // Postamble
        assert_eq!(gcode[3], "M5");
        assert_eq!(gcode[4], "G0 X0 Y0"); // FinishPosition::Origin by default
    }

    #[test]
    fn travel_segment_generates_rapid_move() {
        let plan = make_plan(vec![PlanSegment::Travel {
            start: Point2D::new(0.0, 0.0),
            end: Point2D::new(10.0, 20.0),
        }]);
        let gcode = generate_gcode(&plan, &GcodeConfig::default()).unwrap();
        assert!(gcode.contains(&"G0 X10.000 Y20.000".to_string()));
    }

    #[test]
    fn vector_segment_generates_cut_moves() {
        let plan = make_plan(vec![PlanSegment::Vector {
            polyline: vec![
                Point2D::new(5.0, 5.0),
                Point2D::new(15.0, 5.0),
                Point2D::new(15.0, 15.0),
            ],
            closed: false,
            power_percent: 80.0,
            speed_mm_min: 1000.0,
            layer_id: "l1".to_string(),
            cut_entry_id: String::new(),
            perforation_enabled: false,
            perforation_on_ms: 0.0,
            perforation_off_ms: 0.0,
            source_object_id: None,
            source_subpath_index: None,
        }]);
        let gcode = generate_gcode(&plan, &GcodeConfig::default()).unwrap();
        // Check rapid to start
        assert!(gcode.contains(&"G0 X5.000 Y5.000".to_string()));
        // Check S-value: 80% = 800
        assert!(gcode.contains(&"M4 S800".to_string()));
        // Check cut moves
        assert!(gcode.contains(&"G1 X15.000 Y5.000 F1000".to_string()));
        assert!(gcode.contains(&"G1 X15.000 Y15.000 F1000".to_string()));
    }

    #[test]
    fn absolute_z_offsets_emit_on_target_change_and_return_to_base() {
        let plan = make_plan(vec![
            z_test_vector("entry-a", 0.0),
            z_test_vector("entry-a", 5.0),
            z_test_vector("entry-b", 10.0),
        ]);
        let config = GcodeConfig {
            z_moves_enabled: true,
            z_move_feed_mm_min: 450.0,
            z_base_mm: 2.0,
            z_offset_cut_entry_ids: vec![("entry-a".to_string(), 5.0)],
            ..GcodeConfig::default()
        };

        let gcode = generate_gcode(&plan, &config).unwrap();
        let z_lines: Vec<&str> = gcode
            .iter()
            .filter(|line| line.starts_with("G1 Z"))
            .map(String::as_str)
            .collect();

        assert_eq!(z_lines, vec!["G1 Z7.000 F450", "G1 Z2.000 F450"]);
    }

    #[test]
    fn absolute_z_offset_returns_to_base_before_postamble() {
        let plan = make_plan(vec![z_test_vector("entry-a", 0.0)]);
        let config = GcodeConfig {
            z_moves_enabled: true,
            z_move_feed_mm_min: 300.0,
            z_base_mm: 1.0,
            z_offset_cut_entry_ids: vec![("entry-a".to_string(), 4.0)],
            ..GcodeConfig::default()
        };

        let gcode = generate_gcode(&plan, &config).unwrap();
        let return_z = gcode
            .iter()
            .rposition(|line| line == "G1 Z1.000 F300")
            .unwrap();
        let postamble = gcode.iter().rposition(|line| line == "M5").unwrap();

        assert!(return_z < postamble);
    }

    #[test]
    fn absolute_zero_z_offsets_do_not_emit_z_moves() {
        let plan = make_plan(vec![
            z_test_vector("entry-a", 0.0),
            z_test_vector("entry-b", 5.0),
        ]);
        let config = GcodeConfig {
            z_moves_enabled: true,
            z_move_feed_mm_min: 300.0,
            z_base_mm: 7.0,
            z_offset_cut_entry_ids: vec![
                ("entry-a".to_string(), 0.0),
                ("entry-b".to_string(), 0.0),
            ],
            ..GcodeConfig::default()
        };

        let gcode = generate_gcode(&plan, &config).unwrap();

        assert!(!gcode.iter().any(|line| line.starts_with("G1 Z")));
    }

    #[test]
    fn relative_z_offsets_emit_deltas_and_wrapped_final_return() {
        let plan = make_plan(vec![
            z_test_vector("entry-a", 0.0),
            z_test_vector("entry-b", 5.0),
        ]);
        let config = GcodeConfig {
            z_moves_enabled: true,
            z_mode: FocusTestZMode::RelativeTemporary,
            z_move_feed_mm_min: 300.0,
            z_offset_cut_entry_ids: vec![
                ("entry-a".to_string(), -2.0),
                ("entry-b".to_string(), 2.0),
            ],
            ..GcodeConfig::default()
        };

        let gcode = generate_gcode(&plan, &config).unwrap();
        let z_blocks: Vec<&str> = gcode
            .iter()
            .filter(|line| {
                line.as_str() == "G91" || line.starts_with("G1 Z") || line.as_str() == "G90"
            })
            .map(String::as_str)
            .collect();

        assert_eq!(
            z_blocks,
            vec![
                "G90",
                "G91",
                "G1 Z-2.000 F300",
                "G90",
                "G91",
                "G1 Z4.000 F300",
                "G90",
                "G91",
                "G1 Z-2.000 F300",
                "G90",
            ]
        );
    }

    #[test]
    fn air_assist_enabled_cut_entry_wraps_matching_burn_segments() {
        let plan = make_plan(vec![
            PlanSegment::Vector {
                polyline: vec![Point2D::new(5.0, 5.0), Point2D::new(15.0, 5.0)],
                closed: false,
                power_percent: 80.0,
                speed_mm_min: 1000.0,
                layer_id: "l1".to_string(),
                cut_entry_id: "entry-air".to_string(),
                perforation_enabled: false,
                perforation_on_ms: 0.0,
                perforation_off_ms: 0.0,
                source_object_id: None,
                source_subpath_index: None,
            },
            PlanSegment::Vector {
                polyline: vec![Point2D::new(20.0, 5.0), Point2D::new(25.0, 5.0)],
                closed: false,
                power_percent: 80.0,
                speed_mm_min: 1000.0,
                layer_id: "l2".to_string(),
                cut_entry_id: "entry-no-air".to_string(),
                perforation_enabled: false,
                perforation_on_ms: 0.0,
                perforation_off_ms: 0.0,
                source_object_id: None,
                source_subpath_index: None,
            },
        ]);
        let config = GcodeConfig {
            air_assist_cut_entry_ids: vec!["entry-air".to_string()],
            ..GcodeConfig::default()
        };

        let gcode = generate_gcode(&plan, &config).unwrap();
        let air_on = gcode.iter().position(|line| line == "M7").unwrap();
        let first_burn = gcode.iter().position(|line| line == "M4 S800").unwrap();
        let air_off = gcode.iter().position(|line| line == "M9").unwrap();
        let second_start = gcode
            .iter()
            .position(|line| line == "G0 X20.000 Y5.000")
            .unwrap();

        assert!(air_on < first_burn);
        assert!(air_off < second_start);
    }

    #[test]
    fn air_assist_uses_configured_commands_and_dwell() {
        let plan = make_plan(vec![PlanSegment::Vector {
            polyline: vec![Point2D::new(5.0, 5.0), Point2D::new(15.0, 5.0)],
            closed: false,
            power_percent: 80.0,
            speed_mm_min: 1000.0,
            layer_id: "l1".to_string(),
            cut_entry_id: "entry-air".to_string(),
            perforation_enabled: false,
            perforation_on_ms: 0.0,
            perforation_off_ms: 0.0,
            source_object_id: None,
            source_subpath_index: None,
        }]);
        let config = GcodeConfig {
            air_assist_cut_entry_ids: vec!["entry-air".to_string()],
            air_assist_on_gcode: "M8".to_string(),
            air_assist_off_gcode: "M9".to_string(),
            air_assist_on_delay_ms: 300,
            ..GcodeConfig::default()
        };

        let gcode = generate_gcode(&plan, &config).unwrap();

        let air_on = gcode.iter().position(|line| line == "M8").unwrap();
        let dwell = gcode.iter().position(|line| line == "G4 P0.300").unwrap();
        let first_burn = gcode.iter().position(|line| line == "M4 S800").unwrap();
        assert!(air_on < dwell);
        assert!(dwell < first_burn);
        assert!(gcode.iter().any(|line| line == "M9"));
    }

    #[test]
    fn air_assist_turns_off_before_travel_after_air_segment() {
        let plan = make_plan(vec![
            PlanSegment::Vector {
                polyline: vec![Point2D::new(5.0, 5.0), Point2D::new(15.0, 5.0)],
                closed: false,
                power_percent: 80.0,
                speed_mm_min: 1000.0,
                layer_id: "l1".to_string(),
                cut_entry_id: "entry-air".to_string(),
                perforation_enabled: false,
                perforation_on_ms: 0.0,
                perforation_off_ms: 0.0,
                source_object_id: None,
                source_subpath_index: None,
            },
            PlanSegment::Travel {
                start: Point2D::new(15.0, 5.0),
                end: Point2D::new(40.0, 40.0),
            },
            PlanSegment::Vector {
                polyline: vec![Point2D::new(45.0, 40.0), Point2D::new(55.0, 40.0)],
                closed: false,
                power_percent: 80.0,
                speed_mm_min: 1000.0,
                layer_id: "l2".to_string(),
                cut_entry_id: "entry-no-air".to_string(),
                perforation_enabled: false,
                perforation_on_ms: 0.0,
                perforation_off_ms: 0.0,
                source_object_id: None,
                source_subpath_index: None,
            },
        ]);
        let config = GcodeConfig {
            air_assist_cut_entry_ids: vec!["entry-air".to_string()],
            ..GcodeConfig::default()
        };

        let gcode = generate_gcode(&plan, &config).unwrap();

        let air_off = gcode.iter().position(|line| line == "M9").unwrap();
        let travel = gcode
            .iter()
            .position(|line| line == "G0 X40.000 Y40.000")
            .unwrap();
        assert!(air_off < travel);
    }

    #[test]
    fn closed_vector_segment_generates_closing_cut_move() {
        let plan = make_plan(vec![PlanSegment::Vector {
            polyline: vec![
                Point2D::new(5.0, 5.0),
                Point2D::new(15.0, 5.0),
                Point2D::new(15.0, 15.0),
                Point2D::new(5.0, 15.0),
            ],
            closed: true,
            power_percent: 80.0,
            speed_mm_min: 1000.0,
            layer_id: "l1".to_string(),
            cut_entry_id: String::new(),
            perforation_enabled: false,
            perforation_on_ms: 0.0,
            perforation_off_ms: 0.0,
            source_object_id: None,
            source_subpath_index: None,
        }]);
        let gcode = generate_gcode(&plan, &GcodeConfig::default()).unwrap();

        assert!(gcode.contains(&"G1 X15.000 Y5.000 F1000".to_string()));
        assert!(gcode.contains(&"G1 X15.000 Y15.000 F1000".to_string()));
        assert!(gcode.contains(&"G1 X5.000 Y15.000 F1000".to_string()));
        assert!(gcode.contains(&"G1 X5.000 Y5.000 F1000".to_string()));
    }

    #[test]
    fn closed_vector_segment_does_not_duplicate_existing_closing_point() {
        let plan = make_plan(vec![PlanSegment::Vector {
            polyline: vec![
                Point2D::new(5.0, 5.0),
                Point2D::new(15.0, 5.0),
                Point2D::new(15.0, 15.0),
                Point2D::new(5.0, 5.0),
            ],
            closed: true,
            power_percent: 80.0,
            speed_mm_min: 1000.0,
            layer_id: "l1".to_string(),
            cut_entry_id: String::new(),
            perforation_enabled: false,
            perforation_on_ms: 0.0,
            perforation_off_ms: 0.0,
            source_object_id: None,
            source_subpath_index: None,
        }]);
        let gcode = generate_gcode(&plan, &GcodeConfig::default()).unwrap();
        let closing_move_count = gcode
            .iter()
            .filter(|line| line.as_str() == "G1 X5.000 Y5.000 F1000")
            .count();

        assert_eq!(closing_move_count, 1);
    }

    #[test]
    fn raster_binary_segment_uses_max_power() {
        let plan = make_plan(vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 10.0,
                runs: vec![ScanRun {
                    start_x_mm: 5.0,
                    end_x_mm: 25.0,
                    power_values: vec![],
                }],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Binary,
            speed_mm_min: 2000.0,
            layer_id: "l1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 0.0,
            outlines: vec![],
            scan_axis: ScanAxis::default(),
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }]);
        let gcode = generate_gcode(&plan, &GcodeConfig::default()).unwrap();
        assert!(gcode.contains(&"M4 S1000".to_string()));
        assert!(gcode.contains(&"G0 X5.000 Y10.000".to_string()));
        assert!(gcode.contains(&"G1 X25.000 F2000".to_string()));
    }

    #[test]
    fn raster_multi_run_ltr_overscans_once_and_never_reverses() {
        let runs = vec![
            ScanRun {
                start_x_mm: 5.0,
                end_x_mm: 6.0,
                power_values: vec![],
            },
            ScanRun {
                start_x_mm: 8.0,
                end_x_mm: 9.0,
                power_values: vec![],
            },
        ];
        let plan = make_plan(vec![raster_segment_with_runs(
            runs,
            ScanDirection::LeftToRight,
            PowerMode::Binary,
            ScanAxis::Horizontal,
            0.0,
            2.5,
        )]);

        let gcode = generate_gcode(&plan, &GcodeConfig::default()).unwrap();
        assert!(gcode.contains(&"G0 X2.500 Y10.000".to_string()));
        assert_eq!(
            axis_feed_positions(&gcode, 'X'),
            vec![5.0, 6.0, 8.0, 9.0, 11.5]
        );
        assert!(!gcode.contains(&"G0 X5.500 Y10.000".to_string()));
        assert_eq!(gcode.iter().filter(|line| line.as_str() == "M5").count(), 3);
    }

    #[test]
    fn raster_multi_run_rtl_vertical_sweep_is_monotonic() {
        let runs = vec![
            ScanRun {
                start_x_mm: 5.0,
                end_x_mm: 6.0,
                power_values: vec![],
            },
            ScanRun {
                start_x_mm: 8.0,
                end_x_mm: 9.0,
                power_values: vec![],
            },
        ];
        let plan = make_plan(vec![raster_segment_with_runs(
            runs,
            ScanDirection::RightToLeft,
            PowerMode::Binary,
            ScanAxis::Vertical,
            90.0,
            2.5,
        )]);

        let gcode = generate_gcode(&plan, &GcodeConfig::default()).unwrap();
        assert!(gcode.contains(&"G0 X10.000 Y11.500".to_string()));
        assert_eq!(
            axis_feed_positions(&gcode, 'Y'),
            vec![9.0, 8.0, 6.0, 5.0, 2.5]
        );
        assert!(!gcode.contains(&"G0 X10.000 Y8.500".to_string()));
    }

    #[test]
    fn raster_multi_run_angled_sweep_has_no_inter_run_rapid() {
        let runs = vec![
            ScanRun {
                start_x_mm: 5.0,
                end_x_mm: 6.0,
                power_values: vec![],
            },
            ScanRun {
                start_x_mm: 8.0,
                end_x_mm: 9.0,
                power_values: vec![],
            },
        ];
        let plan = make_plan(vec![raster_segment_with_runs(
            runs,
            ScanDirection::LeftToRight,
            PowerMode::Binary,
            ScanAxis::Horizontal,
            45.0,
            2.5,
        )]);

        let gcode = generate_gcode(&plan, &GcodeConfig::default()).unwrap();
        let row_rapids = gcode
            .iter()
            .filter(|line| line.starts_with("G0 X") && line.as_str() != "G0 X0 Y0")
            .count();
        assert_eq!(row_rapids, 1, "one approach rapid per scanline: {gcode:?}");

        let radians = 45.0_f64.to_radians();
        let projected: Vec<f64> = gcode
            .iter()
            .filter_map(|line| {
                let mut words = line.split_whitespace();
                if words.next()? != "G1" {
                    return None;
                }
                let x = words.next()?.strip_prefix('X')?.parse::<f64>().ok()?;
                let y = words.next()?.strip_prefix('Y')?.parse::<f64>().ok()?;
                Some(x * radians.cos() + y * radians.sin())
            })
            .collect();
        assert!(
            projected.windows(2).all(|pair| pair[1] + 1e-3 >= pair[0]),
            "angled scanline reversed: {projected:?}"
        );
    }

    #[test]
    fn s_value_max_scales_vector_and_raster_power() {
        let config = GcodeConfig {
            s_value_max: 255,
            ..GcodeConfig::default()
        };
        let vector_plan = make_plan(vec![PlanSegment::Vector {
            polyline: vec![Point2D::new(0.0, 0.0), Point2D::new(10.0, 0.0)],
            closed: false,
            power_percent: 50.0,
            speed_mm_min: 1000.0,
            layer_id: "l1".to_string(),
            cut_entry_id: String::new(),
            perforation_enabled: false,
            perforation_on_ms: 0.0,
            perforation_off_ms: 0.0,
            source_object_id: None,
            source_subpath_index: None,
        }]);
        let vector_gcode = generate_gcode(&vector_plan, &config).unwrap();
        assert!(
            vector_gcode.contains(&"M4 S128".to_string()),
            "50% power should round to S128 when s_value_max is 255"
        );

        let raster_plan = make_plan(vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 10.0,
                runs: vec![ScanRun {
                    start_x_mm: 5.0,
                    end_x_mm: 25.0,
                    power_values: vec![],
                }],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Binary,
            speed_mm_min: 2000.0,
            layer_id: "l1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 0.0,
            outlines: vec![],
            scan_axis: ScanAxis::default(),
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }]);
        let raster_gcode = generate_gcode(&raster_plan, &config).unwrap();
        assert!(
            raster_gcode.contains(&"M4 S255".to_string()),
            "100% raster power should emit S255 when s_value_max is 255"
        );
    }

    #[test]
    fn raster_binary_respects_power_max() {
        let plan = make_plan(vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 10.0,
                runs: vec![ScanRun {
                    start_x_mm: 5.0,
                    end_x_mm: 25.0,
                    power_values: vec![],
                }],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Binary,
            speed_mm_min: 2000.0,
            layer_id: "l1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 0.0,
            outlines: vec![],
            scan_axis: ScanAxis::default(),
            power_max_percent: 80.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }]);
        let gcode = generate_gcode(&plan, &GcodeConfig::default()).unwrap();
        assert!(gcode.contains(&"M4 S800".to_string()));
    }

    #[test]
    fn raster_binary_ramp_length_emits_multiple_g1_segments() {
        // 20mm binary run with 2mm ramp per side → 13 G1 segments with varying S
        let plan = make_plan(vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 10.0,
                runs: vec![ScanRun {
                    start_x_mm: 0.0,
                    end_x_mm: 20.0,
                    power_values: vec![],
                }],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Binary,
            speed_mm_min: 2000.0,
            layer_id: "l1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 0.0,
            outlines: vec![],
            scan_axis: ScanAxis::default(),
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 2.0,
            x_pixel_mm: 0.0,
        }]);
        let gcode = generate_gcode(&plan, &GcodeConfig::default()).unwrap();

        // Count G1 X... lines for this raster row (excludes preamble Y move).
        // Default config doesn't emit S on every G1 — power changes come from
        // preceding M4 S<val> lines. 6 ramp-in + 1 const + 6 ramp-out = 13.
        let g1_x = gcode
            .iter()
            .filter(|l| l.starts_with("G1 X") && !l.contains(" Y"))
            .count();
        assert_eq!(g1_x, 13, "expected 13 ramp G1 X segments, got {g1_x}");

        // Final segment should end at X20.000
        assert!(
            gcode.iter().any(|l| l.starts_with("G1 X20.000")),
            "should emit final ramp segment ending at X20.000"
        );
        // Should include peak M4 S1000 line for the constant region
        assert!(
            gcode.iter().any(|l| l == "M4 S1000"),
            "constant region should emit at peak M4 S1000"
        );
        // Should include at least one low-power M4 S line for the ramp ends
        // (first/last ramp step has fraction 1/12 → s ≈ 83)
        let low_s_count = gcode
            .iter()
            .filter(|l| l.starts_with("M4 S") && l.as_str() != "M4 S1000" && l.as_str() != "M4 S0")
            .count();
        assert!(
            low_s_count >= 2,
            "expected multiple sub-peak M4 S values from ramp, got {low_s_count}"
        );
    }

    #[test]
    fn raster_binary_ramp_length_zero_emits_single_g1() {
        // Without ramp, a binary run is a single G1 — sanity that ramp gating
        // works. Confirms zero ramp_length keeps legacy emit path.
        let plan = make_plan(vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 10.0,
                runs: vec![ScanRun {
                    start_x_mm: 0.0,
                    end_x_mm: 20.0,
                    power_values: vec![],
                }],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Binary,
            speed_mm_min: 2000.0,
            layer_id: "l1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 0.0,
            outlines: vec![],
            scan_axis: ScanAxis::default(),
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }]);
        let gcode = generate_gcode(&plan, &GcodeConfig::default()).unwrap();
        // Legacy binary path: one M4 S<max> then one G1 X20.000 F<feed>
        assert!(gcode.contains(&"G1 X20.000 F2000".to_string()));
        // Only one G1 X line for this row — ramp expansion should be skipped
        let g1_x = gcode
            .iter()
            .filter(|l| l.starts_with("G1 X") && !l.contains(" Y"))
            .count();
        assert_eq!(g1_x, 1, "non-ramped binary should emit a single G1 X");
        // Single peak M4 S1000 only, no varying ramp powers
        let m4_lines = gcode.iter().filter(|l| l.starts_with("M4 ")).count();
        assert_eq!(m4_lines, 1, "non-ramped binary should emit a single M4");
    }

    #[test]
    fn raster_grayscale_uniform_power_single_subrun() {
        // Uniform power [128,128,128] → single sub-run
        let plan = make_plan(vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 10.0,
                runs: vec![ScanRun {
                    start_x_mm: 5.0,
                    end_x_mm: 25.0,
                    power_values: vec![128, 128, 128],
                }],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Grayscale,
            speed_mm_min: 2000.0,
            layer_id: "l1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 0.0,
            outlines: vec![],
            scan_axis: ScanAxis::default(),
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }]);
        let gcode = generate_gcode(&plan, &GcodeConfig::default()).unwrap();
        // 128/255 * 1000 ≈ 502
        let m4_count = gcode.iter().filter(|l| l.starts_with("M4 S")).count();
        assert_eq!(m4_count, 1, "Uniform power should emit single M4");
        assert!(gcode.contains(&"M4 S502".to_string()));
        // Single G1 move to end
        assert!(gcode.contains(&"G1 X25.000 F2000".to_string()));
    }

    #[test]
    fn raster_grayscale_gradient_multiple_subruns() {
        // Gradient [255, 128, 64] → three sub-runs with correct S-values and positions
        let plan = make_plan(vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 10.0,
                runs: vec![ScanRun {
                    start_x_mm: 0.0,
                    end_x_mm: 30.0,
                    power_values: vec![255, 128, 64],
                }],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Grayscale,
            speed_mm_min: 3000.0,
            layer_id: "l1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 0.0,
            outlines: vec![],
            scan_axis: ScanAxis::default(),
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }]);
        let gcode = generate_gcode(&plan, &GcodeConfig::default()).unwrap();
        let gcode_str = gcode.join("\n");

        // 3 different powers → 3 M4 commands
        let m4_count = gcode.iter().filter(|l| l.starts_with("M4 S")).count();
        assert_eq!(m4_count, 3, "Gradient should emit 3 sub-runs");

        // S-values: 255→1000, 128→502, 64→251
        assert!(
            gcode_str.contains("M4 S1000"),
            "First sub-run should be S1000"
        );
        assert!(
            gcode_str.contains("M4 S502"),
            "Second sub-run should be S502"
        );
        assert!(
            gcode_str.contains("M4 S251"),
            "Third sub-run should be S251"
        );

        // Positions: pixel_width = 30/3 = 10mm
        // Sub-run 1 ends at 0 + 1*10 = 10
        // Sub-run 2 ends at 0 + 2*10 = 20
        // Sub-run 3 ends at 0 + 3*10 = 30
        assert!(
            gcode_str.contains("G1 X10.000 F3000"),
            "First sub-run end at X10"
        );
        assert!(
            gcode_str.contains("G1 X20.000"),
            "Second sub-run end at X20"
        );
        assert!(gcode_str.contains("G1 X30.000"), "Third sub-run end at X30");

        // Only one M5 per run
        let m5_count = gcode.iter().filter(|l| l.as_str() == "M5").count();
        // Preamble M5 + 1 run M5 + postamble M5 = 3
        assert_eq!(m5_count, 3, "Should have preamble M5, run M5, postamble M5");
    }

    #[test]
    fn raster_grayscale_rtl_direction() {
        let plan = make_plan(vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 10.0,
                runs: vec![ScanRun {
                    start_x_mm: 0.0,
                    end_x_mm: 20.0,
                    power_values: vec![255, 128],
                }],
                direction: ScanDirection::RightToLeft,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Grayscale,
            speed_mm_min: 2000.0,
            layer_id: "l1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 0.0,
            outlines: vec![],
            scan_axis: ScanAxis::default(),
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }]);
        let gcode = generate_gcode(&plan, &GcodeConfig::default()).unwrap();
        // RTL: start_pos = end_x_mm = 20, end_pos = start_x_mm = 0
        // pixel_width = (0 - 20) / 2 = -10
        // Pixel powers are stored left-to-right, so RTL emission must reverse
        // them: the right pixel (128) burns first, then the left pixel (255).
        assert!(gcode.contains(&"G0 X20.000 Y10.000".to_string()));
        assert!(gcode.contains(&"G1 X10.000 F2000".to_string()));
        assert!(gcode.contains(&"G1 X0.000".to_string()));
        let first_power = gcode.iter().position(|line| line == "M4 S502").unwrap();
        let second_power = gcode.iter().position(|line| line == "M4 S1000").unwrap();
        assert!(
            first_power < second_power,
            "RTL powers were not reversed: {gcode:?}"
        );
    }

    #[test]
    fn raster_grayscale_empty_power_values_uses_max() {
        let plan = make_plan(vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 10.0,
                runs: vec![ScanRun {
                    start_x_mm: 5.0,
                    end_x_mm: 25.0,
                    power_values: vec![],
                }],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Grayscale,
            speed_mm_min: 2000.0,
            layer_id: "l1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 0.0,
            outlines: vec![],
            scan_axis: ScanAxis::default(),
            power_max_percent: 80.0,
            power_min_percent: 10.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }]);
        let gcode = generate_gcode(&plan, &GcodeConfig::default()).unwrap();
        // Empty power_values → max_s = 800
        assert!(gcode.contains(&"M4 S800".to_string()));
    }

    #[test]
    fn raster_grayscale_position_accuracy_5_pixels() {
        // 5 pixels across 10mm → each sub-run spans exactly 2mm
        let plan = make_plan(vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 5.0,
                runs: vec![ScanRun {
                    start_x_mm: 0.0,
                    end_x_mm: 10.0,
                    power_values: vec![100, 100, 200, 200, 200],
                }],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Grayscale,
            speed_mm_min: 1500.0,
            layer_id: "l1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 0.0,
            outlines: vec![],
            scan_axis: ScanAxis::default(),
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }]);
        let gcode = generate_gcode(&plan, &GcodeConfig::default()).unwrap();
        // pixel_width = 10/5 = 2mm
        // Sub-run 1 (pv=100, idx 0-1): end = 0 + 2*2 = 4.0
        // Sub-run 2 (pv=200, idx 2-4): end = 0 + 5*2 = 10.0
        assert!(gcode.contains(&"G1 X4.000 F1500".to_string()));
        assert!(gcode.contains(&"G1 X10.000".to_string()));
    }

    #[test]
    fn raster_vertical_axis_subruns() {
        let plan = make_plan(vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 15.0,
                runs: vec![ScanRun {
                    start_x_mm: 5.0,
                    end_x_mm: 25.0,
                    power_values: vec![255, 128],
                }],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Grayscale,
            speed_mm_min: 2000.0,
            layer_id: "l1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 0.0,
            outlines: vec![],
            scan_axis: ScanAxis::Vertical,
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }]);
        let gcode = generate_gcode(&plan, &GcodeConfig::default()).unwrap();
        // Vertical: y_mm=15 → X, start_x=5 → Y start
        assert!(gcode.contains(&"G0 X15.000 Y5.000".to_string()));
        // pixel_width = (25-5)/2 = 10; sub-run 1 end = 5+1*10 = 15, sub-run 2 end = 5+2*10 = 25
        assert!(gcode.contains(&"G1 Y15.000 F2000".to_string()));
        assert!(gcode.contains(&"G1 Y25.000".to_string()));
    }

    #[test]
    fn power_to_s_value_mapping() {
        // Test the helper function
        assert_eq!(super::power_to_s_value(0, 100, 800), 100);
        assert_eq!(super::power_to_s_value(255, 100, 800), 800);
        // 128/255 * 700 + 100 ≈ 451.37 → 451
        assert_eq!(super::power_to_s_value(128, 100, 800), 451);
    }

    #[test]
    fn power_to_s_value_min_zero_max_1000_original_behavior() {
        // min=0, max=1000 reproduces original (pixel/255*1000).round()
        assert_eq!(super::power_to_s_value(0, 0, 1000), 0);
        assert_eq!(super::power_to_s_value(255, 0, 1000), 1000);
        assert_eq!(super::power_to_s_value(128, 0, 1000), 502);
    }

    #[test]
    fn power_to_s_value_degenerate_min_equals_max() {
        assert_eq!(super::power_to_s_value(128, 500, 500), 500);
        assert_eq!(super::power_to_s_value(0, 500, 500), 500);
        assert_eq!(super::power_to_s_value(255, 500, 500), 500);
    }

    #[test]
    fn raster_grayscale_with_min_max_power_mapping() {
        // min=10%, max=80% → min_s=100, max_s=800
        // pixel 128 → power_to_s_value(128, 100, 800) = 451
        let plan = make_plan(vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 10.0,
                runs: vec![ScanRun {
                    start_x_mm: 5.0,
                    end_x_mm: 25.0,
                    power_values: vec![128],
                }],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Grayscale,
            speed_mm_min: 2000.0,
            layer_id: "l1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 0.0,
            outlines: vec![],
            scan_axis: ScanAxis::default(),
            power_max_percent: 80.0,
            power_min_percent: 10.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }]);
        let gcode = generate_gcode(&plan, &GcodeConfig::default()).unwrap();
        assert!(gcode.contains(&"M4 S451".to_string()));
    }

    #[test]
    fn raster_binary_unchanged_when_default_power() {
        // Regression: binary with max=100, min=0 should produce S1000 as before
        let plan = make_plan(vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 10.0,
                runs: vec![ScanRun {
                    start_x_mm: 5.0,
                    end_x_mm: 25.0,
                    power_values: vec![],
                }],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Binary,
            speed_mm_min: 2000.0,
            layer_id: "l1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 0.0,
            outlines: vec![],
            scan_axis: ScanAxis::default(),
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }]);
        let gcode = generate_gcode(&plan, &GcodeConfig::default()).unwrap();
        assert!(gcode.contains(&"M4 S1000".to_string()));
        assert!(gcode.contains(&"G0 X5.000 Y10.000".to_string()));
        assert!(gcode.contains(&"G1 X25.000 F2000".to_string()));
    }

    #[test]
    fn frame_segment_generates_outline() {
        let plan = make_plan(vec![PlanSegment::Frame {
            path: vec![
                Point2D::new(0.0, 0.0),
                Point2D::new(50.0, 0.0),
                Point2D::new(50.0, 50.0),
                Point2D::new(0.0, 50.0),
                Point2D::new(0.0, 0.0),
            ],
            power_percent: 5.0,
            speed_mm_min: 3000.0,
        }]);
        let gcode = generate_gcode(&plan, &GcodeConfig::default()).unwrap();
        assert!(gcode.contains(&"G0 X0.000 Y0.000".to_string()));
        assert!(gcode.contains(&"M4 S50".to_string())); // 5% = 50
        assert!(gcode.contains(&"G1 X50.000 Y0.000 F3000".to_string()));
    }

    #[test]
    fn frame_segment_power_zero_moves_with_laser_off() {
        let plan = make_plan(vec![PlanSegment::Frame {
            path: vec![
                Point2D::new(0.0, 0.0),
                Point2D::new(50.0, 0.0),
                Point2D::new(50.0, 50.0),
            ],
            power_percent: 0.0,
            speed_mm_min: 3000.0,
        }]);
        let config = GcodeConfig {
            emit_s_every_g1: true,
            ..GcodeConfig::default()
        };
        let gcode = generate_gcode(&plan, &config).unwrap();

        assert!(gcode.contains(&"G0 X0.000 Y0.000".to_string()));
        assert!(gcode.contains(&"G1 X50.000 Y0.000 F3000".to_string()));
        assert!(
            !gcode
                .iter()
                .any(|line| line.starts_with("M3 ") || line.starts_with("M4 "))
        );
        assert!(
            !gcode
                .iter()
                .any(|line| line.starts_with("G1 ") && line.contains(" S"))
        );
        assert_eq!(gcode.iter().filter(|line| line.as_str() == "M5").count(), 3);
    }

    #[test]
    fn gcode_prefix_injected_after_preamble() {
        let plan = make_plan(vec![]);
        let config = GcodeConfig {
            gcode_prefix: "M106 S255\nG0 Z10".to_string(),
            ..GcodeConfig::default()
        };
        let gcode = generate_gcode(&plan, &config).unwrap();
        // Preamble: G90, G21, M5
        // Then prefix: M106 S255, G0 Z10
        assert_eq!(gcode[3], "M106 S255");
        assert_eq!(gcode[4], "G0 Z10");
        // Then postamble
        assert_eq!(gcode[5], "M5");
        assert_eq!(gcode[6], "G0 X0 Y0");
    }

    #[test]
    fn gcode_prefix_and_suffix_injected() {
        let plan = make_plan(vec![PlanSegment::Travel {
            start: Point2D::new(0.0, 0.0),
            end: Point2D::new(5.0, 5.0),
        }]);
        let config = GcodeConfig {
            gcode_prefix: "M7".to_string(),
            gcode_suffix: "M9\nG0 Z0".to_string(),
            ..GcodeConfig::default()
        };
        let gcode = generate_gcode(&plan, &config).unwrap();
        // Preamble(3) + prefix(1) + travel(1) + suffix(2) + postamble(2) = 9
        assert_eq!(gcode[3], "M7"); // prefix
        assert_eq!(gcode[4], "G0 X5.000 Y5.000"); // travel
        assert_eq!(gcode[5], "M9"); // suffix
        assert_eq!(gcode[6], "G0 Z0"); // suffix
        assert_eq!(gcode[7], "M5"); // postamble
        assert_eq!(gcode[8], "G0 X0 Y0"); // postamble
    }

    #[test]
    fn vector_perforation_generates_on_off_pattern() {
        // A straight horizontal line of 10mm with perforation:
        // on_ms=100, off_ms=50 at speed 6000mm/min = 100mm/s
        // on_dist = 6000/60000 * 100 = 10mm (covers the whole line in one on-pulse)
        // Use smaller on_ms to get multiple pulses
        let plan = make_plan(vec![PlanSegment::Vector {
            polyline: vec![Point2D::new(0.0, 0.0), Point2D::new(10.0, 0.0)],
            closed: false,
            power_percent: 50.0,
            speed_mm_min: 6000.0,
            layer_id: "l1".to_string(),
            cut_entry_id: String::new(),
            perforation_enabled: true,
            perforation_on_ms: 20.0,  // on_dist = 6000/60000 * 20 = 2mm
            perforation_off_ms: 10.0, // off_dist = 6000/60000 * 10 = 1mm
            source_object_id: None,
            source_subpath_index: None,
        }]);
        let gcode = generate_gcode(&plan, &GcodeConfig::default()).unwrap();
        let gcode_str = gcode.join("\n");

        // Should have multiple M4/M5 pairs (perforation pulses)
        let m4_count = gcode.iter().filter(|l| l.starts_with("M4 S")).count();
        let _m5_count = gcode.iter().filter(|l| *l == "M5").count();
        // Preamble M5 + postamble M5 + perforation off pulses
        assert!(m4_count >= 2, "Expected multiple M4 pulses, got {m4_count}");
        // Gaps are geometric: traversed laser-off with motion, never a
        // stationary G4 dwell (which would stretch the gap timing).
        assert!(
            !gcode_str.contains("G4 P"),
            "Perforation must not emit stationary dwells"
        );
        assert!(
            gcode.iter().any(|l| l.starts_with("G0 X")),
            "Expected laser-off G0 traversal through perforation gaps"
        );
        // Should have G0 rapid moves during off-phase
        let g0_during_perf = gcode
            .iter()
            .enumerate()
            .filter(|(i, l)| *i > 4 && l.starts_with("G0 X") && l != &"G0 X0 Y0")
            .count();
        assert!(g0_during_perf >= 1, "Expected rapid moves during off-phase");
        // S-value should be 500 (50%)
        assert!(gcode_str.contains("M4 S500"));
    }

    #[test]
    fn perforation_phase_carries_across_polyline_vertices() {
        // A 3-segment polyline (L-shape): (0,0)->(3,0)->(3,3)->(6,3)
        // Total path length = 3 + 3 + 3 = 9mm
        // Speed 6000 mm/min => on_dist = 6000/60000*20 = 2mm, off_dist = 6000/60000*10 = 1mm
        // Cycle = 3mm. Over 9mm we expect 3 full on-phases.
        // At vertex (3,0): we're 1mm into the on-phase (first on was 0-2, first off 2-3).
        // The on-phase should NOT restart — it should continue the remaining distance.
        let plan = make_plan(vec![PlanSegment::Vector {
            polyline: vec![
                Point2D::new(0.0, 0.0),
                Point2D::new(3.0, 0.0),
                Point2D::new(3.0, 3.0),
                Point2D::new(6.0, 3.0),
            ],
            closed: false,
            power_percent: 50.0,
            speed_mm_min: 6000.0,
            layer_id: "l1".to_string(),
            cut_entry_id: String::new(),
            perforation_enabled: true,
            perforation_on_ms: 20.0,  // on_dist = 2mm
            perforation_off_ms: 10.0, // off_dist = 1mm
            source_object_id: None,
            source_subpath_index: None,
        }]);
        let gcode = generate_gcode(&plan, &GcodeConfig::default()).unwrap();

        // Count M4 activations (laser on) — should be exactly 3 for 9mm path
        // (on 0-2, off 2-3, on 3-5, off 5-6, on 6-8, off 8-9)
        let m4_count = gcode.iter().filter(|l| l.starts_with("M4 S")).count();
        assert_eq!(
            m4_count, 3,
            "Expected 3 on-phases over 9mm path, got {m4_count}"
        );

        // Gaps are geometric (off_dist traversed laser-off): no stationary
        // dwell may be emitted — a G4 would stretch every gap beyond the
        // configured off-time.
        let g4_count = gcode.iter().filter(|l| l.starts_with("G4 P")).count();
        assert_eq!(g4_count, 0, "Perforation gaps must not dwell: {gcode:?}");

        // Every off-phase must switch the laser off, including phase flips
        // that land exactly on a polyline vertex.
        let m5_count = gcode.iter().filter(|l| l.as_str() == "M5").count();
        assert!(
            m5_count >= 3,
            "Expected a laser-off for each of 3 off-phases, got {m5_count}: {gcode:?}"
        );

        // The gap distance is traversed with motion (G0), keeping geometry.
        let g0_gap_moves = gcode.iter().filter(|l| l.starts_with("G0 X")).count();
        assert!(
            g0_gap_moves >= 3,
            "Expected G0 traversal through each gap, got {g0_gap_moves}"
        );
    }

    #[test]
    fn perforation_disabled_produces_continuous_cut() {
        let plan = make_plan(vec![PlanSegment::Vector {
            polyline: vec![Point2D::new(0.0, 0.0), Point2D::new(10.0, 0.0)],
            closed: false,
            power_percent: 80.0,
            speed_mm_min: 1000.0,
            layer_id: "l1".to_string(),
            cut_entry_id: String::new(),
            perforation_enabled: false,
            perforation_on_ms: 20.0,
            perforation_off_ms: 10.0,
            source_object_id: None,
            source_subpath_index: None,
        }]);
        let gcode = generate_gcode(&plan, &GcodeConfig::default()).unwrap();
        let gcode_str = gcode.join("\n");

        // Should NOT have G4 dwell commands
        assert!(
            !gcode_str.contains("G4 P"),
            "Disabled perforation should not generate dwell"
        );
        // Should have exactly one M4 (cutting) before postamble
        let segment_m4 = gcode.iter().filter(|l| l.starts_with("M4 S")).count();
        assert_eq!(segment_m4, 1);
    }

    #[test]
    fn raster_vertical_scan_axis_emits_y_movement() {
        let plan = make_plan(vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 15.0,
                runs: vec![ScanRun {
                    start_x_mm: 5.0,
                    end_x_mm: 25.0,
                    power_values: vec![],
                }],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Binary,
            speed_mm_min: 2000.0,
            layer_id: "l1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 0.0,
            outlines: vec![],
            scan_axis: ScanAxis::Vertical,
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }]);
        let gcode = generate_gcode(&plan, &GcodeConfig::default()).unwrap();
        // Vertical: y_mm=15 becomes X, start_x=5 becomes Y
        assert!(
            gcode.contains(&"G0 X15.000 Y5.000".to_string()),
            "Vertical raster should rapid to X={{y_mm}} Y={{start_x}}"
        );
        // Burn along Y axis
        assert!(
            gcode.contains(&"G1 Y25.000 F2000".to_string()),
            "Vertical raster should burn along Y axis"
        );
        // Should NOT contain horizontal movement (G1 X...)
        let has_g1_x = gcode
            .iter()
            .any(|l| l.starts_with("G1 X") && !l.contains("Y"));
        assert!(!has_g1_x, "Vertical raster should NOT emit G1 X movement");
    }

    #[test]
    fn raster_horizontal_scan_axis_unchanged() {
        // Regression test: horizontal scan axis should work as before
        let plan = make_plan(vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 10.0,
                runs: vec![ScanRun {
                    start_x_mm: 5.0,
                    end_x_mm: 25.0,
                    power_values: vec![],
                }],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Binary,
            speed_mm_min: 2000.0,
            layer_id: "l1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 0.0,
            outlines: vec![],
            scan_axis: ScanAxis::Horizontal,
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }]);
        let gcode = generate_gcode(&plan, &GcodeConfig::default()).unwrap();
        // Same as existing behavior
        assert!(gcode.contains(&"G0 X5.000 Y10.000".to_string()));
        assert!(gcode.contains(&"G1 X25.000 F2000".to_string()));
    }

    #[test]
    fn s_value_range_0_to_1000() {
        // 0% power
        let plan = make_plan(vec![PlanSegment::Vector {
            polyline: vec![Point2D::new(0.0, 0.0), Point2D::new(10.0, 0.0)],
            closed: false,
            power_percent: 0.0,
            speed_mm_min: 1000.0,
            layer_id: "l1".to_string(),
            cut_entry_id: String::new(),
            perforation_enabled: false,
            perforation_on_ms: 0.0,
            perforation_off_ms: 0.0,
            source_object_id: None,
            source_subpath_index: None,
        }]);
        let gcode = generate_gcode(&plan, &GcodeConfig::default()).unwrap();
        assert!(gcode.contains(&"M4 S0".to_string()));

        // 100% power
        let plan = make_plan(vec![PlanSegment::Vector {
            polyline: vec![Point2D::new(0.0, 0.0), Point2D::new(10.0, 0.0)],
            closed: false,
            power_percent: 100.0,
            speed_mm_min: 1000.0,
            layer_id: "l1".to_string(),
            cut_entry_id: String::new(),
            perforation_enabled: false,
            perforation_on_ms: 0.0,
            perforation_off_ms: 0.0,
            source_object_id: None,
            source_subpath_index: None,
        }]);
        let gcode = generate_gcode(&plan, &GcodeConfig::default()).unwrap();
        assert!(gcode.contains(&"M4 S1000".to_string()));
    }

    #[test]
    fn raster_zero_angle_gcode_unchanged() {
        // Explicitly checking scan_angle_deg: 0.0 goes through orthogonal path
        let plan = make_plan(vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 10.0,
                runs: vec![ScanRun {
                    start_x_mm: 5.0,
                    end_x_mm: 25.0,
                    power_values: vec![],
                }],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Binary,
            speed_mm_min: 2000.0,
            layer_id: "l1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 0.0,
            outlines: vec![],
            scan_axis: ScanAxis::default(),
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }]);
        let gcode = generate_gcode(&plan, &GcodeConfig::default()).unwrap();
        // Should use direct coordinates (orthogonal fast path)
        assert!(gcode.contains(&"G0 X5.000 Y10.000".to_string()));
        assert!(gcode.contains(&"G1 X25.000 F2000".to_string()));
    }

    #[test]
    fn raster_binary_overscan_emits_s0_acceleration_zones() {
        // Overscan should produce S0 lead-in and lead-out (laser off while accelerating/decelerating)
        let plan = make_plan(vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 10.0,
                runs: vec![ScanRun {
                    start_x_mm: 5.0,
                    end_x_mm: 25.0,
                    power_values: vec![],
                }],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Binary,
            speed_mm_min: 2000.0,
            layer_id: "l1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 2.0,
            outlines: vec![],
            scan_axis: ScanAxis::default(),
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }]);
        let gcode = generate_gcode(&plan, &GcodeConfig::default()).unwrap();
        let gcode_str = gcode.join("\n");

        // Rapid to overscan start: 5.0 - 2.0 = 3.0
        assert!(
            gcode_str.contains("G0 X3.000 Y10.000"),
            "Should rapid to overscan start"
        );
        // S0 lead-in
        assert!(gcode_str.contains("M4 S0"), "Should have S0 for overscan");
        // G1 to burn start at feed rate
        assert!(
            gcode_str.contains("G1 X5.000 F2000"),
            "Should accelerate to burn start"
        );
        // Actual burn at max power
        assert!(gcode_str.contains("M4 S1000"), "Should burn at max power");
        assert!(gcode_str.contains("G1 X25.000"), "Should burn to end");
        // Trailing overscan: 25.0 + 2.0 = 27.0
        assert!(
            gcode_str.contains("G1 X27.000 F2000"),
            "Should decelerate through trailing overscan with explicit feed rate"
        );
    }

    #[test]
    fn raster_grayscale_overscan_does_not_stretch_pixels() {
        // With overscan, pixel_width should be computed from burn-only width (not overscan-inflated)
        let plan = make_plan(vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 10.0,
                runs: vec![ScanRun {
                    start_x_mm: 0.0,
                    end_x_mm: 30.0,
                    power_values: vec![255, 128, 64],
                }],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Grayscale,
            speed_mm_min: 3000.0,
            layer_id: "l1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 5.0,
            outlines: vec![],
            scan_axis: ScanAxis::default(),
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }]);
        let gcode = generate_gcode(&plan, &GcodeConfig::default()).unwrap();
        let gcode_str = gcode.join("\n");

        // pixel_width = 30/3 = 10mm (burn-only, not 40/3 ≈ 13.33mm)
        // Sub-run 1 ends at 0 + 1*10 = 10
        // Sub-run 2 ends at 0 + 2*10 = 20
        // Sub-run 3 ends at 0 + 3*10 = 30
        assert!(
            gcode_str.contains("G1 X10.000"),
            "First sub-run end at X10 (not stretched)"
        );
        assert!(
            gcode_str.contains("G1 X20.000"),
            "Second sub-run end at X20"
        );
        assert!(gcode_str.contains("G1 X30.000"), "Third sub-run end at X30");

        // Should have overscan lead-in at X-5.0 and lead-out at X35.0
        assert!(
            gcode_str.contains("G0 X-5.000 Y10.000"),
            "Should rapid to overscan start"
        );
        assert!(
            gcode_str.contains("G1 X35.000 F3000"),
            "Should have trailing overscan with explicit feed rate"
        );
    }

    #[test]
    fn raster_rtl_overscan_extends_correctly() {
        let plan = make_plan(vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 10.0,
                runs: vec![ScanRun {
                    start_x_mm: 5.0,
                    end_x_mm: 25.0,
                    power_values: vec![],
                }],
                direction: ScanDirection::RightToLeft,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Binary,
            speed_mm_min: 2000.0,
            layer_id: "l1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 3.0,
            outlines: vec![],
            scan_axis: ScanAxis::default(),
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }]);
        let gcode = generate_gcode(&plan, &GcodeConfig::default()).unwrap();
        let gcode_str = gcode.join("\n");

        // RTL: start_pos=25, end_pos=5; overscan extends in travel direction
        // os_start = 25 + 3 = 28 (further right, before traveling left)
        // os_end = 5 - 3 = 2 (further left, after traveling left)
        assert!(
            gcode_str.contains("G0 X28.000 Y10.000"),
            "RTL: Should rapid to overscan start (right of burn)"
        );
        assert!(
            gcode_str.contains("G1 X25.000 F2000"),
            "RTL: Should accelerate to burn start"
        );
        assert!(gcode_str.contains("G1 X5.000"), "RTL: Should burn to end");
        assert!(
            gcode_str.contains("G1 X2.000 F2000"),
            "RTL: Should decelerate through trailing overscan with explicit feed rate"
        );
    }

    #[test]
    fn raster_zero_overscan_unchanged() {
        // Regression: zero overscan should produce identical output to before
        let plan = make_plan(vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 10.0,
                runs: vec![ScanRun {
                    start_x_mm: 5.0,
                    end_x_mm: 25.0,
                    power_values: vec![],
                }],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Binary,
            speed_mm_min: 2000.0,
            layer_id: "l1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 0.0,
            outlines: vec![],
            scan_axis: ScanAxis::default(),
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }]);
        let gcode = generate_gcode(&plan, &GcodeConfig::default()).unwrap();

        // Should NOT have S0 overscan zones
        let s0_count = gcode.iter().filter(|l| *l == "M4 S0").count();
        assert_eq!(s0_count, 0, "Zero overscan should not produce S0 zones");
        // Standard behavior
        assert!(gcode.contains(&"G0 X5.000 Y10.000".to_string()));
        assert!(gcode.contains(&"M4 S1000".to_string()));
        assert!(gcode.contains(&"G1 X25.000 F2000".to_string()));
    }

    #[test]
    fn raster_45_degree_angle_both_xy_change() {
        // At 45deg, a horizontal local scanline becomes diagonal in world space
        let plan = make_plan(vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 0.0,
                runs: vec![ScanRun {
                    start_x_mm: -5.0,
                    end_x_mm: 5.0,
                    power_values: vec![],
                }],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Binary,
            speed_mm_min: 2000.0,
            layer_id: "l1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 45.0,
            scan_origin: Point2D::new(15.0, 15.0),
            overscan_mm: 0.0,
            outlines: vec![],
            scan_axis: ScanAxis::default(),
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }]);
        let gcode = generate_gcode(&plan, &GcodeConfig::default()).unwrap();

        // The G1 move should have BOTH X and Y coordinates (not axis-aligned)
        // At 45deg: local (-5, 0) rotated around (15,15) -> world coords
        // cos(45) ~ 0.707, sin(45) ~ 0.707
        // world_x = 15 + (-5)*0.707 - 0*0.707 = 15 - 3.536 = 11.464
        // world_y = 15 + (-5)*0.707 + 0*0.707 = 15 - 3.536 = 11.464

        // Check that G1 line has both X and Y (non-orthogonal path)
        let g1_lines: Vec<_> = gcode.iter().filter(|l| l.starts_with("G1 X")).collect();
        assert!(!g1_lines.is_empty(), "Should have G1 moves");
        let g1 = g1_lines[0];
        assert!(
            g1.contains(" Y"),
            "Non-orthogonal raster G1 should have both X and Y coordinates: {}",
            g1
        );
    }

    #[test]
    fn raster_90_degree_uses_orthogonal_path() {
        let plan = make_plan(vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 10.0,
                runs: vec![ScanRun {
                    start_x_mm: 5.0,
                    end_x_mm: 25.0,
                    power_values: vec![],
                }],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Binary,
            speed_mm_min: 2000.0,
            layer_id: "l1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 90.0,
            scan_origin: Point2D::new(15.0, 15.0),
            overscan_mm: 0.0,
            outlines: vec![],
            scan_axis: ScanAxis::default(),
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }]);
        let gcode = generate_gcode(&plan, &GcodeConfig::default()).unwrap();
        // 90deg should take the orthogonal fast path (no rotation applied)
        // G1 should have X only, no Y (single-axis movement)
        assert!(gcode.contains(&"G1 X25.000 F2000".to_string()));
    }

    #[test]
    fn finish_position_origin_emits_g0_home() {
        let plan = make_plan(vec![]);
        let config = GcodeConfig {
            finish_position: FinishPosition::Origin,
            ..GcodeConfig::default()
        };
        let gcode = generate_gcode(&plan, &config).unwrap();
        assert_eq!(*gcode.last().unwrap(), "G0 X0 Y0");
    }

    #[test]
    fn finish_position_dont_move_no_movement() {
        let plan = make_plan(vec![]);
        let config = GcodeConfig {
            finish_position: FinishPosition::DontMove,
            ..GcodeConfig::default()
        };
        let gcode = generate_gcode(&plan, &config).unwrap();
        // Last line should be M5 (laser off), no G0 after it
        assert_eq!(*gcode.last().unwrap(), "M5");
    }

    #[test]
    fn finish_position_custom_xy_emits_custom_coords() {
        let plan = make_plan(vec![]);
        let config = GcodeConfig {
            finish_position: FinishPosition::CustomXY,
            finish_x: Some(100.0),
            finish_y: Some(50.0),
            ..GcodeConfig::default()
        };
        let gcode = generate_gcode(&plan, &config).unwrap();
        assert_eq!(*gcode.last().unwrap(), "G0 X100.000 Y50.000");
    }

    #[test]
    fn finish_position_custom_xy_defaults_to_zero() {
        let plan = make_plan(vec![]);
        let config = GcodeConfig {
            finish_position: FinishPosition::CustomXY,
            finish_x: None,
            finish_y: None,
            ..GcodeConfig::default()
        };
        let gcode = generate_gcode(&plan, &config).unwrap();
        assert_eq!(*gcode.last().unwrap(), "G0 X0.000 Y0.000");
    }

    #[test]
    fn preamble_no_longer_has_home_move() {
        let plan = make_plan(vec![]);
        let config = GcodeConfig::default();
        let gcode = generate_gcode(&plan, &config).unwrap();
        // Preamble should be G90, G21, M5 — no G0 X0 Y0
        assert_eq!(gcode[0], "G90");
        assert_eq!(gcode[1], "G21");
        assert_eq!(gcode[2], "M5");
        // Next should be postamble M5 (for empty plan)
        assert_eq!(gcode[3], "M5");
    }

    #[test]
    fn rotated_grayscale_emits_sub_runs() {
        // 45° raster with pixel values [100,100,200,200] → 2 distinct M4 S commands
        let plan = make_plan(vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 0.0,
                runs: vec![ScanRun {
                    start_x_mm: -5.0,
                    end_x_mm: 5.0,
                    power_values: vec![100, 100, 200, 200],
                }],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Grayscale,
            speed_mm_min: 2000.0,
            layer_id: "l1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 45.0,
            scan_origin: Point2D::new(10.0, 10.0),
            overscan_mm: 0.0,
            outlines: vec![],
            scan_axis: ScanAxis::Horizontal,
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }]);
        let gcode = generate_gcode(&plan, &GcodeConfig::default()).unwrap();
        let gcode_str = gcode.join("\n");

        // Should have 2 M4 S commands (two different power levels)
        let m4_count = gcode.iter().filter(|l| l.starts_with("M4 S")).count();
        assert_eq!(m4_count, 2, "Should emit 2 sub-runs, got {m4_count}");

        // S-values: 100/255*1000 ≈ 392, 200/255*1000 ≈ 784
        assert!(gcode_str.contains("M4 S392"), "First sub-run S392");
        assert!(gcode_str.contains("M4 S784"), "Second sub-run S784");

        // Both X and Y should change (diagonal movement)
        let g1_lines: Vec<_> = gcode.iter().filter(|l| l.starts_with("G1 ")).collect();
        for line in &g1_lines {
            assert!(
                line.contains('X') && line.contains('Y'),
                "Rotated G1 should have both X and Y: {}",
                line
            );
        }
    }

    #[test]
    fn rotated_raster_includes_overscan() {
        // 45° with overscan_mm=2.0 → S0 lead-in/lead-out lines present
        let plan = make_plan(vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 0.0,
                runs: vec![ScanRun {
                    start_x_mm: -5.0,
                    end_x_mm: 5.0,
                    power_values: vec![200, 200],
                }],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Grayscale,
            speed_mm_min: 2000.0,
            layer_id: "l1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 45.0,
            scan_origin: Point2D::new(10.0, 10.0),
            overscan_mm: 2.0,
            outlines: vec![],
            scan_axis: ScanAxis::Horizontal,
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }]);
        let gcode = generate_gcode(&plan, &GcodeConfig::default()).unwrap();
        let gcode_str = gcode.join("\n");

        // Should have S0 lead-in and S0 lead-out
        let s0_count = gcode.iter().filter(|l| *l == "M4 S0").count();
        assert!(
            s0_count >= 2,
            "Should have S0 lead-in and lead-out, got {s0_count}"
        );

        // The burn should still be present
        assert!(gcode_str.contains("M4 S784"), "Should have burn sub-run");
    }

    #[test]
    fn rotated_binary_includes_overscan() {
        // 45° binary with overscan_mm=2.0
        let plan = make_plan(vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 0.0,
                runs: vec![ScanRun {
                    start_x_mm: -5.0,
                    end_x_mm: 5.0,
                    power_values: vec![],
                }],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Binary,
            speed_mm_min: 2000.0,
            layer_id: "l1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 45.0,
            scan_origin: Point2D::new(10.0, 10.0),
            overscan_mm: 2.0,
            outlines: vec![],
            scan_axis: ScanAxis::Horizontal,
            power_max_percent: 80.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }]);
        let gcode = generate_gcode(&plan, &GcodeConfig::default()).unwrap();

        // S0 overscan lines should be present
        let s0_count = gcode.iter().filter(|l| *l == "M4 S0").count();
        assert!(
            s0_count >= 2,
            "Binary rotated should have overscan S0 lines"
        );

        // Burn at 80% → S800
        assert!(
            gcode.iter().any(|l| l == "M4 S800"),
            "Should have burn at S800"
        );
    }

    #[test]
    fn rotated_binary_overscan_g1_mode_uses_feed_move() {
        let plan = make_plan(vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 0.0,
                runs: vec![ScanRun {
                    start_x_mm: -5.0,
                    end_x_mm: 5.0,
                    power_values: vec![],
                }],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Binary,
            speed_mm_min: 2000.0,
            layer_id: "l1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 45.0,
            scan_origin: Point2D::new(10.0, 10.0),
            overscan_mm: 2.0,
            outlines: vec![],
            scan_axis: ScanAxis::Horizontal,
            power_max_percent: 80.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }]);
        let config = GcodeConfig {
            use_g0_for_overscan: false,
            ..GcodeConfig::default()
        };
        let gcode = generate_gcode(&plan, &config).unwrap();

        assert!(
            gcode
                .iter()
                .any(|line| line.starts_with("G1 X5.050 Y5.050") && line.contains("S0 F2000")),
            "rotated G1 overscan mode should use a laser-off feed move to the overscan start"
        );
    }

    // --- Output-policy and calibration tests ---

    #[test]
    fn m4_default_mode() {
        let plan = make_plan(vec![PlanSegment::Vector {
            polyline: vec![Point2D::new(0.0, 0.0), Point2D::new(10.0, 0.0)],
            closed: false,
            power_percent: 50.0,
            speed_mm_min: 1000.0,
            layer_id: "l1".to_string(),
            cut_entry_id: String::new(),
            perforation_enabled: false,
            perforation_on_ms: 0.0,
            perforation_off_ms: 0.0,
            source_object_id: None,
            source_subpath_index: None,
        }]);
        let gcode = generate_gcode(&plan, &GcodeConfig::default()).unwrap();
        assert!(gcode.iter().any(|l| l.starts_with("M4 S")));
        assert!(!gcode.iter().any(|l| l.starts_with("M3 S")));
    }

    #[test]
    fn m3_constant_power_mode() {
        let plan = make_plan(vec![PlanSegment::Vector {
            polyline: vec![Point2D::new(0.0, 0.0), Point2D::new(10.0, 0.0)],
            closed: false,
            power_percent: 50.0,
            speed_mm_min: 1000.0,
            layer_id: "l1".to_string(),
            cut_entry_id: String::new(),
            perforation_enabled: false,
            perforation_on_ms: 0.0,
            perforation_off_ms: 0.0,
            source_object_id: None,
            source_subpath_index: None,
        }]);
        let config = GcodeConfig {
            use_constant_power: true,
            ..GcodeConfig::default()
        };
        let gcode = generate_gcode(&plan, &config).unwrap();
        assert!(gcode.iter().any(|l| l.starts_with("M3 S")));
        assert!(!gcode.iter().any(|l| l.starts_with("M4 S")));
    }

    #[test]
    fn m3_raster_mode() {
        let plan = make_plan(vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 10.0,
                runs: vec![ScanRun {
                    start_x_mm: 5.0,
                    end_x_mm: 25.0,
                    power_values: vec![],
                }],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Binary,
            speed_mm_min: 2000.0,
            layer_id: "l1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 0.0,
            outlines: vec![],
            scan_axis: ScanAxis::default(),
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }]);
        let config = GcodeConfig {
            use_constant_power: true,
            ..GcodeConfig::default()
        };
        let gcode = generate_gcode(&plan, &config).unwrap();
        assert!(gcode.iter().any(|l| l == "M3 S1000"));
        assert!(!gcode.iter().any(|l| l.starts_with("M4 S")));
    }

    #[test]
    fn m3_perforation_mode() {
        let plan = make_plan(vec![PlanSegment::Vector {
            polyline: vec![Point2D::new(0.0, 0.0), Point2D::new(10.0, 0.0)],
            closed: false,
            power_percent: 50.0,
            speed_mm_min: 6000.0,
            layer_id: "l1".to_string(),
            cut_entry_id: String::new(),
            perforation_enabled: true,
            perforation_on_ms: 20.0,
            perforation_off_ms: 10.0,
            source_object_id: None,
            source_subpath_index: None,
        }]);
        let config = GcodeConfig {
            use_constant_power: true,
            ..GcodeConfig::default()
        };
        let gcode = generate_gcode(&plan, &config).unwrap();
        assert!(gcode.iter().any(|l| l.starts_with("M3 S")));
        assert!(!gcode.iter().any(|l| l.starts_with("M4 S")));
    }

    #[test]
    fn emit_s_every_g1_vector() {
        let plan = make_plan(vec![PlanSegment::Vector {
            polyline: vec![
                Point2D::new(0.0, 0.0),
                Point2D::new(10.0, 0.0),
                Point2D::new(10.0, 10.0),
            ],
            closed: false,
            power_percent: 80.0,
            speed_mm_min: 1000.0,
            layer_id: "l1".to_string(),
            cut_entry_id: String::new(),
            perforation_enabled: false,
            perforation_on_ms: 0.0,
            perforation_off_ms: 0.0,
            source_object_id: None,
            source_subpath_index: None,
        }]);
        let config = GcodeConfig {
            emit_s_every_g1: true,
            ..GcodeConfig::default()
        };
        let gcode = generate_gcode(&plan, &config).unwrap();
        let g1_lines: Vec<_> = gcode.iter().filter(|l| l.starts_with("G1 ")).collect();
        for line in &g1_lines {
            assert!(line.contains(" S800"), "Every G1 should have S800: {line}");
        }
    }

    #[test]
    fn emit_s_every_g1_raster() {
        let plan = make_plan(vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 10.0,
                runs: vec![ScanRun {
                    start_x_mm: 0.0,
                    end_x_mm: 30.0,
                    power_values: vec![255, 128, 64],
                }],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Grayscale,
            speed_mm_min: 3000.0,
            layer_id: "l1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 0.0,
            outlines: vec![],
            scan_axis: ScanAxis::default(),
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }]);
        // emit_s_every_g1 not set — sub-run G1 lines should NOT have S
        let gcode_default = generate_gcode(&plan, &GcodeConfig::default()).unwrap();
        let g1_burn_default: Vec<_> = gcode_default
            .iter()
            .filter(|l| l.starts_with("G1 X") && !l.contains("S0"))
            .collect();
        // At least one G1 burn line should NOT contain S (default behavior)
        assert!(
            g1_burn_default.iter().any(|l| !l.contains(" S")),
            "Default: G1 burn lines should not all have S"
        );

        let gcode_with_s = generate_gcode(
            &plan,
            &GcodeConfig {
                emit_s_every_g1: true,
                ..GcodeConfig::default()
            },
        )
        .unwrap();
        let g1_burn_with_s: Vec<_> = gcode_with_s
            .iter()
            .filter(|l| l.starts_with("G1 X") && !l.contains("S0"))
            .collect();
        assert!(
            !g1_burn_with_s.is_empty(),
            "Expected raster burn moves in emit_s_every_g1 path"
        );
        assert!(
            g1_burn_with_s.iter().all(|l| l.contains(" S")),
            "Every raster burn G1 should include S when emit_s_every_g1 is enabled: {:?}",
            g1_burn_with_s
        );
    }

    #[test]
    fn emit_s_every_g1_off_default() {
        let plan = make_plan(vec![PlanSegment::Vector {
            polyline: vec![Point2D::new(0.0, 0.0), Point2D::new(10.0, 0.0)],
            closed: false,
            power_percent: 80.0,
            speed_mm_min: 1000.0,
            layer_id: "l1".to_string(),
            cut_entry_id: String::new(),
            perforation_enabled: false,
            perforation_on_ms: 0.0,
            perforation_off_ms: 0.0,
            source_object_id: None,
            source_subpath_index: None,
        }]);
        let gcode = generate_gcode(&plan, &GcodeConfig::default()).unwrap();
        let g1_lines: Vec<_> = gcode.iter().filter(|l| l.starts_with("G1 ")).collect();
        // Default: G1 lines should NOT have S
        for line in &g1_lines {
            assert!(
                !line.contains(" S"),
                "Default: G1 should not have S: {line}"
            );
        }
    }

    #[test]
    fn overscan_g0_default() {
        let plan = make_plan(vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 10.0,
                runs: vec![ScanRun {
                    start_x_mm: 5.0,
                    end_x_mm: 25.0,
                    power_values: vec![],
                }],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Binary,
            speed_mm_min: 2000.0,
            layer_id: "l1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 2.0,
            outlines: vec![],
            scan_axis: ScanAxis::default(),
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }]);
        let gcode = generate_gcode(&plan, &GcodeConfig::default()).unwrap();
        assert!(gcode.iter().any(|l| l.starts_with("G0 X3.000")));
    }

    #[test]
    fn overscan_g1_mode() {
        let plan = make_plan(vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 10.0,
                runs: vec![ScanRun {
                    start_x_mm: 5.0,
                    end_x_mm: 25.0,
                    power_values: vec![],
                }],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Binary,
            speed_mm_min: 2000.0,
            layer_id: "l1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 2.0,
            outlines: vec![],
            scan_axis: ScanAxis::default(),
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }]);
        let config = GcodeConfig {
            use_g0_for_overscan: false,
            ..GcodeConfig::default()
        };
        let gcode = generate_gcode(&plan, &config).unwrap();
        // Should NOT have G0 to overscan start
        assert!(!gcode.iter().any(|l| l.starts_with("G0 X3.000")));
        // Should have G1 with S0 and F rate
        assert!(
            gcode
                .iter()
                .any(|l| l.starts_with("G1 X3.000") && l.contains("S0") && l.contains("F2000"))
        );
    }

    #[test]
    fn scanning_offset_empty_table_no_shift() {
        let plan = make_plan(vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 10.0,
                runs: vec![ScanRun {
                    start_x_mm: 5.0,
                    end_x_mm: 25.0,
                    power_values: vec![],
                }],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Binary,
            speed_mm_min: 2000.0,
            layer_id: "l1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 0.0,
            outlines: vec![],
            scan_axis: ScanAxis::default(),
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }]);
        let config = GcodeConfig {
            enable_scanning_offset: true,
            scanning_offsets: vec![],
            ..GcodeConfig::default()
        };
        let gcode = generate_gcode(&plan, &config).unwrap();
        assert!(gcode.contains(&"G0 X5.000 Y10.000".to_string()));
        assert!(gcode.contains(&"G1 X25.000 F2000".to_string()));
    }

    #[test]
    fn scanning_offset_single_entry() {
        let plan = make_plan(vec![PlanSegment::Raster {
            scanlines: vec![
                Scanline {
                    y_mm: 10.0,
                    runs: vec![ScanRun {
                        start_x_mm: 5.0,
                        end_x_mm: 25.0,
                        power_values: vec![],
                    }],
                    direction: ScanDirection::LeftToRight,
                },
                Scanline {
                    y_mm: 11.0,
                    runs: vec![ScanRun {
                        start_x_mm: 5.0,
                        end_x_mm: 25.0,
                        power_values: vec![],
                    }],
                    direction: ScanDirection::RightToLeft,
                },
            ],
            line_interval_mm: 1.0,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Binary,
            speed_mm_min: 2000.0,
            layer_id: "l1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 0.0,
            outlines: vec![],
            scan_axis: ScanAxis::default(),
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }]);
        let config = GcodeConfig {
            enable_scanning_offset: true,
            scanning_offsets: vec![(2000.0, 0.1)],
            ..GcodeConfig::default()
        };
        let gcode = generate_gcode(&plan, &config).unwrap();
        // LTR: positions shifted by -0.1 → 4.9, 24.9
        assert!(gcode.contains(&"G0 X4.900 Y10.000".to_string()));
        assert!(gcode.contains(&"G1 X24.900 F2000".to_string()));
        // RTL: positions shifted by +0.1 → 25.1, 5.1
        assert!(gcode.contains(&"G0 X25.100 Y11.000".to_string()));
        assert!(gcode.contains(&"G1 X5.100 F2000".to_string()));
    }

    #[test]
    fn scanning_offset_shifts_overscan_with_burn_positions() {
        let plan = make_plan(vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 10.0,
                runs: vec![ScanRun {
                    start_x_mm: 5.0,
                    end_x_mm: 25.0,
                    power_values: vec![],
                }],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Binary,
            speed_mm_min: 2000.0,
            layer_id: "l1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 2.0,
            outlines: vec![],
            scan_axis: ScanAxis::default(),
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }]);
        let config = GcodeConfig {
            enable_scanning_offset: true,
            scanning_offsets: vec![(2000.0, 0.1)],
            ..GcodeConfig::default()
        };
        let gcode = generate_gcode(&plan, &config).unwrap();

        // Burn span shifts negative by 0.1 and the overscan endpoints shift
        // WITH it, so the lead-in is always exactly overscan_mm long. (When
        // overscan was computed from the unshifted span, the lead-in came up
        // short by the offset — laser firing while still accelerating.)
        assert!(gcode.contains(&"G0 X2.900 Y10.000".to_string()));
        assert!(gcode.contains(&"G1 X4.900 F2000".to_string()));
        assert!(gcode.contains(&"G1 X24.900".to_string()));
        assert!(gcode.contains(&"G1 X26.900 F2000".to_string()));
    }

    #[test]
    fn scanning_offset_interpolated() {
        // speed=1500, between (1000, 0.05) and (2000, 0.15) → 0.10
        let offset = interpolate_scanning_offset(&[(1000.0, 0.05), (2000.0, 0.15)], 1500.0);
        assert!((offset - 0.10).abs() < 1e-9);
    }

    #[test]
    fn scanning_offset_clamp_below_min() {
        let offset = interpolate_scanning_offset(&[(1000.0, 0.05), (2000.0, 0.15)], 500.0);
        assert!((offset - 0.05).abs() < 1e-9);
    }

    #[test]
    fn scanning_offset_clamp_above_max() {
        let offset = interpolate_scanning_offset(&[(1000.0, 0.05), (2000.0, 0.15)], 3000.0);
        assert!((offset - 0.15).abs() < 1e-9);
    }

    #[test]
    fn scanning_offset_rtl_vs_ltr() {
        // Already tested in scanning_offset_single_entry — opposite shift directions verified
    }

    #[test]
    fn gcode_config_requires_current_output_policy_fields() {
        let json = r#"{"laser_mode":true,"use_relative_coords":false}"#;
        let err = serde_json::from_str::<GcodeConfig>(json).unwrap_err();
        assert!(err.to_string().contains("s_value_max"));
    }

    /// Helper: build a raster segment and assert that all overscan-exit G1 lines
    /// (the ones immediately after S0 and before M5) contain an explicit F parameter.
    fn assert_overscan_exit_has_feed(
        power_mode: PowerMode,
        scan_axis: ScanAxis,
        power_values: Vec<u8>,
        label: &str,
    ) {
        let axis_move_char = match scan_axis {
            ScanAxis::Horizontal => 'X',
            ScanAxis::Vertical => 'Y',
        };
        let segment = PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 10.0,
                direction: ScanDirection::LeftToRight,
                runs: vec![ScanRun {
                    start_x_mm: 5.0,
                    end_x_mm: 15.0,
                    power_values,
                }],
            }],
            scan_axis,
            speed_mm_min: 3000.0,
            power_max_percent: 80.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            power_mode,
            direction_mode: DirectionMode::Bidirectional,
            line_interval_mm: 0.1,
            overscan_mm: 2.0,
            layer_id: "L1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(10.0, 10.0),
            outlines: vec![],
            x_pixel_mm: 0.0,
        };
        let config = GcodeConfig {
            use_g0_for_overscan: true,
            ..GcodeConfig::default()
        };
        let mut lines = Vec::new();
        generate_segment(&mut lines, &segment, &config).unwrap();
        let gcode = lines.join("\n");

        // Find overscan-exit G1 lines: they follow an S0 line and precede M5.
        let line_vec: Vec<&str> = gcode.lines().collect();
        let mut found = 0;
        for (i, line) in line_vec.iter().enumerate() {
            if line.starts_with("G1") && i > 0 && i + 1 < line_vec.len() {
                let prev = line_vec[i - 1];
                let next = line_vec[i + 1];
                if prev.contains("S0") && next == "M5" {
                    assert!(
                        line.contains("F3000"),
                        "[{label}] Overscan exit G1 must include F3000. Got: {line}"
                    );
                    assert!(
                        line.contains(axis_move_char),
                        "[{label}] Overscan exit G1 must move on {axis_move_char} axis. Got: {line}"
                    );
                    found += 1;
                }
            }
        }
        assert!(
            found > 0,
            "[{label}] Expected at least one overscan-exit G1 line in output"
        );
    }

    fn assert_overscan_g1_mode_has_s0_feed(
        power_mode: PowerMode,
        scan_axis: ScanAxis,
        power_values: Vec<u8>,
        label: &str,
    ) {
        let segment = PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 10.0,
                direction: ScanDirection::LeftToRight,
                runs: vec![ScanRun {
                    start_x_mm: 5.0,
                    end_x_mm: 15.0,
                    power_values,
                }],
            }],
            scan_axis,
            speed_mm_min: 3000.0,
            power_max_percent: 80.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            power_mode,
            direction_mode: DirectionMode::Bidirectional,
            line_interval_mm: 0.1,
            overscan_mm: 2.0,
            layer_id: "L1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(10.0, 10.0),
            outlines: vec![],
            x_pixel_mm: 0.0,
        };
        let config = GcodeConfig {
            use_g0_for_overscan: false,
            ..GcodeConfig::default()
        };
        let mut lines = Vec::new();
        generate_segment(&mut lines, &segment, &config).unwrap();

        assert!(
            lines.iter().any(|line| line.starts_with("G1")
                && line.contains("S0")
                && line.contains("F3000")),
            "[{label}] G1 overscan mode should emit laser-off feed moves. Got: {lines:?}"
        );
    }

    #[test]
    fn overscan_exit_g1_binary_horizontal() {
        assert_overscan_exit_has_feed(
            PowerMode::Binary,
            ScanAxis::Horizontal,
            vec![200],
            "Binary+Horizontal",
        );
    }

    #[test]
    fn overscan_exit_g1_binary_vertical() {
        assert_overscan_exit_has_feed(
            PowerMode::Binary,
            ScanAxis::Vertical,
            vec![200],
            "Binary+Vertical",
        );
    }

    #[test]
    fn overscan_exit_g1_grayscale_fallback_horizontal() {
        // Empty power_values triggers the grayscale fallback path
        assert_overscan_exit_has_feed(
            PowerMode::Grayscale,
            ScanAxis::Horizontal,
            vec![],
            "GrayscaleFallback+Horizontal",
        );
    }

    #[test]
    fn overscan_exit_g1_grayscale_fallback_vertical() {
        assert_overscan_exit_has_feed(
            PowerMode::Grayscale,
            ScanAxis::Vertical,
            vec![],
            "GrayscaleFallback+Vertical",
        );
    }

    #[test]
    fn overscan_exit_g1_grayscale_subrun_horizontal() {
        // Non-empty power_values triggers the sub-run path
        assert_overscan_exit_has_feed(
            PowerMode::Grayscale,
            ScanAxis::Horizontal,
            vec![200],
            "GrayscaleSubrun+Horizontal",
        );
    }

    #[test]
    fn overscan_exit_g1_grayscale_subrun_vertical() {
        assert_overscan_exit_has_feed(
            PowerMode::Grayscale,
            ScanAxis::Vertical,
            vec![200],
            "GrayscaleSubrun+Vertical",
        );
    }

    #[test]
    fn overscan_g1_mode_binary_vertical() {
        assert_overscan_g1_mode_has_s0_feed(
            PowerMode::Binary,
            ScanAxis::Vertical,
            vec![200],
            "Binary+Vertical",
        );
    }

    #[test]
    fn overscan_g1_mode_grayscale_fallback_vertical() {
        assert_overscan_g1_mode_has_s0_feed(
            PowerMode::Grayscale,
            ScanAxis::Vertical,
            vec![],
            "GrayscaleFallback+Vertical",
        );
    }

    #[test]
    fn overscan_g1_mode_grayscale_subrun_vertical() {
        assert_overscan_g1_mode_has_s0_feed(
            PowerMode::Grayscale,
            ScanAxis::Vertical,
            vec![200],
            "GrayscaleSubrun+Vertical",
        );
    }

    #[test]
    fn interpolate_scanning_offset_duplicate_speeds_no_div_zero() {
        // Two entries with identical speed values — should return first entry's offset, not NaN
        let offsets = vec![(1000.0, 0.05), (1000.0, 0.15)];
        let result = interpolate_scanning_offset(&offsets, 1000.0);
        assert!(result.is_finite(), "Should not produce NaN/Inf");
        assert!(
            (result - 0.05).abs() < f64::EPSILON,
            "Should return first entry's offset"
        );
    }
}

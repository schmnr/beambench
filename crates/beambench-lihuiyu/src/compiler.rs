use beambench_common::Point2D;
use beambench_planner::{
    ExecutionPlan, PlanSegment, PowerMode, ScanAxis, ScanDirection, ScanRun, Scanline,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::protocol::{
    LihuiyuPacket, LihuiyuPadding, LihuiyuProtocolError, M2_MILLIMETRES_PER_STEP, encode_distance,
    encode_m2_vector_speed, encode_packet,
};

const DEFAULT_GRAYSCALE_THRESHOLD: u8 = 128;
const DEFAULT_MAX_MOTION_COMMANDS: usize = 2_000_000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LihuiyuCoordinateTransform {
    #[serde(default)]
    pub offset_x_mm: f64,
    #[serde(default)]
    pub offset_y_mm: f64,
    #[serde(default)]
    pub flip_x: bool,
    #[serde(default)]
    pub flip_y: bool,
    #[serde(default)]
    pub bed_width_mm: f64,
    #[serde(default)]
    pub bed_height_mm: f64,
}

impl Default for LihuiyuCoordinateTransform {
    fn default() -> Self {
        Self {
            offset_x_mm: 0.0,
            offset_y_mm: 0.0,
            flip_x: false,
            flip_y: false,
            bed_width_mm: 0.0,
            bed_height_mm: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LihuiyuCompilationConfig {
    #[serde(default)]
    pub coordinate_transform: LihuiyuCoordinateTransform,
    /// Position the controller is known to occupy when the stream begins.
    #[serde(default)]
    pub initial_position_mm: Point2D,
    /// Grayscale values at or above this value become binary beam-on spans.
    #[serde(default = "default_grayscale_threshold")]
    pub grayscale_threshold: u8,
    /// Hard ceiling for expanded Bresenham movements.
    #[serde(default = "default_max_motion_commands")]
    pub max_motion_commands: usize,
}

const fn default_grayscale_threshold() -> u8 {
    DEFAULT_GRAYSCALE_THRESHOLD
}

const fn default_max_motion_commands() -> usize {
    DEFAULT_MAX_MOTION_COMMANDS
}

impl Default for LihuiyuCompilationConfig {
    fn default() -> Self {
        Self {
            coordinate_transform: LihuiyuCoordinateTransform::default(),
            initial_position_mm: Point2D::zero(),
            grayscale_threshold: DEFAULT_GRAYSCALE_THRESHOLD,
            max_motion_commands: DEFAULT_MAX_MOTION_COMMANDS,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LihuiyuJobSummary {
    pub vector_path_count: usize,
    pub perforation_path_count: usize,
    pub raster_scanline_count: usize,
    pub frame_path_count: usize,
    pub motion_command_count: usize,
    pub packet_count: usize,
    /// Grayscale/ramp inputs that were collapsed to M2 binary output.
    pub binary_power_conversion_count: usize,
    pub uses_beam: bool,
    pub hardware_set_power: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LihuiyuCompiledJob {
    /// Deterministic EGV transcript. Newlines delimit packets and the final
    /// `-` is a host-side wait marker; neither is sent to the controller.
    pub egv_bytes: Vec<u8>,
    /// Controller command bytes after host-only delimiters are removed.
    pub controller_bytes: Vec<u8>,
    /// M2 packets with ASCII `F` padding and Dallas/Maxim CRC-8.
    pub packets: Vec<LihuiyuPacket>,
    pub waits_for_completion: bool,
    pub bounds_steps: [i32; 4],
    pub final_position_steps: [i32; 2],
    pub summary: LihuiyuJobSummary,
}

#[derive(Debug, Error)]
pub enum LihuiyuCompilationError {
    #[error(transparent)]
    Protocol(#[from] LihuiyuProtocolError),
    #[error("Lihuiyu job contains no supported geometry")]
    EmptyJob,
    #[error("Lihuiyu compiler does not accept {segment} segments")]
    UnsupportedSegment { segment: &'static str },
    #[error("Lihuiyu {field} must be finite and within the supported range")]
    InvalidNumericValue { field: &'static str },
    #[error("Lihuiyu coordinate transform requires a positive {axis} bed dimension when flipping")]
    MissingFlipDimension { axis: &'static str },
    #[error("Lihuiyu coordinate is outside the signed 32-bit M2 step range")]
    CoordinateOutOfRange,
    #[error("Lihuiyu coordinate lies outside the configured machine bed")]
    OutsideBed,
    #[error("Lihuiyu perforation on/off distances must each be at least one M2 step")]
    PerforationBelowNativeResolution,
    #[error("Lihuiyu motion expansion exceeds the configured safety ceiling")]
    MotionLimit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativePoint {
    x: i32,
    y: i32,
}

#[derive(Debug, Clone, Copy)]
struct NativeMotion {
    end: NativePoint,
    beam_on: bool,
}

#[derive(Debug)]
struct NativeRasterLine {
    start: NativePoint,
    motions: Vec<NativeMotion>,
}

#[derive(Debug, Clone, Copy)]
struct NativeBounds {
    min_x: i32,
    min_y: i32,
    max_x: i32,
    max_y: i32,
}

impl NativeBounds {
    fn from_point(point: NativePoint) -> Self {
        Self {
            min_x: point.x,
            min_y: point.y,
            max_x: point.x,
            max_y: point.y,
        }
    }

    fn include(&mut self, point: NativePoint) {
        self.min_x = self.min_x.min(point.x);
        self.min_y = self.min_y.min(point.y);
        self.max_x = self.max_x.max(point.x);
        self.max_y = self.max_y.max(point.y);
    }
}

#[derive(Debug, Default)]
struct SummaryBuilder {
    vector_path_count: usize,
    perforation_path_count: usize,
    raster_scanline_count: usize,
    frame_path_count: usize,
    binary_power_conversion_count: usize,
}

pub fn compile_lihuiyu_job(
    plan: &ExecutionPlan,
    config: &LihuiyuCompilationConfig,
) -> Result<LihuiyuCompiledJob, LihuiyuCompilationError> {
    validate_config(config)?;
    let initial =
        quantize_initial_position(config.initial_position_mm, &config.coordinate_transform)?;
    let mut builder = JobBuilder::new(initial, config.max_motion_commands);
    let mut summary = SummaryBuilder::default();

    for segment in &plan.segments {
        match segment {
            PlanSegment::Travel { .. } => {}
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
                validate_speed_and_power(*speed_mm_min, *power_percent)?;
                if polyline.len() < 2 {
                    continue;
                }
                let mut points = transform_points(polyline, &config.coordinate_transform)?;
                if *closed && points.last() != points.first() {
                    points.push(points[0]);
                }
                let burn = *power_percent > 0.0;
                let speed = *speed_mm_min / 60.0;
                builder.begin_program(points[0], speed, first_delta(&points))?;
                if *perforation_enabled {
                    builder.emit_perforated_polyline(
                        &points,
                        burn,
                        speed,
                        *perforation_on_ms,
                        *perforation_off_ms,
                    )?;
                    summary.perforation_path_count += 1;
                } else {
                    builder.emit_polyline(&points, burn)?;
                    summary.vector_path_count += 1;
                }
                builder.finish_program();
            }
            PlanSegment::Frame {
                path, speed_mm_min, ..
            } => {
                validate_speed(*speed_mm_min)?;
                if path.len() < 2 {
                    continue;
                }
                let points = transform_points(path, &config.coordinate_transform)?;
                builder.begin_program(points[0], *speed_mm_min / 60.0, first_delta(&points))?;
                builder.emit_polyline(&points, false)?;
                builder.finish_program();
                summary.frame_path_count += 1;
            }
            PlanSegment::Raster {
                scanlines,
                line_interval_mm,
                power_mode,
                speed_mm_min,
                scan_angle_deg,
                scan_origin,
                overscan_mm,
                scan_axis,
                power_max_percent,
                power_min_percent,
                ramp_length_mm,
                ..
            } => {
                validate_raster_settings(
                    *line_interval_mm,
                    *speed_mm_min,
                    *scan_angle_deg,
                    *scan_origin,
                    *overscan_mm,
                    *power_min_percent,
                    *power_max_percent,
                    *ramp_length_mm,
                )?;
                let lines = compile_raster_lines(
                    scanlines,
                    *power_mode,
                    *scan_angle_deg,
                    *scan_origin,
                    *overscan_mm,
                    *scan_axis,
                    *power_max_percent,
                    config.grayscale_threshold,
                    &config.coordinate_transform,
                )?;
                if lines.is_empty() {
                    continue;
                }
                let first_motion = lines
                    .iter()
                    .flat_map(|line| {
                        line.motions
                            .iter()
                            .map(move |motion| (line.start, motion.end))
                    })
                    .find(|(start, end)| start != end)
                    .map(|(start, end)| (end.x - start.x, end.y - start.y));
                builder.begin_program(lines[0].start, *speed_mm_min / 60.0, first_motion)?;
                for line in &lines {
                    builder.emit_line_constant(line.start, false)?;
                    for motion in &line.motions {
                        builder.emit_line_constant(motion.end, motion.beam_on)?;
                    }
                }
                builder.finish_program();
                summary.raster_scanline_count += lines.len();
                if matches!(power_mode, PowerMode::Grayscale) || *ramp_length_mm > 0.0 {
                    summary.binary_power_conversion_count += 1;
                }
            }
            PlanSegment::OffsetFill { .. } => {
                return Err(LihuiyuCompilationError::UnsupportedSegment {
                    segment: "unresolved offset-fill",
                });
            }
        }
    }

    if summary.vector_path_count
        + summary.perforation_path_count
        + summary.raster_scanline_count
        + summary.frame_path_count
        == 0
    {
        return Err(LihuiyuCompilationError::EmptyJob);
    }

    let packetized = packetize_egv(&builder.bytes)?;
    let bounds = builder.bounds.ok_or(LihuiyuCompilationError::EmptyJob)?;
    let summary = LihuiyuJobSummary {
        vector_path_count: summary.vector_path_count,
        perforation_path_count: summary.perforation_path_count,
        raster_scanline_count: summary.raster_scanline_count,
        frame_path_count: summary.frame_path_count,
        motion_command_count: builder.motion_commands,
        packet_count: packetized.packets.len(),
        binary_power_conversion_count: summary.binary_power_conversion_count,
        uses_beam: builder.uses_beam,
        hardware_set_power: true,
    };
    Ok(LihuiyuCompiledJob {
        egv_bytes: builder.bytes,
        controller_bytes: packetized.controller_bytes,
        packets: packetized.packets,
        waits_for_completion: packetized.waits_for_completion,
        bounds_steps: [bounds.min_x, bounds.min_y, bounds.max_x, bounds.max_y],
        final_position_steps: [builder.position.x, builder.position.y],
        summary,
    })
}

struct PacketizedEgv {
    controller_bytes: Vec<u8>,
    packets: Vec<LihuiyuPacket>,
    waits_for_completion: bool,
}

fn packetize_egv(egv: &[u8]) -> Result<PacketizedEgv, LihuiyuCompilationError> {
    let mut controller_bytes = Vec::with_capacity(egv.len());
    let mut packets = Vec::new();
    let mut waits_for_completion = false;
    let mut cursor = 0;
    while cursor < egv.len() {
        let remaining = &egv[cursor..];
        let newline = remaining.iter().position(|byte| *byte == b'\n');
        let line_end = newline.unwrap_or(remaining.len());
        let mut line = &remaining[..line_end];
        if newline.is_some() && line.last() == Some(&b'-') {
            line = &line[..line.len() - 1];
            waits_for_completion = true;
        }
        for payload in line.chunks(crate::protocol::PACKET_PAYLOAD_SIZE) {
            controller_bytes.extend_from_slice(payload);
            packets.push(encode_packet(payload, LihuiyuPadding::AsciiF)?);
        }
        cursor += newline.map_or(line_end, |position| position + 1);
    }
    Ok(PacketizedEgv {
        controller_bytes,
        packets,
        waits_for_completion,
    })
}

pub(crate) fn compiled_job_is_consistent(job: &LihuiyuCompiledJob) -> bool {
    packetize_egv(&job.egv_bytes).is_ok_and(|packetized| {
        packetized.controller_bytes == job.controller_bytes
            && packetized.packets == job.packets
            && packetized.waits_for_completion == job.waits_for_completion
            && job.summary.packet_count == job.packets.len()
    })
}

struct JobBuilder {
    bytes: Vec<u8>,
    position: NativePoint,
    x_positive: bool,
    y_positive: bool,
    beam_on: bool,
    uses_beam: bool,
    bounds: Option<NativeBounds>,
    motion_commands: usize,
    max_motion_commands: usize,
}

impl JobBuilder {
    fn new(position: NativePoint, max_motion_commands: usize) -> Self {
        Self {
            bytes: Vec::new(),
            position,
            x_positive: true,
            y_positive: false,
            beam_on: false,
            uses_beam: false,
            bounds: None,
            motion_commands: 0,
            max_motion_commands,
        }
    }

    fn begin_program(
        &mut self,
        start: NativePoint,
        speed_mm_s: f64,
        first_delta: Option<(i32, i32)>,
    ) -> Result<(), LihuiyuCompilationError> {
        self.rapid_to(start)?;
        let (dx, dy) = first_delta.unwrap_or((1, -1));
        if dx != 0 {
            self.x_positive = dx > 0;
        }
        if dy != 0 {
            self.y_positive = dy > 0;
        }
        let horizontal_major = dx.abs() >= dy.abs();
        self.bytes.push(b'I');
        self.bytes.extend(encode_m2_vector_speed(speed_mm_s)?);
        self.bytes.push(b'N');
        if horizontal_major {
            self.bytes.push(y_direction(self.y_positive));
            self.bytes.push(x_direction(self.x_positive));
        } else {
            self.bytes.push(x_direction(self.x_positive));
            self.bytes.push(y_direction(self.y_positive));
        }
        self.bytes.extend_from_slice(b"S1E");
        self.beam_on = false;
        Ok(())
    }

    fn rapid_to(&mut self, target: NativePoint) -> Result<(), LihuiyuCompilationError> {
        self.include(target);
        self.bytes.push(b'I');
        let dx = target.x - self.position.x;
        let dy = target.y - self.position.y;
        if dx != 0 {
            self.bytes.push(x_direction(dx > 0));
            self.bytes.extend(encode_distance(dx.unsigned_abs())?);
        }
        if dy != 0 {
            self.bytes.push(y_direction(dy > 0));
            self.bytes.extend(encode_distance(dy.unsigned_abs())?);
        }
        self.bytes.extend_from_slice(b"S1P\n");
        self.position = target;
        self.beam_on = false;
        Ok(())
    }

    fn finish_program(&mut self) {
        self.bytes.extend_from_slice(b"FNSE-\n");
        self.beam_on = false;
    }

    fn emit_polyline(
        &mut self,
        points: &[NativePoint],
        beam_on: bool,
    ) -> Result<(), LihuiyuCompilationError> {
        for &point in points.iter().skip(1) {
            self.emit_line_constant(point, beam_on)?;
        }
        Ok(())
    }

    fn emit_perforated_polyline(
        &mut self,
        points: &[NativePoint],
        burn: bool,
        speed_mm_s: f64,
        on_ms: f64,
        off_ms: f64,
    ) -> Result<(), LihuiyuCompilationError> {
        for (field, value) in [
            ("perforation on time", on_ms),
            ("perforation off time", off_ms),
        ] {
            if !value.is_finite() || value <= 0.0 {
                return Err(LihuiyuCompilationError::InvalidNumericValue { field });
            }
        }
        let on_distance = speed_mm_s / 1_000.0 * on_ms;
        let off_distance = speed_mm_s / 1_000.0 * off_ms;
        if on_distance < M2_MILLIMETRES_PER_STEP || off_distance < M2_MILLIMETRES_PER_STEP {
            return Err(LihuiyuCompilationError::PerforationBelowNativeResolution);
        }
        let mut phase_on = true;
        let mut remaining = on_distance;
        for &point in points.iter().skip(1) {
            let mut run: Option<(i32, i32, u32, bool)> = None;
            for_each_line_step(self.position, point, |sx, sy| {
                let beam = burn && phase_on;
                match &mut run {
                    Some((rx, ry, count, run_beam))
                        if *rx == sx && *ry == sy && *run_beam == beam =>
                    {
                        *count += 1;
                    }
                    _ => {
                        if let Some((rx, ry, count, run_beam)) = run.take() {
                            self.emit_step_run(rx, ry, count, run_beam)?;
                        }
                        run = Some((sx, sy, 1, beam));
                    }
                }
                let step_mm = if sx != 0 && sy != 0 {
                    M2_MILLIMETRES_PER_STEP * std::f64::consts::SQRT_2
                } else {
                    M2_MILLIMETRES_PER_STEP
                };
                remaining -= step_mm;
                if remaining <= 0.0 {
                    phase_on = !phase_on;
                    remaining += if phase_on { on_distance } else { off_distance };
                }
                Ok(())
            })?;
            if let Some((rx, ry, count, run_beam)) = run {
                self.emit_step_run(rx, ry, count, run_beam)?;
            }
        }
        Ok(())
    }

    fn emit_line_constant(
        &mut self,
        target: NativePoint,
        beam_on: bool,
    ) -> Result<(), LihuiyuCompilationError> {
        let mut run: Option<(i32, i32, u32)> = None;
        for_each_line_step(self.position, target, |sx, sy| {
            match &mut run {
                Some((rx, ry, count)) if *rx == sx && *ry == sy => *count += 1,
                _ => {
                    if let Some((rx, ry, count)) = run.take() {
                        self.emit_step_run(rx, ry, count, beam_on)?;
                    }
                    run = Some((sx, sy, 1));
                }
            }
            Ok(())
        })?;
        if let Some((rx, ry, count)) = run {
            self.emit_step_run(rx, ry, count, beam_on)?;
        }
        Ok(())
    }

    fn emit_step_run(
        &mut self,
        step_x: i32,
        step_y: i32,
        count: u32,
        beam_on: bool,
    ) -> Result<(), LihuiyuCompilationError> {
        if count == 0 {
            return Ok(());
        }
        self.motion_commands = self
            .motion_commands
            .checked_add(1)
            .ok_or(LihuiyuCompilationError::MotionLimit)?;
        if self.motion_commands > self.max_motion_commands {
            return Err(LihuiyuCompilationError::MotionLimit);
        }
        if step_x != 0 && self.x_positive != (step_x > 0) {
            self.x_positive = step_x > 0;
            self.bytes.push(x_direction(self.x_positive));
        }
        if step_y != 0 && self.y_positive != (step_y > 0) {
            self.y_positive = step_y > 0;
            self.bytes.push(y_direction(self.y_positive));
        }
        if step_x != 0 && step_y != 0 {
            self.bytes.push(b'M');
        }
        if self.beam_on != beam_on {
            self.beam_on = beam_on;
            self.bytes.push(if beam_on { b'D' } else { b'U' });
        }
        if beam_on {
            self.uses_beam = true;
        }
        self.bytes.extend(encode_distance(count)?);
        self.position.x += step_x
            * i32::try_from(count).map_err(|_| LihuiyuCompilationError::CoordinateOutOfRange)?;
        self.position.y += step_y
            * i32::try_from(count).map_err(|_| LihuiyuCompilationError::CoordinateOutOfRange)?;
        self.include(self.position);
        Ok(())
    }

    fn include(&mut self, point: NativePoint) {
        if let Some(bounds) = &mut self.bounds {
            bounds.include(point);
        } else {
            self.bounds = Some(NativeBounds::from_point(point));
        }
    }
}

fn for_each_line_step(
    start: NativePoint,
    end: NativePoint,
    mut emit: impl FnMut(i32, i32) -> Result<(), LihuiyuCompilationError>,
) -> Result<(), LihuiyuCompilationError> {
    let mut x = start.x;
    let mut y = start.y;
    let dx = (i64::from(end.x) - i64::from(start.x)).abs();
    let sx = if start.x < end.x { 1 } else { -1 };
    let dy = -(i64::from(end.y) - i64::from(start.y)).abs();
    let sy = if start.y < end.y { 1 } else { -1 };
    let mut error = dx + dy;
    while x != end.x || y != end.y {
        let previous_x = x;
        let previous_y = y;
        let twice_error = 2 * error;
        if twice_error >= dy {
            error += dy;
            x += sx;
        }
        if twice_error <= dx {
            error += dx;
            y += sy;
        }
        emit(x - previous_x, y - previous_y)?;
    }
    Ok(())
}

fn first_delta(points: &[NativePoint]) -> Option<(i32, i32)> {
    points.windows(2).find_map(|pair| {
        let dx = pair[1].x - pair[0].x;
        let dy = pair[1].y - pair[0].y;
        (dx != 0 || dy != 0).then_some((dx, dy))
    })
}

fn x_direction(positive: bool) -> u8 {
    if positive { b'B' } else { b'T' }
}

fn y_direction(positive: bool) -> u8 {
    if positive { b'R' } else { b'L' }
}

#[allow(clippy::too_many_arguments)]
fn compile_raster_lines(
    scanlines: &[Scanline],
    power_mode: PowerMode,
    scan_angle_deg: f64,
    scan_origin: Point2D,
    overscan_mm: f64,
    scan_axis: ScanAxis,
    power_max_percent: f64,
    grayscale_threshold: u8,
    transform: &LihuiyuCoordinateTransform,
) -> Result<Vec<NativeRasterLine>, LihuiyuCompilationError> {
    let orthogonal = is_orthogonal_scan_angle(scan_angle_deg);
    let radians = scan_angle_deg.to_radians();
    let (sin_a, cos_a) = radians.sin_cos();
    let to_native = |run_position: f64, cross_position: f64| {
        let point = if orthogonal {
            match scan_axis {
                ScanAxis::Horizontal => Point2D::new(run_position, cross_position),
                ScanAxis::Vertical => Point2D::new(cross_position, run_position),
            }
        } else {
            Point2D::new(
                scan_origin.x + run_position * cos_a - cross_position * sin_a,
                scan_origin.y + run_position * sin_a + cross_position * cos_a,
            )
        };
        transform_point(point, transform)
    };

    let mut compiled = Vec::new();
    for scanline in scanlines {
        if !scanline.y_mm.is_finite() {
            return Err(LihuiyuCompilationError::InvalidNumericValue {
                field: "raster scanline coordinate",
            });
        }
        let left_to_right = matches!(scanline.direction, ScanDirection::LeftToRight);
        let direction = if left_to_right { 1.0 } else { -1.0 };
        let mut runs: Vec<&ScanRun> = scanline.runs.iter().collect();
        for run in &runs {
            if !run.start_x_mm.is_finite() || !run.end_x_mm.is_finite() {
                return Err(LihuiyuCompilationError::InvalidNumericValue {
                    field: "raster run coordinate",
                });
            }
        }
        runs.retain(|run| (run.end_x_mm - run.start_x_mm).abs() > f64::EPSILON);
        if runs.is_empty() {
            continue;
        }
        runs.sort_by(|a, b| {
            let a_min = a.start_x_mm.min(a.end_x_mm);
            let b_min = b.start_x_mm.min(b.end_x_mm);
            if left_to_right {
                a_min.total_cmp(&b_min)
            } else {
                b_min.total_cmp(&a_min)
            }
        });
        let directed_bounds = |run: &ScanRun| {
            let min = run.start_x_mm.min(run.end_x_mm);
            let max = run.start_x_mm.max(run.end_x_mm);
            if left_to_right {
                (min, max)
            } else {
                (max, min)
            }
        };
        let (first_start, _) = directed_bounds(runs[0]);
        let (_, last_end) = directed_bounds(runs[runs.len() - 1]);
        let start_position = first_start - direction * overscan_mm;
        let mut line = NativeRasterLine {
            start: to_native(start_position, scanline.y_mm)?,
            motions: Vec::new(),
        };
        let mut current_position = start_position;
        for run in runs {
            let (run_start, run_end) = directed_bounds(run);
            if (run_start - current_position).abs() > f64::EPSILON {
                push_raster_motion(
                    &mut line.motions,
                    to_native(run_start, scanline.y_mm)?,
                    false,
                );
            }
            match power_mode {
                PowerMode::Binary => push_raster_motion(
                    &mut line.motions,
                    to_native(run_end, scanline.y_mm)?,
                    power_max_percent > 0.0,
                ),
                PowerMode::Grayscale if run.power_values.is_empty() => push_raster_motion(
                    &mut line.motions,
                    to_native(run_end, scanline.y_mm)?,
                    power_max_percent > 0.0,
                ),
                PowerMode::Grayscale => {
                    let mut powers = run.power_values.clone();
                    if !left_to_right {
                        powers.reverse();
                    }
                    let pixel_width = (run_end - run_start) / powers.len() as f64;
                    let mut group_on = powers[0] >= grayscale_threshold && power_max_percent > 0.0;
                    for index in 1..=powers.len() {
                        let next_on = index < powers.len()
                            && powers[index] >= grayscale_threshold
                            && power_max_percent > 0.0;
                        if index == powers.len() || next_on != group_on {
                            push_raster_motion(
                                &mut line.motions,
                                to_native(run_start + index as f64 * pixel_width, scanline.y_mm)?,
                                group_on,
                            );
                            group_on = next_on;
                        }
                    }
                }
            }
            current_position = run_end;
        }
        let overscan_end = last_end + direction * overscan_mm;
        if (overscan_end - current_position).abs() > f64::EPSILON {
            push_raster_motion(
                &mut line.motions,
                to_native(overscan_end, scanline.y_mm)?,
                false,
            );
        }
        if !line.motions.is_empty() {
            compiled.push(line);
        }
    }
    Ok(compiled)
}

fn push_raster_motion(motions: &mut Vec<NativeMotion>, end: NativePoint, beam_on: bool) {
    // A motion that quantizes to the previous endpoint commands no movement;
    // drop it rather than rewriting the previous motion's beam state.
    if motions.last().is_none_or(|motion| motion.end != end) {
        motions.push(NativeMotion { end, beam_on });
    }
}

fn is_orthogonal_scan_angle(angle_deg: f64) -> bool {
    let normalized = angle_deg.rem_euclid(360.0);
    normalized
        .min(360.0 - normalized)
        .min((normalized - 90.0).abs())
        .min((normalized - 180.0).abs())
        .min((normalized - 270.0).abs())
        < 0.5
}

#[allow(clippy::too_many_arguments)]
fn validate_raster_settings(
    line_interval_mm: f64,
    speed_mm_min: f64,
    scan_angle_deg: f64,
    scan_origin: Point2D,
    overscan_mm: f64,
    power_min_percent: f64,
    power_max_percent: f64,
    ramp_length_mm: f64,
) -> Result<(), LihuiyuCompilationError> {
    validate_speed_and_power(speed_mm_min, power_max_percent)?;
    validate_power(power_min_percent)?;
    for (field, value) in [
        ("raster line interval", line_interval_mm),
        ("raster scan angle", scan_angle_deg),
        ("raster scan origin X", scan_origin.x),
        ("raster scan origin Y", scan_origin.y),
        ("raster overscan", overscan_mm),
        ("raster ramp length", ramp_length_mm),
    ] {
        if !value.is_finite() {
            return Err(LihuiyuCompilationError::InvalidNumericValue { field });
        }
    }
    if line_interval_mm <= 0.0 || overscan_mm < 0.0 || ramp_length_mm < 0.0 {
        return Err(LihuiyuCompilationError::InvalidNumericValue {
            field: "raster geometry",
        });
    }
    if power_min_percent > power_max_percent {
        return Err(LihuiyuCompilationError::InvalidNumericValue {
            field: "minimum power above maximum power",
        });
    }
    Ok(())
}

fn validate_speed_and_power(
    speed_mm_min: f64,
    power_percent: f64,
) -> Result<(), LihuiyuCompilationError> {
    validate_speed(speed_mm_min)?;
    validate_power(power_percent)
}

fn validate_speed(speed_mm_min: f64) -> Result<(), LihuiyuCompilationError> {
    if !speed_mm_min.is_finite() || speed_mm_min <= 0.0 {
        return Err(LihuiyuCompilationError::InvalidNumericValue { field: "speed" });
    }
    Ok(())
}

fn validate_power(power_percent: f64) -> Result<(), LihuiyuCompilationError> {
    if !power_percent.is_finite() || !(0.0..=100.0).contains(&power_percent) {
        return Err(LihuiyuCompilationError::InvalidNumericValue { field: "power" });
    }
    Ok(())
}

fn validate_config(config: &LihuiyuCompilationConfig) -> Result<(), LihuiyuCompilationError> {
    let transform = &config.coordinate_transform;
    for (field, value) in [
        ("X offset", transform.offset_x_mm),
        ("Y offset", transform.offset_y_mm),
        ("bed width", transform.bed_width_mm),
        ("bed height", transform.bed_height_mm),
        ("initial X position", config.initial_position_mm.x),
        ("initial Y position", config.initial_position_mm.y),
    ] {
        if !value.is_finite() {
            return Err(LihuiyuCompilationError::InvalidNumericValue { field });
        }
    }
    if transform.bed_width_mm < 0.0 || transform.bed_height_mm < 0.0 {
        return Err(LihuiyuCompilationError::InvalidNumericValue {
            field: "bed dimension",
        });
    }
    if transform.flip_x && transform.bed_width_mm <= 0.0 {
        return Err(LihuiyuCompilationError::MissingFlipDimension { axis: "X" });
    }
    if transform.flip_y && transform.bed_height_mm <= 0.0 {
        return Err(LihuiyuCompilationError::MissingFlipDimension { axis: "Y" });
    }
    if config.max_motion_commands == 0 {
        return Err(LihuiyuCompilationError::InvalidNumericValue {
            field: "motion command ceiling",
        });
    }
    Ok(())
}

fn transform_points(
    points: &[Point2D],
    transform: &LihuiyuCoordinateTransform,
) -> Result<Vec<NativePoint>, LihuiyuCompilationError> {
    points
        .iter()
        .copied()
        .map(|point| transform_point(point, transform))
        .collect()
}

fn transform_point(
    point: Point2D,
    transform: &LihuiyuCoordinateTransform,
) -> Result<NativePoint, LihuiyuCompilationError> {
    if !point.x.is_finite() || !point.y.is_finite() {
        return Err(LihuiyuCompilationError::InvalidNumericValue {
            field: "coordinate",
        });
    }
    let x = if transform.flip_x {
        transform.bed_width_mm - point.x
    } else {
        point.x
    } + transform.offset_x_mm;
    let y = if transform.flip_y {
        transform.bed_height_mm - point.y
    } else {
        point.y
    } + transform.offset_y_mm;
    quantize_checked(Point2D::new(x, y), transform)
}

fn quantize_initial_position(
    point: Point2D,
    transform: &LihuiyuCoordinateTransform,
) -> Result<NativePoint, LihuiyuCompilationError> {
    quantize_checked(point, transform)
}

fn quantize_checked(
    point: Point2D,
    transform: &LihuiyuCoordinateTransform,
) -> Result<NativePoint, LihuiyuCompilationError> {
    if point.x < 0.0
        || point.y < 0.0
        || (transform.bed_width_mm > 0.0 && point.x > transform.bed_width_mm)
        || (transform.bed_height_mm > 0.0 && point.y > transform.bed_height_mm)
    {
        return Err(LihuiyuCompilationError::OutsideBed);
    }
    Ok(NativePoint {
        x: millimetres_to_steps(point.x)?,
        y: millimetres_to_steps(point.y)?,
    })
}

fn millimetres_to_steps(value: f64) -> Result<i32, LihuiyuCompilationError> {
    let steps = value / M2_MILLIMETRES_PER_STEP;
    if !steps.is_finite() || steps < f64::from(i32::MIN) || steps > f64::from(i32::MAX) {
        return Err(LihuiyuCompilationError::CoordinateOutOfRange);
    }
    Ok(steps.round() as i32)
}

#[cfg(test)]
mod tests {
    use beambench_common::Bounds;
    use beambench_planner::{DirectionMode, PowerMode, ScanRun};

    use super::*;

    fn plan(segments: Vec<PlanSegment>) -> ExecutionPlan {
        ExecutionPlan {
            id: Default::default(),
            project_id: Default::default(),
            revision_hash: "lihuiyu-compiler-test".to_string(),
            created_at: Default::default(),
            bounds: Bounds::new(Point2D::zero(), Point2D::new(100.0, 100.0)),
            total_distance_mm: 0.0,
            estimated_duration_secs: 0.0,
            segments,
            layer_order: Vec::new(),
            warnings: Vec::new(),
            failed_entries: Vec::new(),
        }
    }

    fn vector(points: Vec<Point2D>, closed: bool, power: f64) -> PlanSegment {
        PlanSegment::Vector {
            polyline: points,
            closed,
            power_percent: power,
            speed_mm_min: 900.0,
            layer_id: "vector".to_string(),
            cut_entry_id: "entry".to_string(),
            perforation_enabled: false,
            perforation_on_ms: 0.0,
            perforation_off_ms: 0.0,
            source_object_id: None,
            source_subpath_index: None,
        }
    }

    #[test]
    fn rectangle_matches_pinned_meerk40t_golden_with_stream_terminator() {
        let job = compile_lihuiyu_job(
            &plan(vec![vector(
                vec![
                    Point2D::new(20.0, 20.0),
                    Point2D::new(30.0, 20.0),
                    Point2D::new(30.0, 30.0),
                    Point2D::new(20.0, 30.0),
                ],
                true,
                100.0,
            )]),
            &LihuiyuCompilationConfig::default(),
        )
        .unwrap();
        assert_eq!(
            job.egv_bytes,
            b"IBzzzvRzzzvS1P\nICV2490731016000027CNLBS1EDz139Rz139Tz139Lz139FNSE-\n"
        );
        assert_eq!(
            job.controller_bytes,
            b"IBzzzvRzzzvS1PICV2490731016000027CNLBS1EDz139Rz139Tz139Lz139FNSE"
        );
        assert!(job.waits_for_completion);
        assert!(job.summary.uses_beam);
        assert!(job.summary.hardware_set_power);
        assert_eq!(job.bounds_steps, [787, 787, 1181, 1181]);
    }

    #[test]
    fn packetize_strips_wait_marker_when_line_fills_exact_packet() {
        let mut egv = vec![b'V'; crate::protocol::PACKET_PAYLOAD_SIZE - 1];
        egv.push(b'-');
        egv.push(b'\n');
        let packetized = packetize_egv(&egv).unwrap();
        assert!(packetized.waits_for_completion);
        assert_eq!(
            packetized.controller_bytes,
            vec![b'V'; crate::protocol::PACKET_PAYLOAD_SIZE - 1]
        );
    }

    #[test]
    fn packetize_strips_wait_marker_when_line_spans_multiple_exact_packets() {
        let mut egv = vec![b'V'; 2 * crate::protocol::PACKET_PAYLOAD_SIZE - 1];
        egv.push(b'-');
        egv.push(b'\n');
        let packetized = packetize_egv(&egv).unwrap();
        assert!(packetized.waits_for_completion);
        assert_eq!(
            packetized.controller_bytes,
            vec![b'V'; 2 * crate::protocol::PACKET_PAYLOAD_SIZE - 1]
        );
        assert_eq!(packetized.packets.len(), 2);
    }

    #[test]
    fn degenerate_raster_motion_does_not_rewrite_previous_beam_state() {
        let mut motions = Vec::new();
        push_raster_motion(&mut motions, NativePoint { x: 100, y: 0 }, false);
        push_raster_motion(&mut motions, NativePoint { x: 100, y: 0 }, true);
        assert_eq!(motions.len(), 1);
        assert!(
            !motions[0].beam_on,
            "sub-step burn run must not convert the preceding travel into a burn"
        );

        let mut burn_then_travel = Vec::new();
        push_raster_motion(&mut burn_then_travel, NativePoint { x: 200, y: 0 }, true);
        push_raster_motion(&mut burn_then_travel, NativePoint { x: 200, y: 0 }, false);
        assert_eq!(burn_then_travel.len(), 1);
        assert!(
            burn_then_travel[0].beam_on,
            "sub-step travel must not silently drop the preceding burn"
        );
    }

    #[test]
    fn frame_is_forced_to_zero_output() {
        let segment = PlanSegment::Frame {
            path: vec![Point2D::new(1.0, 1.0), Point2D::new(5.0, 1.0)],
            power_percent: 100.0,
            speed_mm_min: 600.0,
        };
        let job = compile_lihuiyu_job(&plan(vec![segment]), &Default::default()).unwrap();
        assert!(!job.summary.uses_beam);
        assert_eq!(job.summary.frame_path_count, 1);
        assert!(!job.controller_bytes.contains(&b'D'));
    }

    #[test]
    fn arbitrary_vector_is_bresenham_rasterized_without_changing_endpoint() {
        let job = compile_lihuiyu_job(
            &plan(vec![vector(
                vec![Point2D::new(1.0, 1.0), Point2D::new(2.0, 1.5)],
                false,
                50.0,
            )]),
            &Default::default(),
        )
        .unwrap();
        assert!(job.controller_bytes.contains(&b'M'));
        assert_eq!(job.final_position_steps, [79, 59]);
        assert!(job.summary.motion_command_count > 1);
    }

    #[test]
    fn grayscale_raster_is_thresholded_to_binary_spans() {
        let raster = PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 2.0,
                runs: vec![ScanRun {
                    start_x_mm: 1.0,
                    end_x_mm: 5.0,
                    power_values: vec![0, 127, 128, 255],
                }],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 1.0,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Grayscale,
            speed_mm_min: 900.0,
            layer_id: "raster".to_string(),
            cut_entry_id: "entry".to_string(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::zero(),
            overscan_mm: 0.0,
            outlines: Vec::new(),
            scan_axis: ScanAxis::Horizontal,
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 1.0,
        };
        let job = compile_lihuiyu_job(&plan(vec![raster]), &Default::default()).unwrap();
        let stream = String::from_utf8(job.controller_bytes.clone()).unwrap();
        assert!(stream.contains('D'));
        assert_eq!(job.summary.raster_scanline_count, 1);
        assert_eq!(job.summary.binary_power_conversion_count, 1);
    }

    #[test]
    fn perforation_emits_bounded_on_and_off_phases() {
        let mut segment = vector(
            vec![Point2D::new(1.0, 1.0), Point2D::new(20.0, 1.0)],
            false,
            100.0,
        );
        if let PlanSegment::Vector {
            perforation_enabled,
            perforation_on_ms,
            perforation_off_ms,
            ..
        } = &mut segment
        {
            *perforation_enabled = true;
            *perforation_on_ms = 100.0;
            *perforation_off_ms = 100.0;
        }
        let job = compile_lihuiyu_job(&plan(vec![segment]), &Default::default()).unwrap();
        assert!(job.controller_bytes.contains(&b'D'));
        assert!(job.controller_bytes.contains(&b'U'));
        assert_eq!(job.summary.perforation_path_count, 1);
    }

    #[test]
    fn packets_reconstruct_the_clear_stream_and_validate() {
        let job = compile_lihuiyu_job(
            &plan(vec![vector(
                vec![Point2D::new(1.0, 1.0), Point2D::new(20.0, 1.0)],
                false,
                100.0,
            )]),
            &Default::default(),
        )
        .unwrap();
        for packet in &job.packets {
            let payload = crate::protocol::decode_packet(packet.as_ref()).unwrap();
            assert!(!payload.contains(&b'\n'));
            assert!(!payload.contains(&b'-'));
        }
        assert_eq!(job.summary.packet_count, job.packets.len());
        assert!(job.waits_for_completion);
    }

    #[test]
    fn unresolved_and_out_of_bed_geometry_fail_closed() {
        let offset = PlanSegment::OffsetFill {
            layer_id: "layer".to_string(),
            object_id: "object".to_string(),
            offset_mm: 1.0,
            angle_deg: 0.0,
        };
        assert!(matches!(
            compile_lihuiyu_job(&plan(vec![offset]), &Default::default()),
            Err(LihuiyuCompilationError::UnsupportedSegment { .. })
        ));

        let config = LihuiyuCompilationConfig {
            coordinate_transform: LihuiyuCoordinateTransform {
                bed_width_mm: 10.0,
                bed_height_mm: 10.0,
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(matches!(
            compile_lihuiyu_job(
                &plan(vec![vector(
                    vec![Point2D::new(1.0, 1.0), Point2D::new(11.0, 1.0)],
                    false,
                    100.0,
                )]),
                &config,
            ),
            Err(LihuiyuCompilationError::OutsideBed)
        ));
    }
}

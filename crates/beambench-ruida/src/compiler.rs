use std::{
    collections::{HashMap, HashSet},
    io::{self, Write},
};

use beambench_common::{Bounds, Point2D};
use beambench_planner::{ExecutionPlan, PlanSegment, PowerMode, ScanAxis, ScanDirection, Scanline};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::protocol::{
    DEFAULT_MAGIC, RuidaCodec, RuidaProtocolError, encode_i14, encode_i32, encode_power_percent,
    encode_speed_mm_s, encode_u14, encode_u35,
};

const REF_POINT_MACHINE_ZERO: &[u8] = &[0xD8, 0x10];
const REF_POINT_USER_ORIGIN: &[u8] = &[0xD8, 0x11];
const REF_POINT_CURRENT_POSITION: &[u8] = &[0xD8, 0x12];
const SET_ABSOLUTE: &[u8] = &[0xE6, 0x01];
const REF_POINT_SET: &[u8] = &[0xF0];
const ENABLE_BLOCK_CUTTING: &[u8] = &[0xF1, 0x02];
const START_PROCESS: &[u8] = &[0xD8, 0x00];
const FEED_REPEAT: &[u8] = &[0xE7, 0x06];
const SET_FEED_AUTO_PAUSE: &[u8] = &[0xE7, 0x38];
const PROCESS_TOP_LEFT: &[u8] = &[0xE7, 0x03];
const PROCESS_BOTTOM_RIGHT: &[u8] = &[0xE7, 0x07];
const DOCUMENT_MIN_POINT: &[u8] = &[0xE7, 0x50];
const DOCUMENT_MAX_POINT: &[u8] = &[0xE7, 0x51];
const PROCESS_REPEAT: &[u8] = &[0xE7, 0x04];
const ARRAY_DIRECTION: &[u8] = &[0xE7, 0x05];
const SPEED_LASER_1_PART: &[u8] = &[0xC9, 0x04];
const MIN_POWER_1_PART: &[u8] = &[0xC6, 0x31];
const MAX_POWER_1_PART: &[u8] = &[0xC6, 0x32];
const MIN_POWER_2_PART: &[u8] = &[0xC6, 0x41];
const MAX_POWER_2_PART: &[u8] = &[0xC6, 0x42];
const LAYER_COLOR_PART: &[u8] = &[0xCA, 0x06];
const WORK_MODE_PART: &[u8] = &[0xCA, 0x41];
const PART_MIN_POINT: &[u8] = &[0xE7, 0x52];
const PART_MAX_POINT: &[u8] = &[0xE7, 0x53];
const PART_MIN_POINT_EX: &[u8] = &[0xE7, 0x61];
const PART_MAX_POINT_EX: &[u8] = &[0xE7, 0x62];
const MAX_LAYER_PART: &[u8] = &[0xCA, 0x22];
const PEN_OFFSET: &[u8] = &[0xE7, 0x54];
const LAYER_OFFSET: &[u8] = &[0xE7, 0x55];
const DISPLAY_OFFSET: &[u8] = &[0xF1, 0x03];
const FEED_INFO: &[u8] = &[0xE7, 0x0A];
const ARRAY_START: &[u8] = &[0xEA];
const SET_CURRENT_ELEMENT_INDEX: &[u8] = &[0xE7, 0x60];
const ARRAY_ENABLE_MIRROR_CUT: &[u8] = &[0xE7, 0x0B];
const ARRAY_MIN_POINT: &[u8] = &[0xE7, 0x13];
const ARRAY_MAX_POINT: &[u8] = &[0xE7, 0x17];
const ARRAY_ADD: &[u8] = &[0xE7, 0x23];
const ARRAY_MIRROR: &[u8] = &[0xE7, 0x24];
const ARRAY_REPEAT: &[u8] = &[0xE7, 0x08];
const LAYER_NUMBER_PART: &[u8] = &[0xCA, 0x02];
const WORK_MODE_HORIZONTAL_RASTER: &[u8] = &[0xCA, 0x01, 0x01];
const WORK_MODE_VERTICAL_RASTER: &[u8] = &[0xCA, 0x01, 0x03];
const LASER_DEVICE_0: &[u8] = &[0xCA, 0x01, 0x10];
const AIR_ASSIST_OFF: &[u8] = &[0xCA, 0x01, 0x12];
const AIR_ASSIST_ON: &[u8] = &[0xCA, 0x01, 0x13];
const SPEED_LASER_1: &[u8] = &[0xC9, 0x02];
const LASER_ON_DELAY: &[u8] = &[0xC6, 0x12];
const LASER_OFF_DELAY: &[u8] = &[0xC6, 0x13];
const MIN_POWER_1: &[u8] = &[0xC6, 0x01];
const MAX_POWER_1: &[u8] = &[0xC6, 0x02];
const MIN_POWER_2: &[u8] = &[0xC6, 0x21];
const MAX_POWER_2: &[u8] = &[0xC6, 0x22];
const ENABLE_LASER_TUBE_START: &[u8] = &[0xCA, 0x03];
const MOVE_ABS_XY: &[u8] = &[0x88];
const CUT_ABS_XY: &[u8] = &[0xA8];
const BLOCK_END: &[u8] = &[0xE7, 0x00];
const LAYER_END: &[u8] = &[0xCA, 0x01, 0x00];
const DISABLE_LASER_2_OFFSET: &[u8] = &[0xCA, 0x01, 0x30];
const SET_FILE_SUM: &[u8] = &[0xE5, 0x05];
const END_OF_FILE: &[u8] = &[0xD7];
const MIN_NATIVE_PERFORATION_DISTANCE_MM: f64 = 0.001;
const MAX_PERFORATION_MOTIONS: usize = 1_000_000;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuidaReferencePoint {
    #[default]
    MachineZero,
    UserOrigin,
    CurrentPosition,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuidaCoordinateTransform {
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

impl Default for RuidaCoordinateTransform {
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
pub struct RuidaPowerOverride {
    pub cut_entry_id: String,
    pub min_power_percent: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuidaCompilationConfig {
    #[serde(default = "default_magic")]
    pub magic: u8,
    #[serde(default)]
    pub reference_point: RuidaReferencePoint,
    #[serde(default)]
    pub coordinate_transform: RuidaCoordinateTransform,
    #[serde(default)]
    pub air_assist_cut_entry_ids: Vec<String>,
    #[serde(default)]
    pub min_power_overrides: Vec<RuidaPowerOverride>,
}

const fn default_magic() -> u8 {
    DEFAULT_MAGIC
}

impl Default for RuidaCompilationConfig {
    fn default() -> Self {
        Self {
            magic: DEFAULT_MAGIC,
            reference_point: RuidaReferencePoint::MachineZero,
            coordinate_transform: RuidaCoordinateTransform::default(),
            air_assist_cut_entry_ids: Vec::new(),
            min_power_overrides: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuidaLayerSummary {
    pub part: u8,
    pub layer_id: String,
    pub cut_entry_id: String,
    pub path_count: usize,
    pub point_count: usize,
    #[serde(default)]
    pub raster_scanline_count: usize,
    #[serde(default)]
    pub raster_motion_count: usize,
    pub air_assist: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuidaCompiledJob {
    /// Unswizzled command stream used for command-level diagnostics.
    pub clear_bytes: Vec<u8>,
    /// Swizzled `.rd` file bytes suitable for offline comparison tooling.
    pub rd_file_bytes: Vec<u8>,
    pub clear_file_sum: u64,
    pub bounds_micrometres: [i32; 4],
    pub layers: Vec<RuidaLayerSummary>,
}

impl RuidaCompiledJob {
    /// Write the deterministic swizzled `.rd` artifact to an offline sink.
    /// This method performs no controller discovery, upload, or execution.
    pub fn write_rd(&self, writer: &mut impl Write) -> io::Result<()> {
        writer.write_all(&self.rd_file_bytes)
    }
}

/// Compile the deterministic, zero-output artifact used to exercise Ruida
/// storage transfer without exposing an execution path.
///
/// The artifact is a one-millimetre square at the lower-left of the selected
/// coordinate system, uses zero minimum and maximum power on both laser
/// channels, and leaves air assist disabled. Real-hardware validation still
/// requires a manufacturer-supported emission-inhibited controller state.
pub fn compile_ruida_storage_sentinel(
    config: &RuidaCompilationConfig,
) -> Result<RuidaCompiledJob, RuidaCompilationError> {
    let min = Point2D::new(1.0, 1.0);
    let max = Point2D::new(2.0, 2.0);
    let plan = ExecutionPlan {
        id: Default::default(),
        project_id: Default::default(),
        revision_hash: "ruida-storage-sentinel-v1".to_string(),
        created_at: Default::default(),
        bounds: Bounds::new(min, max),
        total_distance_mm: 4.0,
        estimated_duration_secs: 0.4,
        segments: vec![PlanSegment::Vector {
            polyline: vec![
                min,
                Point2D::new(max.x, min.y),
                max,
                Point2D::new(min.x, max.y),
            ],
            closed: true,
            power_percent: 0.0,
            speed_mm_min: 600.0,
            layer_id: "__ruida_storage_sentinel__".to_string(),
            cut_entry_id: "__ruida_storage_sentinel__".to_string(),
            perforation_enabled: false,
            perforation_on_ms: 0.0,
            perforation_off_ms: 0.0,
            source_object_id: None,
            source_subpath_index: None,
        }],
        layer_order: vec!["__ruida_storage_sentinel__".to_string()],
        warnings: Vec::new(),
        failed_entries: Vec::new(),
    };
    compile_ruida_job(&plan, config)
}

#[derive(Debug, Error)]
pub enum RuidaCompilationError {
    #[error(transparent)]
    Protocol(#[from] RuidaProtocolError),
    #[error("Ruida job contains no supported geometry")]
    EmptyJob,
    #[error("Ruida v1 compiler does not yet accept {segment} segments")]
    UnsupportedSegment { segment: &'static str },
    #[error("Ruida perforation on/off distances must each be at least one micrometre")]
    PerforationBelowNativeResolution,
    #[error("Ruida perforation expansion exceeds the one-million-motion safety ceiling")]
    PerforationMotionLimit,
    #[error("Ruida job contains more than 256 independently configured operations")]
    TooManyParts,
    #[error("Ruida {field} must be finite and within the supported range")]
    InvalidNumericValue { field: &'static str },
    #[error("Ruida coordinate transform requires a positive {axis} bed dimension when flipping")]
    MissingFlipDimension { axis: &'static str },
    #[error(
        "Ruida job geometry (including raster overscan travel) extends outside the machine bed"
    )]
    OutsideBed,
    #[error("Ruida coordinate is outside the signed 32-bit micrometre range")]
    CoordinateOutOfRange,
    #[error("Ruida clear job stream exceeds the 35-bit file-sum range")]
    FileSumOutOfRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativePoint {
    x: i32,
    y: i32,
}

#[derive(Debug)]
struct NativeOperation {
    layer_id: String,
    cut_entry_id: String,
    paths: Vec<Vec<NativePoint>>,
    perforation_paths: Vec<Vec<NativePathMotion>>,
    raster_lines: Vec<NativeRasterLine>,
    raster_axis: Option<ScanAxis>,
    speed_mm_min: f64,
    min_power_percent: f64,
    max_power_percent: f64,
    air_assist: bool,
    burn: bool,
    bounds: NativeBounds,
}

#[derive(Debug, Clone, Copy)]
enum NativePathMotion {
    Move(NativePoint),
    Cut(NativePoint),
}

#[derive(Debug)]
struct NativeRasterLine {
    motions: Vec<NativeRasterMotion>,
}

#[derive(Debug, Clone, Copy)]
enum NativeRasterMotion {
    Move(NativePoint),
    Cut {
        end: NativePoint,
        min_power_percent: f64,
        max_power_percent: f64,
    },
}

#[derive(Debug, Clone, Copy)]
struct NativeBounds {
    min_x: i32,
    min_y: i32,
    max_x: i32,
    max_y: i32,
}

impl NativeBounds {
    fn from_points(points: &[NativePoint]) -> Self {
        let first = points[0];
        points.iter().skip(1).fold(
            Self {
                min_x: first.x,
                min_y: first.y,
                max_x: first.x,
                max_y: first.y,
            },
            |mut bounds, point| {
                bounds.min_x = bounds.min_x.min(point.x);
                bounds.min_y = bounds.min_y.min(point.y);
                bounds.max_x = bounds.max_x.max(point.x);
                bounds.max_y = bounds.max_y.max(point.y);
                bounds
            },
        )
    }

    fn include(self, other: Self) -> Self {
        Self {
            min_x: self.min_x.min(other.min_x),
            min_y: self.min_y.min(other.min_y),
            max_x: self.max_x.max(other.max_x),
            max_y: self.max_y.max(other.max_y),
        }
    }

    fn from_raster_lines(lines: &[NativeRasterLine]) -> Option<Self> {
        let mut points = lines.iter().flat_map(|line| {
            line.motions.iter().map(|motion| match motion {
                NativeRasterMotion::Move(point) => *point,
                NativeRasterMotion::Cut { end, .. } => *end,
            })
        });
        let first = points.next()?;
        Some(points.fold(
            Self {
                min_x: first.x,
                min_y: first.y,
                max_x: first.x,
                max_y: first.y,
            },
            |mut bounds, point| {
                bounds.min_x = bounds.min_x.min(point.x);
                bounds.min_y = bounds.min_y.min(point.y);
                bounds.max_x = bounds.max_x.max(point.x);
                bounds.max_y = bounds.max_y.max(point.y);
                bounds
            },
        ))
    }

    fn from_path_motions(motions: &[NativePathMotion]) -> Option<Self> {
        let mut points = motions.iter().map(|motion| match motion {
            NativePathMotion::Move(point) | NativePathMotion::Cut(point) => *point,
        });
        let first = points.next()?;
        Some(points.fold(
            Self {
                min_x: first.x,
                min_y: first.y,
                max_x: first.x,
                max_y: first.y,
            },
            |mut bounds, point| {
                bounds.min_x = bounds.min_x.min(point.x);
                bounds.min_y = bounds.min_y.min(point.y);
                bounds.max_x = bounds.max_x.max(point.x);
                bounds.max_y = bounds.max_y.max(point.y);
                bounds
            },
        ))
    }
}

pub fn compile_ruida_job(
    plan: &ExecutionPlan,
    config: &RuidaCompilationConfig,
) -> Result<RuidaCompiledJob, RuidaCompilationError> {
    validate_transform(&config.coordinate_transform)?;
    let air_assist: HashSet<&str> = config
        .air_assist_cut_entry_ids
        .iter()
        .map(String::as_str)
        .collect();
    let minimum_power: HashMap<&str, f64> = config
        .min_power_overrides
        .iter()
        .map(|entry| (entry.cut_entry_id.as_str(), entry.min_power_percent))
        .collect();
    let mut operations = Vec::new();

    for segment in &plan.segments {
        match segment {
            PlanSegment::Travel { .. } => {}
            PlanSegment::Vector {
                polyline,
                closed,
                power_percent,
                speed_mm_min,
                layer_id,
                cut_entry_id,
                perforation_enabled,
                perforation_on_ms,
                perforation_off_ms,
                ..
            } => {
                if polyline.len() < 2 {
                    continue;
                }
                let mut emission_polyline = polyline.clone();
                if *closed && emission_polyline.last() != emission_polyline.first() {
                    emission_polyline.push(emission_polyline[0]);
                }
                let min_power = minimum_power
                    .get(cut_entry_id.as_str())
                    .copied()
                    .unwrap_or(*power_percent);
                if *perforation_enabled {
                    let motions = compile_perforation_path(
                        &emission_polyline,
                        *speed_mm_min,
                        *perforation_on_ms,
                        *perforation_off_ms,
                        &config.coordinate_transform,
                    )?;
                    add_perforation_operation(
                        &mut operations,
                        layer_id.clone(),
                        cut_entry_id.clone(),
                        motions,
                        *speed_mm_min,
                        min_power,
                        *power_percent,
                        air_assist.contains(cut_entry_id.as_str()),
                    )?;
                } else {
                    add_operation(
                        &mut operations,
                        layer_id.clone(),
                        cut_entry_id.clone(),
                        transform_points(&emission_polyline, &config.coordinate_transform)?,
                        *speed_mm_min,
                        min_power,
                        *power_percent,
                        air_assist.contains(cut_entry_id.as_str()),
                    )?;
                }
            }
            PlanSegment::Frame {
                path,
                power_percent,
                speed_mm_min,
            } => {
                if path.len() < 2 {
                    continue;
                }
                add_operation(
                    &mut operations,
                    "__frame__".to_string(),
                    "__frame__".to_string(),
                    transform_points(path, &config.coordinate_transform)?,
                    *speed_mm_min,
                    *power_percent,
                    *power_percent,
                    false,
                )?;
            }
            PlanSegment::Raster {
                scanlines,
                line_interval_mm,
                power_mode,
                speed_mm_min,
                layer_id,
                cut_entry_id,
                scan_angle_deg,
                scan_origin,
                overscan_mm,
                scan_axis,
                power_max_percent,
                power_min_percent,
                ramp_length_mm,
                ..
            } => {
                let min_power = minimum_power
                    .get(cut_entry_id.as_str())
                    .copied()
                    .unwrap_or(*power_min_percent);
                let raster_lines = compile_raster_lines(
                    scanlines,
                    *line_interval_mm,
                    *power_mode,
                    *scan_angle_deg,
                    *scan_origin,
                    *scan_axis,
                    *overscan_mm,
                    *ramp_length_mm,
                    min_power,
                    *power_max_percent,
                    &config.coordinate_transform,
                )?;
                if raster_lines.is_empty() {
                    continue;
                }
                add_raster_operation(
                    &mut operations,
                    layer_id.clone(),
                    cut_entry_id.clone(),
                    raster_lines,
                    *scan_axis,
                    *speed_mm_min,
                    min_power,
                    *power_max_percent,
                    air_assist.contains(cut_entry_id.as_str()),
                )?;
            }
            PlanSegment::OffsetFill { .. } => {
                return Err(RuidaCompilationError::UnsupportedSegment {
                    segment: "unresolved offset-fill",
                });
            }
        }
    }

    if operations.is_empty() {
        return Err(RuidaCompilationError::EmptyJob);
    }
    if operations.len() > 256 {
        return Err(RuidaCompilationError::TooManyParts);
    }

    let job_bounds = operations
        .iter()
        .skip(1)
        .fold(operations[0].bounds, |bounds, operation| {
            bounds.include(operation.bounds)
        });
    let mut builder = JobBuilder::default();
    write_header(
        &mut builder,
        &operations,
        job_bounds,
        config.reference_point,
    )?;
    for (index, operation) in operations.iter().enumerate() {
        write_operation(&mut builder, index as u8, operation)?;
        write_layer_end(&mut builder);
    }
    let clear_file_sum = builder
        .bytes
        .iter()
        .try_fold(0_u64, |sum, byte| sum.checked_add(u64::from(*byte)))
        .and_then(|sum| sum.checked_add(u64::from(END_OF_FILE[0])))
        .ok_or(RuidaCompilationError::FileSumOutOfRange)?;
    if clear_file_sum > 0x07_FFFF_FFFF {
        return Err(RuidaCompilationError::FileSumOutOfRange);
    }
    builder.push(SET_FILE_SUM);
    builder.push(&encode_u35(clear_file_sum)?);
    builder.push(END_OF_FILE);

    let clear_bytes = builder.bytes;
    let rd_file_bytes = RuidaCodec::new(config.magic).swizzle(&clear_bytes);
    let layers = operations
        .iter()
        .enumerate()
        .map(|(index, operation)| RuidaLayerSummary {
            part: index as u8,
            layer_id: operation.layer_id.clone(),
            cut_entry_id: operation.cut_entry_id.clone(),
            path_count: operation.paths.len() + operation.perforation_paths.len(),
            point_count: operation.paths.iter().map(Vec::len).sum::<usize>()
                + operation
                    .perforation_paths
                    .iter()
                    .map(Vec::len)
                    .sum::<usize>(),
            raster_scanline_count: operation.raster_lines.len(),
            raster_motion_count: operation
                .raster_lines
                .iter()
                .map(|line| line.motions.len())
                .sum(),
            air_assist: operation.air_assist,
        })
        .collect();
    Ok(RuidaCompiledJob {
        clear_bytes,
        rd_file_bytes,
        clear_file_sum,
        bounds_micrometres: [
            job_bounds.min_x,
            job_bounds.min_y,
            job_bounds.max_x,
            job_bounds.max_y,
        ],
        layers,
    })
}

fn validate_transform(transform: &RuidaCoordinateTransform) -> Result<(), RuidaCompilationError> {
    for (field, value) in [
        ("X offset", transform.offset_x_mm),
        ("Y offset", transform.offset_y_mm),
        ("bed width", transform.bed_width_mm),
        ("bed height", transform.bed_height_mm),
    ] {
        if !value.is_finite() {
            return Err(RuidaCompilationError::InvalidNumericValue { field });
        }
    }
    if transform.flip_x && transform.bed_width_mm <= 0.0 {
        return Err(RuidaCompilationError::MissingFlipDimension { axis: "X" });
    }
    if transform.flip_y && transform.bed_height_mm <= 0.0 {
        return Err(RuidaCompilationError::MissingFlipDimension { axis: "Y" });
    }
    Ok(())
}

fn transform_points(
    points: &[Point2D],
    transform: &RuidaCoordinateTransform,
) -> Result<Vec<NativePoint>, RuidaCompilationError> {
    points
        .iter()
        .map(|point| transform_point(*point, transform))
        .collect()
}

fn transform_point(
    point: Point2D,
    transform: &RuidaCoordinateTransform,
) -> Result<NativePoint, RuidaCompilationError> {
    if !point.x.is_finite() || !point.y.is_finite() {
        return Err(RuidaCompilationError::InvalidNumericValue {
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
    // Fail closed on any commanded position outside a known bed — this
    // includes raster overscan travel, which extends past the burn bounds
    // that preflight checks against the bed.
    if (transform.bed_width_mm > 0.0 && (x < 0.0 || x > transform.bed_width_mm))
        || (transform.bed_height_mm > 0.0 && (y < 0.0 || y > transform.bed_height_mm))
    {
        return Err(RuidaCompilationError::OutsideBed);
    }
    Ok(NativePoint {
        x: millimetres_to_micrometres(x)?,
        y: millimetres_to_micrometres(y)?,
    })
}

fn millimetres_to_micrometres(value: f64) -> Result<i32, RuidaCompilationError> {
    let value = value * 1_000.0;
    if !value.is_finite() || value < f64::from(i32::MIN) || value > f64::from(i32::MAX) {
        return Err(RuidaCompilationError::CoordinateOutOfRange);
    }
    Ok(value.round() as i32)
}

#[allow(clippy::too_many_arguments)]
fn add_operation(
    operations: &mut Vec<NativeOperation>,
    layer_id: String,
    cut_entry_id: String,
    points: Vec<NativePoint>,
    speed_mm_min: f64,
    min_power_percent: f64,
    max_power_percent: f64,
    air_assist: bool,
) -> Result<(), RuidaCompilationError> {
    validate_operation_settings(speed_mm_min, min_power_percent, max_power_percent)?;
    let path_bounds = NativeBounds::from_points(&points);
    let burn = max_power_percent > 0.0;
    if let Some(operation) = operations.last_mut().filter(|operation| {
        operation.layer_id == layer_id
            && operation.cut_entry_id == cut_entry_id
            && operation.speed_mm_min.to_bits() == speed_mm_min.to_bits()
            && operation.min_power_percent.to_bits() == min_power_percent.to_bits()
            && operation.max_power_percent.to_bits() == max_power_percent.to_bits()
            && operation.air_assist == air_assist
            && operation.burn == burn
            && operation.raster_axis.is_none()
    }) {
        operation.bounds = operation.bounds.include(path_bounds);
        operation.paths.push(points);
    } else {
        operations.push(NativeOperation {
            layer_id,
            cut_entry_id,
            bounds: path_bounds,
            burn,
            paths: vec![points],
            perforation_paths: Vec::new(),
            raster_lines: Vec::new(),
            raster_axis: None,
            speed_mm_min,
            min_power_percent,
            max_power_percent,
            air_assist,
        });
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn add_perforation_operation(
    operations: &mut Vec<NativeOperation>,
    layer_id: String,
    cut_entry_id: String,
    motions: Vec<NativePathMotion>,
    speed_mm_min: f64,
    min_power_percent: f64,
    max_power_percent: f64,
    air_assist: bool,
) -> Result<(), RuidaCompilationError> {
    validate_operation_settings(speed_mm_min, min_power_percent, max_power_percent)?;
    let path_bounds =
        NativeBounds::from_path_motions(&motions).ok_or(RuidaCompilationError::EmptyJob)?;
    let burn = max_power_percent > 0.0;
    if let Some(operation) = operations.last_mut().filter(|operation| {
        operation.layer_id == layer_id
            && operation.cut_entry_id == cut_entry_id
            && operation.speed_mm_min.to_bits() == speed_mm_min.to_bits()
            && operation.min_power_percent.to_bits() == min_power_percent.to_bits()
            && operation.max_power_percent.to_bits() == max_power_percent.to_bits()
            && operation.air_assist == air_assist
            && operation.burn == burn
            && operation.raster_axis.is_none()
    }) {
        operation.bounds = operation.bounds.include(path_bounds);
        operation.perforation_paths.push(motions);
    } else {
        operations.push(NativeOperation {
            layer_id,
            cut_entry_id,
            paths: Vec::new(),
            perforation_paths: vec![motions],
            raster_lines: Vec::new(),
            raster_axis: None,
            speed_mm_min,
            min_power_percent,
            max_power_percent,
            air_assist,
            burn,
            bounds: path_bounds,
        });
    }
    Ok(())
}

fn compile_perforation_path(
    points: &[Point2D],
    speed_mm_min: f64,
    on_ms: f64,
    off_ms: f64,
    transform: &RuidaCoordinateTransform,
) -> Result<Vec<NativePathMotion>, RuidaCompilationError> {
    if !speed_mm_min.is_finite() || speed_mm_min <= 0.0 {
        return Err(RuidaCompilationError::InvalidNumericValue { field: "speed" });
    }
    for (field, value) in [
        ("perforation on time", on_ms),
        ("perforation off time", off_ms),
    ] {
        if !value.is_finite() || value <= 0.0 {
            return Err(RuidaCompilationError::InvalidNumericValue { field });
        }
    }
    let on_distance = speed_mm_min / 60_000.0 * on_ms;
    let off_distance = speed_mm_min / 60_000.0 * off_ms;
    if on_distance < MIN_NATIVE_PERFORATION_DISTANCE_MM
        || off_distance < MIN_NATIVE_PERFORATION_DISTANCE_MM
    {
        return Err(RuidaCompilationError::PerforationBelowNativeResolution);
    }

    let mut motions = vec![NativePathMotion::Move(transform_point(
        points[0], transform,
    )?)];
    let mut in_on_phase = true;
    let mut remaining = on_distance;
    for pair in points.windows(2) {
        let start = pair[0];
        let end = pair[1];
        if !start.x.is_finite() || !start.y.is_finite() || !end.x.is_finite() || !end.y.is_finite()
        {
            return Err(RuidaCompilationError::InvalidNumericValue {
                field: "coordinate",
            });
        }
        let dx = end.x - start.x;
        let dy = end.y - start.y;
        let segment_length = dx.hypot(dy);
        if segment_length < 1e-9 {
            continue;
        }
        let ux = dx / segment_length;
        let uy = dy / segment_length;
        let mut travelled = 0.0;
        while travelled < segment_length - 1e-9 {
            let advance = (segment_length - travelled).min(remaining);
            travelled += advance;
            remaining -= advance;
            let native = transform_point(
                Point2D::new(start.x + ux * travelled, start.y + uy * travelled),
                transform,
            )?;
            motions.push(if in_on_phase {
                NativePathMotion::Cut(native)
            } else {
                NativePathMotion::Move(native)
            });
            if motions.len() > MAX_PERFORATION_MOTIONS {
                return Err(RuidaCompilationError::PerforationMotionLimit);
            }
            if remaining < 1e-9 {
                in_on_phase = !in_on_phase;
                remaining = if in_on_phase {
                    on_distance
                } else {
                    off_distance
                };
            }
        }
    }
    Ok(motions)
}

#[allow(clippy::too_many_arguments)]
fn add_raster_operation(
    operations: &mut Vec<NativeOperation>,
    layer_id: String,
    cut_entry_id: String,
    raster_lines: Vec<NativeRasterLine>,
    raster_axis: ScanAxis,
    speed_mm_min: f64,
    min_power_percent: f64,
    max_power_percent: f64,
    air_assist: bool,
) -> Result<(), RuidaCompilationError> {
    validate_operation_settings(speed_mm_min, min_power_percent, max_power_percent)?;
    let raster_bounds =
        NativeBounds::from_raster_lines(&raster_lines).ok_or(RuidaCompilationError::EmptyJob)?;
    let burn = max_power_percent > 0.0;
    if let Some(operation) = operations.last_mut().filter(|operation| {
        operation.layer_id == layer_id
            && operation.cut_entry_id == cut_entry_id
            && operation.speed_mm_min.to_bits() == speed_mm_min.to_bits()
            && operation.min_power_percent.to_bits() == min_power_percent.to_bits()
            && operation.max_power_percent.to_bits() == max_power_percent.to_bits()
            && operation.air_assist == air_assist
            && operation.burn == burn
            && operation.raster_axis == Some(raster_axis)
    }) {
        operation.bounds = operation.bounds.include(raster_bounds);
        operation.raster_lines.extend(raster_lines);
    } else {
        operations.push(NativeOperation {
            layer_id,
            cut_entry_id,
            paths: Vec::new(),
            perforation_paths: Vec::new(),
            raster_lines,
            raster_axis: Some(raster_axis),
            speed_mm_min,
            min_power_percent,
            max_power_percent,
            air_assist,
            burn,
            bounds: raster_bounds,
        });
    }
    Ok(())
}

fn validate_operation_settings(
    speed_mm_min: f64,
    min_power_percent: f64,
    max_power_percent: f64,
) -> Result<(), RuidaCompilationError> {
    if !speed_mm_min.is_finite() || speed_mm_min <= 0.0 {
        return Err(RuidaCompilationError::InvalidNumericValue { field: "speed" });
    }
    for (field, power) in [
        ("minimum power", min_power_percent),
        ("maximum power", max_power_percent),
    ] {
        if !power.is_finite() || !(0.0..=100.0).contains(&power) {
            return Err(RuidaCompilationError::InvalidNumericValue { field });
        }
    }
    if min_power_percent > max_power_percent {
        return Err(RuidaCompilationError::InvalidNumericValue {
            field: "minimum power above maximum power",
        });
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn compile_raster_lines(
    scanlines: &[Scanline],
    line_interval_mm: f64,
    power_mode: PowerMode,
    scan_angle_deg: f64,
    scan_origin: Point2D,
    scan_axis: ScanAxis,
    overscan_mm: f64,
    ramp_length_mm: f64,
    min_power_percent: f64,
    max_power_percent: f64,
    transform: &RuidaCoordinateTransform,
) -> Result<Vec<NativeRasterLine>, RuidaCompilationError> {
    validate_operation_settings(1.0, min_power_percent, max_power_percent)?;
    for (field, value) in [
        ("raster line interval", line_interval_mm),
        ("raster scan angle", scan_angle_deg),
        ("raster scan origin X", scan_origin.x),
        ("raster scan origin Y", scan_origin.y),
        ("raster overscan", overscan_mm),
        ("raster ramp length", ramp_length_mm),
    ] {
        if !value.is_finite() {
            return Err(RuidaCompilationError::InvalidNumericValue { field });
        }
    }
    if line_interval_mm <= 0.0 {
        return Err(RuidaCompilationError::InvalidNumericValue {
            field: "raster line interval",
        });
    }
    if overscan_mm < 0.0 {
        return Err(RuidaCompilationError::InvalidNumericValue {
            field: "raster overscan",
        });
    }
    if ramp_length_mm < 0.0 {
        return Err(RuidaCompilationError::InvalidNumericValue {
            field: "raster ramp length",
        });
    }

    let orthogonal = is_orthogonal_scan_angle(scan_angle_deg);
    let (cos_a, sin_a) = if orthogonal {
        (1.0, 0.0)
    } else {
        let radians = scan_angle_deg.to_radians();
        (radians.cos(), radians.sin())
    };
    let to_native = |run_pos: f64, cross_pos: f64| {
        let point = if orthogonal {
            match scan_axis {
                ScanAxis::Horizontal => Point2D::new(run_pos, cross_pos),
                ScanAxis::Vertical => Point2D::new(cross_pos, run_pos),
            }
        } else {
            Point2D::new(
                scan_origin.x + run_pos * cos_a - cross_pos * sin_a,
                scan_origin.y + run_pos * sin_a + cross_pos * cos_a,
            )
        };
        transform_point(point, transform)
    };

    let mut compiled = Vec::new();
    for scanline in scanlines {
        if !scanline.y_mm.is_finite() {
            return Err(RuidaCompilationError::InvalidNumericValue {
                field: "raster scanline coordinate",
            });
        }
        let is_ltr = matches!(scanline.direction, ScanDirection::LeftToRight);
        let direction = if is_ltr { 1.0 } else { -1.0 };
        let mut runs: Vec<_> = scanline.runs.iter().collect();
        for run in &runs {
            if !run.start_x_mm.is_finite() || !run.end_x_mm.is_finite() {
                return Err(RuidaCompilationError::InvalidNumericValue {
                    field: "raster run coordinate",
                });
            }
        }
        runs.retain(|run| (run.start_x_mm - run.end_x_mm).abs() > 1e-9);
        if runs.is_empty() {
            continue;
        }
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
            let min = run.start_x_mm.min(run.end_x_mm);
            let max = run.start_x_mm.max(run.end_x_mm);
            if is_ltr { (min, max) } else { (max, min) }
        };
        let (row_start, _) = directed_bounds(runs[0]);
        let (_, row_end) = directed_bounds(runs[runs.len() - 1]);
        let overscan_start = row_start - direction * overscan_mm;
        let overscan_end = row_end + direction * overscan_mm;
        let mut motions = Vec::new();
        if overscan_mm > 0.0 {
            motions.push(NativeRasterMotion::Move(to_native(
                overscan_start,
                scanline.y_mm,
            )?));
        }
        motions.push(NativeRasterMotion::Move(to_native(
            row_start,
            scanline.y_mm,
        )?));

        let mut current_pos = row_start;
        for run in runs {
            let (run_start, run_end) = directed_bounds(run);
            if (run_start - current_pos).abs() > 1e-9 {
                motions.push(NativeRasterMotion::Move(to_native(
                    run_start,
                    scanline.y_mm,
                )?));
            }
            match power_mode {
                PowerMode::Binary => {
                    for segment in beambench_planner::ramp::expand_threshold_run(
                        run_start,
                        run_end,
                        ramp_length_mm,
                    ) {
                        let (min_power, max_power) = if ramp_length_mm > 0.0 {
                            let power = interpolate_power_percent(
                                segment.power_fraction,
                                min_power_percent,
                                max_power_percent,
                            );
                            (power, power)
                        } else {
                            (min_power_percent, max_power_percent)
                        };
                        push_raster_cut(
                            &mut motions,
                            to_native(segment.end_x_mm, scanline.y_mm)?,
                            min_power,
                            max_power,
                        );
                    }
                }
                PowerMode::Grayscale if run.power_values.is_empty() => {
                    push_raster_cut(
                        &mut motions,
                        to_native(run_end, scanline.y_mm)?,
                        max_power_percent,
                        max_power_percent,
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
                            let power = interpolate_power_percent(
                                f64::from(group_power) / 255.0,
                                min_power_percent,
                                max_power_percent,
                            );
                            push_raster_cut(
                                &mut motions,
                                to_native(end_pos, scanline.y_mm)?,
                                power,
                                power,
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
            motions.push(NativeRasterMotion::Move(to_native(
                overscan_end,
                scanline.y_mm,
            )?));
        }
        compiled.push(NativeRasterLine { motions });
    }
    Ok(compiled)
}

fn is_orthogonal_scan_angle(angle_deg: f64) -> bool {
    let angle = angle_deg.abs();
    angle < 0.5
        || (angle - 90.0).abs() < 0.5
        || (angle - 180.0).abs() < 0.5
        || (angle - 270.0).abs() < 0.5
        || (angle - 360.0).abs() < 0.5
}

fn interpolate_power_percent(fraction: f64, min_power: f64, max_power: f64) -> f64 {
    if min_power >= max_power {
        max_power
    } else {
        min_power + fraction.clamp(0.0, 1.0) * (max_power - min_power)
    }
}

fn push_raster_cut(
    motions: &mut Vec<NativeRasterMotion>,
    end: NativePoint,
    min_power_percent: f64,
    max_power_percent: f64,
) {
    if max_power_percent > 0.0 {
        motions.push(NativeRasterMotion::Cut {
            end,
            min_power_percent,
            max_power_percent,
        });
    } else {
        motions.push(NativeRasterMotion::Move(end));
    }
}

#[derive(Default)]
struct JobBuilder {
    bytes: Vec<u8>,
}

impl JobBuilder {
    fn push(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
    }

    fn push_point(&mut self, point: NativePoint) {
        self.push(&encode_i32(point.x));
        self.push(&encode_i32(point.y));
    }
}

fn write_header(
    builder: &mut JobBuilder,
    operations: &[NativeOperation],
    bounds: NativeBounds,
    reference_point: RuidaReferencePoint,
) -> Result<(), RuidaCompilationError> {
    builder.push(match reference_point {
        RuidaReferencePoint::MachineZero => REF_POINT_MACHINE_ZERO,
        RuidaReferencePoint::UserOrigin => REF_POINT_USER_ORIGIN,
        RuidaReferencePoint::CurrentPosition => REF_POINT_CURRENT_POSITION,
    });
    builder.push(SET_ABSOLUTE);
    builder.push(REF_POINT_SET);
    builder.push(ENABLE_BLOCK_CUTTING);
    builder.push(&[0]);
    builder.push(START_PROCESS);
    builder.push(FEED_REPEAT);
    builder.push(&encode_i32(0));
    builder.push(&encode_i32(0));
    builder.push(SET_FEED_AUTO_PAUSE);
    builder.push(&[0]);

    let top_left = NativePoint {
        x: bounds.max_x,
        y: bounds.min_y,
    };
    let bottom_right = NativePoint {
        x: bounds.min_x,
        y: bounds.max_y,
    };
    for (command, point) in [
        (PROCESS_TOP_LEFT, top_left),
        (PROCESS_BOTTOM_RIGHT, bottom_right),
        (DOCUMENT_MIN_POINT, top_left),
        (DOCUMENT_MAX_POINT, bottom_right),
    ] {
        builder.push(command);
        builder.push_point(point);
    }
    builder.push(PROCESS_REPEAT);
    for value in [1, 1, 0, 0, 0, 0, 0] {
        builder.push(&encode_u14(value)?);
    }
    builder.push(ARRAY_DIRECTION);
    builder.push(&[0]);

    for (index, operation) in operations.iter().enumerate() {
        let part = index as u8;
        let min_point = NativePoint {
            x: operation.bounds.min_x,
            y: operation.bounds.min_y,
        };
        let max_point = NativePoint {
            x: operation.bounds.max_x,
            y: operation.bounds.max_y,
        };
        push_part_speed(builder, SPEED_LASER_1_PART, part, operation.speed_mm_min)?;
        push_part_power(builder, MIN_POWER_1_PART, part, operation.min_power_percent)?;
        push_part_power(builder, MAX_POWER_1_PART, part, operation.max_power_percent)?;
        push_part_power(builder, MIN_POWER_2_PART, part, operation.min_power_percent)?;
        push_part_power(builder, MAX_POWER_2_PART, part, operation.max_power_percent)?;
        builder.push(LAYER_COLOR_PART);
        builder.push(&[part]);
        builder.push(&encode_i32(layer_color(index)));
        builder.push(WORK_MODE_PART);
        builder.push(&[part, 0]);
        for (command, point) in [
            (PART_MIN_POINT, min_point),
            (PART_MAX_POINT, max_point),
            (PART_MIN_POINT_EX, min_point),
            (PART_MAX_POINT_EX, max_point),
        ] {
            builder.push(command);
            builder.push(&[part]);
            builder.push_point(point);
        }
    }
    builder.push(MAX_LAYER_PART);
    builder.push(&[(operations.len() - 1) as u8]);
    for command in [PEN_OFFSET, LAYER_OFFSET] {
        for axis in [0, 1] {
            builder.push(command);
            builder.push(&[axis]);
            builder.push(&encode_i32(0));
        }
    }
    builder.push(DISPLAY_OFFSET);
    builder.push_point(NativePoint { x: 0, y: 0 });
    builder.push(FEED_INFO);
    builder.push(&encode_i32(0));
    builder.push(ARRAY_START);
    builder.push(&[0]);
    builder.push(SET_CURRENT_ELEMENT_INDEX);
    builder.push(&[0]);
    builder.push(ARRAY_ENABLE_MIRROR_CUT);
    builder.push(&[0]);
    builder.push(ARRAY_MIN_POINT);
    builder.push_point(NativePoint {
        x: bounds.min_x,
        y: bounds.min_y,
    });
    builder.push(ARRAY_MAX_POINT);
    builder.push_point(NativePoint {
        x: bounds.max_x,
        y: bounds.max_y,
    });
    builder.push(ARRAY_ADD);
    builder.push_point(NativePoint { x: 0, y: 0 });
    builder.push(ARRAY_MIRROR);
    builder.push(&[0]);
    builder.push(ARRAY_REPEAT);
    for value in [1_i16, 1, 0, 1_123, -3_328, 4, 3_480] {
        builder.push(&encode_i14(value)?);
    }
    Ok(())
}

fn write_operation(
    builder: &mut JobBuilder,
    part: u8,
    operation: &NativeOperation,
) -> Result<(), RuidaCompilationError> {
    if let Some(axis) = operation.raster_axis {
        builder.push(match axis {
            ScanAxis::Horizontal => WORK_MODE_HORIZONTAL_RASTER,
            ScanAxis::Vertical => WORK_MODE_VERTICAL_RASTER,
        });
    }
    builder.push(LAYER_NUMBER_PART);
    builder.push(&[part]);
    builder.push(LASER_DEVICE_0);
    builder.push(if operation.air_assist {
        AIR_ASSIST_ON
    } else {
        AIR_ASSIST_OFF
    });
    builder.push(SPEED_LASER_1);
    builder.push(&encode_speed_mm_s(operation.speed_mm_min / 60.0)?);
    builder.push(LASER_ON_DELAY);
    builder.push(&encode_i32(0));
    builder.push(LASER_OFF_DELAY);
    builder.push(&encode_i32(0));
    push_power(builder, MIN_POWER_1, operation.min_power_percent)?;
    push_power(builder, MAX_POWER_1, operation.max_power_percent)?;
    push_power(builder, MIN_POWER_2, operation.min_power_percent)?;
    push_power(builder, MAX_POWER_2, operation.max_power_percent)?;
    builder.push(ENABLE_LASER_TUBE_START);
    builder.push(&[1]);

    for path in &operation.paths {
        builder.push(MOVE_ABS_XY);
        builder.push_point(path[0]);
        for point in path.iter().skip(1) {
            builder.push(if operation.burn {
                CUT_ABS_XY
            } else {
                MOVE_ABS_XY
            });
            builder.push_point(*point);
        }
    }
    for path in &operation.perforation_paths {
        for motion in path {
            let (command, point) = match *motion {
                NativePathMotion::Move(point) => (MOVE_ABS_XY, point),
                NativePathMotion::Cut(point) if operation.burn => (CUT_ABS_XY, point),
                NativePathMotion::Cut(point) => (MOVE_ABS_XY, point),
            };
            builder.push(command);
            builder.push_point(point);
        }
    }
    let mut active_raster_power = (
        operation.min_power_percent.to_bits(),
        operation.max_power_percent.to_bits(),
    );
    for line in &operation.raster_lines {
        for motion in &line.motions {
            match *motion {
                NativeRasterMotion::Move(point) => {
                    builder.push(MOVE_ABS_XY);
                    builder.push_point(point);
                }
                NativeRasterMotion::Cut {
                    end,
                    min_power_percent,
                    max_power_percent,
                } => {
                    let requested = (min_power_percent.to_bits(), max_power_percent.to_bits());
                    if requested != active_raster_power {
                        // MeerK40t's observed Ruida raster path changes channel-one
                        // max then min power immediately before the affected cut.
                        push_power(builder, MAX_POWER_1, max_power_percent)?;
                        push_power(builder, MIN_POWER_1, min_power_percent)?;
                        active_raster_power = requested;
                    }
                    builder.push(CUT_ABS_XY);
                    builder.push_point(end);
                }
            }
        }
    }
    Ok(())
}

fn write_layer_end(builder: &mut JobBuilder) {
    builder.push(BLOCK_END);
    builder.push(LAYER_END);
    builder.push(DISABLE_LASER_2_OFFSET);
}

fn push_part_speed(
    builder: &mut JobBuilder,
    command: &[u8],
    part: u8,
    speed_mm_min: f64,
) -> Result<(), RuidaProtocolError> {
    builder.push(command);
    builder.push(&[part]);
    builder.push(&encode_speed_mm_s(speed_mm_min / 60.0)?);
    Ok(())
}

fn push_part_power(
    builder: &mut JobBuilder,
    command: &[u8],
    part: u8,
    power_percent: f64,
) -> Result<(), RuidaProtocolError> {
    builder.push(command);
    builder.push(&[part]);
    builder.push(&encode_power_percent(power_percent)?);
    Ok(())
}

fn push_power(
    builder: &mut JobBuilder,
    command: &[u8],
    power_percent: f64,
) -> Result<(), RuidaProtocolError> {
    builder.push(command);
    builder.push(&encode_power_percent(power_percent)?);
    Ok(())
}

fn layer_color(index: usize) -> i32 {
    const COLORS: [i32; 8] = [
        0x0000FF, 0xFF0000, 0x00AA00, 0x00A5FF, 0x800080, 0x00FFFF, 0x808000, 0x404040,
    ];
    COLORS[index % COLORS.len()]
}

#[cfg(test)]
mod tests {
    use beambench_common::{Bounds, Point2D};
    use beambench_planner::{
        DirectionMode, PlanSegment, PowerMode, ScanAxis, ScanDirection, ScanRun, Scanline,
    };

    use super::*;

    fn plan(segments: Vec<PlanSegment>) -> ExecutionPlan {
        ExecutionPlan {
            id: Default::default(),
            project_id: Default::default(),
            revision_hash: "ruida-vector-test".to_string(),
            created_at: Default::default(),
            bounds: Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
            total_distance_mm: 0.0,
            estimated_duration_secs: 0.0,
            segments,
            layer_order: vec!["layer-1".to_string()],
            warnings: Vec::new(),
            failed_entries: Vec::new(),
        }
    }

    fn square() -> PlanSegment {
        PlanSegment::Vector {
            polyline: vec![
                Point2D::new(1.0, 2.0),
                Point2D::new(11.0, 2.0),
                Point2D::new(11.0, 12.0),
                Point2D::new(1.0, 12.0),
            ],
            closed: true,
            power_percent: 40.0,
            speed_mm_min: 1_200.0,
            layer_id: "layer-1".to_string(),
            cut_entry_id: "cut-1".to_string(),
            perforation_enabled: false,
            perforation_on_ms: 0.0,
            perforation_off_ms: 0.0,
            source_object_id: None,
            source_subpath_index: None,
        }
    }

    fn raster_segment(
        direction: ScanDirection,
        power_mode: PowerMode,
        power_values: Vec<u8>,
    ) -> PlanSegment {
        PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 2.0,
                runs: vec![ScanRun {
                    start_x_mm: 1.0,
                    end_x_mm: 3.0,
                    power_values,
                }],
                direction,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode,
            speed_mm_min: 3_000.0,
            layer_id: "layer-1".to_string(),
            cut_entry_id: "cut-1".to_string(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 0.5,
            outlines: Vec::new(),
            scan_axis: ScanAxis::Horizontal,
            power_max_percent: 60.0,
            power_min_percent: 20.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 1.0,
        }
    }

    #[test]
    fn geometry_outside_a_known_bed_fails_closed() {
        let config = RuidaCompilationConfig {
            coordinate_transform: RuidaCoordinateTransform {
                bed_width_mm: 10.0,
                bed_height_mm: 10.0,
                ..RuidaCoordinateTransform::default()
            },
            ..RuidaCompilationConfig::default()
        };
        // square() reaches (11, 12) which is outside a 10x10 bed.
        let error = compile_ruida_job(&plan(vec![square()]), &config).unwrap_err();
        assert!(matches!(error, RuidaCompilationError::OutsideBed));
    }

    #[test]
    fn raster_overscan_travel_beyond_the_bed_fails_closed() {
        let config = RuidaCompilationConfig {
            coordinate_transform: RuidaCoordinateTransform {
                bed_width_mm: 3.2,
                bed_height_mm: 100.0,
                ..RuidaCoordinateTransform::default()
            },
            ..RuidaCompilationConfig::default()
        };
        // The burn run ends at x=3.0 (inside the 3.2mm bed) but the 0.5mm
        // overscan travel reaches x=3.5, which would drive the head past the
        // rails without a compiler-level check.
        let error = compile_ruida_job(
            &plan(vec![raster_segment(
                ScanDirection::LeftToRight,
                PowerMode::Binary,
                Vec::new(),
            )]),
            &config,
        )
        .unwrap_err();
        assert!(matches!(error, RuidaCompilationError::OutsideBed));
    }

    #[test]
    fn vector_job_is_deterministic_and_closes_the_path() {
        let job =
            compile_ruida_job(&plan(vec![square()]), &RuidaCompilationConfig::default()).unwrap();
        assert_eq!(job.bounds_micrometres, [1_000, 2_000, 11_000, 12_000]);
        assert_eq!(job.layers.len(), 1);
        assert_eq!(job.layers[0].path_count, 1);
        assert_eq!(job.layers[0].point_count, 5);
        assert_eq!(job.clear_bytes.last(), Some(&0xD7));
        assert_eq!(
            job.rd_file_bytes,
            RuidaCodec::default().swizzle(&job.clear_bytes)
        );
        let mut offline_file = Vec::new();
        job.write_rd(&mut offline_file).unwrap();
        assert_eq!(offline_file, job.rd_file_bytes);
        assert_eq!(
            job,
            compile_ruida_job(&plan(vec![square()]), &RuidaCompilationConfig::default()).unwrap()
        );
    }

    #[test]
    fn storage_sentinel_is_small_deterministic_and_zero_power() {
        let job = compile_ruida_storage_sentinel(&RuidaCompilationConfig::default()).unwrap();
        assert_eq!(job.bounds_micrometres, [1_000, 1_000, 2_000, 2_000]);
        assert_eq!(job.layers.len(), 1);
        assert_eq!(job.layers[0].layer_id, "__ruida_storage_sentinel__");
        assert_eq!(job.layers[0].point_count, 5);
        assert!(!job.layers[0].air_assist);
        for command in [
            [MIN_POWER_1_PART[0], MIN_POWER_1_PART[1], 0, 0, 0],
            [MAX_POWER_1_PART[0], MAX_POWER_1_PART[1], 0, 0, 0],
            [MIN_POWER_2_PART[0], MIN_POWER_2_PART[1], 0, 0, 0],
            [MAX_POWER_2_PART[0], MAX_POWER_2_PART[1], 0, 0, 0],
        ] {
            assert!(
                job.clear_bytes
                    .windows(command.len())
                    .any(|bytes| bytes == command)
            );
        }
        for command in [MIN_POWER_1, MAX_POWER_1, MIN_POWER_2, MAX_POWER_2] {
            let expected = [command[0], command[1], 0, 0];
            assert!(
                job.clear_bytes
                    .windows(expected.len())
                    .any(|bytes| bytes == expected)
            );
        }
        assert_eq!(
            job,
            compile_ruida_storage_sentinel(&RuidaCompilationConfig::default()).unwrap()
        );
    }

    #[test]
    fn paths_with_the_same_cut_settings_share_one_controller_part() {
        let mut second = square();
        if let PlanSegment::Vector { polyline, .. } = &mut second {
            for point in polyline {
                point.x += 20.0;
            }
        }
        let job = compile_ruida_job(
            &plan(vec![square(), second]),
            &RuidaCompilationConfig::default(),
        )
        .unwrap();
        assert_eq!(job.layers.len(), 1);
        assert_eq!(job.layers[0].path_count, 2);
        assert_eq!(job.layers[0].point_count, 10);
        assert_eq!(job.bounds_micrometres, [1_000, 2_000, 31_000, 12_000]);
    }

    #[test]
    fn coordinate_transform_and_air_assist_are_explicit() {
        let config = RuidaCompilationConfig {
            coordinate_transform: RuidaCoordinateTransform {
                offset_x_mm: 1.0,
                offset_y_mm: -2.0,
                flip_x: true,
                flip_y: false,
                bed_width_mm: 100.0,
                bed_height_mm: 0.0,
            },
            air_assist_cut_entry_ids: vec!["cut-1".to_string()],
            min_power_overrides: vec![RuidaPowerOverride {
                cut_entry_id: "cut-1".to_string(),
                min_power_percent: 20.0,
            }],
            ..RuidaCompilationConfig::default()
        };
        let job = compile_ruida_job(&plan(vec![square()]), &config).unwrap();
        assert_eq!(job.bounds_micrometres, [90_000, 0, 100_000, 10_000]);
        assert!(job.layers[0].air_assist);
        assert!(
            job.clear_bytes
                .windows(AIR_ASSIST_ON.len())
                .any(|window| window == AIR_ASSIST_ON)
        );
    }

    #[test]
    fn binary_raster_emits_native_scan_mode_overscan_and_cut_motion() {
        let job = compile_ruida_job(
            &plan(vec![raster_segment(
                ScanDirection::LeftToRight,
                PowerMode::Binary,
                Vec::new(),
            )]),
            &RuidaCompilationConfig::default(),
        )
        .unwrap();
        assert_eq!(job.bounds_micrometres, [500, 2_000, 3_500, 2_000]);
        assert_eq!(job.layers[0].path_count, 0);
        assert_eq!(job.layers[0].raster_scanline_count, 1);
        assert_eq!(job.layers[0].raster_motion_count, 4);
        assert!(
            job.clear_bytes
                .windows(WORK_MODE_HORIZONTAL_RASTER.len())
                .any(|window| window == WORK_MODE_HORIZONTAL_RASTER)
        );
        assert!(
            job.clear_bytes
                .windows(CUT_ABS_XY.len())
                .any(|window| window == CUT_ABS_XY)
        );
    }

    #[test]
    fn grayscale_raster_reverses_pixels_and_rotates_native_coordinates() {
        let mut raster = raster_segment(
            ScanDirection::RightToLeft,
            PowerMode::Grayscale,
            vec![64, 128],
        );
        if let PlanSegment::Raster {
            scanlines,
            scan_angle_deg,
            scan_origin,
            overscan_mm,
            power_min_percent,
            power_max_percent,
            ..
        } = &mut raster
        {
            scanlines[0].y_mm = 0.0;
            scanlines[0].runs[0].start_x_mm = 0.0;
            scanlines[0].runs[0].end_x_mm = 2.0;
            *scan_angle_deg = 45.0;
            *scan_origin = Point2D::new(10.0, 20.0);
            *overscan_mm = 0.0;
            *power_min_percent = 10.0;
            *power_max_percent = 90.0;
        }
        let job =
            compile_ruida_job(&plan(vec![raster]), &RuidaCompilationConfig::default()).unwrap();
        assert_eq!(job.bounds_micrometres, [10_000, 20_000, 11_414, 21_414]);
        assert_eq!(job.layers[0].raster_motion_count, 3);

        let first_power = interpolate_power_percent(128.0 / 255.0, 10.0, 90.0);
        let mut first_power_command = MAX_POWER_1.to_vec();
        first_power_command.extend(encode_power_percent(first_power).unwrap());
        assert!(
            job.clear_bytes
                .windows(first_power_command.len())
                .any(|window| window == first_power_command)
        );
    }

    #[test]
    fn vertical_raster_swaps_axes_and_uses_vertical_work_mode() {
        let mut raster = raster_segment(ScanDirection::LeftToRight, PowerMode::Binary, Vec::new());
        if let PlanSegment::Raster { scan_axis, .. } = &mut raster {
            *scan_axis = ScanAxis::Vertical;
        }
        let job =
            compile_ruida_job(&plan(vec![raster]), &RuidaCompilationConfig::default()).unwrap();
        assert_eq!(job.bounds_micrometres, [2_000, 500, 2_000, 3_500]);
        assert!(
            job.clear_bytes
                .windows(WORK_MODE_VERTICAL_RASTER.len())
                .any(|window| window == WORK_MODE_VERTICAL_RASTER)
        );
    }

    #[test]
    fn nonadjacent_matching_operations_preserve_plan_order() {
        let mut raster = raster_segment(ScanDirection::LeftToRight, PowerMode::Binary, Vec::new());
        if let PlanSegment::Raster {
            layer_id,
            cut_entry_id,
            ..
        } = &mut raster
        {
            *layer_id = "raster-layer".to_string();
            *cut_entry_id = "raster-cut".to_string();
        }
        let job = compile_ruida_job(
            &plan(vec![square(), raster, square()]),
            &RuidaCompilationConfig::default(),
        )
        .unwrap();
        assert_eq!(job.layers.len(), 3);
        assert_eq!(job.layers[0].cut_entry_id, "cut-1");
        assert_eq!(job.layers[1].cut_entry_id, "raster-cut");
        assert_eq!(job.layers[2].cut_entry_id, "cut-1");
    }

    #[test]
    fn perforation_expands_to_native_cut_and_move_phases_across_vertices() {
        let points = vec![
            Point2D::new(0.0, 0.0),
            Point2D::new(2.5, 0.0),
            Point2D::new(5.0, 0.0),
        ];
        let motions = compile_perforation_path(
            &points,
            6_000.0,
            20.0,
            10.0,
            &RuidaCoordinateTransform::default(),
        )
        .unwrap();
        let emitted: Vec<_> = motions
            .iter()
            .map(|motion| match motion {
                NativePathMotion::Move(point) => ("move", point.x),
                NativePathMotion::Cut(point) => ("cut", point.x),
            })
            .collect();
        assert_eq!(
            emitted,
            vec![
                ("move", 0),
                ("cut", 2_000),
                ("move", 2_500),
                ("move", 3_000),
                ("cut", 5_000),
            ]
        );

        let mut perforated = square();
        if let PlanSegment::Vector {
            polyline,
            closed,
            speed_mm_min,
            perforation_enabled,
            perforation_on_ms,
            perforation_off_ms,
            ..
        } = &mut perforated
        {
            *polyline = points;
            *closed = false;
            *speed_mm_min = 6_000.0;
            *perforation_enabled = true;
            *perforation_on_ms = 20.0;
            *perforation_off_ms = 10.0;
        }
        let job =
            compile_ruida_job(&plan(vec![perforated]), &RuidaCompilationConfig::default()).unwrap();
        assert_eq!(job.bounds_micrometres, [0, 0, 5_000, 0]);
        assert_eq!(job.layers[0].path_count, 1);
        assert_eq!(job.layers[0].point_count, 5);
    }

    #[test]
    fn unsupported_geometry_and_invalid_settings_fail_closed() {
        let empty_raster_plan = plan(vec![PlanSegment::Raster {
            scanlines: Vec::new(),
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Binary,
            speed_mm_min: 1_000.0,
            layer_id: "layer-1".to_string(),
            cut_entry_id: "cut-1".to_string(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 0.0,
            outlines: Vec::new(),
            scan_axis: ScanAxis::Horizontal,
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.1,
        }]);
        assert!(matches!(
            compile_ruida_job(&empty_raster_plan, &RuidaCompilationConfig::default()),
            Err(RuidaCompilationError::EmptyJob)
        ));
        let mut raster = raster_segment(ScanDirection::LeftToRight, PowerMode::Binary, Vec::new());
        if let PlanSegment::Raster {
            line_interval_mm, ..
        } = &mut raster
        {
            *line_interval_mm = 0.0;
        }
        assert!(matches!(
            compile_ruida_job(&plan(vec![raster]), &RuidaCompilationConfig::default()),
            Err(RuidaCompilationError::InvalidNumericValue {
                field: "raster line interval"
            })
        ));

        let raster_plan = plan(vec![square()]);
        let invalid = RuidaCompilationConfig {
            min_power_overrides: vec![RuidaPowerOverride {
                cut_entry_id: "cut-1".to_string(),
                min_power_percent: 80.0,
            }],
            ..RuidaCompilationConfig::default()
        };
        assert!(compile_ruida_job(&raster_plan, &invalid).is_err());

        assert!(matches!(
            compile_perforation_path(
                &[Point2D::new(0.0, 0.0), Point2D::new(1.0, 0.0)],
                60.0,
                0.5,
                0.5,
                &RuidaCoordinateTransform::default(),
            ),
            Err(RuidaCompilationError::PerforationBelowNativeResolution)
        ));
    }
}

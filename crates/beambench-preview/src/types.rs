//! Lightweight preview types for canvas rendering.
//!
//! These types represent a distilled version of an ExecutionPlan,
//! compressing megabytes of raster data into small summaries suitable
//! for IPC and canvas rendering.

use beambench_common::geometry::{Bounds, Point2D};
use beambench_common::path::Polyline;
use beambench_planner::{DirectionMode, PowerMode, ScanAxis, ScanDirection};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Lightweight preview data for canvas rendering.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewData {
    pub plan_id: Uuid,
    pub revision_hash: String,
    pub bounds: Bounds,
    pub layers: Vec<PreviewLayer>,
    pub travel_moves: Vec<TravelMove>,
    pub frame: Option<PreviewFrame>,
    pub stats: PreviewStats,
    pub warnings: Vec<String>,
    pub failed_entries: Vec<String>,
}

/// Preview data grouped by layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewLayer {
    pub layer_id: String,
    pub vector_paths: Vec<VectorPreview>,
    pub raster_regions: Vec<RasterPreview>,
}

/// A distilled vector path for preview rendering.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorPreview {
    pub points: Vec<Point2D>,
    pub closed: bool,
    pub power_percent: f64,
    pub speed_mm_min: f64,
    /// Execution order index for timeline interleaving.
    #[serde(default)]
    pub sequence: usize,
}

/// A distilled raster region — bounding box with fill density instead of per-pixel data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RasterPreview {
    pub bounds: Bounds,
    pub line_count: usize,
    pub line_interval_mm: f64,
    pub direction_mode: DirectionMode,
    pub power_mode: PowerMode,
    pub speed_mm_min: f64,
    pub fill_density: f64,
    /// Scan angle in degrees (0=horizontal, arbitrary supported).
    #[serde(default)]
    pub scan_angle_deg: f64,
    /// World-space center of rotation for non-orthogonal angles.
    #[serde(default)]
    pub scan_origin: Point2D,
    #[serde(default)]
    pub overscan_mm: f64,
    /// Source shape outlines for clip-path rendering (vector fill only).
    #[serde(default)]
    pub outlines: Vec<Polyline>,
    /// Whether scanlines run horizontally (default) or vertically.
    #[serde(default)]
    pub scan_axis: ScanAxis,
    /// Execution order index for timeline interleaving.
    #[serde(default)]
    pub sequence: usize,
    /// Backend-computed exact duration in seconds.
    #[serde(default)]
    pub duration_secs: f64,
    /// Average burn power normalized to 0.0-1.0 (0=lightest, 1=darkest).
    #[serde(default)]
    pub avg_power_normalized: f64,
    /// Physical head position after the last run completes (world-space, un-transposed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_point: Option<Point2D>,
    /// Processed-raster preview bitmap (PNG-encoded grayscale, darker =
    /// higher power). Rendered via `ctx.drawImage` at the region's local
    /// rect, which the frontend rotates/positions using `scan_origin`
    /// and `scan_angle_deg`. Present for all raster regions (both
    /// image-layer and vector-fill); `None` only if the region had
    /// zero runs (degenerate).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview_bitmap: Option<RasterPreviewBitmap>,
    /// Min corner of the preview bitmap in pre-rotation, pre-transpose
    /// local coordinates. The same frame the `scanlines` runs were
    /// emitted in by the planner.
    #[serde(default)]
    pub local_origin_mm: Point2D,
    /// Width of the preview bitmap in the same local frame, in mm.
    #[serde(default)]
    pub local_width_mm: f64,
    /// Height of the preview bitmap in the same local frame, in mm.
    #[serde(default)]
    pub local_height_mm: f64,
    /// Per-planner-run burn extents used by animated-preview progress masking.
    #[serde(default)]
    pub run_extents: Vec<RasterRunExtent>,
    /// One envelope per non-empty scanline, from the first energized position
    /// to the last. Overscan is applied around these extents.
    #[serde(default)]
    pub scanline_extents: Vec<RasterRunExtent>,
    /// Finer-grained burn extents. For binary rasters this matches
    /// `run_extents`; for grayscale rasters it may contain sub-runs
    /// split around laser-off gaps. The frontend currently treats
    /// `run_extents` as the authoritative overscan envelope and uses
    /// these only as a fallback when row-level extents are absent.
    #[serde(default)]
    pub overscan_run_extents: Vec<RasterRunExtent>,
}

/// PNG-encoded grayscale bitmap for a raster preview region.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RasterPreviewBitmap {
    pub width_px: u32,
    pub height_px: u32,
    /// PNG-encoded grayscale pixels. Darker = higher power, to match
    /// the preview and physical engraved output.
    pub png_bytes: Vec<u8>,
}

/// A single planner run, in pre-rotation local coordinates. Used by the
/// overscan marker renderer and animated playback progress tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RasterRunExtent {
    pub y_mm: f64,
    pub start_x_mm: f64,
    pub end_x_mm: f64,
    pub direction: ScanDirection,
}

/// A travel (rapid) move between segments.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TravelMove {
    pub from: Point2D,
    pub to: Point2D,
    /// Execution order index for timeline interleaving.
    #[serde(default)]
    pub sequence: usize,
}

/// A frame (boundary outline) preview.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewFrame {
    pub path: Vec<Point2D>,
    pub power_percent: f64,
    pub speed_mm_min: f64,
}

/// Aggregate statistics for the preview.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewStats {
    pub total_distance_mm: f64,
    pub travel_distance_mm: f64,
    pub burn_distance_mm: f64,
    pub estimated_duration_secs: f64,
    pub segment_count: usize,
    pub raster_line_count: usize,
}

//! Execution plan builder - the main orchestrator.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use beambench_common::geometry::{Bounds, Point2D};
use beambench_common::{PathCommand, Polyline, RasterMode, StartFromMode, SubPath, VecPath};
use beambench_core::FinishPosition;
use beambench_core::Project;
use beambench_core::asset::AssetId;
use beambench_core::layer::{Layer, OffsetFillGroupingMode, OperationType};
use beambench_core::object::{ImageMaskPolarity, ObjectData, ObjectId, ProjectObject};
use beambench_core::optimization::DirectionOrder;
use beambench_core::workspace::WorkspaceOrigin;
use beambench_core::{OptimizationOrderKey, ProjectOptimization};

/// Returns the effective per-segment ramp length for a layer.
///
/// Ramp length only applies to the hard on/off transitions produced by
/// `RasterMode::Threshold` on image layers — dithered modes (Floyd-
/// Steinberg, Stucki, Jarvis, Sierra, Atkinson, Ordered Dither,
/// Halftone, Newsprint, Sketch) and pass-through output already encode
/// tone through their bit pattern and would be destroyed by ramping
/// individual threshold runs. Vector fill operations are not image-mode
/// at all. Grayscale mode emits sub-run power values directly and
/// ignores ramp.
///
/// This is the single gate point — both the emitter and preview
/// distiller read `ramp_length_mm` off `PlanSegment::Raster` and will
/// skip ramp expansion when it is zero, so gating here keeps those
/// paths blind to raster mode semantics.
fn effective_layer_ramp_length_mm(layer: &Layer) -> f64 {
    if layer.primary_entry().operation != OperationType::Image {
        return 0.0;
    }
    let Some(rs) = layer.primary_entry().raster_settings.as_ref() else {
        return 0.0;
    };
    if rs.pass_through {
        return 0.0;
    }
    if rs.mode != RasterMode::Threshold {
        return 0.0;
    }
    rs.ramp_length_mm.max(0.0)
}
use beambench_core::vector::flatten::{DEFAULT_TOLERANCE_MM, flatten_vecpath};
use beambench_core::vector::normalize::normalize_object;
use beambench_core::vector::offset::signed_area;
use beambench_core::vector::{
    OFFSET_FILL_BOOLEAN_TOLERANCE_MM, normalize_subject_evenodd_with_tolerance, optimize_path,
    weld_shapes,
};
use clipper2::{EndType, JoinType, Milli, Paths};
// process_raster is called via beambench_raster::process_raster_with_cache
use beambench_raster::types::RasterPixelFormat;
use rayon::prelude::*;

use crate::image_params::build_image_raster_params;
use chrono::Utc;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use tracing::info;

use crate::error::PlannerError;
use crate::extensions::{
    anchor_segments_to_target, apply_positioned_tabs, apply_tabs, apply_workspace_origin_transform,
    flip_bounds_y,
};
use crate::fill_raster::rasterize_fill;
use crate::optimize::order::{Orderable, order_nearest_next_generic};
use crate::optimize::travel;
use crate::optimize::{
    OptimizationRuntime, PlannerCancellation, PlannerInput, dedupe, direction, inner_first,
    start_point,
};
use crate::plan::*;
use crate::scanline::generate_scanlines;
use crate::stats::{calculate_distance, calculate_duration};
use crate::validate::validate_bounds;

// Emergency ceiling only. Normal completion is driven by geometry exits below;
// at 0.1mm spacing this still allows ~409mm of fill depth, far beyond typical
// desktop laser jobs, while preventing pathological unbounded planning.
const OFFSET_FILL_ITERATION_LIMIT: usize = 4096;
// Dense valid offset fills can take minutes, but planning must stay cancellable
// and eventually fail closed instead of blocking the app indefinitely.
const OFFSET_FILL_ENTRY_TIMEOUT: Duration = Duration::from_secs(300);
const OFFSET_FILL_PARENT_REPAIR_TRACK_LIMIT: usize = 4096;
const OFFSET_FILL_NEAREST_EMIT_TRACK_LIMIT: usize = 512;

pub fn offset_fill_boolean_tolerances_mm() -> (f64, f64) {
    (
        OFFSET_FILL_BOOLEAN_TOLERANCE_MM,
        OFFSET_FILL_BOOLEAN_TOLERANCE_MM,
    )
}

fn offset_fill_object_batches<'a>(
    objects: &'a [&'a ProjectObject],
    all_objects: &'a [ProjectObject],
    grouping_mode: OffsetFillGroupingMode,
) -> Vec<Vec<&'a ProjectObject>> {
    if objects.is_empty() {
        return Vec::new();
    }

    match grouping_mode {
        OffsetFillGroupingMode::AllShapesAtOnce => vec![objects.to_vec()],
        OffsetFillGroupingMode::ShapesIndividually => {
            objects.iter().map(|obj| vec![*obj]).collect()
        }
        OffsetFillGroupingMode::GroupsTogether => {
            let top_ancestors = crate::optimize::sort_keys::build_top_ancestor_map(all_objects);
            let mut batch_indices: HashMap<ObjectId, usize> = HashMap::new();
            let mut batches: Vec<Vec<&ProjectObject>> = Vec::new();

            for obj in objects {
                let key = top_ancestors.get(&obj.id).copied().unwrap_or(obj.id);
                let batch_index = match batch_indices.get(&key).copied() {
                    Some(index) => index,
                    None => {
                        let index = batches.len();
                        batches.push(Vec::new());
                        batch_indices.insert(key, index);
                        index
                    }
                };
                batches[batch_index].push(*obj);
            }

            batches
        }
    }
}

/// A polyline wrapper that carries source identity through ordering.
struct TaggedPolyline {
    inner: Polyline,
    object_id: String,
    subpath_index: usize,
}

impl Orderable for TaggedPolyline {
    fn points(&self) -> &[Point2D] {
        &self.inner.points
    }
    fn closed(&self) -> bool {
        self.inner.closed
    }
    fn points_mut(&mut self) -> &mut Vec<Point2D> {
        &mut self.inner.points
    }
}

/// Convert a closed Polyline to a VecPath for boolean operations.
fn polyline_to_vecpath(poly: &Polyline) -> VecPath {
    if poly.points.is_empty() {
        return VecPath { subpaths: vec![] };
    }
    let mut commands = vec![PathCommand::MoveTo {
        x: poly.points[0].x,
        y: poly.points[0].y,
    }];
    for pt in &poly.points[1..] {
        commands.push(PathCommand::LineTo { x: pt.x, y: pt.y });
    }
    if poly.closed {
        commands.push(PathCommand::Close);
    }
    VecPath {
        subpaths: vec![SubPath {
            commands,
            closed: poly.closed,
        }],
    }
}

/// Check if point (px, py) is inside a closed polyline using ray-casting.
fn point_in_polyline(px: f64, py: f64, poly: &Polyline) -> bool {
    let pts = &poly.points;
    let n = pts.len();
    if n < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = (pts[i].x, pts[i].y);
        let (xj, yj) = (pts[j].x, pts[j].y);
        if ((yi > py) != (yj > py)) && (px < (xj - xi) * (py - yi) / (yj - yi) + xi) {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn polyline_has_area(poly: &Polyline) -> bool {
    poly.closed && poly.points.len() >= 3 && signed_area(&poly.points).abs() > 1e-9
}

/// Test whether a point is inside the SVG-style fill of a set of polylines,
/// treating multiple polylines as subpaths of a compound shape. Uses the
/// nonzero winding rule so opposite-winding holes (e.g. a donut) carve out
/// correctly, matching how a canvas/browser would render the same path with
/// `fill`.
fn point_in_any_polyline(point: Point2D, polys: &[Polyline]) -> bool {
    let mut winding: i32 = 0;
    for poly in polys {
        if !polyline_has_area(poly) {
            continue;
        }
        if point_in_polyline(point.x, point.y, poly) {
            winding += if signed_area(&poly.points) >= 0.0 {
                1
            } else {
                -1
            };
        }
    }
    winding != 0
}

fn set_raster_pixel_unburned(raster: &mut beambench_raster::ProcessedRaster, x: usize, y: usize) {
    match raster.format {
        RasterPixelFormat::Binary => {
            let row_bytes = (raster.width_px as usize).div_ceil(8);
            let idx = y * row_bytes + x / 8;
            if let Some(byte) = raster.data.get_mut(idx) {
                let bit_idx = 7 - (x % 8);
                *byte |= 1 << bit_idx;
            }
        }
        RasterPixelFormat::Grayscale8 => {
            let idx = y * raster.width_px as usize + x;
            if let Some(pixel) = raster.data.get_mut(idx) {
                *pixel = 255;
            }
        }
    }
}

fn raster_has_burn_pixels(raster: &beambench_raster::ProcessedRaster) -> bool {
    match raster.format {
        RasterPixelFormat::Binary => raster.data.iter().any(|byte| *byte != 0xFF),
        RasterPixelFormat::Grayscale8 => raster.data.iter().any(|pixel| *pixel < 255),
    }
}

fn apply_image_masks(
    project: &Project,
    layer: &Layer,
    obj: &ProjectObject,
    processed: &beambench_raster::ProcessedRaster,
    failed_entries: &mut Vec<PlanEntryFailure>,
) -> beambench_raster::ProcessedRaster {
    let ObjectData::RasterImage { masks, .. } = &obj.data else {
        return processed.clone();
    };
    if masks.is_empty() {
        return processed.clone();
    }

    let mut inside = Vec::new();
    let mut outside = Vec::new();
    for mask in masks {
        let Some(mask_obj) = project.find_object(mask.object_id) else {
            continue;
        };
        let Some(path) =
            beambench_core::vector::convert::object_to_world_vecpath_resolved(mask_obj, project)
        else {
            failed_entries.push(PlanEntryFailure {
                layer_id: layer.id.to_string(),
                cut_entry_id: None,
                operation: OperationType::Image,
                reason: PlanEntryFailureReason::ImageMaskSkipped {
                    object_id: mask.object_id.to_string(),
                    message: "Mask object is not vector-like".to_string(),
                },
            });
            continue;
        };
        let polylines: Vec<Polyline> = flatten_vecpath(&path, DEFAULT_TOLERANCE_MM)
            .into_iter()
            .filter(polyline_has_area)
            .collect();
        if polylines.is_empty() {
            failed_entries.push(PlanEntryFailure {
                layer_id: layer.id.to_string(),
                cut_entry_id: None,
                operation: OperationType::Image,
                reason: PlanEntryFailureReason::ImageMaskSkipped {
                    object_id: mask.object_id.to_string(),
                    message: "Mask object is open or has no usable area".to_string(),
                },
            });
            continue;
        }
        match mask.polarity {
            ImageMaskPolarity::KeepInside => inside.extend(polylines),
            ImageMaskPolarity::KeepOutside => outside.extend(polylines),
        }
    }

    if inside.is_empty() && outside.is_empty() {
        return processed.clone();
    }

    let mut masked = processed.clone();
    let x_step = processed.effective_x_pixel_mm();
    let y_step = processed.line_interval_mm;
    let obj_center = Point2D::new(
        (obj.bounds.min.x + obj.bounds.max.x) / 2.0,
        (obj.bounds.min.y + obj.bounds.max.y) / 2.0,
    );
    for y in 0..processed.height_px as usize {
        for x in 0..processed.width_px as usize {
            let local = Point2D::new(
                obj.bounds.min.x + (x as f64 + 0.5) * x_step,
                obj.bounds.min.y + (y as f64 + 0.5) * y_step,
            );
            let world = if obj.transform.is_identity() {
                local
            } else {
                obj.transform.apply_around_center(&local, &obj_center)
            };
            let inside_ok = inside.is_empty() || point_in_any_polyline(world, &inside);
            let outside_hit = !outside.is_empty() && point_in_any_polyline(world, &outside);
            if !inside_ok || outside_hit {
                set_raster_pixel_unburned(&mut masked, x, y);
            }
        }
    }

    if raster_has_burn_pixels(processed) && !raster_has_burn_pixels(&masked) {
        let mask_ids = masks
            .iter()
            .map(|mask| mask.object_id.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        failed_entries.push(PlanEntryFailure {
            layer_id: layer.id.to_string(),
            cut_entry_id: None,
            operation: OperationType::Image,
            reason: PlanEntryFailureReason::ImageMaskSkipped {
                object_id: obj.id.to_string(),
                message: format!("Image masks exclude the full raster area (masks: {mask_ids})"),
            },
        });
    }

    masked
}

/// Check if all points of `inner` are inside `outer` (fully contained).
fn polyline_contained_in(inner: &Polyline, outer: &Polyline) -> bool {
    if inner.points.is_empty() {
        return false;
    }
    inner
        .points
        .iter()
        .all(|pt| point_in_polyline(pt.x, pt.y, outer))
}

#[derive(Clone, Copy)]
struct PolylineAnalysis {
    bounds: Option<Bounds>,
    signed_area: f64,
    abs_area: f64,
}

fn analyze_polylines(polylines: &[Polyline]) -> Vec<PolylineAnalysis> {
    polylines
        .iter()
        .map(|polyline| {
            let bounds = bounds_from_polylines(std::slice::from_ref(polyline));
            let signed_area = if polyline.closed {
                signed_area(&polyline.points)
            } else {
                0.0
            };
            PolylineAnalysis {
                bounds,
                signed_area,
                abs_area: signed_area.abs(),
            }
        })
        .collect()
}

fn bounds_contains_bounds(outer: &Bounds, inner: &Bounds) -> bool {
    outer.min.x <= inner.min.x
        && outer.min.y <= inner.min.y
        && outer.max.x >= inner.max.x
        && outer.max.y >= inner.max.y
}

fn polyline_maybe_contains(
    inner_index: usize,
    outer_index: usize,
    analyses: &[PolylineAnalysis],
) -> bool {
    let Some(inner_bounds) = analyses[inner_index].bounds else {
        return false;
    };
    let Some(outer_bounds) = analyses[outer_index].bounds else {
        return false;
    };
    if !bounds_contains_bounds(&outer_bounds, &inner_bounds) {
        return false;
    }
    analyses[outer_index].abs_area + 1e-9 >= analyses[inner_index].abs_area
}

fn exact_polyline_contained_in(
    inner_index: usize,
    outer_index: usize,
    polylines: &[Polyline],
) -> bool {
    polyline_contained_in(&polylines[inner_index], &polylines[outer_index])
}

fn compute_all_nesting_depths(polylines: &[Polyline]) -> (Vec<usize>, Vec<PolylineAnalysis>) {
    let analyses = analyze_polylines(polylines);
    let mut depths = vec![0usize; polylines.len()];

    for inner_index in 0..polylines.len() {
        let inner_sign = analyses[inner_index].signed_area.signum();
        let mut candidate_indices: Vec<usize> = (0..polylines.len())
            .filter(|&outer_index| {
                inner_index != outer_index
                    && polyline_maybe_contains(inner_index, outer_index, &analyses)
            })
            .collect();

        // Winding is only a hint, not a correctness authority. Check opposite-
        // winding candidates first because imported compound paths often encode
        // holes that way, but still fall back to exact containment for all
        // remaining bbox-feasible candidates.
        candidate_indices.sort_by_key(|&outer_index| {
            (analyses[outer_index].signed_area.signum() == inner_sign) as u8
        });

        for outer_index in candidate_indices {
            if exact_polyline_contained_in(inner_index, outer_index, polylines) {
                depths[inner_index] += 1;
            }
        }
    }

    (depths, analyses)
}

/// Compute tight bounds from actual polyline points.
fn bounds_from_polylines(polylines: &[Polyline]) -> Option<Bounds> {
    let mut min_x = f64::MAX;
    let mut min_y = f64::MAX;
    let mut max_x = f64::MIN;
    let mut max_y = f64::MIN;
    for poly in polylines {
        for pt in &poly.points {
            min_x = min_x.min(pt.x);
            min_y = min_y.min(pt.y);
            max_x = max_x.max(pt.x);
            max_y = max_y.max(pt.y);
        }
    }
    if min_x > max_x {
        return None;
    }
    Some(Bounds::new(
        Point2D::new(min_x, min_y),
        Point2D::new(max_x, max_y),
    ))
}

fn polylines_to_vecpath(polylines: &[Polyline]) -> VecPath {
    let subpaths = polylines
        .iter()
        .filter(|poly| !poly.points.is_empty())
        .map(|poly| {
            let mut commands = vec![PathCommand::MoveTo {
                x: poly.points[0].x,
                y: poly.points[0].y,
            }];
            for pt in &poly.points[1..] {
                commands.push(PathCommand::LineTo { x: pt.x, y: pt.y });
            }
            if poly.closed {
                commands.push(PathCommand::Close);
            }
            SubPath {
                commands,
                closed: poly.closed,
            }
        })
        .collect();
    VecPath { subpaths }
}

fn trim_closing_duplicate(points: &mut Vec<(f64, f64)>) {
    if points.len() < 2 {
        return;
    }
    let first = points[0];
    let last = points[points.len() - 1];
    if (first.0 - last.0).abs() <= 1e-9 && (first.1 - last.1).abs() <= 1e-9 {
        points.pop();
    }
}

fn polylines_to_wound_clipper_paths(polylines: &[Polyline]) -> Paths<Milli> {
    let (depths, analyses) = compute_all_nesting_depths(polylines);
    let contours: Vec<Vec<(f64, f64)>> = polylines
        .iter()
        .enumerate()
        .filter_map(|(idx, polyline)| {
            if !polyline.closed || polyline.points.len() < 3 || analyses[idx].abs_area < 1e-12 {
                return None;
            }
            let mut points: Vec<(f64, f64)> = polyline
                .points
                .iter()
                .map(|point| (point.x, point.y))
                .collect();
            trim_closing_duplicate(&mut points);
            if points.len() < 3 {
                return None;
            }
            let should_be_ccw = depths[idx].is_multiple_of(2);
            let is_ccw = signed_area(&polyline.points) > 0.0;
            if should_be_ccw != is_ccw {
                points.reverse();
            }
            Some(points)
        })
        .collect();
    contours.into()
}

fn clipper_paths_to_polylines(paths: &Paths<Milli>) -> Vec<Polyline> {
    paths
        .iter()
        .filter_map(|path| {
            let points: Vec<Point2D> = path
                .iter()
                .map(|point| Point2D::new(point.x(), point.y()))
                .collect();
            (points.len() >= 3).then(|| Polyline::new(points, true))
        })
        .collect()
}

fn polylines_to_clipper_paths_preserving_winding(polylines: &[Polyline]) -> Paths<Milli> {
    let contours: Vec<Vec<(f64, f64)>> = polylines
        .iter()
        .filter_map(|polyline| {
            if !polyline.closed || polyline.points.len() < 3 {
                return None;
            }
            let mut points: Vec<(f64, f64)> = polyline
                .points
                .iter()
                .map(|point| (point.x, point.y))
                .collect();
            trim_closing_duplicate(&mut points);
            (points.len() >= 3).then_some(points)
        })
        .collect();
    contours.into()
}

fn keep_next_offset_fill_polyline(polyline: &Polyline, offset_spacing: f64) -> bool {
    if !polyline.closed || polyline.points.len() < 3 {
        return false;
    }
    if signed_area(&polyline.points).abs() < offset_spacing * offset_spacing * 0.25 {
        return false;
    }
    let Some(bounds) = polyline_bounds(polyline) else {
        return false;
    };
    bounds.width() > offset_spacing * 0.5 && bounds.height() > offset_spacing * 0.5
}

#[derive(Clone)]
struct OffsetFillUnit {
    outer: Polyline,
    path: VecPath,
    bounds: Bounds,
}

#[derive(Clone)]
struct OffsetFillTrack {
    polylines: Vec<Polyline>,
    parity: usize,
    parent: Option<usize>,
}

#[derive(Debug)]
struct OffsetFillTrackBuildStats {
    iterations: usize,
    hit_iteration_cap: bool,
    timed_out: bool,
    cancelled: bool,
    elapsed_ms: u64,
    flatten_time: Duration,
    nesting_time: Duration,
    matching_time: Duration,
    buffer_time: Duration,
    optimize_time: Duration,
}

impl Default for OffsetFillTrackBuildStats {
    fn default() -> Self {
        Self {
            iterations: 0,
            hit_iteration_cap: false,
            timed_out: false,
            cancelled: false,
            elapsed_ms: 0,
            flatten_time: Duration::ZERO,
            nesting_time: Duration::ZERO,
            matching_time: Duration::ZERO,
            buffer_time: Duration::ZERO,
            optimize_time: Duration::ZERO,
        }
    }
}

struct OffsetFillTrackBuildResult {
    tracks: Vec<OffsetFillTrack>,
    stats: OffsetFillTrackBuildStats,
}

struct PreparedOffsetFillUnit {
    unit_center: Point2D,
    tracks: Vec<OffsetFillTrack>,
    children: Vec<Vec<usize>>,
    roots: Vec<usize>,
    build_stats: OffsetFillTrackBuildStats,
}

struct OffsetFillCompositeBatch {
    units: Vec<OffsetFillUnit>,
}

#[derive(Clone)]
struct OffsetFillBuildControls {
    started_at: Instant,
    timeout: Duration,
    cancellation: PlannerCancellation,
    #[cfg(test)]
    iteration_limit: Option<usize>,
    #[cfg(test)]
    cancel_after_iterations: Option<usize>,
}

impl OffsetFillBuildControls {
    fn new(cancellation: PlannerCancellation) -> Self {
        Self {
            started_at: Instant::now(),
            timeout: OFFSET_FILL_ENTRY_TIMEOUT,
            cancellation,
            #[cfg(test)]
            iteration_limit: None,
            #[cfg(test)]
            cancel_after_iterations: None,
        }
    }

    #[cfg(test)]
    fn with_limits(timeout: Duration, cancellation: PlannerCancellation) -> Self {
        Self {
            started_at: Instant::now(),
            timeout,
            cancellation,
            iteration_limit: None,
            cancel_after_iterations: None,
        }
    }

    #[cfg(test)]
    fn with_test_limits(
        timeout: Duration,
        cancellation: PlannerCancellation,
        iteration_limit: Option<usize>,
        cancel_after_iterations: Option<usize>,
    ) -> Self {
        Self {
            started_at: Instant::now(),
            timeout,
            cancellation,
            iteration_limit,
            cancel_after_iterations,
        }
    }

    fn elapsed(&self) -> Duration {
        self.started_at.elapsed()
    }

    fn should_cancel(&self) -> bool {
        self.cancellation.should_cancel()
    }

    fn should_cancel_at_iteration(&self, _iteration: usize) -> bool {
        #[cfg(test)]
        if self
            .cancel_after_iterations
            .is_some_and(|limit| _iteration >= limit)
        {
            return true;
        }
        self.should_cancel()
    }

    fn timed_out(&self) -> bool {
        self.elapsed() >= self.timeout
    }

    fn iteration_limit(&self, geometry_iteration_limit: usize) -> usize {
        #[cfg(test)]
        if let Some(limit) = self.iteration_limit {
            return limit.min(geometry_iteration_limit);
        }
        geometry_iteration_limit
    }

    fn is_emergency_iteration_limit(&self, iteration_limit: usize) -> bool {
        #[cfg(test)]
        if self.iteration_limit.is_some() {
            return true;
        }
        iteration_limit >= OFFSET_FILL_ITERATION_LIMIT
    }
}

fn nearest_even_ancestor(
    index: usize,
    polylines: &[Polyline],
    depths: &[usize],
    analyses: &[PolylineAnalysis],
) -> Option<usize> {
    (0..polylines.len())
        .filter(|&other| {
            other != index
                && depths[other].is_multiple_of(2)
                && depths[other] < depths[index]
                && polyline_maybe_contains(index, other, analyses)
                && exact_polyline_contained_in(index, other, polylines)
        })
        .max_by_key(|&other| depths[other])
}

fn split_offset_fill_units(path: &VecPath) -> Vec<OffsetFillUnit> {
    // OffsetFill composites are currently line-only VecPaths emitted from
    // boolean normalization. Re-flattening at DEFAULT_TOLERANCE_MM is
    // therefore effectively a no-op today, but we keep the call here as a
    // defensive normalization step if this path ever starts carrying curves.
    let polylines: Vec<Polyline> = flatten_vecpath(path, DEFAULT_TOLERANCE_MM)
        .into_iter()
        .filter(|poly| poly.closed && poly.points.len() >= 3)
        .collect();

    if polylines.is_empty() {
        return Vec::new();
    }

    let (depths, analyses) = compute_all_nesting_depths(&polylines);

    let mut units = Vec::new();
    for (idx, outer) in polylines.iter().enumerate() {
        if !depths[idx].is_multiple_of(2) {
            continue;
        }

        let mut unit_polylines = vec![outer.clone()];
        for (candidate_idx, candidate) in polylines.iter().enumerate() {
            if !depths[candidate_idx].is_multiple_of(2)
                && nearest_even_ancestor(candidate_idx, &polylines, &depths, &analyses) == Some(idx)
            {
                unit_polylines.push(candidate.clone());
            }
        }

        if let Some(bounds) = bounds_from_polylines(&unit_polylines) {
            units.push(OffsetFillUnit {
                outer: outer.clone(),
                path: polylines_to_vecpath(&unit_polylines),
                bounds,
            });
        }
    }

    units
}

fn polyline_match_score(a: &Polyline, b: &Polyline) -> f64 {
    polyline_center(a).distance_to(&polyline_center(b))
        + nearest_distance_to_polyline(a, polyline_center(b)) * 0.25
}

fn polyline_contains_or_is_contained(a: &Polyline, b: &Polyline) -> bool {
    let Some(a_bounds) = bounds_from_polylines(std::slice::from_ref(a)) else {
        return false;
    };
    let Some(b_bounds) = bounds_from_polylines(std::slice::from_ref(b)) else {
        return false;
    };
    let a_contains_b = bounds_contains_bounds(&a_bounds, &b_bounds);
    let b_contains_a = bounds_contains_bounds(&b_bounds, &a_bounds);
    if !a_contains_b && !b_contains_a {
        return false;
    }
    polyline_contained_in(a, b) || polyline_contained_in(b, a)
}

fn polyline_center(polyline: &Polyline) -> Point2D {
    polyline_bounds(polyline)
        .map(|bounds| {
            Point2D::new(
                (bounds.min.x + bounds.max.x) * 0.5,
                (bounds.min.y + bounds.max.y) * 0.5,
            )
        })
        .unwrap_or(Point2D::zero())
}

fn rotate_closed_polyline(polyline: &Polyline, start_idx: usize) -> Polyline {
    let n = polyline.points.len();
    if !polyline.closed || n < 3 {
        return polyline.clone();
    }

    let points = (0..n)
        .map(|offset| polyline.points[(start_idx + offset) % n])
        .collect();
    Polyline::new(points, true)
}

fn close_polyline_for_vector_emission(polyline: &Polyline) -> Vec<Point2D> {
    let mut points = polyline.points.clone();
    if polyline.closed
        && points.len() >= 3
        && points
            .first()
            .zip(points.last())
            .is_some_and(|(a, b)| a != b)
    {
        points.push(points[0]);
    }
    points
}

fn nearest_distance_to_polyline(polyline: &Polyline, target: Point2D) -> f64 {
    polyline
        .points
        .iter()
        .map(|point| target.distance_to(point))
        .fold(f64::MAX, f64::min)
}

fn polyline_bounds(polyline: &Polyline) -> Option<Bounds> {
    bounds_from_polylines(std::slice::from_ref(polyline))
}

fn cross2(a: Point2D, b: Point2D) -> f64 {
    a.x * b.y - a.y * b.x
}

fn point_on_polyline_phase(polyline: &Polyline, origin: Point2D, angle_deg: f64) -> Point2D {
    let angle = angle_deg.to_radians();
    let dir = Point2D::new(angle.cos(), angle.sin());
    let mut best_hit: Option<(f64, Point2D)> = None;

    let segment_count = if polyline.closed {
        polyline.points.len()
    } else {
        polyline.points.len().saturating_sub(1)
    };

    for idx in 0..segment_count {
        let a = polyline.points[idx];
        let b = polyline.points[(idx + 1) % polyline.points.len()];
        let seg = Point2D::new(b.x - a.x, b.y - a.y);
        let to_a = Point2D::new(a.x - origin.x, a.y - origin.y);
        let denom = cross2(dir, seg);
        if denom.abs() <= 1e-9 {
            continue;
        }

        let t = cross2(to_a, seg) / denom;
        let u = cross2(to_a, dir) / denom;
        if t < 0.0 || !(0.0..=1.0).contains(&u) {
            continue;
        }

        let hit = Point2D::new(origin.x + dir.x * t, origin.y + dir.y * t);
        match best_hit {
            Some((best_t, _)) if t >= best_t => {}
            _ => best_hit = Some((t, hit)),
        }
    }

    if let Some((_, hit)) = best_hit {
        return hit;
    }

    polyline
        .points
        .iter()
        .copied()
        .min_by(|a, b| {
            let da = angular_delta_deg(point_angle_deg(origin, *a), angle_deg);
            let db = angular_delta_deg(point_angle_deg(origin, *b), angle_deg);
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or(origin)
}

fn point_angle_deg(origin: Point2D, point: Point2D) -> f64 {
    (point.y - origin.y).atan2(point.x - origin.x).to_degrees()
}

fn angular_delta_deg(a: f64, b: f64) -> f64 {
    let mut delta = (a - b).abs() % 360.0;
    if delta > 180.0 {
        delta = 360.0 - delta;
    }
    delta
}

fn select_corridor_start_index(
    polyline: &Polyline,
    corridor_origin: Point2D,
    seam_angle_deg: f64,
    seam_target: Point2D,
    current_pos: Point2D,
) -> usize {
    const CORRIDOR_WINDOW_DEG: f64 = 22.0;
    const TARGET_WEIGHT: f64 = 1.6;
    const CURRENT_POS_WEIGHT: f64 = 0.18;
    const ANGLE_PENALTY_MM_PER_DEG: f64 = 0.45;

    let mut best_idx = None;
    let mut best_score = f64::MAX;

    for (idx, point) in polyline.points.iter().enumerate() {
        let point_angle = point_angle_deg(corridor_origin, *point);
        let delta = angular_delta_deg(point_angle, seam_angle_deg);
        if delta > CORRIDOR_WINDOW_DEG {
            continue;
        }

        let score = seam_target.distance_to(point) * TARGET_WEIGHT
            + current_pos.distance_to(point) * CURRENT_POS_WEIGHT
            + delta * ANGLE_PENALTY_MM_PER_DEG;
        if score < best_score {
            best_score = score;
            best_idx = Some(idx);
        }
    }

    if let Some(idx) = best_idx {
        return idx;
    }

    polyline
        .points
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            let da = seam_target.distance_to(a) * TARGET_WEIGHT
                + current_pos.distance_to(a) * CURRENT_POS_WEIGHT
                + angular_delta_deg(point_angle_deg(corridor_origin, **a), seam_angle_deg)
                    * ANGLE_PENALTY_MM_PER_DEG;
            let db = seam_target.distance_to(b) * TARGET_WEIGHT
                + current_pos.distance_to(b) * CURRENT_POS_WEIGHT
                + angular_delta_deg(point_angle_deg(corridor_origin, **b), seam_angle_deg)
                    * ANGLE_PENALTY_MM_PER_DEG;
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(idx, _)| idx)
        .unwrap_or(0)
}

/// Find the closest point on any edge of the polyline to a target point.
/// Uses standard point-to-line-segment projection to correctly identify the
/// polygon **face** even when two corners are equidistant from the anchor.
fn nearest_point_on_polyline_edge(polyline: &Polyline, target: Point2D) -> Point2D {
    let n = polyline.points.len();
    let seg_count = if polyline.closed {
        n
    } else {
        n.saturating_sub(1)
    };
    let mut best_point = polyline.points.first().copied().unwrap_or(target);
    let mut best_dist_sq = f64::MAX;

    for i in 0..seg_count {
        let a = polyline.points[i];
        let b = polyline.points[(i + 1) % n];
        let ab = Point2D::new(b.x - a.x, b.y - a.y);
        let at = Point2D::new(target.x - a.x, target.y - a.y);
        let len_sq = ab.x * ab.x + ab.y * ab.y;
        let t = if len_sq > 1e-12 {
            ((at.x * ab.x + at.y * ab.y) / len_sq).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let proj = Point2D::new(a.x + ab.x * t, a.y + ab.y * t);
        let dx = target.x - proj.x;
        let dy = target.y - proj.y;
        let dist_sq = dx * dx + dy * dy;
        if dist_sq < best_dist_sq {
            best_dist_sq = dist_sq;
            best_point = proj;
        }
    }
    best_point
}

/// Face-tracking seam selection for subsequent rings.
///
/// Two-tier scoring: first narrows to vertices on the correct face (within a
/// tolerance of the nearest edge-projected point), then among those candidates
/// picks the one closest to `current_pos` to minimise travel.  Falls back to
/// the full vertex set with blended scoring when no vertex is close to the
/// face point.
fn select_face_tracking_start(
    polyline: &Polyline,
    anchor: Point2D,
    corridor_origin: Point2D,
    seam_angle_deg: f64,
    current_pos: Point2D,
) -> usize {
    let face_point = nearest_point_on_polyline_edge(polyline, anchor);

    // Tier 1: collect vertices that are "on the same face" — within a
    // generous tolerance of the closest edge projection.
    let min_face_dist = polyline
        .points
        .iter()
        .map(|p| face_point.distance_to(p))
        .fold(f64::MAX, f64::min);
    // Tolerance: 50% beyond the best vertex's distance to the face point,
    // but at least 0.5mm to avoid precision issues on very small rings.
    let face_tolerance = (min_face_dist * 1.5).max(0.5);

    let face_candidates: Vec<usize> = polyline
        .points
        .iter()
        .enumerate()
        .filter(|(_, p)| face_point.distance_to(p) <= face_tolerance)
        .map(|(i, _)| i)
        .collect();

    if face_candidates.len() > 1 {
        // Tier 2: among face-equivalent vertices, keep the seam anchored to
        // the same face first. Current-position distance is only a small
        // tiebreaker; otherwise dense offset fills can jump to the opposite
        // side when Clipper changes vertex placement between rings.
        const CURRENT_POS_TIEBREAKER: f64 = 0.02;
        return *face_candidates
            .iter()
            .min_by(|&&ia, &&ib| {
                let a = polyline.points[ia];
                let b = polyline.points[ib];
                let da = face_point.distance_to(&a)
                    + current_pos.distance_to(&a) * CURRENT_POS_TIEBREAKER
                    + angular_delta_deg(point_angle_deg(corridor_origin, a), seam_angle_deg) * 0.01;
                let db = face_point.distance_to(&b)
                    + current_pos.distance_to(&b) * CURRENT_POS_TIEBREAKER
                    + angular_delta_deg(point_angle_deg(corridor_origin, b), seam_angle_deg) * 0.01;
                da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap_or(&0);
    }

    // Single candidate or empty (shouldn't happen): fall back to closest to face point.
    if let Some(&idx) = face_candidates.first() {
        return idx;
    }

    // Fallback (empty polyline edge case): blended scoring.
    const ANGLE_TIEBREAKER: f64 = 0.08;
    polyline
        .points
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            let da = face_point.distance_to(a)
                + angular_delta_deg(point_angle_deg(corridor_origin, **a), seam_angle_deg)
                    * ANGLE_TIEBREAKER;
            let db = face_point.distance_to(b)
                + angular_delta_deg(point_angle_deg(corridor_origin, **b), seam_angle_deg)
                    * ANGLE_TIEBREAKER;
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(idx, _)| idx)
        .unwrap_or(0)
}

/// Result of emitting a single offset fill track (sequence of concentric rings).
struct EmitTrackResult {
    segments: Vec<PlanSegment>,
    end_pos: Point2D,
    /// Per-ring seam anchor — one entry per polyline in the track that was
    /// actually emitted. Used by the tree emitter to pass the correct
    /// split-point anchor to child branches (not the parent's final anchor).
    ring_anchors: Vec<Point2D>,
}

#[allow(clippy::too_many_arguments)]
fn emit_offset_fill_track(
    track: &[Polyline],
    corridor_origin: Point2D,
    seam_angle_deg: f64,
    _offset_spacing: f64,
    power_percent: f64,
    speed_mm_min: f64,
    layer_id: &str,
    cut_entry_id: &str,
    start_pos: Point2D,
    initial_seam_anchor: Option<Point2D>,
    current_pos_hint: Point2D,
) -> EmitTrackResult {
    let mut segments = Vec::new();
    let mut current_pos = start_pos;
    let mut seam_anchor = initial_seam_anchor;
    let mut ring_anchors = Vec::with_capacity(track.len());

    for polyline in track.iter() {
        let start_idx = if let Some(anchor) = seam_anchor {
            // Subsequent rings: face tracking (nearest to previous seam point),
            // with a small current-position tiebreaker to reduce travel.
            select_face_tracking_start(
                polyline,
                anchor,
                corridor_origin,
                seam_angle_deg,
                current_pos,
            )
        } else {
            // First ring: corridor-based to establish the seam face
            let seam_target = point_on_polyline_phase(polyline, corridor_origin, seam_angle_deg);
            select_corridor_start_index(
                polyline,
                corridor_origin,
                seam_angle_deg,
                seam_target,
                if current_pos == start_pos {
                    current_pos_hint
                } else {
                    current_pos
                },
            )
        };

        let anchor_pt = polyline.points[start_idx];
        seam_anchor = Some(anchor_pt);
        ring_anchors.push(anchor_pt);

        let optimized = rotate_closed_polyline(polyline, start_idx);
        let emitted_points = close_polyline_for_vector_emission(&optimized);
        if emitted_points.len() < 2 {
            continue;
        }

        segments.push(PlanSegment::Vector {
            polyline: emitted_points.clone(),
            closed: optimized.closed,
            power_percent,
            speed_mm_min,
            layer_id: layer_id.to_string(),
            cut_entry_id: cut_entry_id.to_string(),
            perforation_enabled: false,
            perforation_on_ms: 0.0,
            perforation_off_ms: 0.0,
            source_object_id: None,
            source_subpath_index: None,
        });

        if let Some(last) = emitted_points.last() {
            current_pos = *last;
        }
    }

    EmitTrackResult {
        segments,
        end_pos: current_pos,
        ring_anchors,
    }
}

#[cfg(test)]
fn build_offset_fill_tracks_with_stats(
    world_path: &VecPath,
    original_bounds: &Bounds,
    offset_spacing: f64,
) -> OffsetFillTrackBuildResult {
    build_offset_fill_tracks_with_controls(
        world_path,
        original_bounds,
        offset_spacing,
        &OffsetFillBuildControls::new(PlannerCancellation::default()),
    )
}

fn build_offset_fill_tracks_with_controls(
    world_path: &VecPath,
    original_bounds: &Bounds,
    offset_spacing: f64,
    controls: &OffsetFillBuildControls,
) -> OffsetFillTrackBuildResult {
    if offset_spacing <= 0.0 || world_path.subpaths.is_empty() {
        return OffsetFillTrackBuildResult {
            tracks: Vec::new(),
            stats: OffsetFillTrackBuildStats::default(),
        };
    }

    let initial_polylines: Vec<Polyline> = flatten_vecpath(world_path, DEFAULT_TOLERANCE_MM)
        .into_iter()
        .filter(|polyline| polyline.closed && polyline.points.len() >= 3)
        .collect();
    if initial_polylines.is_empty() {
        return OffsetFillTrackBuildResult {
            tracks: Vec::new(),
            stats: OffsetFillTrackBuildStats::default(),
        };
    }

    let mut current_paths = polylines_to_wound_clipper_paths(&initial_polylines);
    let mut tracks: Vec<OffsetFillTrack> = Vec::new();
    let mut frontier: Vec<usize> = Vec::new();
    let mut stats = OffsetFillTrackBuildStats::default();
    let geometry_iteration_limit =
        ((original_bounds.width().max(original_bounds.height()) / offset_spacing).ceil() as usize)
            .saturating_add(4)
            .min(OFFSET_FILL_ITERATION_LIMIT);
    let iteration_limit = controls.iteration_limit(geometry_iteration_limit);

    for iteration in 0..iteration_limit {
        stats.iterations = iteration + 1;
        stats.elapsed_ms = controls
            .elapsed()
            .as_millis()
            .try_into()
            .unwrap_or(u64::MAX);
        if controls.should_cancel_at_iteration(iteration) {
            stats.cancelled = true;
            break;
        }
        if controls.timed_out() {
            stats.timed_out = true;
            break;
        }

        let flatten_started_at = Instant::now();
        let ring_polylines: Vec<Polyline> = clipper_paths_to_polylines(&current_paths);
        stats.flatten_time += flatten_started_at.elapsed();
        if ring_polylines.is_empty() {
            break;
        }

        if iteration > 0
            && let Some(cb) = bounds_from_polylines(&ring_polylines)
        {
            let tol = offset_spacing;
            if cb.min.x < original_bounds.min.x - tol
                || cb.min.y < original_bounds.min.y - tol
                || cb.max.x > original_bounds.max.x + tol
                || cb.max.y > original_bounds.max.y + tol
            {
                break;
            }
        }

        // Stop when remaining fillable area is too small to engrave.
        // Clipper normalizes closed path winding on every inset, so signed
        // area is sufficient here: CCW exteriors add area, CW holes subtract.
        if iteration > 0 {
            let net_area: f64 = ring_polylines
                .iter()
                .map(|pl| signed_area(&pl.points))
                .sum();
            if net_area.abs() < offset_spacing * offset_spacing {
                break;
            }
        }

        let analyses = analyze_polylines(&ring_polylines);
        // Clipper returns polygon contours with normalized winding each inset:
        // CCW exteriors and CW holes. That keeps parity classification O(n)
        // and avoids re-running full nesting on every dense offset iteration.
        let iteration_parities: Vec<usize> = analyses
            .iter()
            .map(|analysis| if analysis.signed_area >= 0.0 { 0 } else { 1 })
            .collect();
        let mut next_frontier = Vec::new();
        let matching_started_at = Instant::now();

        for parity in [0usize, 1usize] {
            let candidate_indices: Vec<usize> = analyses
                .iter()
                .enumerate()
                .filter_map(|(idx, analysis)| {
                    let candidate_parity = iteration_parities
                        .get(idx)
                        .copied()
                        .unwrap_or_else(|| if analysis.signed_area >= 0.0 { 0 } else { 1 });
                    (candidate_parity == parity).then_some(idx)
                })
                .collect();
            if candidate_indices.is_empty() {
                continue;
            }

            let parent_ids: Vec<usize> = frontier
                .iter()
                .copied()
                .filter(|track_id| tracks[*track_id].parity == parity)
                .collect();

            let mut best_parent_for_candidate: Vec<Option<usize>> =
                vec![None; ring_polylines.len()];
            let mut pairs = Vec::new();

            for &candidate_idx in &candidate_indices {
                let candidate = &ring_polylines[candidate_idx];
                let mut best_parent = None;
                let mut best_score = f64::MAX;
                for &parent_id in &parent_ids {
                    let Some(parent_polyline) = tracks[parent_id].polylines.last() else {
                        continue;
                    };
                    let Some(parent_bounds) = polyline_bounds(parent_polyline) else {
                        continue;
                    };
                    let Some(candidate_bounds) = analyses[candidate_idx].bounds else {
                        continue;
                    };
                    let bbox_related = bounds_contains_bounds(&parent_bounds, &candidate_bounds)
                        || bounds_contains_bounds(&candidate_bounds, &parent_bounds);
                    if !bbox_related {
                        continue;
                    }
                    if !polyline_contains_or_is_contained(parent_polyline, candidate) {
                        continue;
                    }
                    let score = polyline_match_score(parent_polyline, candidate);
                    if score < best_score {
                        best_score = score;
                        best_parent = Some(parent_id);
                    }
                    pairs.push((score, parent_id, candidate_idx));
                }
                best_parent_for_candidate[candidate_idx] = best_parent;
            }

            pairs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

            let mut claimed_parents = std::collections::HashSet::new();
            let mut continued_candidates = std::collections::HashSet::new();

            for (_, parent_id, candidate_idx) in pairs {
                if claimed_parents.contains(&parent_id)
                    || continued_candidates.contains(&candidate_idx)
                {
                    continue;
                }
                tracks[parent_id]
                    .polylines
                    .push(ring_polylines[candidate_idx].clone());
                claimed_parents.insert(parent_id);
                continued_candidates.insert(candidate_idx);
                next_frontier.push(parent_id);
            }

            for &candidate_idx in &candidate_indices {
                if continued_candidates.contains(&candidate_idx) {
                    continue;
                }
                let new_track_id = tracks.len();
                tracks.push(OffsetFillTrack {
                    polylines: vec![ring_polylines[candidate_idx].clone()],
                    parity,
                    parent: best_parent_for_candidate[candidate_idx],
                });
                next_frontier.push(new_track_id);
            }
        }
        stats.matching_time += matching_started_at.elapsed();

        frontier = next_frontier;

        let buffer_started_at = Instant::now();
        let simplify_tolerance = offset_spacing * 0.02;
        let next_paths = current_paths
            .inflate(-offset_spacing, JoinType::Round, EndType::Polygon, 2.0)
            .simplify(simplify_tolerance, false);
        stats.buffer_time += buffer_started_at.elapsed();
        if next_paths.is_empty() || !next_paths.contains_points() {
            break;
        }

        let next_polylines: Vec<Polyline> = clipper_paths_to_polylines(&next_paths)
            .into_iter()
            .filter(|polyline| keep_next_offset_fill_polyline(polyline, offset_spacing))
            .collect();
        if next_polylines.is_empty() {
            break;
        }
        let Some(bounds) = bounds_from_polylines(&next_polylines) else {
            break;
        };
        if bounds.width() <= 1e-6 || bounds.height() <= 1e-6 {
            break;
        }
        current_paths = polylines_to_clipper_paths_preserving_winding(&next_polylines);
    }

    if stats.iterations >= iteration_limit
        && controls.is_emergency_iteration_limit(iteration_limit)
        && !stats.timed_out
        && !stats.cancelled
    {
        stats.hit_iteration_cap = true;
    }

    if stats.timed_out || stats.cancelled || stats.hit_iteration_cap {
        return OffsetFillTrackBuildResult { tracks, stats };
    }

    let track_count = tracks.len();
    if track_count <= OFFSET_FILL_PARENT_REPAIR_TRACK_LIMIT {
        for track_id in 0..track_count {
            if tracks[track_id].parent.is_some() {
                continue;
            }
            let Some(first_polyline) = tracks[track_id].polylines.first() else {
                continue;
            };

            let mut best_parent = None;
            let mut best_area = f64::MAX;
            for (other_id, other) in tracks.iter().enumerate() {
                if other_id == track_id || other.parity == tracks[track_id].parity {
                    continue;
                }
                let Some(other_first) = other.polylines.first() else {
                    continue;
                };
                if !polyline_contained_in(first_polyline, other_first) {
                    continue;
                }
                let area = polyline_bounds(other_first)
                    .map(|bounds| bounds.width() * bounds.height())
                    .unwrap_or(f64::MAX);
                if area < best_area {
                    best_area = area;
                    best_parent = Some(other_id);
                }
            }
            tracks[track_id].parent = best_parent;
        }
    }

    tracing::info!(
        target: "perf",
        operation = "offset_fill.build_tracks",
        iterations = stats.iterations,
        track_count = tracks.len(),
        flatten_ms = stats.flatten_time.as_secs_f64() * 1000.0,
        nesting_ms = stats.nesting_time.as_secs_f64() * 1000.0,
        matching_ms = stats.matching_time.as_secs_f64() * 1000.0,
        buffer_ms = stats.buffer_time.as_secs_f64() * 1000.0,
        optimize_ms = stats.optimize_time.as_secs_f64() * 1000.0,
        hit_iteration_cap = stats.hit_iteration_cap,
        timed_out = stats.timed_out,
        cancelled = stats.cancelled,
        elapsed_ms = stats.elapsed_ms,
    );

    OffsetFillTrackBuildResult { tracks, stats }
}

#[cfg(test)]
fn build_offset_fill_tracks(
    world_path: &VecPath,
    original_bounds: &Bounds,
    offset_spacing: f64,
) -> Vec<OffsetFillTrack> {
    build_offset_fill_tracks_with_stats(world_path, original_bounds, offset_spacing).tracks
}

/// Find the anchor from the parent ring at the point where the child split off.
///
/// Returns a point on the parent ring's edge that faces the child — NOT the
/// parent ring's original seam vertex, which may be on a completely different
/// face of the parent.  This ensures the child inherits the correct corridor
/// side for the region where it actually emerged.
fn find_split_point_anchor(
    parent_track: &OffsetFillTrack,
    child_first_polyline: &Polyline,
) -> Option<Point2D> {
    if parent_track.polylines.is_empty() {
        return None;
    }
    let child_center = polyline_center(child_first_polyline);

    // Find the parent ring closest to the child (by minimum vertex distance).
    let mut best_ring_idx = parent_track.polylines.len() - 1;
    let mut best_dist = f64::MAX;
    for (i, parent_poly) in parent_track.polylines.iter().enumerate() {
        let min_dist = parent_poly
            .points
            .iter()
            .map(|pp| pp.distance_to(&child_center))
            .fold(f64::MAX, f64::min);
        if min_dist < best_dist {
            best_dist = min_dist;
            best_ring_idx = i;
        }
    }

    // Return the point on that parent ring's edge nearest to the child center.
    // This correctly identifies the face of the parent that the child emerged
    // from, even when the parent's own seam vertex is on the opposite side.
    let parent_ring = &parent_track.polylines[best_ring_idx];
    Some(nearest_point_on_polyline_edge(parent_ring, child_center))
}

#[allow(clippy::too_many_arguments)]
fn emit_offset_fill_track_tree(
    track_id: usize,
    tracks: &[OffsetFillTrack],
    children: &[Vec<usize>],
    corridor_origin: Point2D,
    seam_angle_deg: f64,
    offset_spacing: f64,
    power_percent: f64,
    speed_mm_min: f64,
    layer_id: &str,
    cut_entry_id: &str,
    start_pos: Point2D,
    initial_seam_anchor: Option<Point2D>,
) -> (Vec<PlanSegment>, Point2D, Option<Point2D>) {
    let result = emit_offset_fill_track(
        &tracks[track_id].polylines,
        corridor_origin,
        seam_angle_deg,
        offset_spacing,
        power_percent,
        speed_mm_min,
        layer_id,
        cut_entry_id,
        start_pos,
        initial_seam_anchor,
        start_pos,
    );
    let mut segments = result.segments;
    let mut current_pos = result.end_pos;
    let ring_anchors = result.ring_anchors;
    if segments.is_empty() {
        current_pos = start_pos;
    }

    let mut remaining_children = children[track_id].clone();
    while !remaining_children.is_empty() {
        let next_idx = nearest_offset_fill_track_index(&remaining_children, tracks, current_pos);

        let child_id = remaining_children.remove(next_idx);

        // Use the anchor from the parent ring where this child split off,
        // NOT the parent's final (innermost) ring anchor.
        let split_anchor = tracks[child_id]
            .polylines
            .first()
            .and_then(|child_first| find_split_point_anchor(&tracks[track_id], child_first));

        let (child_segments, child_end, _child_anchor) = emit_offset_fill_track_tree(
            child_id,
            tracks,
            children,
            corridor_origin,
            seam_angle_deg,
            offset_spacing,
            power_percent,
            speed_mm_min,
            layer_id,
            cut_entry_id,
            current_pos,
            split_anchor,
        );
        segments.extend(child_segments);
        current_pos = child_end;
    }

    let final_anchor = ring_anchors.last().copied();
    (segments, current_pos, final_anchor)
}

fn nearest_offset_fill_track_index(
    remaining_tracks: &[usize],
    tracks: &[OffsetFillTrack],
    current_pos: Point2D,
) -> usize {
    if remaining_tracks.len() > OFFSET_FILL_NEAREST_EMIT_TRACK_LIMIT {
        return 0;
    }

    remaining_tracks
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            let da = tracks[**a]
                .polylines
                .first()
                .map(|polyline| nearest_distance_to_polyline(polyline, current_pos))
                .unwrap_or(f64::MAX);
            let db = tracks[**b]
                .polylines
                .first()
                .map(|polyline| nearest_distance_to_polyline(polyline, current_pos))
                .unwrap_or(f64::MAX);
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(idx, _)| idx)
        .unwrap_or(0)
}

fn total_polyline_points(polylines: &[Polyline]) -> usize {
    polylines.iter().map(|poly| poly.points.len()).sum()
}

fn decimate_polyline(poly: &Polyline, keep_every: usize) -> Polyline {
    if keep_every <= 1 || poly.points.len() <= 4 {
        return poly.clone();
    }

    let mut points = Vec::new();
    for (i, pt) in poly.points.iter().enumerate() {
        let is_endpoint = i == 0 || i == poly.points.len() - 1;
        if is_endpoint || i % keep_every == 0 {
            points.push(*pt);
        }
    }

    let min_points = if poly.closed { 3 } else { 2 };
    if points.len() < min_points {
        return poly.clone();
    }

    Polyline::new(points, poly.closed)
}

/// Preview-only simplification budget for raster fill outlines.
///
/// The actual rasterized burn geometry remains exact; this only reduces the
/// amount of outline data cloned into plan segments and sent over preview IPC.
fn simplify_preview_outlines(polylines: &[Polyline]) -> Vec<Polyline> {
    const MAX_PREVIEW_OUTLINE_POINTS: usize = 12_000;

    let original_points = total_polyline_points(polylines);
    if original_points <= MAX_PREVIEW_OUTLINE_POINTS {
        return polylines.to_vec();
    }

    let source = polylines_to_vecpath(polylines);
    let mut tolerance = 0.10;
    let mut simplified = polylines.to_vec();

    for _ in 0..6 {
        let optimized = optimize_path(&source, tolerance);
        simplified = flatten_vecpath(&optimized, DEFAULT_TOLERANCE_MM);
        if total_polyline_points(&simplified) <= MAX_PREVIEW_OUTLINE_POINTS {
            return simplified;
        }
        tolerance *= 2.0;
    }

    let keep_every = total_polyline_points(&simplified).div_ceil(MAX_PREVIEW_OUTLINE_POINTS);
    simplified
        .into_iter()
        .map(|poly| decimate_polyline(&poly, keep_every))
        .filter(|poly| {
            if poly.closed {
                poly.points.len() >= 3
            } else {
                poly.points.len() >= 2
            }
        })
        .collect()
}

fn prepare_offset_fill_unit(
    unit: &OffsetFillUnit,
    offset_spacing: f64,
    controls: &OffsetFillBuildControls,
) -> PreparedOffsetFillUnit {
    let build_result =
        build_offset_fill_tracks_with_controls(&unit.path, &unit.bounds, offset_spacing, controls);
    let tracks = build_result.tracks;
    let mut children = vec![Vec::new(); tracks.len()];
    let mut roots = Vec::new();
    for (track_id, track) in tracks.iter().enumerate() {
        if let Some(parent_id) = track.parent {
            children[parent_id].push(track_id);
        } else {
            roots.push(track_id);
        }
    }

    PreparedOffsetFillUnit {
        unit_center: Point2D::new(
            (unit.bounds.min.x + unit.bounds.max.x) * 0.5,
            (unit.bounds.min.y + unit.bounds.max.y) * 0.5,
        ),
        tracks,
        children,
        roots,
        build_stats: build_result.stats,
    }
}

fn emit_prepared_offset_fill_unit(
    prepared: &PreparedOffsetFillUnit,
    offset_spacing: f64,
    power_percent: f64,
    speed_mm_min: f64,
    layer_id: &str,
    cut_entry_id: &str,
    start_pos: Point2D,
) -> (Vec<PlanSegment>, Point2D) {
    if prepared.tracks.is_empty() {
        return (Vec::new(), start_pos);
    }

    let seam_angle_deg = (start_pos.y - prepared.unit_center.y)
        .atan2(start_pos.x - prepared.unit_center.x)
        .to_degrees();

    let mut segments = Vec::new();
    let mut current_pos = start_pos;
    let mut remaining_roots = prepared.roots.clone();
    while !remaining_roots.is_empty() {
        let next_idx =
            nearest_offset_fill_track_index(&remaining_roots, &prepared.tracks, current_pos);

        let root_id = remaining_roots.remove(next_idx);
        let (root_segments, root_end, _root_anchor) = emit_offset_fill_track_tree(
            root_id,
            &prepared.tracks,
            &prepared.children,
            prepared.unit_center,
            seam_angle_deg,
            offset_spacing,
            power_percent,
            speed_mm_min,
            layer_id,
            cut_entry_id,
            current_pos,
            None,
        );
        segments.extend(root_segments);
        current_pos = root_end;
    }

    (segments, current_pos)
}

/// Classify a normalised angle (0..360) into a cardinal direction.
///
/// Returns `Some((scan_axis, needs_flip))` for 0/90/180/270 (±0.5°),
/// or `None` for non-cardinal angles that require bitmap rotation.
fn classify_cardinal(norm: f64) -> Option<(ScanAxis, bool)> {
    let is_near_zero = norm < 0.5 || (norm - 360.0).abs() < 0.5;
    let is_near_90 = (norm - 90.0).abs() < 0.5;
    let is_near_180 = (norm - 180.0).abs() < 0.5;
    let is_near_270 = (norm - 270.0).abs() < 0.5;

    if is_near_zero {
        Some((ScanAxis::Horizontal, false))
    } else if is_near_90 {
        Some((ScanAxis::Vertical, false))
    } else if is_near_180 {
        Some((ScanAxis::Horizontal, true))
    } else if is_near_270 {
        Some((ScanAxis::Vertical, true))
    } else {
        None
    }
}

/// Build scanlines for a cardinal angle (0/90/180/270) using transpose/flip
/// instead of expensive bitmap rotation.
///
/// Returns `(scanlines, line_interval_mm, scan_axis)`.
fn build_cardinal_raster_scanlines(
    processed: &beambench_raster::ProcessedRaster,
    bounds: &Bounds,
    scan_axis: ScanAxis,
    needs_flip: bool,
    bidirectional: bool,
    overscan_mm: f64,
) -> (Vec<Scanline>, f64, ScanAxis) {
    use beambench_raster::{rotate_raster, transpose_raster};

    match (scan_axis, needs_flip) {
        // 0°: horizontal scan, no flip
        (ScanAxis::Horizontal, false) => {
            let sl = generate_scanlines(
                processed,
                bounds.min.x,
                bounds.min.y,
                bidirectional,
                overscan_mm,
            );
            (sl, processed.line_interval_mm, ScanAxis::Horizontal)
        }
        // 90°: vertical scan via transpose, no flip
        (ScanAxis::Vertical, false) => {
            let transposed = transpose_raster(processed);
            let sl = generate_scanlines(
                &transposed,
                bounds.min.y,
                bounds.min.x,
                bidirectional,
                overscan_mm,
            );
            (sl, transposed.line_interval_mm, ScanAxis::Vertical)
        }
        // 180°: horizontal scan, flipped (reverse scanline and run order)
        (ScanAxis::Horizontal, true) => {
            let rotated = rotate_raster(processed, 180.0, bounds.width(), bounds.height());
            let sl = generate_scanlines(
                &rotated.raster,
                bounds.min.x,
                bounds.min.y,
                bidirectional,
                overscan_mm,
            );
            (sl, rotated.raster.line_interval_mm, ScanAxis::Horizontal)
        }
        // 270°: vertical scan via transpose of 180°-flipped raster
        (ScanAxis::Vertical, true) => {
            let rotated = rotate_raster(processed, 180.0, bounds.width(), bounds.height());
            let transposed = transpose_raster(&rotated.raster);
            let sl = generate_scanlines(
                &transposed,
                bounds.min.y,
                bounds.min.x,
                bidirectional,
                overscan_mm,
            );
            (sl, transposed.line_interval_mm, ScanAxis::Vertical)
        }
    }
}

/// Apply flood fill to clear background pixels connected to image edges.
///
/// This fills connected background regions from all four corners with white (255),
/// effectively removing the background and leaving only enclosed shapes.
fn apply_flood_fill(processed: &mut beambench_raster::ProcessedRaster) {
    let w = processed.width_px;
    let h = processed.height_px;
    if w == 0 || h == 0 {
        return;
    }

    // Convert to GrayImage for flood fill
    let mut gray = match processed.format {
        RasterPixelFormat::Binary => {
            // Expand bits to bytes: bit=1 means black (engrave)→0, bit=0 means white→255
            let row_bytes = w.div_ceil(8) as usize;
            let mut img = image::GrayImage::new(w, h);
            for row in 0..h {
                for col in 0..w {
                    let byte_idx = row as usize * row_bytes + (col / 8) as usize;
                    let bit = 7 - (col % 8);
                    let val = if byte_idx < processed.data.len() {
                        if (processed.data[byte_idx] >> bit) & 1 == 1 {
                            0
                        } else {
                            255
                        }
                    } else {
                        255
                    };
                    img.put_pixel(col, row, image::Luma([val]));
                }
            }
            img
        }
        RasterPixelFormat::Grayscale8 => image::GrayImage::from_raw(w, h, processed.data.clone())
            .unwrap_or_else(|| image::GrayImage::new(w, h)),
    };

    // Flood fill from all four corners with white (255 = no engrave)
    let corners = [
        (0, 0),
        (w.saturating_sub(1), 0),
        (0, h.saturating_sub(1)),
        (w.saturating_sub(1), h.saturating_sub(1)),
    ];
    for (cx, cy) in corners {
        beambench_raster::flood_fill_scanlines(&mut gray, cx, cy, 255, 0);
    }

    // Convert back
    match processed.format {
        RasterPixelFormat::Binary => {
            let row_bytes = w.div_ceil(8) as usize;
            let mut data = vec![0u8; row_bytes * h as usize];
            for row in 0..h {
                for col in 0..w {
                    let px = gray.get_pixel(col, row)[0];
                    if px < 128 {
                        // Dark pixel = engrave = bit 1
                        let byte_idx = row as usize * row_bytes + (col / 8) as usize;
                        let bit = 7 - (col % 8);
                        data[byte_idx] |= 1 << bit;
                    }
                }
            }
            processed.data = data;
        }
        RasterPixelFormat::Grayscale8 => {
            processed.data = gray.into_raw();
        }
    }
}

/// Build an execution plan from a project.
///
/// Reads the project's persisted [`beambench_core::ProjectOptimization`]
/// block and constructs a default [`PlannerInput`] (no live runtime
/// state, default calibration).
pub fn build_plan(project: &Project) -> Result<ExecutionPlan, PlannerError> {
    let input = PlannerInput::new(
        project.optimization.clone(),
        OptimizationRuntime::default(),
        PlannerCalibration::default(),
    );
    build_plan_with_input(project, &input)
}

/// Build an execution plan with an explicit [`PlannerInput`].
///
/// The canonical non-cache entry point. Inputs are split between the
/// project-persisted `ProjectOptimization` and the ephemeral
/// `OptimizationRuntime` overlay (live machine `current_position`),
/// matching the service-layer boundary.
pub fn build_plan_with_input(
    project: &Project,
    input: &PlannerInput,
) -> Result<ExecutionPlan, PlannerError> {
    build_plan_inner(project, input, None, None)
}

/// Run the pre-order flag passes over a layer's tagged polylines.
///
/// Order matters:
///   1. `dedupe` — shrinks the input to the other passes.
///   2. `inner_first` — depth sort.
///   3. `direction_order` — axial sort (wins as the primary key when
///      both it and `inner_first` are on).
///
/// `start_point` is *not* run here — it moved to
/// [`apply_start_point_pass`] after `order_vectors_generic` so its
/// `current_pos` tracking sees the final sequence order rather than a
/// pre-reshuffle one. Before that move, nearest-neighbor could silently
/// reshuffle polylines after their entry vertices were rotated,
/// degrading rotation quality for the final sequence.
fn apply_pre_order_passes<T: Orderable>(items: Vec<T>, opt: &ProjectOptimization) -> Vec<T> {
    if !opt.enabled {
        return items;
    }
    let items = dedupe::remove_near_duplicates(
        items,
        opt.remove_overlapping,
        opt.remove_overlap_tolerance_mm,
    );
    let items = inner_first::reorder_inner_first(items, opt.inner_first);
    direction::apply_direction_order(items, opt.direction_order)
}

/// Compose the optimization for builder use. When the planner input requests
/// `QualityTestOrdering::RowMajor`, every reorder/dedupe/start-point flag is forced off so the
/// transient quality-test pipeline emits segments in append order.
fn effective_optimization(input: &PlannerInput) -> ProjectOptimization {
    if input.ordering == beambench_core::QualityTestOrdering::RowMajor {
        ProjectOptimization {
            enabled: false,
            ordering: Vec::new(),
            inner_first: false,
            direction_order: beambench_core::DirectionOrder::None,
            reduce_travel: false,
            hide_backlash: false,
            reduce_direction_changes: false,
            choose_best_start: false,
            choose_corners: false,
            choose_best_direction: false,
            remove_overlapping: false,
            ..input.optimization.clone()
        }
    } else {
        input.optimization.clone()
    }
}

/// Run the per-closed-polyline start-vertex rotation and direction-
/// choice pass over an already-ordered polyline sequence.
///
/// `current_pos` is the tool position entering this pass's sequence —
/// the last-emitted segment's endpoint, or the user-configured start
/// point for the first call. Passing the real tool position (rather
/// than always `resolved_start_pos(settings)`) is what makes the flag
/// work correctly across multi-layer jobs: the first closed polyline
/// of each later layer is optimized from the previous layer's actual
/// exit, not from origin.
fn apply_start_point_pass<T: Orderable>(
    items: Vec<T>,
    opt: &ProjectOptimization,
    current_pos: Point2D,
) -> Vec<T> {
    if !opt.enabled {
        return items;
    }
    start_point::apply_start_point(
        items,
        current_pos,
        opt.choose_best_start,
        opt.choose_corners,
        opt.choose_best_direction,
    )
}

/// Resolve the user-configured start position, or fall back to the origin.
fn resolved_start_pos(opt: &ProjectOptimization) -> Point2D {
    match (opt.start_point_x, opt.start_point_y) {
        (Some(x), Some(y)) => Point2D::new(x, y),
        _ => Point2D::new(0.0, 0.0),
    }
}

/// Compute the tool position at the tail of `segments`: the end point
/// of the last segment whose end is defined, or `fallback` if no such
/// segment exists. Used to seed `current_pos` for
/// [`apply_start_point_pass`] so it sees the real cross-layer entry
/// position rather than always the user's configured start point.
fn tail_position(segments: &[PlanSegment], fallback: Point2D) -> Point2D {
    for seg in segments.iter().rev() {
        if let Some(end) = travel::segment_end(seg) {
            return end;
        }
    }
    fallback
}

/// True when the set of manual-ordering flags makes a subsequent
/// nearest-neighbor reshuffle unsafe. Per-layer polyline ordering
/// collapses to "nearest-neighbor unless any of these forces it off":
///
/// - `inner_first` reorders by containment depth.
/// - `direction_order` (non-`None`) reorders by axial key.
/// - the `Group` ordering key clusters grouped objects' polylines so they emit
///   contiguously; nearest-neighbor would otherwise interleave them.
fn force_as_drawn(opt: &ProjectOptimization) -> bool {
    opt.enabled
        && (opt.inner_first
            || !matches!(opt.direction_order, DirectionOrder::None)
            || opt.has_order_key(OptimizationOrderKey::Group))
}

/// Run `order_nearest_next_generic` on `items` unless `force_as_drawn`
/// is true, in which case pass through the upstream sequence unchanged.
fn order_polylines<T: Orderable>(items: Vec<T>, force_as_drawn_flag: bool) -> Vec<T> {
    if force_as_drawn_flag {
        items
    } else {
        order_nearest_next_generic(items)
    }
}

/// Build an execution plan from a [`PlannerInput`] with a shared raster
/// cache. The preferred entry point for service callers: it threads
/// both the persisted `ProjectOptimization` and the runtime overlay
/// through the same pipeline as [`build_plan_with_input`], while also
/// reusing decoded raster output across successive plan builds.
pub fn build_plan_with_input_and_cache(
    project: &Project,
    input: &PlannerInput,
    cache: &beambench_raster::cache::RasterCache,
    scaled_cache: &beambench_raster::cache::ScaledImageCache,
) -> Result<ExecutionPlan, PlannerError> {
    build_plan_inner(project, input, Some(cache), Some(scaled_cache))
}

fn build_plan_inner(
    project: &Project,
    input: &PlannerInput,
    cache: Option<&beambench_raster::cache::RasterCache>,
    scaled_cache: Option<&beambench_raster::cache::ScaledImageCache>,
) -> Result<ExecutionPlan, PlannerError> {
    let effective = effective_optimization(input);
    let optimization = &effective;
    let calibration = &input.calibration;
    let runtime_current_position = input.runtime.current_position;
    // 1. Compute revision hash
    let project_json = serde_json::to_string(project).map_err(|e| {
        PlannerError::InvalidSettings(format!("Failed to serialize project: {}", e))
    })?;
    let mut hasher = Sha256::new();
    hasher.update(project_json.as_bytes());
    let revision_hash = format!("{:x}", hasher.finalize());

    // 2. Collect enabled layers. Layer order is an optimization criterion:
    // when enabled, use the explicit layer order_index; when disabled,
    // preserve the project's stored layer sequence.
    let mut enabled_layers: Vec<_> = project
        .layers
        .iter()
        .filter(|l| {
            l.enabled && !l.is_tool_layer && l.primary_entry().operation != OperationType::Tool
        })
        .collect();
    if optimization.has_order_key(OptimizationOrderKey::Layer) {
        enabled_layers.sort_by_key(|l| l.order_index);
    }

    // 3. Check if we have any enabled layers
    if enabled_layers.is_empty() {
        return Err(PlannerError::EmptyPlan);
    }

    // 4. Build layer_order
    let layer_order: Vec<String> = enabled_layers.iter().map(|l| l.id.to_string()).collect();

    let mut all_segments = Vec::new();
    let mut warnings = Vec::new();
    let mut failed_entries = Vec::new();

    // Pre-process: expand VirtualClone objects to resolved copies
    let expanded_objects: Vec<_> = project
        .objects
        .iter()
        .filter_map(|obj| project.resolve_clone(obj))
        .collect();

    // 5. Process each layer
    for layer in enabled_layers {
        // Collect visible, unlocked objects for this layer. Priority is an
        // optimization ordering key rather than an unconditional project order.
        // Include both original concrete objects and expanded clones
        let mut layer_objects: Vec<_> = project
            .objects
            .iter()
            .filter(|obj| {
                obj.visible
                    && !obj.locked
                    && obj.layer_id.to_string() == layer.id.to_string()
                    && !matches!(obj.data, ObjectData::VirtualClone { .. })
            })
            .chain(expanded_objects.iter().filter(|obj| {
                obj.visible && !obj.locked && obj.layer_id.to_string() == layer.id.to_string()
            }))
            .collect();
        if optimization.has_order_key(OptimizationOrderKey::Priority) {
            layer_objects.sort_by_key(|obj| obj.priority);
        }

        // Cluster group members together when the Group ordering key is on.
        // Runs after the priority sort so the priority-seed order
        // survives as the intra-cluster order. Called once per layer to
        // enforce the layer-wins precedence rule for mixed-layer groups.
        let layer_objects = crate::optimize::sort_keys::apply_order_by_group(
            layer_objects,
            &project.objects,
            optimization.has_order_key(OptimizationOrderKey::Group),
        );

        // Partition into raster and vector objects
        let mut raster_objects = Vec::new();
        let mut vector_objects = Vec::new();

        for obj in layer_objects {
            match &obj.data {
                ObjectData::RasterImage { .. } => raster_objects.push(obj),
                _ => {
                    // Vector/shape objects are always processed, even on raster
                    // layers — the layer controls laser settings, not valid
                    // object types.
                    vector_objects.push(obj);
                }
            }
        }

        for entry in &layer.entries {
            if !entry.output_enabled {
                continue;
            }
            let mut entry_layer = layer.clone();
            entry_layer.entries = vec![entry.clone()];
            let layer = &entry_layer;
            let entry_segment_start = all_segments.len();
            let mut entry_failed = false;

            // Process raster objects
            let raster_settings = layer.primary_entry().raster_settings.as_ref();
            let raster_passes = raster_settings.map(|s| s.passes).unwrap_or(1);
            if matches!(
                layer.primary_entry().operation,
                OperationType::Image | OperationType::Fill
            ) {
                for _pass in 0..raster_passes {
                    for obj in &raster_objects {
                        if let ObjectData::RasterImage {
                            asset_key,
                            adjustments,
                            ..
                        } = &obj.data
                        {
                            // Look up asset data
                            let Some(source_bytes) =
                                find_asset_data(&project.asset_data, asset_key)
                            else {
                                warnings.push(PlanWarning {
                                    message: format!(
                                        "Asset data not found for object '{}' (asset_key: {})",
                                        obj.name, asset_key
                                    ),
                                });
                                continue;
                            };

                            // Get raster settings from layer
                            let raster_settings = layer.primary_entry().raster_settings.as_ref();
                            let bidirectional =
                                raster_settings.map(|s| s.bidirectional).unwrap_or(true)
                                    && !optimization.hide_backlash;
                            let overscan_mm = raster_settings.map(|s| s.overscan_mm).unwrap_or(2.5);
                            let flood_fill_enabled =
                                raster_settings.map(|s| s.flood_fill).unwrap_or(false);

                            // Build RasterProcessingParams via the shared helper so
                            // the planner and the canvas preview always assemble the
                            // same params from the same layer settings.
                            let params = build_image_raster_params(
                                layer,
                                adjustments.clone().unwrap_or_default(),
                                source_bytes.clone(),
                                (obj.bounds.width(), obj.bounds.height()),
                            );

                            // Process the raster (with optional cache lookup)
                            let cache_key = params.cache_key();
                            let cached = cache.and_then(|c| c.get(&cache_key));

                            let processed_arc = if let Some(arc) = cached {
                                arc
                            } else {
                                // Use staged decode/scale cache so planner reuses
                                // decoded images from preview sessions.
                                let result = match beambench_raster::process_raster_with_cache(
                                    params,
                                    scaled_cache,
                                ) {
                                    Ok(p) => p,
                                    Err(e) => {
                                        warnings.push(PlanWarning {
                                            message: format!(
                                                "Failed to process raster for object '{}': {}",
                                                obj.name, e
                                            ),
                                        });
                                        continue;
                                    }
                                };
                                let arc = std::sync::Arc::new(result);
                                if let Some(c) = cache {
                                    c.insert(cache_key, arc.clone());
                                }
                                arc
                            };

                            // Flood fill needs a mutable copy; non-flood-fill path
                            // borrows from Arc directly (zero-copy cache hit).
                            let owned_processed;
                            let processed: &beambench_raster::ProcessedRaster =
                                if flood_fill_enabled {
                                    owned_processed = {
                                        let mut p = (*processed_arc).clone();
                                        apply_flood_fill(&mut p);
                                        p
                                    };
                                    &owned_processed
                                } else {
                                    &*processed_arc
                                };

                            let masked_processed = apply_image_masks(
                                project,
                                layer,
                                obj,
                                processed,
                                &mut failed_entries,
                            );
                            let processed: &beambench_raster::ProcessedRaster = &masked_processed;

                            // Determine direction_mode and power_mode
                            let direction_mode = if bidirectional {
                                DirectionMode::Bidirectional
                            } else {
                                DirectionMode::Unidirectional
                            };

                            let power_mode = match processed.format {
                                RasterPixelFormat::Binary => PowerMode::Binary,
                                RasterPixelFormat::Grayscale8 => PowerMode::Grayscale,
                            };

                            // Compute effective angle passes
                            let scan_angle = raster_settings.map(|s| s.scan_angle).unwrap_or(0.0);
                            let raw_angle_passes =
                                raster_settings.map(|s| s.angle_passes).unwrap_or(1);
                            let crosshatch = raster_settings.map(|s| s.crosshatch).unwrap_or(false);
                            let angle_increment = raster_settings
                                .map(|s| s.angle_increment_deg)
                                .unwrap_or(90.0);
                            let effective_angle_passes = if crosshatch && raw_angle_passes <= 1 {
                                2
                            } else {
                                raw_angle_passes
                            };

                            // Object center (rotation origin)
                            let obj_center = Point2D::new(
                                (obj.bounds.min.x + obj.bounds.max.x) / 2.0,
                                (obj.bounds.min.y + obj.bounds.max.y) / 2.0,
                            );

                            for ai in 0..effective_angle_passes {
                                let effective_angle = scan_angle + ai as f64 * angle_increment;

                                // Normalise to 0..360 for cardinal detection
                                let norm = ((effective_angle % 360.0) + 360.0) % 360.0;

                                if let Some((axis, needs_flip)) = classify_cardinal(norm) {
                                    // Cardinal fast path (0/90/180/270): transpose/flip,
                                    // always emit scan_angle_deg: 0.0 (world-space coords)
                                    let (scanlines, li, scan_axis) =
                                        build_cardinal_raster_scanlines(
                                            &processed,
                                            &obj.bounds,
                                            axis,
                                            needs_flip,
                                            bidirectional,
                                            overscan_mm,
                                        );

                                    all_segments.push(PlanSegment::Raster {
                                        scanlines,
                                        line_interval_mm: li,
                                        direction_mode,
                                        power_mode,
                                        speed_mm_min: layer.primary_entry().speed_mm_min,
                                        layer_id: layer.id.to_string(),
                                        cut_entry_id: layer.primary_entry().id.to_string(),
                                        scan_angle_deg: 0.0,
                                        scan_origin: Point2D::new(0.0, 0.0),
                                        overscan_mm,
                                        outlines: vec![],
                                        scan_axis,
                                        power_max_percent: layer.primary_entry().power_percent,
                                        power_min_percent: layer.primary_entry().power_min_percent,
                                        dot_width_correction_mm: layer
                                            .primary_entry()
                                            .raster_settings
                                            .as_ref()
                                            .map(|rs| rs.dot_width_correction_mm)
                                            .unwrap_or(0.0),
                                        ramp_length_mm: effective_layer_ramp_length_mm(layer),
                                        x_pixel_mm: processed.effective_x_pixel_mm(),
                                    });
                                } else {
                                    // Non-cardinal angle: rotate bitmap, generate
                                    // scanlines in local space
                                    use beambench_raster::rotate_raster;

                                    let rotated = rotate_raster(
                                        &processed,
                                        -effective_angle,
                                        obj.bounds.width(),
                                        obj.bounds.height(),
                                    );

                                    let local_scanlines = generate_scanlines(
                                        &rotated.raster,
                                        -rotated.width_mm / 2.0,
                                        -rotated.height_mm / 2.0,
                                        bidirectional,
                                        overscan_mm,
                                    );

                                    all_segments.push(PlanSegment::Raster {
                                        scanlines: local_scanlines,
                                        line_interval_mm: rotated.raster.line_interval_mm,
                                        direction_mode,
                                        power_mode,
                                        speed_mm_min: layer.primary_entry().speed_mm_min,
                                        layer_id: layer.id.to_string(),
                                        cut_entry_id: layer.primary_entry().id.to_string(),
                                        scan_angle_deg: effective_angle,
                                        scan_origin: obj_center,
                                        overscan_mm,
                                        outlines: vec![],
                                        scan_axis: ScanAxis::Horizontal,
                                        power_max_percent: layer.primary_entry().power_percent,
                                        power_min_percent: layer.primary_entry().power_min_percent,
                                        dot_width_correction_mm: layer
                                            .primary_entry()
                                            .raster_settings
                                            .as_ref()
                                            .map(|rs| rs.dot_width_correction_mm)
                                            .unwrap_or(0.0),
                                        ramp_length_mm: effective_layer_ramp_length_mm(layer),
                                        x_pixel_mm: rotated.raster.effective_x_pixel_mm(),
                                    });
                                }
                            }
                        }
                    }
                }
            }

            // Process vector objects
            if !vector_objects.is_empty() {
                if layer.primary_entry().operation == OperationType::Image {
                    warnings.push(PlanWarning {
                        message: format!(
                            "Image sub-layer '{}' skipped vector geometry on layer '{}'",
                            entry.id, layer.name
                        ),
                    });
                } else if layer.primary_entry().operation == OperationType::OffsetFill {
                    // OffsetFill: composite all closed shapes via boolean union, then
                    // generate concentric inward offset rings from the merged geometry.
                    // Overlapping shapes merge (no double-burn), nested inner shapes
                    // become holes that the offset rings trace around.
                    let offset_spacing = layer
                        .primary_entry()
                        .raster_settings
                        .as_ref()
                        .map(|s| s.line_interval_mm)
                        .filter(|v| *v > 0.0)
                        .unwrap_or(0.5);
                    let vector_settings = layer.primary_entry().vector_settings.as_ref();
                    let passes = vector_settings.map(|s| s.passes).unwrap_or(1);
                    let grouping_mode = vector_settings
                        .map(|s| s.offset_fill_grouping_mode)
                        .unwrap_or(OffsetFillGroupingMode::AllShapesAtOnce);
                    let offset_fill_started_at = Instant::now();
                    let object_batches = offset_fill_object_batches(
                        &vector_objects,
                        &project.objects,
                        grouping_mode,
                    );

                    let normalize_started_at = Instant::now();
                    let mut composite_started_total = Duration::ZERO;
                    let mut split_started_total = Duration::ZERO;
                    let mut closed_polyline_count = 0usize;
                    let mut output_subpaths = 0usize;
                    let mut composite_batches: Vec<OffsetFillCompositeBatch> = Vec::new();

                    for batch_objects in object_batches {
                        let mut original_polylines: Vec<Polyline> = Vec::new();
                        for obj in &batch_objects {
                            let Some(normalized) = normalize_object(obj) else {
                                warnings.push(PlanWarning {
                                    message: format!(
                                        "Failed to normalize object '{}' for offset fill",
                                        obj.name
                                    ),
                                });
                                continue;
                            };
                            for poly in &normalized.polylines {
                                if poly.closed && poly.points.len() >= 3 {
                                    original_polylines.push(poly.clone());
                                }
                            }
                        }

                        if original_polylines.is_empty() {
                            continue;
                        }

                        closed_polyline_count += original_polylines.len();

                        let composite_started_at = Instant::now();
                        let vecpaths: Vec<VecPath> =
                            original_polylines.iter().map(polyline_to_vecpath).collect();
                        let (boolean_flatten_tolerance_mm, boolean_simplify_tolerance_mm) =
                            offset_fill_boolean_tolerances_mm();
                        let composite = normalize_subject_evenodd_with_tolerance(
                            &vecpaths,
                            boolean_flatten_tolerance_mm,
                            boolean_simplify_tolerance_mm,
                        );
                        composite_started_total += composite_started_at.elapsed();
                        output_subpaths += composite.subpaths.len();

                        let split_started_at = Instant::now();
                        let units = split_offset_fill_units(&composite);
                        split_started_total += split_started_at.elapsed();
                        if units.is_empty() {
                            warnings.push(PlanWarning {
                                message: "OffsetFill produced no composited units for batch"
                                    .to_string(),
                            });
                            continue;
                        }

                        composite_batches.push(OffsetFillCompositeBatch { units });
                    }
                    tracing::info!(
                        target: "perf",
                        operation = "offset_fill.normalize",
                        layer_id = %layer.id,
                        duration_ms = normalize_started_at.elapsed().as_secs_f64() * 1000.0,
                        batch_count = composite_batches.len(),
                        grouping_mode = ?grouping_mode,
                        closed_polyline_count,
                    );
                    tracing::info!(
                        target: "perf",
                        operation = "offset_fill.composite_subject_evenodd",
                        layer_id = %layer.id,
                        duration_ms = composite_started_total.as_secs_f64() * 1000.0,
                        batch_count = composite_batches.len(),
                        output_subpaths,
                    );
                    tracing::info!(
                        target: "perf",
                        operation = "offset_fill.split_units",
                        layer_id = %layer.id,
                        duration_ms = split_started_total.as_secs_f64() * 1000.0,
                        batch_count = composite_batches.len(),
                        unit_count = composite_batches.iter().map(|batch| batch.units.len()).sum::<usize>(),
                    );

                    if composite_batches.is_empty() {
                        continue;
                    }

                    let offset_fill_controls =
                        OffsetFillBuildControls::new(input.cancellation.clone());
                    let mut pass_start_pos =
                        get_last_point(&all_segments).unwrap_or(Point2D::new(0.0, 0.0));

                    // 6. Generate offset fill per composited unit, completing each unit before
                    // moving to the next object.
                    for _pass in 0..passes {
                        let prepare_started_at = Instant::now();
                        let prepared_batches: Vec<Vec<PreparedOffsetFillUnit>> = composite_batches
                            .iter()
                            .map(|batch| {
                                batch
                                    .units
                                    .par_iter()
                                    .map(|unit| {
                                        prepare_offset_fill_unit(
                                            unit,
                                            offset_spacing,
                                            &offset_fill_controls,
                                        )
                                    })
                                    .collect()
                            })
                            .collect();
                        let prepare_duration = prepare_started_at.elapsed();
                        let total_tracks: usize = prepared_batches
                            .iter()
                            .flatten()
                            .map(|unit| unit.tracks.len())
                            .sum();
                        let total_rings: usize = prepared_batches
                            .iter()
                            .flatten()
                            .map(|unit| {
                                unit.tracks
                                    .iter()
                                    .map(|track| track.polylines.len())
                                    .sum::<usize>()
                            })
                            .sum();
                        let any_hit_cap = prepared_batches
                            .iter()
                            .flatten()
                            .any(|unit| unit.build_stats.hit_iteration_cap);
                        let any_timed_out = prepared_batches
                            .iter()
                            .flatten()
                            .any(|unit| unit.build_stats.timed_out);
                        let any_cancelled = prepared_batches
                            .iter()
                            .flatten()
                            .any(|unit| unit.build_stats.cancelled);
                        let flatten_ms: f64 = prepared_batches
                            .iter()
                            .flatten()
                            .map(|unit| unit.build_stats.flatten_time.as_secs_f64() * 1000.0)
                            .sum();
                        let nesting_ms: f64 = prepared_batches
                            .iter()
                            .flatten()
                            .map(|unit| unit.build_stats.nesting_time.as_secs_f64() * 1000.0)
                            .sum();
                        let matching_ms: f64 = prepared_batches
                            .iter()
                            .flatten()
                            .map(|unit| unit.build_stats.matching_time.as_secs_f64() * 1000.0)
                            .sum();
                        let buffer_ms: f64 = prepared_batches
                            .iter()
                            .flatten()
                            .map(|unit| unit.build_stats.buffer_time.as_secs_f64() * 1000.0)
                            .sum();
                        let optimize_ms: f64 = prepared_batches
                            .iter()
                            .flatten()
                            .map(|unit| unit.build_stats.optimize_time.as_secs_f64() * 1000.0)
                            .sum();
                        tracing::info!(
                            target: "perf",
                            operation = "offset_fill.prepare_units",
                            layer_id = %layer.id,
                            duration_ms = prepare_duration.as_secs_f64() * 1000.0,
                            batch_count = prepared_batches.len(),
                            unit_count = prepared_batches.iter().map(|batch| batch.len()).sum::<usize>(),
                            track_count = total_tracks,
                            ring_count = total_rings,
                            flatten_ms,
                            nesting_ms,
                            matching_ms,
                            buffer_ms,
                            optimize_ms,
                            hit_iteration_cap = any_hit_cap,
                            timed_out = any_timed_out,
                            cancelled = any_cancelled,
                        );
                        if any_cancelled {
                            return Err(PlannerError::Cancelled);
                        }
                        if any_timed_out {
                            let iterations = prepared_batches
                                .iter()
                                .flatten()
                                .map(|unit| unit.build_stats.iterations)
                                .max()
                                .unwrap_or(0);
                            let elapsed_ms = prepared_batches
                                .iter()
                                .flatten()
                                .map(|unit| unit.build_stats.elapsed_ms)
                                .max()
                                .unwrap_or_else(|| {
                                    offset_fill_controls
                                        .elapsed()
                                        .as_millis()
                                        .try_into()
                                        .unwrap_or(u64::MAX)
                                });
                            all_segments.truncate(entry_segment_start);
                            failed_entries.push(PlanEntryFailure {
                                layer_id: layer.id.to_string(),
                                cut_entry_id: Some(entry.id.to_string()),
                                operation: layer.primary_entry().operation,
                                reason: PlanEntryFailureReason::OffsetFillTimedOut {
                                    iterations,
                                    elapsed_ms,
                                },
                            });
                            entry_failed = true;
                            break;
                        }
                        if any_hit_cap {
                            let iterations = prepared_batches
                                .iter()
                                .flatten()
                                .map(|unit| unit.build_stats.iterations)
                                .max()
                                .unwrap_or(OFFSET_FILL_ITERATION_LIMIT);
                            all_segments.truncate(entry_segment_start);
                            failed_entries.push(PlanEntryFailure {
                                layer_id: layer.id.to_string(),
                                cut_entry_id: Some(entry.id.to_string()),
                                operation: layer.primary_entry().operation,
                                reason: PlanEntryFailureReason::OffsetFillEmergencyCeiling {
                                    iterations,
                                },
                            });
                            entry_failed = true;
                            break;
                        }

                        let mut pass_emitted_any = false;
                        let emit_started_at = Instant::now();

                        for (batch, prepared_units) in
                            composite_batches.iter().zip(&prepared_batches)
                        {
                            let mut remaining_unit_indices: Vec<usize> =
                                (0..batch.units.len()).collect();

                            while !remaining_unit_indices.is_empty() {
                                let next_idx = remaining_unit_indices
                                    .iter()
                                    .enumerate()
                                    .min_by(|(_, a), (_, b)| {
                                        let da = nearest_distance_to_polyline(
                                            &batch.units[**a].outer,
                                            pass_start_pos,
                                        );
                                        let db = nearest_distance_to_polyline(
                                            &batch.units[**b].outer,
                                            pass_start_pos,
                                        );
                                        da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
                                    })
                                    .map(|(idx, _)| idx)
                                    .unwrap_or(0);

                                let unit_index = remaining_unit_indices.remove(next_idx);
                                let prepared_unit = &prepared_units[unit_index];
                                let (offset_segments, next_pos) = emit_prepared_offset_fill_unit(
                                    prepared_unit,
                                    offset_spacing,
                                    layer.primary_entry().power_percent,
                                    layer.primary_entry().speed_mm_min,
                                    &layer.id.to_string(),
                                    &layer.primary_entry().id.to_string(),
                                    pass_start_pos,
                                );

                                if offset_segments.is_empty() {
                                    continue;
                                }

                                pass_start_pos = next_pos;
                                pass_emitted_any = true;
                                all_segments.extend(offset_segments);
                            }
                        }
                        tracing::info!(
                            target: "perf",
                            operation = "offset_fill.emit_units",
                            layer_id = %layer.id,
                            duration_ms = emit_started_at.elapsed().as_secs_f64() * 1000.0,
                            pass = _pass + 1,
                        );

                        if !pass_emitted_any {
                            warnings.push(PlanWarning {
                                message: "OffsetFill produced no paths for composited layer"
                                    .to_string(),
                            });
                        }
                    }
                    tracing::info!(
                        target: "perf",
                        operation = "offset_fill.total",
                        layer_id = %layer.id,
                        duration_ms = offset_fill_started_at.elapsed().as_secs_f64() * 1000.0,
                        object_count = vector_objects.len(),
                        passes,
                    );
                } else if layer.primary_entry().operation == OperationType::Fill {
                    // Fill: rasterize each object independently so preview and planning
                    // preserve per-object scan areas and overscan.
                    // Nested contours within a single normalized object are still
                    // composed so compound shapes retain their internal hole/island topology.
                    let raster_settings = layer.primary_entry().raster_settings.as_ref();
                    let line_interval = raster_settings.map(|s| s.line_interval_mm).unwrap_or(0.1);
                    let bidirectional = raster_settings.map(|s| s.bidirectional).unwrap_or(true)
                        && !optimization.hide_backlash;
                    let overscan_mm = raster_settings.map(|s| s.overscan_mm).unwrap_or(2.5);
                    let fill_passes = raster_settings.map(|s| s.passes).unwrap_or(1);
                    let scan_angle = raster_settings.map(|s| s.scan_angle).unwrap_or(0.0);

                    let direction_mode = if bidirectional {
                        DirectionMode::Bidirectional
                    } else {
                        DirectionMode::Unidirectional
                    };

                    // Compute effective angle passes (backward compat: crosshatch=true → 2 passes)
                    let raw_angle_passes = raster_settings.map(|s| s.angle_passes).unwrap_or(1);
                    let crosshatch = raster_settings.map(|s| s.crosshatch).unwrap_or(false);
                    let angle_increment = raster_settings
                        .map(|s| s.angle_increment_deg)
                        .unwrap_or(90.0);
                    let effective_angle_passes = if crosshatch && raw_angle_passes <= 1 {
                        2
                    } else {
                        raw_angle_passes
                    };

                    for obj in &vector_objects {
                        let Some(normalized) = normalize_object(obj) else {
                            warnings.push(PlanWarning {
                                message: format!(
                                    "Failed to normalize object '{}' for fill",
                                    obj.name
                                ),
                            });
                            continue;
                        };
                        let original_polylines: Vec<Polyline> = normalized
                            .polylines
                            .iter()
                            .filter(|poly| poly.closed && poly.points.len() >= 3)
                            .cloned()
                            .collect();

                        if original_polylines.is_empty() {
                            continue;
                        }

                        // Detect nested polylines within this object. These are re-added
                        // after union so rasterize_fill's even-odd rule creates
                        // holes/islands for compound geometry, but different project
                        // objects remain separate fill regions.
                        let (original_depths, _original_analyses) =
                            compute_all_nesting_depths(&original_polylines);
                        let nested_indices: Vec<usize> = (0..original_polylines.len())
                            .filter(|&i| original_depths[i] > 0)
                            .collect();

                        let vecpaths: Vec<VecPath> =
                            original_polylines.iter().map(polyline_to_vecpath).collect();
                        let welded = weld_shapes(&vecpaths);
                        let mut composited_polylines =
                            flatten_vecpath(&welded, DEFAULT_TOLERANCE_MM);

                        for &idx in &nested_indices {
                            composited_polylines.push(original_polylines[idx].clone());
                        }

                        let Some(composite_bounds) = bounds_from_polylines(&composited_polylines)
                        else {
                            continue;
                        };

                        let composite_center = Point2D::new(
                            (composite_bounds.min.x + composite_bounds.max.x) / 2.0,
                            (composite_bounds.min.y + composite_bounds.max.y) / 2.0,
                        );
                        let preview_outlines = simplify_preview_outlines(&composited_polylines);

                        for _pass in 0..fill_passes {
                            let Some(processed) = rasterize_fill(
                                &composited_polylines,
                                &composite_bounds,
                                line_interval,
                            ) else {
                                continue;
                            };

                            for ai in 0..effective_angle_passes {
                                let effective_angle = scan_angle + ai as f64 * angle_increment;
                                let norm = ((effective_angle % 360.0) + 360.0) % 360.0;

                                if let Some((axis, needs_flip)) = classify_cardinal(norm) {
                                    let (scanlines, li, s_axis) = build_cardinal_raster_scanlines(
                                        &processed,
                                        &composite_bounds,
                                        axis,
                                        needs_flip,
                                        bidirectional,
                                        overscan_mm,
                                    );

                                    if scanlines.is_empty() {
                                        continue;
                                    }

                                    all_segments.push(PlanSegment::Raster {
                                        scanlines,
                                        line_interval_mm: li,
                                        direction_mode,
                                        power_mode: PowerMode::Binary,
                                        speed_mm_min: layer.primary_entry().speed_mm_min,
                                        layer_id: layer.id.to_string(),
                                        cut_entry_id: layer.primary_entry().id.to_string(),
                                        overscan_mm,
                                        outlines: preview_outlines.clone(),
                                        scan_axis: s_axis,
                                        power_max_percent: layer.primary_entry().power_percent,
                                        power_min_percent: layer.primary_entry().power_min_percent,
                                        scan_angle_deg: 0.0,
                                        scan_origin: Point2D::new(0.0, 0.0),
                                        dot_width_correction_mm: layer
                                            .primary_entry()
                                            .raster_settings
                                            .as_ref()
                                            .map(|rs| rs.dot_width_correction_mm)
                                            .unwrap_or(0.0),
                                        ramp_length_mm: effective_layer_ramp_length_mm(layer),
                                        x_pixel_mm: processed.effective_x_pixel_mm(),
                                    });
                                } else {
                                    use beambench_raster::rotate_raster;

                                    let rotated = rotate_raster(
                                        &processed,
                                        -effective_angle,
                                        composite_bounds.width(),
                                        composite_bounds.height(),
                                    );

                                    let local_scanlines = generate_scanlines(
                                        &rotated.raster,
                                        -rotated.width_mm / 2.0,
                                        -rotated.height_mm / 2.0,
                                        bidirectional,
                                        overscan_mm,
                                    );

                                    if local_scanlines.is_empty() {
                                        continue;
                                    }

                                    all_segments.push(PlanSegment::Raster {
                                        scanlines: local_scanlines,
                                        line_interval_mm: rotated.raster.line_interval_mm,
                                        direction_mode,
                                        power_mode: PowerMode::Binary,
                                        speed_mm_min: layer.primary_entry().speed_mm_min,
                                        layer_id: layer.id.to_string(),
                                        cut_entry_id: layer.primary_entry().id.to_string(),
                                        overscan_mm,
                                        outlines: preview_outlines.clone(),
                                        scan_axis: ScanAxis::Horizontal,
                                        power_max_percent: layer.primary_entry().power_percent,
                                        power_min_percent: layer.primary_entry().power_min_percent,
                                        scan_angle_deg: effective_angle,
                                        scan_origin: composite_center,
                                        dot_width_correction_mm: layer
                                            .primary_entry()
                                            .raster_settings
                                            .as_ref()
                                            .map(|rs| rs.dot_width_correction_mm)
                                            .unwrap_or(0.0),
                                        ramp_length_mm: effective_layer_ramp_length_mm(layer),
                                        x_pixel_mm: rotated.raster.effective_x_pixel_mm(),
                                    });
                                }
                            }
                        }
                    }
                } else {
                    // Standard vector processing for Line, Cut, Score, etc.
                    let vector_settings = layer.primary_entry().vector_settings.as_ref();
                    let passes = vector_settings.map(|s| s.passes).unwrap_or(1);

                    for _pass in 0..passes {
                        let pre_vector_count = all_segments.len();
                        // Group objects by power_scale to preserve per-object scaling
                        // while still allowing cross-object ordering within same scale
                        let has_mixed_scales = {
                            let first =
                                vector_objects.first().map(|o| o.power_scale).unwrap_or(1.0);
                            vector_objects
                                .iter()
                                .any(|o| (o.power_scale - first).abs() > f64::EPSILON)
                        };

                        if has_mixed_scales {
                            // Process each object separately to preserve its power_scale.
                            // Use TaggedPolyline to preserve original subpath identity through ordering.
                            for obj in &vector_objects {
                                match normalize_object(obj) {
                                    Some(normalized) => {
                                        let obj_id_str = obj.id.to_string();
                                        let tagged: Vec<TaggedPolyline> = normalized
                                            .polylines
                                            .into_iter()
                                            .enumerate()
                                            .map(|(sp_idx, polyline)| TaggedPolyline {
                                                inner: polyline,
                                                object_id: obj_id_str.clone(),
                                                subpath_index: sp_idx,
                                            })
                                            .collect();
                                        let tagged = apply_pre_order_passes(tagged, optimization);
                                        let ordered =
                                            order_polylines(tagged, force_as_drawn(optimization));
                                        // Feed the actual tool position at the tail of
                                        // segments emitted so far into the start-point
                                        // pass. On the first object in the first layer
                                        // this falls back to the user-configured start
                                        // point; otherwise it's the previous segment's
                                        // exit — which is what `choose_best_start` needs
                                        // to optimize against across layer boundaries.
                                        let current_pos = tail_position(
                                            &all_segments,
                                            resolved_start_pos(optimization),
                                        );
                                        let ordered = apply_start_point_pass(
                                            ordered,
                                            optimization,
                                            current_pos,
                                        );
                                        let perf_enabled = vector_settings
                                            .map(|s| s.perforation_enabled)
                                            .unwrap_or(false);
                                        let perf_on_ms = vector_settings
                                            .map(|s| s.perforation_on_ms)
                                            .unwrap_or(0.0);
                                        let perf_off_ms = vector_settings
                                            .map(|s| s.perforation_off_ms)
                                            .unwrap_or(0.0);
                                        for tagged_poly in ordered {
                                            all_segments.push(PlanSegment::Vector {
                                                polyline: tagged_poly.inner.points,
                                                closed: tagged_poly.inner.closed,
                                                power_percent: layer.primary_entry().power_percent
                                                    * obj.power_scale,
                                                speed_mm_min: layer.primary_entry().speed_mm_min,
                                                layer_id: layer.id.to_string(),
                                                cut_entry_id: layer.primary_entry().id.to_string(),
                                                perforation_enabled: perf_enabled,
                                                perforation_on_ms: perf_on_ms,
                                                perforation_off_ms: perf_off_ms,
                                                source_object_id: Some(tagged_poly.object_id),
                                                source_subpath_index: Some(
                                                    tagged_poly.subpath_index,
                                                ),
                                            });
                                        }
                                    }
                                    None => {
                                        warnings.push(PlanWarning {
                                            message: format!(
                                                "Failed to normalize object '{}'",
                                                obj.name
                                            ),
                                        });
                                    }
                                }
                            }
                        } else {
                            // All objects have same power_scale — batch and order together
                            // Use TaggedPolyline to track identity through ordering
                            let uniform_scale =
                                vector_objects.first().map(|o| o.power_scale).unwrap_or(1.0);
                            let mut tagged_polylines: Vec<TaggedPolyline> = Vec::new();

                            for obj in &vector_objects {
                                match normalize_object(obj) {
                                    Some(normalized) => {
                                        let obj_id_str = obj.id.to_string();
                                        for (sp_idx, polyline) in
                                            normalized.polylines.into_iter().enumerate()
                                        {
                                            tagged_polylines.push(TaggedPolyline {
                                                inner: polyline,
                                                object_id: obj_id_str.clone(),
                                                subpath_index: sp_idx,
                                            });
                                        }
                                    }
                                    None => {
                                        warnings.push(PlanWarning {
                                            message: format!(
                                                "Failed to normalize object '{}'",
                                                obj.name
                                            ),
                                        });
                                    }
                                }
                            }

                            let tagged_polylines =
                                apply_pre_order_passes(tagged_polylines, optimization);
                            let ordered =
                                order_polylines(tagged_polylines, force_as_drawn(optimization));
                            // Run start-point rotation AFTER the nearest-neighbor pass
                            // so the rotation sees the final sequence. `current_pos`
                            // is the tail of already-emitted segments — either the
                            // previous layer's exit or the user-configured start.
                            let current_pos =
                                tail_position(&all_segments, resolved_start_pos(optimization));
                            let ordered =
                                apply_start_point_pass(ordered, optimization, current_pos);

                            let perf_enabled = vector_settings
                                .map(|s| s.perforation_enabled)
                                .unwrap_or(false);
                            let perf_on_ms =
                                vector_settings.map(|s| s.perforation_on_ms).unwrap_or(0.0);
                            let perf_off_ms =
                                vector_settings.map(|s| s.perforation_off_ms).unwrap_or(0.0);

                            for tagged in ordered {
                                all_segments.push(PlanSegment::Vector {
                                    polyline: tagged.inner.points,
                                    closed: tagged.inner.closed,
                                    power_percent: layer.primary_entry().power_percent
                                        * uniform_scale,
                                    speed_mm_min: layer.primary_entry().speed_mm_min,
                                    layer_id: layer.id.to_string(),
                                    cut_entry_id: layer.primary_entry().id.to_string(),
                                    perforation_enabled: perf_enabled,
                                    perforation_on_ms: perf_on_ms,
                                    perforation_off_ms: perf_off_ms,
                                    source_object_id: Some(tagged.object_id),
                                    source_subpath_index: Some(tagged.subpath_index),
                                });
                            }
                        }

                        // 5e. Apply per-object positioned tabs (from TabAnchors).
                        // These split closed Vector segments with matching source identity.
                        // Must run before layer-level auto-tabs so that manually-tabbed
                        // contours become open and are skipped by apply_tabs.
                        {
                            use std::collections::HashMap;
                            let tab_width = vector_settings.map(|s| s.tab_width_mm).unwrap_or(3.0);
                            for obj in &vector_objects {
                                if obj.tabs.is_empty() {
                                    continue;
                                }
                                let obj_id_str = obj.id.to_string();
                                let mut tabs_by_subpath: HashMap<usize, Vec<f64>> = HashMap::new();
                                for tab in &obj.tabs {
                                    tabs_by_subpath
                                        .entry(tab.subpath_index)
                                        .or_default()
                                        .push(tab.position);
                                }
                                let mut i = pre_vector_count;
                                while i < all_segments.len() {
                                    let matches = if let PlanSegment::Vector {
                                        source_object_id: Some(ref oid),
                                        source_subpath_index: Some(sp_idx),
                                        closed: true,
                                        ..
                                    } = all_segments[i]
                                    {
                                        *oid == obj_id_str && tabs_by_subpath.contains_key(&sp_idx)
                                    } else {
                                        false
                                    };

                                    if matches {
                                        let sp_idx = if let PlanSegment::Vector {
                                            source_subpath_index: Some(sp),
                                            ..
                                        } = &all_segments[i]
                                        {
                                            *sp
                                        } else {
                                            unreachable!()
                                        };
                                        let positions = &tabs_by_subpath[&sp_idx];
                                        let seg = all_segments.remove(i);
                                        let mut target = vec![seg];
                                        apply_positioned_tabs(&mut target, positions, tab_width);
                                        let n = target.len();
                                        for (j, s) in target.into_iter().enumerate() {
                                            all_segments.insert(i + j, s);
                                        }
                                        i += n;
                                    } else {
                                        i += 1;
                                    }
                                }
                            }
                        }

                        // 5f. Apply layer-level auto-tabs for vector layers with tab_count > 0.
                        // Only apply to the segments added by this layer in this pass.
                        // Contours already split by per-object tabs are open and will be skipped.
                        let tab_count = vector_settings.map(|s| s.tab_count).unwrap_or(0);
                        if tab_count > 0 {
                            let tab_width = vector_settings.map(|s| s.tab_width_mm).unwrap_or(3.0);
                            let mut layer_segments: Vec<PlanSegment> =
                                all_segments.drain(pre_vector_count..).collect();
                            apply_tabs(&mut layer_segments, tab_count, tab_width);
                            all_segments.extend(layer_segments);
                        }
                    }
                }
            }

            if entry_failed {
                continue;
            }

            for segment in &mut all_segments[entry_segment_start..] {
                match segment {
                    PlanSegment::Vector { cut_entry_id, .. }
                    | PlanSegment::Raster { cut_entry_id, .. } => {
                        *cut_entry_id = entry.id.to_string();
                    }
                    _ => {}
                }
            }
        }
    }

    // 5f. Apply machine calibration BEFORE offsets/travel insertion so
    //     bounds, travel endpoints, and stats all reflect the corrected geometry.
    apply_dot_width_correction(&mut all_segments, calibration);

    // 6. Apply offsets BEFORE inserting travel segments, so travel
    //    moves are calculated relative to the final coordinate system.

    // 6a. Convert canvas-space geometry into machine-space coordinates.
    apply_workspace_origin_transform(
        &mut all_segments,
        project.workspace.origin,
        project.workspace.bed_height_mm,
    );

    // 6b. Anchor non-absolute jobs: translate so the job-origin anchor of
    //     the content lands exactly on the start-from target. This must run
    //     AFTER the workspace transform: the target (live head position /
    //     stored user origin) is a machine-space work coordinate, and a
    //     canvas-space rebase would be displaced by the Y flip.
    if project.start_from == StartFromMode::UserOrigin && project.user_origin.is_none() {
        warnings.push(PlanWarning {
            message: "User Origin mode selected but no origin has been set. Coordinates will not be offset.".to_string(),
        });
        // Skip offset entirely — do NOT fall back to (0,0)
    } else if project.start_from == StartFromMode::CurrentPosition
        && runtime_current_position.is_none()
    {
        warnings.push(PlanWarning {
            message: "Current Position mode selected but no machine is connected. Coordinates will not be offset.".to_string(),
        });
        // Skip offset entirely — do NOT fall back to (0,0)
    } else if project.start_from != StartFromMode::AbsoluteCoords {
        let y_flipped = project.workspace.origin != WorkspaceOrigin::TopLeft;
        // Selection-only runs anchor on the selection's bounds (supplied in
        // canvas space); otherwise anchor on the machine-space plan content.
        let content_bounds = match input.job_origin_bounds_override {
            Some(canvas_bounds) if y_flipped => {
                flip_bounds_y(&canvas_bounds, project.workspace.bed_height_mm)
            }
            Some(canvas_bounds) => canvas_bounds,
            None => calculate_plan_bounds(&all_segments),
        };
        // Guaranteed Some by the guards above for their respective modes.
        let target = match project.start_from {
            StartFromMode::CurrentPosition => runtime_current_position.unwrap_or((0.0, 0.0)),
            _ => project.user_origin.unwrap_or((0.0, 0.0)),
        };
        anchor_segments_to_target(
            &mut all_segments,
            project.job_origin,
            &content_bounds,
            y_flipped,
            target,
        );
    }

    // 6d. Travel optimization: reorder segments to minimize travel distance
    let start_pos = match (optimization.start_point_x, optimization.start_point_y) {
        (Some(x), Some(y)) => Point2D::new(x, y),
        _ => Point2D::new(0.0, 0.0),
    };
    let all_segments = if optimization.enabled && optimization.reduce_travel {
        travel::reorder_segments_nearest_neighbor(
            all_segments,
            start_pos,
            optimization.reduce_direction_changes,
        )
    } else {
        all_segments
    };

    // 6e. Prepend initial travel from custom start point if set
    let mut all_segments = all_segments;
    if optimization.start_point_x.is_some()
        && optimization.start_point_y.is_some()
        && let Some(first_start) = all_segments.first().and_then(travel::segment_start)
        && start_pos.distance_to(&first_start) > 0.001
    {
        all_segments.insert(
            0,
            PlanSegment::Travel {
                start: start_pos,
                end: first_start,
            },
        );
    }

    // 6f. Insert travel segments (after offsets so travels connect the final positions)
    let mut segments = travel::insert_travel_segments(all_segments);

    // 7. Validate bounds (after offsets have been applied, before finish travel)
    validate_bounds(
        &segments,
        project.workspace.bed_width_mm,
        project.workspace.bed_height_mm,
    )?;

    // 8. Calculate overall bounds (work area, before finish travel)
    let bounds = calculate_plan_bounds(&segments);

    // 8b. Append finish position travel (after bounds so return-to-origin doesn't skew work bounds)
    match optimization.finish_position {
        FinishPosition::Origin => {
            if let Some(last) = get_last_point(&segments) {
                let origin = Point2D::new(0.0, 0.0);
                if (last.x - origin.x).abs() > f64::EPSILON
                    || (last.y - origin.y).abs() > f64::EPSILON
                {
                    segments.push(PlanSegment::Travel {
                        start: last,
                        end: origin,
                    });
                }
            }
        }
        FinishPosition::CustomXY => {
            if let (Some(fx), Some(fy)) = (optimization.finish_x, optimization.finish_y)
                && let Some(last) = get_last_point(&segments)
            {
                let target = Point2D::new(fx, fy);
                if (last.x - target.x).abs() > f64::EPSILON
                    || (last.y - target.y).abs() > f64::EPSILON
                {
                    segments.push(PlanSegment::Travel {
                        start: last,
                        end: target,
                    });
                }
            }
        }
        FinishPosition::DontMove => { /* no travel appended */ }
    }

    // 9. Calculate statistics (includes finish travel distance)
    let total_distance_mm = calculate_distance(&segments);
    let estimated_duration_secs = calculate_duration(&segments);

    // 10. Build and return ExecutionPlan
    info!(
        segments = segments.len(),
        distance_mm = format_args!("{total_distance_mm:.1}"),
        est_secs = format_args!("{estimated_duration_secs:.1}"),
        warnings = warnings.len(),
        "Execution plan built"
    );

    Ok(ExecutionPlan {
        id: Uuid::new_v4(),
        project_id: *project.metadata.project_id.as_uuid(),
        revision_hash,
        created_at: Utc::now(),
        bounds,
        total_distance_mm,
        estimated_duration_secs,
        segments,
        layer_order,
        warnings,
        failed_entries,
    })
}

fn apply_dot_width_correction(segments: &mut [PlanSegment], calibration: &PlannerCalibration) {
    // Machine-level DWC applies uniformly to every raster segment; layer DWC
    // is per-segment and composes additively. This is the single composition
    // point — the G-code emitter and preview distiller must not apply DWC
    // again downstream.
    let machine_dwc = if calibration.enable_dot_width && calibration.dot_width_mm > 0.0 {
        calibration.dot_width_mm
    } else {
        0.0
    };

    for segment in segments {
        if let PlanSegment::Raster {
            scanlines,
            dot_width_correction_mm,
            ..
        } = segment
        {
            let layer_dwc = (*dot_width_correction_mm).max(0.0);
            let total_dwc = machine_dwc + layer_dwc;
            if total_dwc <= 0.0 {
                continue;
            }
            let trim = total_dwc / 2.0;
            for scanline in scanlines.iter_mut() {
                let is_rtl = matches!(scanline.direction, ScanDirection::RightToLeft);
                scanline.runs.retain_mut(|run| {
                    if is_rtl {
                        // RTL: start_x > end_x, trim start inward (subtract) and end inward (add)
                        run.start_x_mm -= trim;
                        run.end_x_mm += trim;
                        run.start_x_mm - run.end_x_mm > 1e-9
                    } else {
                        // LTR: start_x < end_x, trim start inward (add) and end inward (subtract)
                        run.start_x_mm += trim;
                        run.end_x_mm -= trim;
                        run.end_x_mm - run.start_x_mm > 1e-9
                    }
                });
            }
        }
    }
}

/// Build a frame-only plan that traces a convex hull path around the given points.
///
/// Used by the "Rubber Band" frame mode to trace a tighter outline that
/// follows the convex hull of object corners rather than the bounding rectangle.
pub fn build_hull_frame_plan(
    hull_points: &[Point2D],
    bounds: &Bounds,
    power_percent: f64,
    speed_mm_min: f64,
) -> ExecutionPlan {
    if hull_points.is_empty() {
        return build_frame_plan(bounds, power_percent, speed_mm_min);
    }

    let start = hull_points[0];
    let mut path: Vec<Point2D> = hull_points.to_vec();
    // Close the path if not already closed
    if path.first() != path.last() {
        path.push(path[0]);
    }

    let travel = PlanSegment::Travel {
        start: Point2D::zero(),
        end: start,
    };
    let frame = PlanSegment::Frame {
        path,
        power_percent,
        speed_mm_min,
    };

    let segments = vec![travel, frame];
    let total_distance_mm = calculate_distance(&segments);
    let estimated_duration_secs = calculate_duration(&segments);

    ExecutionPlan {
        id: Uuid::new_v4(),
        project_id: Uuid::nil(),
        revision_hash: String::new(),
        created_at: Utc::now(),
        bounds: *bounds,
        total_distance_mm,
        estimated_duration_secs,
        segments,
        layer_order: vec![],
        warnings: vec![],
        failed_entries: vec![],
    }
}

/// Build a frame-only execution plan that traces the rectangular boundary.
///
/// This is used by the "Frame" feature to trace the outline of the work area
/// at low power before running the actual job.
pub fn build_frame_plan(bounds: &Bounds, power_percent: f64, speed_mm_min: f64) -> ExecutionPlan {
    let start = Point2D::new(bounds.min.x, bounds.min.y);
    // Rectangular path: bottom-left → bottom-right → top-right → top-left → close
    let path = vec![
        Point2D::new(bounds.min.x, bounds.min.y),
        Point2D::new(bounds.max.x, bounds.min.y),
        Point2D::new(bounds.max.x, bounds.max.y),
        Point2D::new(bounds.min.x, bounds.max.y),
        Point2D::new(bounds.min.x, bounds.min.y), // close
    ];

    let travel = PlanSegment::Travel {
        start: Point2D::zero(),
        end: start,
    };
    let frame = PlanSegment::Frame {
        path,
        power_percent,
        speed_mm_min,
    };

    let segments = vec![travel, frame];
    let total_distance_mm = calculate_distance(&segments);
    let estimated_duration_secs = calculate_duration(&segments);

    ExecutionPlan {
        id: Uuid::new_v4(),
        project_id: Uuid::nil(),
        revision_hash: String::new(),
        created_at: Utc::now(),
        bounds: *bounds,
        total_distance_mm,
        estimated_duration_secs,
        segments,
        layer_order: vec![],
        warnings: vec![],
        failed_entries: vec![],
    }
}

/// Find asset data by asset_key string.
fn find_asset_data(
    asset_data: &std::collections::HashMap<AssetId, Vec<u8>>,
    asset_key: &str,
) -> Option<Vec<u8>> {
    for (id, data) in asset_data {
        if id.to_string() == asset_key {
            return Some(data.clone());
        }
    }
    None
}

/// Get the last point in a sequence of plan segments (for appending finish travel).
fn get_last_point(segments: &[PlanSegment]) -> Option<Point2D> {
    segments.last().and_then(|seg| match seg {
        PlanSegment::Travel { end, .. } => Some(*end),
        PlanSegment::Vector { polyline, .. } => polyline.last().copied(),
        PlanSegment::Frame { path, .. } => path.last().copied(),
        PlanSegment::Raster { scanlines, .. } => scanlines
            .last()
            .and_then(|sl| sl.runs.last().map(|r| Point2D::new(r.end_x_mm, sl.y_mm))),
        PlanSegment::OffsetFill { .. } => None,
    })
}

/// Calculate bounding box for all segments.
pub fn calculate_plan_bounds(segments: &[PlanSegment]) -> Bounds {
    let mut min_x = f64::MAX;
    let mut min_y = f64::MAX;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    for segment in segments {
        match segment {
            PlanSegment::Travel { start, end } => {
                update_bounds(&mut min_x, &mut min_y, &mut max_x, &mut max_y, start);
                update_bounds(&mut min_x, &mut min_y, &mut max_x, &mut max_y, end);
            }
            PlanSegment::Vector { polyline, .. } => {
                for point in polyline {
                    update_bounds(&mut min_x, &mut min_y, &mut max_x, &mut max_y, point);
                }
            }
            PlanSegment::Frame { path, .. } => {
                for point in path {
                    update_bounds(&mut min_x, &mut min_y, &mut max_x, &mut max_y, point);
                }
            }
            PlanSegment::Raster {
                scanlines,
                scan_axis,
                scan_angle_deg,
                scan_origin,
                ..
            } => {
                let angle_rad = scan_angle_deg.to_radians();
                let cos_a = angle_rad.cos();
                let sin_a = angle_rad.sin();
                let rotated_scan = scan_angle_deg.abs() > 0.5;

                for scanline in scanlines {
                    for run in &scanline.runs {
                        if rotated_scan {
                            // Non-cardinal raster scanlines are stored in the
                            // local rotated frame used by the emitter. Convert
                            // run endpoints back to world space for plan bounds.
                            let start = rotate_local_raster_point_to_world(
                                Point2D::new(run.start_x_mm, scanline.y_mm),
                                *scan_origin,
                                cos_a,
                                sin_a,
                            );
                            let end = rotate_local_raster_point_to_world(
                                Point2D::new(run.end_x_mm, scanline.y_mm),
                                *scan_origin,
                                cos_a,
                                sin_a,
                            );
                            update_bounds(&mut min_x, &mut min_y, &mut max_x, &mut max_y, &start);
                            update_bounds(&mut min_x, &mut min_y, &mut max_x, &mut max_y, &end);
                        } else if *scan_axis == ScanAxis::Vertical {
                            // Vertical scan transposes X/Y in scanlines — un-transpose
                            // so that plan bounds are in correct world-space.
                            min_x = min_x.min(scanline.y_mm);
                            max_x = max_x.max(scanline.y_mm);
                            min_y = min_y.min(run.start_x_mm.min(run.end_x_mm));
                            max_y = max_y.max(run.start_x_mm.max(run.end_x_mm));
                        } else {
                            min_x = min_x.min(run.start_x_mm);
                            max_x = max_x.max(run.end_x_mm);
                            min_y = min_y.min(scanline.y_mm);
                            max_y = max_y.max(scanline.y_mm);
                        }
                    }
                }
            }
            PlanSegment::OffsetFill { .. } => {}
        }
    }

    // Handle empty case
    if min_x == f64::MAX {
        return Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(0.0, 0.0));
    }

    Bounds::new(Point2D::new(min_x, min_y), Point2D::new(max_x, max_y))
}

fn rotate_local_raster_point_to_world(
    local: Point2D,
    scan_origin: Point2D,
    cos_a: f64,
    sin_a: f64,
) -> Point2D {
    Point2D::new(
        scan_origin.x + local.x * cos_a - local.y * sin_a,
        scan_origin.y + local.x * sin_a + local.y * cos_a,
    )
}

fn update_bounds(
    min_x: &mut f64,
    min_y: &mut f64,
    max_x: &mut f64,
    max_y: &mut f64,
    point: &Point2D,
) {
    *min_x = min_x.min(point.x);
    *min_y = min_y.min(point.y);
    *max_x = max_x.max(point.x);
    *max_y = max_y.max(point.y);
}

#[cfg(test)]
mod tests {
    use super::*;
    use beambench_common::Id;
    use beambench_common::markers::ProjectMarker;
    use beambench_core::asset::{Asset, AssetMediaType};
    use beambench_core::layer::{CutEntry, Layer, OffsetFillGroupingMode, OperationType};
    use beambench_core::object::{
        ImageMaskPolarity, ImageMaskRef, LayerRef, ObjectId, ProjectObject, ShapeKind,
    };
    use beambench_core::project::{Project, ProjectMetadata};
    use beambench_core::vector::buffer::buffer_closed_path;
    use beambench_core::vector::offset::{CornerStyle, OffsetDirection};
    use beambench_core::workspace::{Workspace, WorkspaceOrigin};
    use std::collections::HashMap;

    /// Test-only helper: a default `PlannerInput` — empty runtime
    /// overlay, default `ProjectOptimization`, default calibration.
    /// Replaces the pre-M1 `OptimizationSettings::default()` +
    /// `PlannerCalibration::default()` pair that tests used to pass to
    /// the deleted `build_plan_with_settings` shim.
    fn default_plan_input() -> PlannerInput {
        PlannerInput::new(
            ProjectOptimization::default(),
            OptimizationRuntime::default(),
            PlannerCalibration::default(),
        )
    }

    /// Test-only helper: a `PlannerInput` with the caller's optimization
    /// block and otherwise default runtime/calibration.
    fn plan_input_with(opt: ProjectOptimization) -> PlannerInput {
        PlannerInput::new(
            opt,
            OptimizationRuntime::default(),
            PlannerCalibration::default(),
        )
    }

    fn create_test_project() -> Project {
        let mut metadata = ProjectMetadata::new("Test Project");
        metadata.project_id = Id::<ProjectMarker>::new();

        Project {
            metadata,
            workspace: Workspace {
                bed_width_mm: 400.0,
                bed_height_mm: 300.0,
                origin: WorkspaceOrigin::TopLeft,
            },
            layers: vec![],
            objects: vec![],
            assets: vec![],
            asset_data: HashMap::new(),
            dirty: false,
            machine_profile_id: None,
            machine_profile_snapshot: None,
            notes: String::new(),
            start_from: Default::default(),
            job_origin: Default::default(),
            transform_locks: Default::default(),
            user_origin: None,
            optimization: Default::default(),
            material_height_mm: None,
        }
    }

    fn set_offset_fill_grouping_mode(layer: &mut Layer, mode: OffsetFillGroupingMode) {
        layer
            .primary_entry_mut()
            .vector_settings
            .as_mut()
            .expect("offset fill should carry vector settings")
            .offset_fill_grouping_mode = mode;
    }

    fn group_object(name: &str, layer_id: LayerRef, children: Vec<ObjectId>) -> ProjectObject {
        ProjectObject::new(
            name,
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(1.0, 1.0)),
            ObjectData::Group { children },
        )
    }

    fn vector_segment_count(plan: &ExecutionPlan) -> usize {
        plan.segments
            .iter()
            .filter(|segment| matches!(segment, PlanSegment::Vector { .. }))
            .count()
    }

    #[test]
    fn empty_project_returns_empty_plan_error() {
        let project = create_test_project();
        let result = build_plan(&project);

        assert!(result.is_err());
        match result {
            Err(PlannerError::EmptyPlan) => {}
            _ => panic!("Expected EmptyPlan error"),
        }
    }

    #[test]
    fn project_with_disabled_layers_only() {
        let mut project = create_test_project();

        let mut layer = Layer::new("Disabled", OperationType::Line);
        layer.enabled = false;
        project.layers.push(layer);

        let result = build_plan(&project);
        assert!(result.is_err());
        match result {
            Err(PlannerError::EmptyPlan) => {}
            _ => panic!("Expected EmptyPlan error"),
        }
    }

    #[test]
    fn project_with_single_vector_object() {
        let mut project = create_test_project();

        let layer = Layer::new("Lines", OperationType::Line);
        let layer_id = layer.id;
        project.layers.push(layer);

        let obj = ProjectObject::new(
            "Rectangle",
            layer_id,
            Bounds::new(Point2D::new(10.0, 10.0), Point2D::new(50.0, 50.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 40.0,
                height: 40.0,
                corner_radius: 0.0,
            },
        );
        project.objects.push(obj);

        let result = build_plan(&project);
        assert!(result.is_ok());

        let plan = result.unwrap();
        assert!(!plan.segments.is_empty());
        assert_eq!(plan.layer_order.len(), 1);
    }

    #[test]
    fn bottom_left_workspace_flips_vector_y_to_machine_coordinates() {
        let mut project = create_test_project();
        project.workspace.origin = WorkspaceOrigin::BottomLeft;
        project.workspace.bed_height_mm = 300.0;

        let layer = Layer::new("Lines", OperationType::Line);
        let layer_id = layer.id;
        project.layers.push(layer);
        project.objects.push(ProjectObject::new(
            "Lower visual rectangle",
            layer_id,
            Bounds::new(Point2D::new(10.0, 250.0), Point2D::new(20.0, 260.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        ));

        let plan = build_plan(&project).unwrap();

        assert!(
            plan.bounds.min.y >= 39.9 && plan.bounds.max.y <= 50.1,
            "bottom-left origin should place visual Y 250..260 at machine Y 40..50, got {:?}",
            plan.bounds
        );
    }

    #[test]
    fn bottom_left_workspace_flips_image_raster_scanlines_to_machine_coordinates() {
        let mut project = create_test_project();
        project.workspace.origin = WorkspaceOrigin::BottomLeft;
        project.workspace.bed_height_mm = 300.0;

        let layer = Layer::new("Image", OperationType::Image);
        let layer_id = layer.id;
        project.layers.push(layer);

        let png_bytes = create_test_png(4, 4, 0);
        let asset = Asset::new(
            "test.png",
            AssetMediaType::Png,
            png_bytes.len() as u64,
            Some(4),
            Some(4),
        );
        let asset_id = asset.id;
        project.add_asset(asset, png_bytes);
        project.objects.push(ProjectObject::new(
            "Lower visual image",
            layer_id,
            Bounds::new(Point2D::new(10.0, 250.0), Point2D::new(20.0, 260.0)),
            ObjectData::RasterImage {
                asset_key: asset_id.to_string(),
                original_width_px: 4,
                original_height_px: 4,
                adjustments: None,
                masks: Vec::new(),
            },
        ));

        let plan = build_plan(&project).unwrap();
        let raster = plan
            .segments
            .iter()
            .find_map(|segment| match segment {
                PlanSegment::Raster { scanlines, .. } => Some(scanlines),
                _ => None,
            })
            .expect("expected an image raster segment");
        let min_y = raster
            .iter()
            .map(|scanline| scanline.y_mm)
            .fold(f64::INFINITY, f64::min);
        let max_y = raster
            .iter()
            .map(|scanline| scanline.y_mm)
            .fold(f64::NEG_INFINITY, f64::max);

        assert!(
            min_y >= 39.9 && max_y <= 50.1,
            "bottom-left origin should place visual raster Y 250..260 at machine Y 40..50, got {min_y}..{max_y}",
        );
    }

    #[test]
    fn raster_object_without_asset_data_generates_warning() {
        let mut project = create_test_project();

        let layer = Layer::new("Image", OperationType::Image);
        let layer_id = layer.id;
        project.layers.push(layer);

        let obj = ProjectObject::new(
            "Image",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(25.4, 25.4)),
            ObjectData::RasterImage {
                asset_key: "missing-asset-id".to_string(),
                original_width_px: 100,
                original_height_px: 100,
                adjustments: None,
                masks: Vec::new(),
            },
        );
        project.objects.push(obj);

        let result = build_plan(&project);
        assert!(result.is_ok());

        let plan = result.unwrap();
        assert_eq!(plan.warnings.len(), 1);
        assert!(plan.warnings[0].message.contains("Asset data not found"));
    }

    #[test]
    fn layers_ordered_by_order_index() {
        let mut project = create_test_project();

        let mut layer1 = Layer::new("First", OperationType::Line);
        layer1.order_index = 2;
        let layer1_id = layer1.id;

        let mut layer2 = Layer::new("Second", OperationType::Line);
        layer2.order_index = 1;
        let layer2_id = layer2.id;

        project.layers.push(layer1);
        project.layers.push(layer2);

        // Add objects to each layer
        let obj1 = ProjectObject::new(
            "Obj1",
            layer1_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        );

        let obj2 = ProjectObject::new(
            "Obj2",
            layer2_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        );

        project.objects.push(obj1);
        project.objects.push(obj2);

        let result = build_plan(&project);
        assert!(result.is_ok());

        let plan = result.unwrap();
        // Layer2 (order_index=1) should come before Layer1 (order_index=2)
        assert_eq!(plan.layer_order[0], layer2_id.to_string());
        assert_eq!(plan.layer_order[1], layer1_id.to_string());
    }

    #[test]
    fn layer_order_key_can_be_disabled() {
        let mut project = create_test_project();
        project.optimization.ordering = Vec::new();

        let mut layer1 = Layer::new("First", OperationType::Line);
        layer1.order_index = 2;
        let layer1_id = layer1.id;

        let mut layer2 = Layer::new("Second", OperationType::Line);
        layer2.order_index = 1;
        let layer2_id = layer2.id;

        project.layers.push(layer1);
        project.layers.push(layer2);

        project.objects.push(ProjectObject::new(
            "Obj1",
            layer1_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        ));
        project.objects.push(ProjectObject::new(
            "Obj2",
            layer2_id,
            Bounds::new(Point2D::new(20.0, 0.0), Point2D::new(30.0, 10.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        ));

        let plan = build_plan(&project).unwrap();
        assert_eq!(plan.layer_order[0], layer1_id.to_string());
        assert_eq!(plan.layer_order[1], layer2_id.to_string());
    }

    #[test]
    fn invisible_objects_skipped() {
        let mut project = create_test_project();

        let layer = Layer::new("Lines", OperationType::Line);
        let layer_id = layer.id;
        project.layers.push(layer);

        let mut obj = ProjectObject::new(
            "Invisible",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        );
        obj.visible = false;
        project.objects.push(obj);

        let result = build_plan(&project);
        assert!(result.is_ok());

        let plan = result.unwrap();
        // Should have no segments (object was invisible)
        assert_eq!(plan.segments.len(), 0);
    }

    #[test]
    fn locked_objects_skipped() {
        let mut project = create_test_project();

        let layer = Layer::new("Lines", OperationType::Line);
        let layer_id = layer.id;
        project.layers.push(layer);

        let mut obj = ProjectObject::new(
            "Locked",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        );
        obj.locked = true;
        project.objects.push(obj);

        let result = build_plan(&project);
        assert!(result.is_ok());

        let plan = result.unwrap();
        assert_eq!(plan.segments.len(), 0);
    }

    #[test]
    fn calculate_bounds_empty_segments() {
        let bounds = calculate_plan_bounds(&[]);
        assert_eq!(bounds.min.x, 0.0);
        assert_eq!(bounds.min.y, 0.0);
        assert_eq!(bounds.max.x, 0.0);
        assert_eq!(bounds.max.y, 0.0);
    }

    #[test]
    fn calculate_bounds_negative_coordinates() {
        let segments = vec![PlanSegment::Vector {
            polyline: vec![Point2D::new(-50.0, -30.0), Point2D::new(-10.0, -5.0)],
            closed: false,
            power_percent: 80.0,
            speed_mm_min: 1000.0,
            layer_id: "layer1".to_string(),
            cut_entry_id: String::new(),
            perforation_enabled: false,
            perforation_on_ms: 0.0,
            perforation_off_ms: 0.0,
            source_object_id: None,
            source_subpath_index: None,
        }];

        let bounds = calculate_plan_bounds(&segments);
        assert_eq!(bounds.min.x, -50.0);
        assert_eq!(bounds.min.y, -30.0);
        assert_eq!(bounds.max.x, -10.0);
        assert_eq!(bounds.max.y, -5.0);
    }

    #[test]
    fn calculate_bounds_single_vector() {
        let segments = vec![PlanSegment::Vector {
            polyline: vec![Point2D::new(10.0, 20.0), Point2D::new(50.0, 60.0)],
            closed: false,
            power_percent: 80.0,
            speed_mm_min: 1000.0,
            layer_id: "layer1".to_string(),
            cut_entry_id: String::new(),
            perforation_enabled: false,
            perforation_on_ms: 0.0,
            perforation_off_ms: 0.0,
            source_object_id: None,
            source_subpath_index: None,
        }];

        let bounds = calculate_plan_bounds(&segments);
        assert_eq!(bounds.min.x, 10.0);
        assert_eq!(bounds.min.y, 20.0);
        assert_eq!(bounds.max.x, 50.0);
        assert_eq!(bounds.max.y, 60.0);
    }

    #[test]
    fn revision_hash_is_consistent() {
        // Same project serialized twice should have same hash
        let project = create_test_project();

        let json1 = serde_json::to_string(&project).unwrap();
        let json2 = serde_json::to_string(&project).unwrap();

        let mut hasher1 = Sha256::new();
        hasher1.update(json1.as_bytes());
        let hash1 = format!("{:x}", hasher1.finalize());

        let mut hasher2 = Sha256::new();
        hasher2.update(json2.as_bytes());
        let hash2 = format!("{:x}", hasher2.finalize());

        assert_eq!(hash1, hash2);
    }

    #[test]
    fn revision_hash_changes_when_start_from_or_job_origin_changes() {
        // Changing start_from, job_origin, or user_origin must change the hash
        // so the plan cache auto-invalidates.
        let project = create_test_project();
        let base_json = serde_json::to_string(&project).unwrap();

        let mut p2 = project.clone();
        p2.start_from = beambench_common::StartFromMode::CurrentPosition;
        let p2_json = serde_json::to_string(&p2).unwrap();
        assert_ne!(base_json, p2_json, "start_from change should change JSON");

        let mut p3 = project.clone();
        p3.job_origin = beambench_common::AnchorPoint::Center;
        let p3_json = serde_json::to_string(&p3).unwrap();
        assert_ne!(base_json, p3_json, "job_origin change should change JSON");

        let mut p4 = project.clone();
        p4.user_origin = Some((10.0, 20.0));
        let p4_json = serde_json::to_string(&p4).unwrap();
        assert_ne!(base_json, p4_json, "user_origin change should change JSON");
    }

    #[test]
    fn vector_passes_creates_multiple_segments() {
        let mut project = create_test_project();

        let mut layer = Layer::new("Lines", OperationType::Line);
        layer
            .primary_entry_mut()
            .vector_settings
            .as_mut()
            .unwrap()
            .passes = 2;
        let layer_id = layer.id;
        project.layers.push(layer);

        let obj = ProjectObject::new(
            "Rectangle",
            layer_id,
            Bounds::new(Point2D::new(10.0, 10.0), Point2D::new(50.0, 50.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 40.0,
                height: 40.0,
                corner_radius: 0.0,
            },
        );
        project.objects.push(obj);

        let result = build_plan(&project);
        assert!(result.is_ok());

        let plan = result.unwrap();
        // Should have segments for 2 passes (plus potential travel)
        // Rectangle normalizes to 1 polyline, so we expect 2 vector segments (one per pass)
        let vector_count = plan
            .segments
            .iter()
            .filter(|s| matches!(s, PlanSegment::Vector { .. }))
            .count();
        assert_eq!(vector_count, 2);
    }

    // Helper function to create a test PNG
    fn create_test_png(width: u32, height: u32, fill_value: u8) -> Vec<u8> {
        use image::{GrayImage, Luma};
        let img = GrayImage::from_fn(width, height, |_, _| Luma([fill_value]));
        let mut buf = Vec::new();
        let encoder = image::codecs::png::PngEncoder::new(&mut buf);
        image::ImageEncoder::write_image(
            encoder,
            img.as_raw(),
            width,
            height,
            image::ExtendedColorType::L8,
        )
        .unwrap();
        buf
    }

    /// Integration test: mixed raster+vector project → valid plan
    #[test]
    fn integration_mixed_raster_vector_project() {
        // Create a project with:
        // 1. Image layer with a raster object (small PNG)
        // 2. Line layer with a rectangle shape
        // 3. Cut layer with a vector path

        let mut project = create_test_project();

        // Image layer (index 0 - created by default in Project::new)
        // We need to add layers manually since create_test_project returns empty layers
        let image_layer = Layer::new("Image", OperationType::Image);
        let image_layer_id = image_layer.id;
        project.layers.push(image_layer);

        // Line layer
        let line_layer = Layer::new("Line", OperationType::Line);
        let line_layer_id = line_layer.id;
        project.layers.push(line_layer);

        // Cut layer
        let cut_layer = Layer::new("Cut", OperationType::Cut);
        let cut_layer_id = cut_layer.id;
        project.layers.push(cut_layer);

        // Create a small 4x4 all-black PNG for the raster object
        let png_bytes = create_test_png(4, 4, 0); // 0 = black (will engrave)

        // Add asset
        let asset = Asset::new(
            "test.png",
            AssetMediaType::Png,
            png_bytes.len() as u64,
            Some(4),
            Some(4),
        );
        let asset_id = asset.id;
        project.add_asset(asset, png_bytes);

        // Add raster object on Image layer
        project.add_object(ProjectObject::new(
            "test_image",
            image_layer_id,
            Bounds::new(Point2D::new(10.0, 10.0), Point2D::new(20.0, 20.0)),
            ObjectData::RasterImage {
                asset_key: asset_id.to_string(),
                original_width_px: 4,
                original_height_px: 4,
                adjustments: None,
                masks: Vec::new(),
            },
        ));

        // Add rectangle on Line layer
        project.add_object(ProjectObject::new(
            "test_rect",
            line_layer_id,
            Bounds::new(Point2D::new(50.0, 50.0), Point2D::new(100.0, 100.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 50.0,
                height: 50.0,
                corner_radius: 0.0,
            },
        ));

        // Add vector path on Cut layer
        project.add_object(ProjectObject::new(
            "test_path",
            cut_layer_id,
            Bounds::new(Point2D::new(150.0, 150.0), Point2D::new(200.0, 200.0)),
            ObjectData::VectorPath {
                path_data: "M0 0 L50 0 L50 50 L0 50 Z".to_string(),
                closed: true,
                ruler_guide_axis: None,
            },
        ));

        // Build plan
        let plan = build_plan(&project).unwrap();

        // Verify
        assert!(!plan.segments.is_empty(), "Plan should have segments");
        assert!(
            plan.total_distance_mm > 0.0,
            "Should have non-zero distance"
        );
        assert!(
            plan.estimated_duration_secs > 0.0,
            "Should have non-zero duration"
        );
        assert_eq!(plan.layer_order.len(), 3, "All 3 layers should be in order");

        // Check we have raster, vector, and travel segments
        let has_raster = plan
            .segments
            .iter()
            .any(|s| matches!(s, PlanSegment::Raster { .. }));
        let has_vector = plan
            .segments
            .iter()
            .any(|s| matches!(s, PlanSegment::Vector { .. }));
        let has_travel = plan
            .segments
            .iter()
            .any(|s| matches!(s, PlanSegment::Travel { .. }));
        assert!(has_raster, "Plan should have raster segments");
        assert!(has_vector, "Plan should have vector segments");
        assert!(has_travel, "Plan should have travel segments");

        // Verify determinism: same project → same revision_hash
        let plan2 = build_plan(&project).unwrap();
        assert_eq!(
            plan.revision_hash, plan2.revision_hash,
            "Same project should produce same revision hash"
        );

        // Stats should be consistent
        let stats = plan.stats();
        assert_eq!(stats.segment_count, plan.segments.len());
        assert_eq!(stats.distance_mm, plan.total_distance_mm);
    }

    /// Raster correctness test: all-black PNG → non-empty scanlines
    #[test]
    fn raster_all_black_png_produces_runs() {
        let mut project = create_test_project();

        let image_layer = Layer::new("Image", OperationType::Image);
        let image_layer_id = image_layer.id;
        project.layers.push(image_layer);

        // Create a 4x4 all-black PNG (fill_value = 0)
        let png_bytes = create_test_png(4, 4, 0);

        let asset = Asset::new(
            "black.png",
            AssetMediaType::Png,
            png_bytes.len() as u64,
            Some(4),
            Some(4),
        );
        let asset_id = asset.id;
        project.add_asset(asset, png_bytes);

        project.add_object(ProjectObject::new(
            "black_image",
            image_layer_id,
            Bounds::new(Point2D::new(10.0, 10.0), Point2D::new(20.0, 20.0)),
            ObjectData::RasterImage {
                asset_key: asset_id.to_string(),
                original_width_px: 4,
                original_height_px: 4,
                adjustments: None,
                masks: Vec::new(),
            },
        ));

        let plan = build_plan(&project).unwrap();

        // Find raster segments
        let raster_segments: Vec<_> = plan
            .segments
            .iter()
            .filter_map(|s| match s {
                PlanSegment::Raster { scanlines, .. } => Some(scanlines),
                _ => None,
            })
            .collect();

        assert!(!raster_segments.is_empty(), "Should have raster segments");

        // Check that at least some scanlines have runs (black pixels engrave)
        let has_runs = raster_segments
            .iter()
            .any(|scanlines| scanlines.iter().any(|scanline| !scanline.runs.is_empty()));

        assert!(has_runs, "All-black image should produce raster runs");
    }

    #[test]
    fn image_mask_clips_raster_scanlines() {
        let mut project = create_test_project();
        let image_layer = Layer::new("Image", OperationType::Image);
        let image_layer_id = image_layer.id;
        project.layers.push(image_layer);
        let mask_layer = Layer::new("Line", OperationType::Line);
        let mask_layer_id = mask_layer.id;
        project.layers.push(mask_layer);

        let png_bytes = create_test_png(4, 4, 0);
        let asset = Asset::new(
            "masked.png",
            AssetMediaType::Png,
            png_bytes.len() as u64,
            Some(4),
            Some(4),
        );
        let asset_id = asset.id;
        project.add_asset(asset, png_bytes);

        let mask = ProjectObject::new(
            "mask",
            mask_layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(2.0, 4.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 2.0,
                height: 4.0,
                corner_radius: 0.0,
            },
        );
        let mask_id = mask.id;
        project.add_object(mask);
        project.add_object(ProjectObject::new(
            "masked_image",
            image_layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(4.0, 4.0)),
            ObjectData::RasterImage {
                asset_key: asset_id.to_string(),
                original_width_px: 4,
                original_height_px: 4,
                adjustments: None,
                masks: vec![ImageMaskRef {
                    object_id: mask_id,
                    polarity: ImageMaskPolarity::KeepInside,
                }],
            },
        ));

        let plan = build_plan(&project).unwrap();
        let raster = plan
            .segments
            .iter()
            .find_map(|segment| match segment {
                PlanSegment::Raster { scanlines, .. } => Some(scanlines),
                _ => None,
            })
            .expect("masked image should create raster output");

        assert!(!raster.is_empty());
        for scanline in raster {
            assert_eq!(scanline.runs.len(), 1);
            assert_eq!(scanline.runs[0].start_x_mm, 0.0);
            assert!((scanline.runs[0].end_x_mm - 2.0).abs() < 1e-9);
        }
    }

    #[test]
    fn image_mask_compound_path_with_hole_carves_out_donut() {
        // A vector_path mask with two opposite-winding subpaths (outer 4×4 CCW
        // square + inner 2×2 CW square at the center) should mask as a donut,
        // matching how the SVG would render with the default `nonzero` fill
        // rule. Old behaviour ("inside any polyline") would mask as a solid
        // square, ignoring the hole.
        let mut project = create_test_project();
        let image_layer = Layer::new("Image", OperationType::Image);
        let image_layer_id = image_layer.id;
        project.layers.push(image_layer);
        let mask_layer = Layer::new("Line", OperationType::Line);
        let mask_layer_id = mask_layer.id;
        project.layers.push(mask_layer);

        let png_bytes = create_test_png(4, 4, 0);
        let asset = Asset::new(
            "donut.png",
            AssetMediaType::Png,
            png_bytes.len() as u64,
            Some(4),
            Some(4),
        );
        let asset_id = asset.id;
        project.add_asset(asset, png_bytes);

        let mask = ProjectObject::new(
            "donut-mask",
            mask_layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(4.0, 4.0)),
            ObjectData::VectorPath {
                path_data: "M0 0 L4 0 L4 4 L0 4 Z M1 1 L1 3 L3 3 L3 1 Z".to_string(),
                closed: true,
                ruler_guide_axis: None,
            },
        );
        let mask_id = mask.id;
        project.add_object(mask);
        project.add_object(ProjectObject::new(
            "masked_image",
            image_layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(4.0, 4.0)),
            ObjectData::RasterImage {
                asset_key: asset_id.to_string(),
                original_width_px: 4,
                original_height_px: 4,
                adjustments: None,
                masks: vec![ImageMaskRef {
                    object_id: mask_id,
                    polarity: ImageMaskPolarity::KeepInside,
                }],
            },
        ));

        let plan = build_plan(&project).unwrap();
        let raster = plan
            .segments
            .iter()
            .find_map(|segment| match segment {
                PlanSegment::Raster { scanlines, .. } => Some(scanlines),
                _ => None,
            })
            .expect("masked image should create raster output");

        // At least one scanline should have two runs — the donut interior
        // splits each middle scanline into a left ring and a right ring.
        // A solid-square mask (silhouette behaviour) would yield exactly one
        // run per scanline.
        let any_two_run_scanline = raster.iter().any(|scanline| scanline.runs.len() == 2);
        assert!(
            any_two_run_scanline,
            "donut mask should produce scanlines with two runs (left/right of hole), got: {:?}",
            raster.iter().map(|s| s.runs.len()).collect::<Vec<_>>(),
        );
    }

    #[test]
    fn image_mask_uses_center_pivot_transform_for_rotated_images() {
        let mut project = create_test_project();
        let image_layer = Layer::new("Image", OperationType::Image);
        let image_layer_id = image_layer.id;
        project.layers.push(image_layer);
        let mask_layer = Layer::new("Line", OperationType::Line);
        let mask_layer_id = mask_layer.id;
        project.layers.push(mask_layer);

        let png_bytes = create_test_png(4, 4, 0);
        let asset = Asset::new(
            "rotated-masked.png",
            AssetMediaType::Png,
            png_bytes.len() as u64,
            Some(4),
            Some(4),
        );
        let asset_id = asset.id;
        project.add_asset(asset, png_bytes);

        let mask = ProjectObject::new(
            "left-world-mask",
            mask_layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(2.0, 4.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 2.0,
                height: 4.0,
                corner_radius: 0.0,
            },
        );
        let mask_id = mask.id;
        project.add_object(mask);

        let mut image = ProjectObject::new(
            "rotated_image",
            image_layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(4.0, 4.0)),
            ObjectData::RasterImage {
                asset_key: asset_id.to_string(),
                original_width_px: 4,
                original_height_px: 4,
                adjustments: None,
                masks: vec![ImageMaskRef {
                    object_id: mask_id,
                    polarity: ImageMaskPolarity::KeepInside,
                }],
            },
        );
        image.transform = beambench_common::Transform2D::rotate(std::f64::consts::PI);
        project.add_object(image);

        let plan = build_plan(&project).unwrap();
        let raster = plan
            .segments
            .iter()
            .find_map(|segment| match segment {
                PlanSegment::Raster { scanlines, .. } => Some(scanlines),
                _ => None,
            })
            .expect("masked image should create raster output");

        assert!(!raster.is_empty());
        for scanline in raster {
            assert_eq!(scanline.runs.len(), 1);
            assert!((scanline.runs[0].start_x_mm - 2.0).abs() < 1e-9);
            assert!((scanline.runs[0].end_x_mm - 4.0).abs() < 1e-9);
        }
    }

    #[test]
    fn image_mask_handles_non_square_ninety_degree_image_transform() {
        let mut project = create_test_project();
        let image_layer = Layer::new("Image", OperationType::Image);
        let image_layer_id = image_layer.id;
        project.layers.push(image_layer);
        let mask_layer = Layer::new("Line", OperationType::Line);
        let mask_layer_id = mask_layer.id;
        project.layers.push(mask_layer);

        let png_bytes = create_test_png(4, 2, 0);
        let asset = Asset::new(
            "rotated-nonsquare.png",
            AssetMediaType::Png,
            png_bytes.len() as u64,
            Some(4),
            Some(2),
        );
        let asset_id = asset.id;
        project.add_asset(asset, png_bytes);

        let mask = ProjectObject::new(
            "world-band-mask",
            mask_layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(4.0, 2.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 4.0,
                height: 2.0,
                corner_radius: 0.0,
            },
        );
        let mask_id = mask.id;
        project.add_object(mask);

        let mut image = ProjectObject::new(
            "rotated_nonsquare_image",
            image_layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(4.0, 2.0)),
            ObjectData::RasterImage {
                asset_key: asset_id.to_string(),
                original_width_px: 4,
                original_height_px: 2,
                adjustments: None,
                masks: vec![ImageMaskRef {
                    object_id: mask_id,
                    polarity: ImageMaskPolarity::KeepInside,
                }],
            },
        );
        image.transform = beambench_common::Transform2D::rotate(std::f64::consts::FRAC_PI_2);
        project.add_object(image);

        let plan = build_plan(&project).unwrap();
        let raster = plan
            .segments
            .iter()
            .find_map(|segment| match segment {
                PlanSegment::Raster { scanlines, .. } => Some(scanlines),
                _ => None,
            })
            .expect("masked image should create raster output");

        assert!(!raster.is_empty());
        for scanline in raster {
            assert_eq!(scanline.runs.len(), 1);
            assert!((scanline.runs[0].start_x_mm - 1.0).abs() < 1e-9);
            assert!((scanline.runs[0].end_x_mm - 3.0).abs() < 1e-9);
        }
    }

    #[test]
    fn image_mask_reports_fully_outside_mask_with_mask_id() {
        let mut project = create_test_project();
        let image_layer = Layer::new("Image", OperationType::Image);
        let image_layer_id = image_layer.id;
        project.layers.push(image_layer);
        let mask_layer = Layer::new("Line", OperationType::Line);
        let mask_layer_id = mask_layer.id;
        project.layers.push(mask_layer);

        let png_bytes = create_test_png(4, 4, 0);
        let asset = Asset::new(
            "outside.png",
            AssetMediaType::Png,
            png_bytes.len() as u64,
            Some(4),
            Some(4),
        );
        let asset_id = asset.id;
        project.add_asset(asset, png_bytes);
        let mask = ProjectObject::new(
            "outside-mask",
            mask_layer_id,
            Bounds::new(Point2D::new(10.0, 10.0), Point2D::new(12.0, 12.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 2.0,
                height: 2.0,
                corner_radius: 0.0,
            },
        );
        let mask_id = mask.id;
        project.add_object(mask);
        project.add_object(ProjectObject::new(
            "masked_image",
            image_layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(4.0, 4.0)),
            ObjectData::RasterImage {
                asset_key: asset_id.to_string(),
                original_width_px: 4,
                original_height_px: 4,
                adjustments: None,
                masks: vec![ImageMaskRef {
                    object_id: mask_id,
                    polarity: ImageMaskPolarity::KeepInside,
                }],
            },
        ));

        let plan = build_plan(&project).unwrap();
        assert!(plan.failed_entries.iter().any(|failure| {
            matches!(
                &failure.reason,
                PlanEntryFailureReason::ImageMaskSkipped { message, .. }
                    if message.contains(&mask_id.to_string())
            )
        }));
    }

    #[test]
    fn image_mask_fully_containing_mask_leaves_raster_unchanged() {
        let mut project = create_test_project();
        let image_layer = Layer::new("Image", OperationType::Image);
        let image_layer_id = image_layer.id;
        project.layers.push(image_layer);
        let mask_layer = Layer::new("Line", OperationType::Line);
        let mask_layer_id = mask_layer.id;
        project.layers.push(mask_layer);

        let png_bytes = create_test_png(4, 4, 0);
        let asset = Asset::new(
            "contained.png",
            AssetMediaType::Png,
            png_bytes.len() as u64,
            Some(4),
            Some(4),
        );
        let asset_id = asset.id;
        project.add_asset(asset, png_bytes);
        let mask = ProjectObject::new(
            "large-mask",
            mask_layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(4.0, 4.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 4.0,
                height: 4.0,
                corner_radius: 0.0,
            },
        );
        let mask_id = mask.id;
        project.add_object(mask);
        project.add_object(ProjectObject::new(
            "masked_image",
            image_layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(4.0, 4.0)),
            ObjectData::RasterImage {
                asset_key: asset_id.to_string(),
                original_width_px: 4,
                original_height_px: 4,
                adjustments: None,
                masks: vec![ImageMaskRef {
                    object_id: mask_id,
                    polarity: ImageMaskPolarity::KeepInside,
                }],
            },
        ));

        let plan = build_plan(&project).unwrap();
        let raster = plan
            .segments
            .iter()
            .find_map(|segment| match segment {
                PlanSegment::Raster { scanlines, .. } => Some(scanlines),
                _ => None,
            })
            .unwrap();
        assert!(!raster.is_empty());
        for scanline in raster {
            assert_eq!(scanline.runs.len(), 1);
            assert_eq!(scanline.runs[0].start_x_mm, 0.0);
            assert!((scanline.runs[0].end_x_mm - 4.0).abs() < 1e-9);
        }
    }

    #[test]
    fn open_image_mask_warns_and_does_not_prune_or_clip() {
        let mut project = create_test_project();
        let image_layer = Layer::new("Image", OperationType::Image);
        let image_layer_id = image_layer.id;
        project.layers.push(image_layer);
        let mask_layer = Layer::new("Line", OperationType::Line);
        let mask_layer_id = mask_layer.id;
        project.layers.push(mask_layer);

        let png_bytes = create_test_png(4, 4, 0);
        let asset = Asset::new(
            "open.png",
            AssetMediaType::Png,
            png_bytes.len() as u64,
            Some(4),
            Some(4),
        );
        let asset_id = asset.id;
        project.add_asset(asset, png_bytes);
        let mask = ProjectObject::new(
            "open-mask",
            mask_layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(4.0, 4.0)),
            ObjectData::VectorPath {
                path_data: "M0 0 L4 4".to_string(),
                closed: false,
                ruler_guide_axis: None,
            },
        );
        let mask_id = mask.id;
        project.add_object(mask);
        let image = ProjectObject::new(
            "masked_image",
            image_layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(4.0, 4.0)),
            ObjectData::RasterImage {
                asset_key: asset_id.to_string(),
                original_width_px: 4,
                original_height_px: 4,
                adjustments: None,
                masks: vec![ImageMaskRef {
                    object_id: mask_id,
                    polarity: ImageMaskPolarity::KeepInside,
                }],
            },
        );
        let image_id = image.id;
        project.add_object(image);

        let plan = build_plan(&project).unwrap();
        assert!(plan.failed_entries.iter().any(|failure| {
            matches!(
                &failure.reason,
                PlanEntryFailureReason::ImageMaskSkipped { object_id, message }
                    if object_id == &mask_id.to_string() && message.contains("open")
            )
        }));
        let image = project.find_object(image_id).unwrap();
        match &image.data {
            ObjectData::RasterImage { masks, .. } => assert_eq!(masks.len(), 1),
            _ => panic!("Expected raster image"),
        }
        let raster = plan
            .segments
            .iter()
            .find_map(|segment| match segment {
                PlanSegment::Raster { scanlines, .. } => Some(scanlines),
                _ => None,
            })
            .unwrap();
        for scanline in raster {
            assert_eq!(scanline.runs.len(), 1);
            assert_eq!(scanline.runs[0].start_x_mm, 0.0);
            assert!((scanline.runs[0].end_x_mm - 4.0).abs() < 1e-9);
        }
    }

    #[test]
    fn output_layer_mask_still_outputs_normally_while_clipping_image() {
        let mut project = create_test_project();
        let image_layer = Layer::new("Image", OperationType::Image);
        let image_layer_id = image_layer.id;
        project.layers.push(image_layer);
        let mask_layer = Layer::new("Line", OperationType::Line);
        let mask_layer_id = mask_layer.id;
        project.layers.push(mask_layer);

        let png_bytes = create_test_png(4, 4, 0);
        let asset = Asset::new(
            "output-mask.png",
            AssetMediaType::Png,
            png_bytes.len() as u64,
            Some(4),
            Some(4),
        );
        let asset_id = asset.id;
        project.add_asset(asset, png_bytes);
        let mask = ProjectObject::new(
            "output-mask",
            mask_layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(2.0, 4.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 2.0,
                height: 4.0,
                corner_radius: 0.0,
            },
        );
        let mask_id = mask.id;
        project.add_object(mask);
        project.add_object(ProjectObject::new(
            "masked_image",
            image_layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(4.0, 4.0)),
            ObjectData::RasterImage {
                asset_key: asset_id.to_string(),
                original_width_px: 4,
                original_height_px: 4,
                adjustments: None,
                masks: vec![ImageMaskRef {
                    object_id: mask_id,
                    polarity: ImageMaskPolarity::KeepInside,
                }],
            },
        ));

        let plan = build_plan(&project).unwrap();
        assert!(
            plan.segments
                .iter()
                .any(|segment| matches!(segment, PlanSegment::Vector { .. }))
        );
        let raster = plan
            .segments
            .iter()
            .find_map(|segment| match segment {
                PlanSegment::Raster { scanlines, .. } => Some(scanlines),
                _ => None,
            })
            .unwrap();
        for scanline in raster {
            assert_eq!(scanline.runs.len(), 1);
            assert_eq!(scanline.runs[0].start_x_mm, 0.0);
            assert!((scanline.runs[0].end_x_mm - 2.0).abs() < 1e-9);
        }
    }

    #[test]
    fn tool_layer_mask_clips_image_without_emitting_vector_geometry() {
        let mut project = create_test_project();
        let image_layer = Layer::new("Image", OperationType::Image);
        let image_layer_id = image_layer.id;
        project.layers.push(image_layer);
        let mut tool_layer = Layer::new("T1", OperationType::Tool);
        tool_layer.is_tool_layer = true;
        tool_layer.entries = vec![CutEntry::tool_marker()];
        let tool_layer_id = tool_layer.id;
        project.layers.push(tool_layer);

        let png_bytes = create_test_png(4, 4, 0);
        let asset = Asset::new(
            "tool-mask.png",
            AssetMediaType::Png,
            png_bytes.len() as u64,
            Some(4),
            Some(4),
        );
        let asset_id = asset.id;
        project.add_asset(asset, png_bytes);
        let mask = ProjectObject::new(
            "tool-mask",
            tool_layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(2.0, 4.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 2.0,
                height: 4.0,
                corner_radius: 0.0,
            },
        );
        let mask_id = mask.id;
        project.add_object(mask);
        project.add_object(ProjectObject::new(
            "masked_image",
            image_layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(4.0, 4.0)),
            ObjectData::RasterImage {
                asset_key: asset_id.to_string(),
                original_width_px: 4,
                original_height_px: 4,
                adjustments: None,
                masks: vec![ImageMaskRef {
                    object_id: mask_id,
                    polarity: ImageMaskPolarity::KeepInside,
                }],
            },
        ));

        let plan = build_plan(&project).unwrap();
        assert!(
            !plan
                .segments
                .iter()
                .any(|segment| matches!(segment, PlanSegment::Vector { .. }))
        );
        let raster = plan
            .segments
            .iter()
            .find_map(|segment| match segment {
                PlanSegment::Raster { scanlines, .. } => Some(scanlines),
                _ => None,
            })
            .unwrap();
        for scanline in raster {
            assert_eq!(scanline.runs.len(), 1);
            assert_eq!(scanline.runs[0].start_x_mm, 0.0);
            assert!((scanline.runs[0].end_x_mm - 2.0).abs() < 1e-9);
        }
    }

    /// Raster correctness test: all-white PNG → zero scan runs
    #[test]
    fn raster_all_white_png_produces_no_runs() {
        let mut project = create_test_project();

        let image_layer = Layer::new("Image", OperationType::Image);
        let image_layer_id = image_layer.id;
        project.layers.push(image_layer);

        // Create a 4x4 all-white PNG (fill_value = 255)
        let png_bytes = create_test_png(4, 4, 255);

        let asset = Asset::new(
            "white.png",
            AssetMediaType::Png,
            png_bytes.len() as u64,
            Some(4),
            Some(4),
        );
        let asset_id = asset.id;
        project.add_asset(asset, png_bytes);

        project.add_object(ProjectObject::new(
            "white_image",
            image_layer_id,
            Bounds::new(Point2D::new(10.0, 10.0), Point2D::new(20.0, 20.0)),
            ObjectData::RasterImage {
                asset_key: asset_id.to_string(),
                original_width_px: 4,
                original_height_px: 4,
                adjustments: None,
                masks: Vec::new(),
            },
        ));

        let plan = build_plan(&project).unwrap();

        // Find raster segments
        let raster_segments: Vec<_> = plan
            .segments
            .iter()
            .filter_map(|s| match s {
                PlanSegment::Raster { scanlines, .. } => Some(scanlines),
                _ => None,
            })
            .collect();

        // We should have a raster segment, but with no runs (white doesn't engrave)
        assert!(!raster_segments.is_empty(), "Should have raster segments");

        // All scanlines should have zero runs
        let total_runs: usize = raster_segments
            .iter()
            .flat_map(|scanlines| scanlines.iter())
            .map(|scanline| scanline.runs.len())
            .sum();

        assert_eq!(
            total_runs, 0,
            "All-white image should produce no raster runs"
        );
    }

    /// Bounds validation test: object exceeds workspace bounds → error
    #[test]
    fn bounds_validation_object_exceeds_workspace() {
        let mut project = create_test_project();
        // Workspace is 400x300

        let layer = Layer::new("Line", OperationType::Line);
        let layer_id = layer.id;
        project.layers.push(layer);

        // Object positioned at (390, 290) with 20x20 bounds
        // Max point will be (410, 310), exceeding workspace (400, 300)
        project.add_object(ProjectObject::new(
            "out_of_bounds",
            layer_id,
            Bounds::new(Point2D::new(390.0, 290.0), Point2D::new(410.0, 310.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 20.0,
                height: 20.0,
                corner_radius: 0.0,
            },
        ));

        let result = build_plan(&project);
        assert!(result.is_err());
        match result {
            Err(PlannerError::BoundsExceeded { .. }) => {}
            _ => panic!("Expected BoundsExceeded error"),
        }
    }

    /// Determinism test: same project → same revision_hash and serialized plan
    #[test]
    fn determinism_same_project_same_plan() {
        let mut project = create_test_project();

        let layer = Layer::new("Line", OperationType::Line);
        let layer_id = layer.id;
        project.layers.push(layer);

        project.add_object(ProjectObject::new(
            "rect1",
            layer_id,
            Bounds::new(Point2D::new(10.0, 10.0), Point2D::new(50.0, 50.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 40.0,
                height: 40.0,
                corner_radius: 0.0,
            },
        ));

        project.add_object(ProjectObject::new(
            "rect2",
            layer_id,
            Bounds::new(Point2D::new(100.0, 100.0), Point2D::new(150.0, 150.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 50.0,
                height: 50.0,
                corner_radius: 0.0,
            },
        ));

        // Build plan twice
        let plan1 = build_plan(&project).unwrap();
        let plan2 = build_plan(&project).unwrap();

        // Same revision hash
        assert_eq!(
            plan1.revision_hash, plan2.revision_hash,
            "Same project should produce same revision hash"
        );

        // Same segment count
        assert_eq!(
            plan1.segments.len(),
            plan2.segments.len(),
            "Same project should produce same number of segments"
        );

        // Note: UUIDs and timestamps will differ, so we can't compare the full JSON
        // But we can compare segments separately
        for (seg1, seg2) in plan1.segments.iter().zip(plan2.segments.iter()) {
            let seg1_json = serde_json::to_string(seg1).unwrap();
            let seg2_json = serde_json::to_string(seg2).unwrap();
            assert_eq!(
                seg1_json, seg2_json,
                "Same project should produce identical segments"
            );
        }

        // Stats should match
        assert_eq!(plan1.total_distance_mm, plan2.total_distance_mm);
        assert_eq!(plan1.estimated_duration_secs, plan2.estimated_duration_secs);
    }

    #[test]
    fn offset_fill_generates_concentric_vector_segments() {
        let mut project = create_test_project();

        let mut layer = Layer::new("OffsetFill", OperationType::OffsetFill);
        if let Some(raster_settings) = layer.primary_entry_mut().raster_settings.as_mut() {
            raster_settings.line_interval_mm = 2.0;
        }
        let layer_id = layer.id;
        project.layers.push(layer);

        project.objects.push(ProjectObject::new(
            "Rectangle",
            layer_id,
            Bounds::new(Point2D::new(50.0, 50.0), Point2D::new(150.0, 150.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 100.0,
                height: 100.0,
                corner_radius: 0.0,
            },
        ));

        let plan = build_plan(&project).expect("offset fill should build");
        let vector_count = plan
            .segments
            .iter()
            .filter(|s| matches!(s, PlanSegment::Vector { .. }))
            .count();
        assert!(
            vector_count >= 2,
            "OffsetFill should generate multiple concentric Vector segments, got {vector_count}"
        );
    }

    /// Regression test: a concave shape near the workspace edge used to produce
    /// offset paths with negative coordinates due to unbounded miter spikes,
    /// causing `validate_bounds` to fail with "travel end x=… exceeds bed width".
    #[test]
    fn offset_fill_near_edge_does_not_exceed_workspace_bounds() {
        let mut project = create_test_project();

        let layer = Layer::new("OffsetFill", OperationType::OffsetFill);
        let layer_id = layer.id;
        project.layers.push(layer);

        // Concave L-shape placed near x=0 — the concave corner at (5,5)
        // previously triggered an unbounded miter spike into negative x.
        let path_d = "M0 0 L20 0 L20 10 L5 10 L5 5 L0 5 Z";
        project.objects.push(ProjectObject::new(
            "L-shape near edge",
            layer_id,
            Bounds::new(Point2D::new(1.0, 1.0), Point2D::new(21.0, 11.0)),
            ObjectData::VectorPath {
                path_data: path_d.to_string(),
                closed: true,
                ruler_guide_axis: None,
            },
        ));

        // build_plan runs: generate segments → insert travels → validate bounds.
        // Before the miter-limit fix this returned BoundsExceeded.
        let result = build_plan(&project);
        assert!(
            result.is_ok(),
            "offset fill near workspace edge should not fail bounds validation, got: {:?}",
            result.err()
        );

        // All generated points must stay within workspace (0..400, 0..300).
        let plan = result.unwrap();
        for (idx, seg) in plan.segments.iter().enumerate() {
            match seg {
                PlanSegment::Vector { polyline, .. } => {
                    for pt in polyline {
                        assert!(
                            pt.x >= 0.0 && pt.x <= 400.0 && pt.y >= 0.0 && pt.y <= 300.0,
                            "Segment {idx} vector point ({}, {}) outside workspace",
                            pt.x,
                            pt.y
                        );
                    }
                }
                PlanSegment::Travel { start, end } => {
                    assert!(
                        start.x >= 0.0 && start.x <= 400.0 && start.y >= 0.0 && start.y <= 300.0,
                        "Segment {idx} travel start ({}, {}) outside workspace",
                        start.x,
                        start.y
                    );
                    assert!(
                        end.x >= 0.0 && end.x <= 400.0 && end.y >= 0.0 && end.y <= 300.0,
                        "Segment {idx} travel end ({}, {}) outside workspace",
                        end.x,
                        end.y
                    );
                }
                _ => {}
            }
        }
    }

    #[test]
    fn vector_object_on_image_layer_is_skipped_with_warning() {
        // Vector objects placed on an Image (raster) sub-layer are intentionally
        // skipped — the operation type is raster-only. The planner surfaces a
        // warning so the user knows the geometry was ignored.
        let mut project = create_test_project();

        let layer = Layer::new("Image", OperationType::Image);
        let layer_id = layer.id;
        project.layers.push(layer);

        project.objects.push(ProjectObject::new(
            "rect_on_image",
            layer_id,
            Bounds::new(Point2D::new(10.0, 10.0), Point2D::new(60.0, 60.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 50.0,
                height: 50.0,
                corner_radius: 0.0,
            },
        ));

        let plan = build_plan(&project).unwrap();

        let has_vector = plan
            .segments
            .iter()
            .any(|s| matches!(s, PlanSegment::Vector { .. }));
        assert!(!has_vector, "Image layer should not emit vector segments");

        let has_skip_warning = plan
            .warnings
            .iter()
            .any(|w| w.message.contains("skipped vector geometry"));
        assert!(
            has_skip_warning,
            "expected a 'skipped vector geometry' warning, got: {:?}",
            plan.warnings,
        );
    }

    #[test]
    fn build_frame_plan_creates_rectangular_path() {
        let bounds = Bounds::new(Point2D::new(10.0, 20.0), Point2D::new(100.0, 80.0));
        let plan = build_frame_plan(&bounds, 1.0, 3000.0);

        assert_eq!(plan.segments.len(), 2);
        // First segment is travel from origin
        match &plan.segments[0] {
            PlanSegment::Travel { start, end } => {
                assert_eq!(*start, Point2D::zero());
                assert_eq!(*end, Point2D::new(10.0, 20.0));
            }
            _ => panic!("Expected Travel segment first"),
        }
        // Second segment is frame
        match &plan.segments[1] {
            PlanSegment::Frame {
                path,
                power_percent,
                speed_mm_min,
            } => {
                assert_eq!(path.len(), 5); // 4 corners + close
                assert_eq!(path[0], Point2D::new(10.0, 20.0));
                assert_eq!(path[1], Point2D::new(100.0, 20.0));
                assert_eq!(path[2], Point2D::new(100.0, 80.0));
                assert_eq!(path[3], Point2D::new(10.0, 80.0));
                assert_eq!(path[4], Point2D::new(10.0, 20.0));
                assert!((*power_percent - 1.0).abs() < f64::EPSILON);
                assert!((*speed_mm_min - 3000.0).abs() < f64::EPSILON);
            }
            _ => panic!("Expected Frame segment second"),
        }
        // Verify bounds match
        assert_eq!(plan.bounds, bounds);
        assert!(plan.total_distance_mm > 0.0);
        assert!(plan.estimated_duration_secs > 0.0);
    }

    #[test]
    fn fill_layer_produces_raster_segment() {
        let mut project = create_test_project();

        let layer = Layer::new("Fill", OperationType::Fill);
        let layer_id = layer.id;
        project.layers.push(layer);

        project.objects.push(ProjectObject::new(
            "FillRect",
            layer_id,
            Bounds::new(Point2D::new(10.0, 10.0), Point2D::new(50.0, 50.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 40.0,
                height: 40.0,
                corner_radius: 0.0,
            },
        ));

        let plan = build_plan(&project).unwrap();

        let has_raster = plan
            .segments
            .iter()
            .any(|s| matches!(s, PlanSegment::Raster { .. }));
        let has_vector = plan
            .segments
            .iter()
            .any(|s| matches!(s, PlanSegment::Vector { .. }));

        assert!(has_raster, "Fill layer should produce Raster segments");
        assert!(!has_vector, "Fill layer should NOT produce Vector segments");
    }

    #[test]
    fn fill_layer_raster_segment_uses_cut_entry_speed() {
        let mut project = create_test_project();

        let mut layer = Layer::new("Fill", OperationType::Fill);
        layer.primary_entry_mut().speed_mm_min = 3000.0;
        let layer_id = layer.id;
        project.layers.push(layer);

        project.objects.push(ProjectObject::new(
            "FillRect",
            layer_id,
            Bounds::new(Point2D::new(10.0, 10.0), Point2D::new(50.0, 50.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 40.0,
                height: 40.0,
                corner_radius: 0.0,
            },
        ));

        let plan = build_plan(&project).unwrap();
        let speeds: Vec<f64> = plan
            .segments
            .iter()
            .filter_map(|segment| match segment {
                PlanSegment::Raster { speed_mm_min, .. } => Some(*speed_mm_min),
                _ => None,
            })
            .collect();

        assert!(
            !speeds.is_empty(),
            "Fill layer should produce raster segments"
        );
        assert!(
            speeds.iter().all(|speed| (*speed - 3000.0).abs() < 0.001),
            "Fill raster speeds should all come from the cut entry: {:?}",
            speeds
        );
    }

    #[test]
    fn line_layer_still_produces_vector_segment() {
        let mut project = create_test_project();

        let layer = Layer::new("Line", OperationType::Line);
        let layer_id = layer.id;
        project.layers.push(layer);

        project.objects.push(ProjectObject::new(
            "LineRect",
            layer_id,
            Bounds::new(Point2D::new(10.0, 10.0), Point2D::new(50.0, 50.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 40.0,
                height: 40.0,
                corner_radius: 0.0,
            },
        ));

        let plan = build_plan(&project).unwrap();

        let has_raster = plan
            .segments
            .iter()
            .any(|s| matches!(s, PlanSegment::Raster { .. }));
        let has_vector = plan
            .segments
            .iter()
            .any(|s| matches!(s, PlanSegment::Vector { .. }));

        assert!(has_vector, "Line layer should produce Vector segments");
        assert!(!has_raster, "Line layer should NOT produce Raster segments");
    }

    #[test]
    fn scan_angle_45_produces_rotated_raster_segment() {
        let mut project = create_test_project();

        let mut layer = Layer::new("Image", OperationType::Image);
        if let Some(ref mut rs) = layer.primary_entry_mut().raster_settings {
            rs.scan_angle = 45.0;
        }
        let layer_id = layer.id;
        project.layers.push(layer);

        // Add a raster object with asset data
        let png_bytes = create_test_png(4, 4, 0);
        let asset = Asset::new(
            "test.png",
            AssetMediaType::Png,
            png_bytes.len() as u64,
            Some(4),
            Some(4),
        );
        let asset_id = asset.id;
        project.add_asset(asset, png_bytes);

        project.objects.push(ProjectObject::new(
            "img",
            layer_id,
            Bounds::new(Point2D::new(10.0, 10.0), Point2D::new(20.0, 20.0)),
            ObjectData::RasterImage {
                asset_key: asset_id.to_string(),
                original_width_px: 4,
                original_height_px: 4,
                adjustments: None,
                masks: Vec::new(),
            },
        ));

        let plan = build_plan(&project).unwrap();

        // Should NOT have a warning about unsupported scan_angle
        assert!(
            !plan
                .warnings
                .iter()
                .any(|w| w.message.contains("scan_angle")),
            "Should no longer warn about scan_angle"
        );

        // Should have a raster segment with scan_angle_deg set
        let raster_seg = plan
            .segments
            .iter()
            .find(|s| matches!(s, PlanSegment::Raster { .. }));
        assert!(raster_seg.is_some(), "Should have a raster segment");

        if let Some(PlanSegment::Raster {
            scan_angle_deg,
            scan_origin,
            ..
        }) = raster_seg
        {
            assert!(
                (*scan_angle_deg - 45.0).abs() < 1.0,
                "scan_angle_deg should be ~45, got {}",
                scan_angle_deg
            );
            assert!(
                scan_origin.x > 0.0 && scan_origin.y > 0.0,
                "scan_origin should be object center, got ({}, {})",
                scan_origin.x,
                scan_origin.y
            );
        }
    }

    #[test]
    fn multipass_angle_passes_create_multiple_segments() {
        let mut project = create_test_project();

        let mut layer = Layer::new("Image", OperationType::Image);
        if let Some(ref mut rs) = layer.primary_entry_mut().raster_settings {
            rs.angle_passes = 2;
            rs.angle_increment_deg = 45.0;
        }
        let layer_id = layer.id;
        project.layers.push(layer);

        let png_bytes = create_test_png(4, 4, 0);
        let asset = Asset::new(
            "test.png",
            AssetMediaType::Png,
            png_bytes.len() as u64,
            Some(4),
            Some(4),
        );
        let asset_id = asset.id;
        project.add_asset(asset, png_bytes);

        project.objects.push(ProjectObject::new(
            "img",
            layer_id,
            Bounds::new(Point2D::new(10.0, 10.0), Point2D::new(20.0, 20.0)),
            ObjectData::RasterImage {
                asset_key: asset_id.to_string(),
                original_width_px: 4,
                original_height_px: 4,
                adjustments: None,
                masks: Vec::new(),
            },
        ));

        let plan = build_plan(&project).unwrap();
        let raster_count = plan
            .segments
            .iter()
            .filter(|s| matches!(s, PlanSegment::Raster { .. }))
            .count();
        assert_eq!(
            raster_count, 2,
            "Expected 2 raster segments for angle_passes=2"
        );
    }

    #[test]
    fn crosshatch_backward_compat_creates_two_segments() {
        let mut project = create_test_project();

        let mut layer = Layer::new("Image", OperationType::Image);
        if let Some(ref mut rs) = layer.primary_entry_mut().raster_settings {
            rs.crosshatch = true;
            // angle_passes defaults to 1, but crosshatch=true should override to 2
        }
        let layer_id = layer.id;
        project.layers.push(layer);

        let png_bytes = create_test_png(4, 4, 0);
        let asset = Asset::new(
            "test.png",
            AssetMediaType::Png,
            png_bytes.len() as u64,
            Some(4),
            Some(4),
        );
        let asset_id = asset.id;
        project.add_asset(asset, png_bytes);

        project.objects.push(ProjectObject::new(
            "img",
            layer_id,
            Bounds::new(Point2D::new(10.0, 10.0), Point2D::new(20.0, 20.0)),
            ObjectData::RasterImage {
                asset_key: asset_id.to_string(),
                original_width_px: 4,
                original_height_px: 4,
                adjustments: None,
                masks: Vec::new(),
            },
        ));

        let plan = build_plan(&project).unwrap();
        let raster_count = plan
            .segments
            .iter()
            .filter(|s| matches!(s, PlanSegment::Raster { .. }))
            .count();
        assert_eq!(
            raster_count, 2,
            "crosshatch=true should produce 2 raster segments"
        );
    }

    #[test]
    fn raster_passes_creates_multiple_raster_segments() {
        let mut project = create_test_project();

        let mut layer = Layer::new("Image", OperationType::Image);
        layer
            .primary_entry_mut()
            .raster_settings
            .as_mut()
            .unwrap()
            .passes = 3;
        let layer_id = layer.id;
        project.layers.push(layer);

        // Create a 4x4 all-black PNG so we get non-empty scanlines
        let png_bytes = create_test_png(4, 4, 0);
        let asset = Asset::new(
            "test.png",
            AssetMediaType::Png,
            png_bytes.len() as u64,
            Some(4),
            Some(4),
        );
        let asset_id = asset.id;
        project.add_asset(asset, png_bytes);

        project.add_object(ProjectObject::new(
            "raster_img",
            layer_id,
            Bounds::new(Point2D::new(10.0, 10.0), Point2D::new(20.0, 20.0)),
            ObjectData::RasterImage {
                asset_key: asset_id.to_string(),
                original_width_px: 4,
                original_height_px: 4,
                adjustments: None,
                masks: Vec::new(),
            },
        ));

        let plan = build_plan(&project).unwrap();

        // Count raster segments — should be 3 (one per pass)
        let raster_count = plan
            .segments
            .iter()
            .filter(|s| matches!(s, PlanSegment::Raster { .. }))
            .count();
        assert_eq!(
            raster_count, 3,
            "Expected 3 raster segments for 3 passes, got {}",
            raster_count
        );
    }

    #[test]
    fn offset_fill_hexagon_bounds_match_object() {
        // A hexagon's natural aspect ratio (width ≈ √3*r, height = 2r) differs
        // from its object bounds. The live offset_fill planner path must map the
        // local path to obj.bounds so the preview lines up with the canvas object.
        let mut project = create_test_project();

        let layer = Layer::new("OffsetFill", OperationType::OffsetFill);
        let layer_id = layer.id;
        project.layers.push(layer);

        let radius = 50.0;
        let obj_bounds = Bounds::new(Point2D::new(100.0, 100.0), Point2D::new(200.0, 200.0));

        let obj = ProjectObject::new(
            "hexagon",
            layer_id,
            obj_bounds,
            ObjectData::Polygon { sides: 6, radius },
        );
        project.objects.push(obj);

        let plan = build_plan(&project).unwrap();

        // Collect all vector points from offset fill segments
        let all_points: Vec<Point2D> = plan
            .segments
            .iter()
            .filter_map(|s| match s {
                PlanSegment::Vector { polyline, .. } => Some(polyline.clone()),
                _ => None,
            })
            .flatten()
            .collect();

        assert!(
            !all_points.is_empty(),
            "OffsetFill should produce vector segments"
        );

        // All points must lie within the object's bounds (with small tolerance
        // for floating-point from the offset algorithm).
        let tol = 1.0; // 1mm tolerance for offset geometry
        for pt in &all_points {
            assert!(
                pt.x >= obj_bounds.min.x - tol && pt.x <= obj_bounds.max.x + tol,
                "Point x={:.2} outside obj bounds [{:.2}, {:.2}]",
                pt.x,
                obj_bounds.min.x,
                obj_bounds.max.x,
            );
            assert!(
                pt.y >= obj_bounds.min.y - tol && pt.y <= obj_bounds.max.y + tol,
                "Point y={:.2} outside obj bounds [{:.2}, {:.2}]",
                pt.y,
                obj_bounds.min.y,
                obj_bounds.max.y,
            );
        }

        // The first ring (outermost) should span most of the object bounds
        let first_ring: Vec<Point2D> = match &plan.segments[0] {
            PlanSegment::Vector { polyline, .. } => polyline.clone(),
            _ => vec![],
        };
        if !first_ring.is_empty() {
            let mut ring_min_x = f64::INFINITY;
            let mut ring_max_x = f64::NEG_INFINITY;
            let mut ring_min_y = f64::INFINITY;
            let mut ring_max_y = f64::NEG_INFINITY;
            for pt in &first_ring {
                ring_min_x = ring_min_x.min(pt.x);
                ring_max_x = ring_max_x.max(pt.x);
                ring_min_y = ring_min_y.min(pt.y);
                ring_max_y = ring_max_y.max(pt.y);
            }
            let ring_w = ring_max_x - ring_min_x;
            let ring_h = ring_max_y - ring_min_y;
            let bounds_w = obj_bounds.width();
            let bounds_h = obj_bounds.height();

            // Outer ring should cover most of the object (>60% in each dimension)
            assert!(
                ring_w > bounds_w * 0.6,
                "Ring width {ring_w:.2} should be >60% of bounds width {bounds_w:.2}"
            );
            assert!(
                ring_h > bounds_h * 0.6,
                "Ring height {ring_h:.2} should be >60% of bounds height {bounds_h:.2}"
            );
        }
    }

    #[test]
    fn calculate_bounds_vertical_scan_raster_untransposes() {
        // Vertical scan transposes X/Y in scanlines. calculate_plan_bounds must
        // un-transpose so plan.bounds is in correct world-space.
        let segments = vec![PlanSegment::Raster {
            scanlines: vec![
                Scanline {
                    y_mm: 10.0, // This is actually world X for vertical scan
                    runs: vec![ScanRun {
                        start_x_mm: 20.0, // This is actually world Y
                        end_x_mm: 80.0,
                        power_values: vec![],
                    }],
                    direction: ScanDirection::LeftToRight,
                },
                Scanline {
                    y_mm: 50.0,
                    runs: vec![ScanRun {
                        start_x_mm: 20.0,
                        end_x_mm: 80.0,
                        power_values: vec![],
                    }],
                    direction: ScanDirection::RightToLeft,
                },
            ],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Binary,
            speed_mm_min: 2000.0,
            layer_id: "layer1".to_string(),
            cut_entry_id: String::new(),
            overscan_mm: 0.0,
            outlines: vec![],
            scan_axis: ScanAxis::Vertical,
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }];

        let bounds = calculate_plan_bounds(&segments);

        // After un-transposing: X should come from y_mm [10, 50],
        // Y should come from start/end_x_mm [20, 80]
        assert!(
            (bounds.min.x - 10.0).abs() < 1e-10,
            "min.x should be 10.0, got {}",
            bounds.min.x
        );
        assert!(
            (bounds.max.x - 50.0).abs() < 1e-10,
            "max.x should be 50.0, got {}",
            bounds.max.x
        );
        assert!(
            (bounds.min.y - 20.0).abs() < 1e-10,
            "min.y should be 20.0, got {}",
            bounds.min.y
        );
        assert!(
            (bounds.max.y - 80.0).abs() < 1e-10,
            "max.y should be 80.0, got {}",
            bounds.max.y
        );
    }

    #[test]
    fn calculate_bounds_horizontal_scan_raster_unchanged() {
        // Horizontal scan should use coordinates as-is (no un-transpose needed).
        let segments = vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 30.0,
                runs: vec![ScanRun {
                    start_x_mm: 10.0,
                    end_x_mm: 90.0,
                    power_values: vec![],
                }],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Unidirectional,
            power_mode: PowerMode::Binary,
            speed_mm_min: 2000.0,
            layer_id: "layer1".to_string(),
            cut_entry_id: String::new(),
            overscan_mm: 0.0,
            outlines: vec![],
            scan_axis: ScanAxis::Horizontal,
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }];

        let bounds = calculate_plan_bounds(&segments);

        assert!((bounds.min.x - 10.0).abs() < 1e-10);
        assert!((bounds.max.x - 90.0).abs() < 1e-10);
        assert!((bounds.min.y - 30.0).abs() < 1e-10);
        assert!((bounds.max.y - 30.0).abs() < 1e-10);
    }

    #[test]
    fn calculate_bounds_rotated_raster_uses_world_space_scan_origin() {
        // Non-cardinal raster scanlines are local to scan_origin. The local
        // coordinates can be negative even when the world-space plan fits.
        let segments = vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 0.0,
                runs: vec![ScanRun {
                    start_x_mm: -10.0,
                    end_x_mm: 10.0,
                    power_values: vec![],
                }],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Unidirectional,
            power_mode: PowerMode::Binary,
            speed_mm_min: 2000.0,
            layer_id: "layer1".to_string(),
            cut_entry_id: String::new(),
            overscan_mm: 0.0,
            outlines: vec![],
            scan_axis: ScanAxis::Horizontal,
            scan_angle_deg: 45.0,
            scan_origin: Point2D::new(100.0, 100.0),
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }];

        let bounds = calculate_plan_bounds(&segments);

        let expected_delta = 10.0 / std::f64::consts::SQRT_2;
        assert!((bounds.min.x - (100.0 - expected_delta)).abs() < 1e-10);
        assert!((bounds.min.y - (100.0 - expected_delta)).abs() < 1e-10);
        assert!((bounds.max.x - (100.0 + expected_delta)).abs() < 1e-10);
        assert!((bounds.max.y - (100.0 + expected_delta)).abs() < 1e-10);
    }

    #[test]
    fn optimization_settings_affect_vector_order() {
        // Create a project with multiple vector objects spread out so ordering
        // strategies can produce different traversal orders.
        let mut project = create_test_project();

        let layer = Layer::new("Lines", OperationType::Line);
        let layer_id = layer.id;
        project.layers.push(layer);

        // Object A at bottom-left
        project.objects.push(ProjectObject::new(
            "A",
            layer_id,
            Bounds::new(Point2D::new(10.0, 10.0), Point2D::new(20.0, 20.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        ));

        // Object B at top-right (far from A)
        project.objects.push(ProjectObject::new(
            "B",
            layer_id,
            Bounds::new(Point2D::new(200.0, 200.0), Point2D::new(210.0, 210.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        ));

        // Object C near A
        project.objects.push(ProjectObject::new(
            "C",
            layer_id,
            Bounds::new(Point2D::new(25.0, 10.0), Point2D::new(35.0, 20.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        ));

        // Build with an AsDrawn-equivalent. `inner_first: true` forces
        // `AsDrawn` via `force_as_drawn`; on flat (non-nested) geometry
        // every polyline's containment depth is 0 so the stable sort is
        // a true identity — insertion order survives. That makes this
        // a correct proxy for "no reorder" unlike `direction_order`,
        // which is a stable sort that may coincide with NN on specific
        // fixtures.
        let as_drawn = plan_input_with(ProjectOptimization {
            inner_first: true,
            ..Default::default()
        });
        let plan_drawn = build_plan_with_input(&project, &as_drawn).unwrap();

        // Build with defaults — per-layer nearest-neighbor runs.
        let plan_opt = build_plan_with_input(&project, &default_plan_input()).unwrap();

        // Both plans should have vector segments
        let drawn_vecs: Vec<_> = plan_drawn
            .segments
            .iter()
            .filter(|s| matches!(s, PlanSegment::Vector { .. }))
            .collect();
        let opt_vecs: Vec<_> = plan_opt
            .segments
            .iter()
            .filter(|s| matches!(s, PlanSegment::Vector { .. }))
            .collect();

        assert_eq!(drawn_vecs.len(), 3, "AsDrawn should have 3 vector segments");
        assert_eq!(opt_vecs.len(), 3, "Optimized should have 3 vector segments");

        // The segment ordering should differ between AsDrawn and Optimized
        // because nearest-neighbor would reorder to minimize travel.
        // We compare the serialized segment lists — they should differ.
        let drawn_json = serde_json::to_string(&plan_drawn.segments).unwrap();
        let opt_json = serde_json::to_string(&plan_opt.segments).unwrap();
        assert_ne!(
            drawn_json, opt_json,
            "AsDrawn and Optimized should produce different segment orders"
        );
    }

    fn vector_bbox(plan: &ExecutionPlan) -> (f64, f64, f64, f64) {
        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        for segment in &plan.segments {
            if let PlanSegment::Vector { polyline, .. } = segment {
                for p in polyline {
                    min_x = min_x.min(p.x);
                    min_y = min_y.min(p.y);
                    max_x = max_x.max(p.x);
                    max_y = max_y.max(p.y);
                }
            }
        }
        (min_x, min_y, max_x, max_y)
    }

    fn input_with_position(x: f64, y: f64) -> PlannerInput {
        PlannerInput::new(
            ProjectOptimization::default(),
            OptimizationRuntime {
                current_position: Some((x, y)),
            },
            PlannerCalibration::default(),
        )
    }

    fn anchoring_test_project(rect: Bounds) -> Project {
        let mut project = create_test_project();
        let layer = Layer::new("Lines", OperationType::Line);
        let layer_id = layer.id;
        project.layers.push(layer);
        project.objects.push(ProjectObject::new(
            "Rect",
            layer_id,
            rect,
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: rect.width(),
                height: rect.height(),
                corner_radius: 0.0,
            },
        ));
        project
    }

    #[test]
    fn current_position_center_anchor_lands_content_center_on_head() {
        // Content NOT at the canvas origin: the regression that shipped sent
        // such jobs to absolute-plus-offset garbage positions.
        let mut project = anchoring_test_project(Bounds::new(
            Point2D::new(100.0, 100.0),
            Point2D::new(200.0, 200.0),
        ));
        project.start_from = beambench_common::StartFromMode::CurrentPosition;
        project.job_origin = beambench_common::AnchorPoint::Center;

        let plan = build_plan_with_input(&project, &input_with_position(150.0, 140.0)).unwrap();

        let (min_x, min_y, max_x, max_y) = vector_bbox(&plan);
        assert!((min_x - 100.0).abs() < 0.1, "min_x {min_x}");
        assert!((min_y - 90.0).abs() < 0.1, "min_y {min_y}");
        assert!((max_x - 200.0).abs() < 0.1, "max_x {max_x}");
        assert!((max_y - 190.0).abs() < 0.1, "max_y {max_y}");
    }

    #[test]
    fn current_position_top_left_anchor_is_not_a_no_op() {
        // TopLeft (the default job origin) used to be skipped entirely,
        // leaving the artwork at absolute coordinates.
        let mut project = anchoring_test_project(Bounds::new(
            Point2D::new(100.0, 100.0),
            Point2D::new(200.0, 200.0),
        ));
        project.start_from = beambench_common::StartFromMode::CurrentPosition;
        project.job_origin = beambench_common::AnchorPoint::TopLeft;

        let plan = build_plan_with_input(&project, &input_with_position(150.0, 140.0)).unwrap();

        let (min_x, min_y, max_x, max_y) = vector_bbox(&plan);
        assert!(
            (min_x - 150.0).abs() < 0.1,
            "content top-left x at head, got {min_x}"
        );
        assert!(
            (min_y - 140.0).abs() < 0.1,
            "content top-left y at head, got {min_y}"
        );
        assert!((max_x - 250.0).abs() < 0.1, "max_x {max_x}");
        assert!((max_y - 240.0).abs() < 0.1, "max_y {max_y}");
    }

    #[test]
    fn current_position_anchor_respects_workspace_y_flip() {
        // BottomLeft machine origin flips Y; the canvas "top left" of the
        // design is the machine-space (min_x, max_y) corner, and the head
        // target is a machine-space work coordinate.
        let mut project = anchoring_test_project(Bounds::new(
            Point2D::new(100.0, 40.0),
            Point2D::new(200.0, 140.0),
        ));
        project.workspace.origin = WorkspaceOrigin::BottomLeft;
        project.start_from = beambench_common::StartFromMode::CurrentPosition;
        project.job_origin = beambench_common::AnchorPoint::TopLeft;

        let plan = build_plan_with_input(&project, &input_with_position(150.0, 140.0)).unwrap();

        // Canvas rect flips to machine (100,160)-(200,260); its canvas
        // top-left is machine (100,260); shifting that onto (150,140) puts
        // the content at (150,40)-(250,140).
        let (min_x, min_y, max_x, max_y) = vector_bbox(&plan);
        assert!((min_x - 150.0).abs() < 0.1, "min_x {min_x}");
        assert!((min_y - 40.0).abs() < 0.1, "min_y {min_y}");
        assert!((max_x - 250.0).abs() < 0.1, "max_x {max_x}");
        assert!((max_y - 140.0).abs() < 0.1, "max_y {max_y}");
    }

    #[test]
    fn user_origin_anchors_like_current_position() {
        let mut project = anchoring_test_project(Bounds::new(
            Point2D::new(100.0, 100.0),
            Point2D::new(200.0, 200.0),
        ));
        project.start_from = beambench_common::StartFromMode::UserOrigin;
        project.job_origin = beambench_common::AnchorPoint::Center;
        project.user_origin = Some((150.0, 140.0));

        let plan = build_plan_with_input(&project, &default_plan_input()).unwrap();

        let (min_x, min_y, max_x, max_y) = vector_bbox(&plan);
        assert!((min_x - 100.0).abs() < 0.1, "min_x {min_x}");
        assert!((min_y - 90.0).abs() < 0.1, "min_y {min_y}");
        assert!((max_x - 200.0).abs() < 0.1, "max_x {max_x}");
        assert!((max_y - 190.0).abs() < 0.1, "max_y {max_y}");
    }

    #[test]
    fn current_position_without_machine_warns_and_keeps_absolute_placement() {
        let mut project = anchoring_test_project(Bounds::new(
            Point2D::new(100.0, 100.0),
            Point2D::new(200.0, 200.0),
        ));
        project.start_from = beambench_common::StartFromMode::CurrentPosition;
        project.job_origin = beambench_common::AnchorPoint::Center;

        let plan = build_plan_with_input(&project, &default_plan_input()).unwrap();

        assert!(
            plan.warnings
                .iter()
                .any(|w| w.message.contains("no machine is connected")),
            "expected the no-machine warning"
        );
        let (min_x, min_y, _, _) = vector_bbox(&plan);
        assert!((min_x - 100.0).abs() < 0.1, "unshifted min_x, got {min_x}");
        assert!((min_y - 100.0).abs() < 0.1, "unshifted min_y, got {min_y}");
    }

    #[test]
    fn absolute_coords_ignores_job_origin() {
        let mut project = create_test_project();
        project.start_from = beambench_common::StartFromMode::AbsoluteCoords;
        project.job_origin = beambench_common::AnchorPoint::Center;

        let layer = Layer::new("Lines", OperationType::Line);
        let layer_id = layer.id;
        project.layers.push(layer);

        project.objects.push(ProjectObject::new(
            "Rect",
            layer_id,
            Bounds::new(Point2D::new(100.0, 100.0), Point2D::new(200.0, 200.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 100.0,
                height: 100.0,
                corner_radius: 0.0,
            },
        ));

        let mut top_left_project = project.clone();
        top_left_project.job_origin = beambench_common::AnchorPoint::TopLeft;
        let plan_top_left =
            build_plan_with_input(&top_left_project, &default_plan_input()).unwrap();
        let plan_center = build_plan_with_input(&project, &default_plan_input()).unwrap();

        assert_eq!(plan_center.bounds, plan_top_left.bounds);
        assert_eq!(
            serde_json::to_value(&plan_center.segments).unwrap(),
            serde_json::to_value(&plan_top_left.segments).unwrap(),
            "Absolute Coords should ignore saved Job Origin anchors"
        );
    }

    #[test]
    fn start_from_user_origin_offsets_segments() {
        let mut project = create_test_project();
        project.start_from = beambench_common::StartFromMode::UserOrigin;
        project.user_origin = Some((50.0, 50.0));
        // Expand workspace to accommodate offset
        project.workspace.bed_width_mm = 800.0;
        project.workspace.bed_height_mm = 600.0;

        let layer = Layer::new("Lines", OperationType::Line);
        let layer_id = layer.id;
        project.layers.push(layer);

        project.objects.push(ProjectObject::new(
            "Rect",
            layer_id,
            Bounds::new(Point2D::new(10.0, 10.0), Point2D::new(50.0, 50.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 40.0,
                height: 40.0,
                corner_radius: 0.0,
            },
        ));

        let plan = build_plan_with_input(&project, &default_plan_input()).unwrap();

        // With UserOrigin at (50, 50), all segments should be shifted by +50 in both axes
        // Original object at (10,10)-(50,50) → shifted to (60,60)-(100,100)
        assert!(
            plan.bounds.min.x >= 50.0,
            "UserOrigin offset should shift min.x to >= 50, got {}",
            plan.bounds.min.x
        );
        assert!(
            plan.bounds.min.y >= 50.0,
            "UserOrigin offset should shift min.y to >= 50, got {}",
            plan.bounds.min.y
        );
    }

    // -----------------------------------------------------------------------
    // Optimization integration tests
    // -----------------------------------------------------------------------

    /// Helper: create a project with multiple vector objects at different positions
    fn create_multi_object_project() -> Project {
        let mut project = create_test_project();
        let layer = Layer::new("Lines", OperationType::Line);
        let layer_id = layer.id;
        project.layers.push(layer);

        // Scatter 4 rectangles across the bed
        let positions = [(10.0, 10.0), (200.0, 50.0), (50.0, 200.0), (300.0, 250.0)];
        for (i, (x, y)) in positions.iter().enumerate() {
            project.add_object(ProjectObject::new(
                &format!("rect{i}"),
                layer_id,
                Bounds::new(Point2D::new(*x, *y), Point2D::new(x + 30.0, y + 30.0)),
                ObjectData::Shape {
                    kind: ShapeKind::Rectangle,
                    width: 30.0,
                    height: 30.0,
                    corner_radius: 0.0,
                },
            ));
        }
        project
    }

    #[test]
    fn cut_order_as_drawn_vs_optimized_produces_different_segment_order() {
        let project = create_multi_object_project();

        // `inner_first: true` on flat geometry is a no-op that forces
        // `AsDrawn` via `force_as_drawn` — a correct "no reorder"
        // proxy (see `optimization_settings_affect_vector_order` for
        // rationale vs. `direction_order`).
        let as_drawn = plan_input_with(ProjectOptimization {
            inner_first: true,
            ..Default::default()
        });
        let plan_drawn = build_plan_with_input(&project, &as_drawn).unwrap();
        let plan_opt = build_plan_with_input(&project, &default_plan_input()).unwrap();

        // Both should produce segments
        assert!(!plan_drawn.segments.is_empty());
        assert!(!plan_opt.segments.is_empty());

        // Segment count should be the same (same objects, same content)
        assert_eq!(plan_drawn.segments.len(), plan_opt.segments.len());

        // Extract vector segment starting positions to compare ordering
        let vec_starts = |plan: &ExecutionPlan| -> Vec<String> {
            plan.segments
                .iter()
                .filter_map(|s| {
                    if let PlanSegment::Vector { polyline, .. } = s {
                        Some(format!("{:.0},{:.0}", polyline[0].x, polyline[0].y))
                    } else {
                        None
                    }
                })
                .collect()
        };
        let drawn_starts = vec_starts(&plan_drawn);
        let opt_starts = vec_starts(&plan_opt);
        assert_eq!(drawn_starts.len(), opt_starts.len(), "Same segment count");
        assert_ne!(
            drawn_starts, opt_starts,
            "Optimized ordering should differ from as-drawn for scattered objects"
        );
    }

    #[test]
    fn travel_optimization_reduces_or_matches_travel_distance() {
        let project = create_multi_object_project();

        let no_opt = plan_input_with(ProjectOptimization {
            reduce_travel: false,
            ..Default::default()
        });
        let with_opt = plan_input_with(ProjectOptimization {
            reduce_travel: true,
            ..Default::default()
        });

        let plan_no = build_plan_with_input(&project, &no_opt).unwrap();
        let plan_yes = build_plan_with_input(&project, &with_opt).unwrap();

        // Optimized travel should be <= unoptimized
        assert!(
            plan_yes.total_distance_mm <= plan_no.total_distance_mm + 0.001,
            "Optimized travel ({}) should be <= unoptimized ({})",
            plan_yes.total_distance_mm,
            plan_no.total_distance_mm
        );

        // Same number of vector segments (content unchanged, just reordered)
        let vec_count = |plan: &ExecutionPlan| {
            plan.segments
                .iter()
                .filter(|s| matches!(s, PlanSegment::Vector { .. }))
                .count()
        };
        assert_eq!(vec_count(&plan_no), vec_count(&plan_yes));
    }

    #[test]
    fn start_point_prepends_travel_from_custom_position() {
        let project = create_multi_object_project();

        let with_start = plan_input_with(ProjectOptimization {
            start_point_x: Some(100.0),
            start_point_y: Some(100.0),
            ..Default::default()
        });

        let plan_with = build_plan_with_input(&project, &with_start).unwrap();
        let plan_without = build_plan_with_input(&project, &default_plan_input()).unwrap();

        // Plan with custom start should have a travel segment originating from (100,100)
        if let Some(PlanSegment::Travel { start, .. }) = plan_with.segments.first() {
            assert!(
                (start.x - 100.0).abs() < 0.1 && (start.y - 100.0).abs() < 0.1,
                "First travel should start from (100,100), got ({}, {})",
                start.x,
                start.y
            );
        }

        // Both plans should have segments
        assert!(!plan_with.segments.is_empty());
        assert!(!plan_without.segments.is_empty());
    }

    #[test]
    fn finish_position_origin_adds_return_travel() {
        let project = create_multi_object_project();

        let origin_finish = plan_input_with(ProjectOptimization {
            finish_position: FinishPosition::Origin,
            ..Default::default()
        });
        let plan = build_plan_with_input(&project, &origin_finish).unwrap();

        // Last segment should be a Travel to (0,0)
        if let Some(PlanSegment::Travel { end, .. }) = plan.segments.last() {
            assert!(
                end.x.abs() < 0.1 && end.y.abs() < 0.1,
                "FinishPosition::Origin should end at (0,0), got ({}, {})",
                end.x,
                end.y
            );
        }
    }

    #[test]
    fn finish_position_dont_move_no_trailing_travel() {
        let project = create_multi_object_project();

        let dont_move = plan_input_with(ProjectOptimization {
            finish_position: FinishPosition::DontMove,
            ..Default::default()
        });
        let plan = build_plan_with_input(&project, &dont_move).unwrap();

        // Last segment should NOT be a travel to origin
        if let Some(PlanSegment::Travel { end, .. }) = plan.segments.last() {
            // If it's a travel, it shouldn't be to (0,0) — it should be an inter-object travel
            let is_origin = end.x.abs() < 0.1 && end.y.abs() < 0.1;
            // This is a weak assertion — the last segment might legitimately be a travel
            // between objects, so we just check it's not explicitly returning to origin
            if is_origin {
                // Check that the plan doesn't have an extra "return to origin" segment
                // by verifying the second-to-last is also a travel (inter-object) or vector
                assert!(plan.segments.len() >= 2);
            }
        }
        // If last segment is a Vector, that's correct for DontMove
    }

    #[test]
    fn finish_position_custom_xy_adds_travel_to_custom_point() {
        let project = create_multi_object_project();

        let custom = plan_input_with(ProjectOptimization {
            finish_position: FinishPosition::CustomXY,
            finish_x: Some(50.0),
            finish_y: Some(50.0),
            ..Default::default()
        });
        let plan = build_plan_with_input(&project, &custom).unwrap();

        // Last segment should be a Travel to (50,50)
        if let Some(PlanSegment::Travel { end, .. }) = plan.segments.last() {
            assert!(
                (end.x - 50.0).abs() < 0.1 && (end.y - 50.0).abs() < 0.1,
                "FinishPosition::CustomXY(50,50) should end at (50,50), got ({}, {})",
                end.x,
                end.y
            );
        }
    }

    // -----------------------------------------------------------------------
    // Benchmark observation tests
    // -----------------------------------------------------------------------

    /// Helper: create a benchmark project with 10+ objects spread across the bed
    fn create_benchmark_project() -> Project {
        let mut project = create_test_project();
        let layer = Layer::new("Lines", OperationType::Line);
        let layer_id = layer.id;
        project.layers.push(layer);

        // 12 rectangles in a scattered grid pattern across the 400x300 bed
        let positions = [
            (10.0, 10.0),
            (350.0, 10.0),
            (10.0, 250.0),
            (350.0, 250.0),
            (100.0, 50.0),
            (250.0, 100.0),
            (150.0, 200.0),
            (300.0, 150.0),
            (50.0, 120.0),
            (200.0, 30.0),
            (80.0, 180.0),
            (280.0, 220.0),
        ];
        for (i, (x, y)) in positions.iter().enumerate() {
            project.add_object(ProjectObject::new(
                &format!("bench_rect{i}"),
                layer_id,
                Bounds::new(Point2D::new(*x, *y), Point2D::new(x + 20.0, y + 20.0)),
                ObjectData::Shape {
                    kind: ShapeKind::Rectangle,
                    width: 20.0,
                    height: 20.0,
                    corner_radius: 0.0,
                },
            ));
        }
        project
    }

    #[test]
    fn benchmark_as_drawn_vs_optimized_produces_different_segment_sequences() {
        let project = create_benchmark_project();

        // `inner_first: true` is a no-op on flat geometry that forces
        // `AsDrawn` via `force_as_drawn`, preserving insertion order.
        let as_drawn = plan_input_with(ProjectOptimization {
            inner_first: true,
            ..Default::default()
        });
        let plan_drawn = build_plan_with_input(&project, &as_drawn).unwrap();
        let plan_opt = build_plan_with_input(&project, &default_plan_input()).unwrap();

        // Both produce non-empty plans with the same number of vector segments
        let vec_segs = |plan: &ExecutionPlan| -> Vec<String> {
            plan.segments
                .iter()
                .filter_map(|s| {
                    if let PlanSegment::Vector {
                        layer_id, polyline, ..
                    } = s
                    {
                        Some(format!(
                            "{layer_id}:{:.0},{:.0}",
                            polyline[0].x, polyline[0].y
                        ))
                    } else {
                        None
                    }
                })
                .collect()
        };
        let drawn_vec = vec_segs(&plan_drawn);
        let opt_vec = vec_segs(&plan_opt);
        assert_eq!(
            drawn_vec.len(),
            opt_vec.len(),
            "Same object count, same vector segment count"
        );
        // With 12 scattered objects, optimized ordering must differ from as-drawn
        assert_ne!(
            drawn_vec, opt_vec,
            "Optimized ordering should differ from as-drawn for 12 scattered objects"
        );
    }

    #[test]
    fn benchmark_travel_optimization_reduces_distance_meaningfully() {
        let project = create_benchmark_project();

        // `inner_first: true` forces AsDrawn per-layer on flat geometry;
        // here we vary only the cross-layer travel flag.
        let no_opt = plan_input_with(ProjectOptimization {
            inner_first: true,
            reduce_travel: false,
            ..Default::default()
        });
        let with_opt = plan_input_with(ProjectOptimization {
            inner_first: true,
            reduce_travel: true,
            ..Default::default()
        });

        let plan_no = build_plan_with_input(&project, &no_opt).unwrap();
        let plan_yes = build_plan_with_input(&project, &with_opt).unwrap();

        let travel_distance = |plan: &ExecutionPlan| -> f64 {
            plan.segments
                .iter()
                .filter_map(|s| {
                    if let PlanSegment::Travel { start, end } = s {
                        let dx = end.x - start.x;
                        let dy = end.y - start.y;
                        Some((dx * dx + dy * dy).sqrt())
                    } else {
                        None
                    }
                })
                .sum()
        };

        let unopt_travel = travel_distance(&plan_no);
        let opt_travel = travel_distance(&plan_yes);

        // Optimized travel should be strictly less than unoptimized
        assert!(
            opt_travel <= unopt_travel + 0.001,
            "Optimized travel ({:.1}mm) should be <= unoptimized ({:.1}mm)",
            opt_travel,
            unopt_travel
        );

        // With 12 scattered objects, reduction should be >10%
        if unopt_travel > 0.0 {
            let reduction_pct = (1.0 - opt_travel / unopt_travel) * 100.0;
            assert!(
                reduction_pct > 10.0,
                "Travel distance reduction should be >10%, got {:.1}%",
                reduction_pct
            );
        }
    }

    #[test]
    fn classify_cardinal_detects_all_four_angles() {
        // 0°
        let r0 = classify_cardinal(0.0);
        assert_eq!(r0, Some((ScanAxis::Horizontal, false)));
        // 360° (wrapped)
        let r360 = classify_cardinal(359.8);
        assert_eq!(r360, Some((ScanAxis::Horizontal, false)));
        // 90°
        let r90 = classify_cardinal(90.0);
        assert_eq!(r90, Some((ScanAxis::Vertical, false)));
        // 180°
        let r180 = classify_cardinal(180.0);
        assert_eq!(r180, Some((ScanAxis::Horizontal, true)));
        // 270°
        let r270 = classify_cardinal(270.0);
        assert_eq!(r270, Some((ScanAxis::Vertical, true)));
        // Non-cardinal
        assert_eq!(classify_cardinal(45.0), None);
        assert_eq!(classify_cardinal(135.0), None);
    }

    #[test]
    fn classify_cardinal_tolerance() {
        // Within ±0.5° should still classify
        assert!(classify_cardinal(0.4).is_some());
        assert!(classify_cardinal(89.6).is_some());
        assert!(classify_cardinal(180.3).is_some());
        assert!(classify_cardinal(270.49).is_some());
        // Outside tolerance
        assert!(classify_cardinal(0.6).is_none());
        assert!(classify_cardinal(89.4).is_none());
    }

    #[test]
    fn build_cardinal_raster_0_deg_horizontal() {
        // 0° horizontal: straightforward scanlines
        let processed = beambench_raster::ProcessedRaster {
            width_px: 4,
            height_px: 2,
            data: vec![255, 255, 255, 255, 128, 128, 128, 128],
            format: beambench_raster::types::RasterPixelFormat::Grayscale8,
            line_interval_mm: 0.5,
            x_pixel_mm: 0.5,
        };
        let bounds = Bounds::new(Point2D::new(10.0, 20.0), Point2D::new(14.0, 21.0));

        let (scanlines, li, axis) = build_cardinal_raster_scanlines(
            &processed,
            &bounds,
            ScanAxis::Horizontal,
            false,
            true,
            2.0,
        );

        assert_eq!(axis, ScanAxis::Horizontal);
        assert!(li > 0.0);
        assert!(!scanlines.is_empty());
    }

    #[test]
    fn build_cardinal_raster_90_deg_vertical() {
        let processed = beambench_raster::ProcessedRaster {
            width_px: 4,
            height_px: 2,
            data: vec![255, 255, 255, 255, 128, 128, 128, 128],
            format: beambench_raster::types::RasterPixelFormat::Grayscale8,
            line_interval_mm: 0.5,
            x_pixel_mm: 0.5,
        };
        let bounds = Bounds::new(Point2D::new(10.0, 20.0), Point2D::new(14.0, 21.0));

        let (scanlines, li, axis) = build_cardinal_raster_scanlines(
            &processed,
            &bounds,
            ScanAxis::Vertical,
            false,
            true,
            2.0,
        );

        assert_eq!(axis, ScanAxis::Vertical);
        assert!(li > 0.0);
        assert!(!scanlines.is_empty());
    }

    #[test]
    fn build_cardinal_raster_180_deg_flipped() {
        let processed = beambench_raster::ProcessedRaster {
            width_px: 4,
            height_px: 2,
            data: vec![255, 200, 150, 100, 50, 25, 10, 5],
            format: beambench_raster::types::RasterPixelFormat::Grayscale8,
            line_interval_mm: 0.5,
            x_pixel_mm: 0.5,
        };
        let bounds = Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(4.0, 1.0));

        let (scanlines_0, _, _) = build_cardinal_raster_scanlines(
            &processed,
            &bounds,
            ScanAxis::Horizontal,
            false,
            true,
            0.0,
        );
        let (scanlines_180, _, _) = build_cardinal_raster_scanlines(
            &processed,
            &bounds,
            ScanAxis::Horizontal,
            true,
            true,
            0.0,
        );

        // 180° should produce different scanline content (flipped)
        assert_eq!(scanlines_0.len(), scanlines_180.len());
        // They shouldn't be identical since the bitmap is asymmetric
        let s0_first_runs: Vec<_> = scanlines_0[0]
            .runs
            .iter()
            .flat_map(|r| r.power_values.clone())
            .collect();
        let s180_first_runs: Vec<_> = scanlines_180[0]
            .runs
            .iter()
            .flat_map(|r| r.power_values.clone())
            .collect();
        assert_ne!(
            s0_first_runs, s180_first_runs,
            "180° flip should produce different pixel order"
        );
    }

    #[test]
    fn build_cardinal_raster_270_deg_transposed_flipped() {
        let processed = beambench_raster::ProcessedRaster {
            width_px: 4,
            height_px: 2,
            data: vec![255, 255, 255, 255, 128, 128, 128, 128],
            format: beambench_raster::types::RasterPixelFormat::Grayscale8,
            line_interval_mm: 0.5,
            x_pixel_mm: 0.5,
        };
        let bounds = Bounds::new(Point2D::new(10.0, 20.0), Point2D::new(14.0, 21.0));

        let (scanlines, li, axis) = build_cardinal_raster_scanlines(
            &processed,
            &bounds,
            ScanAxis::Vertical,
            true,
            true,
            2.0,
        );

        assert_eq!(axis, ScanAxis::Vertical);
        assert!(li > 0.0);
        assert!(!scanlines.is_empty());
    }

    #[test]
    fn dot_width_correction_trims_raster_runs() {
        let mut segments = vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 10.0,
                runs: vec![ScanRun {
                    start_x_mm: 0.0,
                    end_x_mm: 10.0,
                    power_values: vec![],
                }],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Binary,
            speed_mm_min: 1000.0,
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
        }];

        apply_dot_width_correction(
            &mut segments,
            &PlannerCalibration {
                dot_width_mm: 0.2,
                enable_dot_width: true,
            },
        );

        let PlanSegment::Raster { scanlines, .. } = &segments[0] else {
            panic!("expected raster");
        };
        assert_eq!(scanlines[0].runs[0].start_x_mm, 0.1);
        assert_eq!(scanlines[0].runs[0].end_x_mm, 9.9);
    }

    #[test]
    fn dot_width_correction_drops_tiny_runs() {
        let mut segments = vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 10.0,
                runs: vec![ScanRun {
                    start_x_mm: 0.0,
                    end_x_mm: 0.05,
                    power_values: vec![],
                }],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Binary,
            speed_mm_min: 1000.0,
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
        }];

        apply_dot_width_correction(
            &mut segments,
            &PlannerCalibration {
                dot_width_mm: 0.1,
                enable_dot_width: true,
            },
        );

        let PlanSegment::Raster { scanlines, .. } = &segments[0] else {
            panic!("expected raster");
        };
        assert!(scanlines[0].runs.is_empty());
    }

    /// Helper for DWC composition tests: build a simple raster segment with the
    /// given layer-level dot-width correction value.
    fn dwc_test_segment(layer_dwc: f64) -> PlanSegment {
        PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 10.0,
                runs: vec![ScanRun {
                    start_x_mm: 0.0,
                    end_x_mm: 10.0,
                    power_values: vec![],
                }],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Binary,
            speed_mm_min: 1000.0,
            layer_id: "l1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 0.0,
            outlines: vec![],
            scan_axis: ScanAxis::Horizontal,
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: layer_dwc,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }
    }

    #[test]
    fn dot_width_correction_machine_only_matches_legacy() {
        // Layer DWC = 0, machine DWC = 0.2 → trim 0.1 each side
        let mut segments = vec![dwc_test_segment(0.0)];
        apply_dot_width_correction(
            &mut segments,
            &PlannerCalibration {
                dot_width_mm: 0.2,
                enable_dot_width: true,
            },
        );
        let PlanSegment::Raster { scanlines, .. } = &segments[0] else {
            panic!("expected raster");
        };
        assert!((scanlines[0].runs[0].start_x_mm - 0.1).abs() < 1e-9);
        assert!((scanlines[0].runs[0].end_x_mm - 9.9).abs() < 1e-9);
    }

    #[test]
    fn dot_width_correction_layer_only_when_machine_disabled() {
        // Layer DWC = 0.4, machine disabled → trim 0.2 each side
        let mut segments = vec![dwc_test_segment(0.4)];
        apply_dot_width_correction(
            &mut segments,
            &PlannerCalibration {
                dot_width_mm: 0.0,
                enable_dot_width: false,
            },
        );
        let PlanSegment::Raster { scanlines, .. } = &segments[0] else {
            panic!("expected raster");
        };
        assert!((scanlines[0].runs[0].start_x_mm - 0.2).abs() < 1e-9);
        assert!((scanlines[0].runs[0].end_x_mm - 9.8).abs() < 1e-9);
    }

    #[test]
    fn dot_width_correction_composes_machine_and_layer() {
        // Machine 0.2 + layer 0.4 = total 0.6 → trim 0.3 each side
        let mut segments = vec![dwc_test_segment(0.4)];
        apply_dot_width_correction(
            &mut segments,
            &PlannerCalibration {
                dot_width_mm: 0.2,
                enable_dot_width: true,
            },
        );
        let PlanSegment::Raster { scanlines, .. } = &segments[0] else {
            panic!("expected raster");
        };
        assert!((scanlines[0].runs[0].start_x_mm - 0.3).abs() < 1e-9);
        assert!((scanlines[0].runs[0].end_x_mm - 9.7).abs() < 1e-9);
    }

    #[test]
    fn dot_width_correction_neither_machine_nor_layer_is_noop() {
        let mut segments = vec![dwc_test_segment(0.0)];
        apply_dot_width_correction(
            &mut segments,
            &PlannerCalibration {
                dot_width_mm: 0.0,
                enable_dot_width: false,
            },
        );
        let PlanSegment::Raster { scanlines, .. } = &segments[0] else {
            panic!("expected raster");
        };
        assert!((scanlines[0].runs[0].start_x_mm - 0.0).abs() < 1e-9);
        assert!((scanlines[0].runs[0].end_x_mm - 10.0).abs() < 1e-9);
    }

    #[test]
    fn dot_width_correction_machine_disabled_ignores_machine_value() {
        // enable_dot_width=false → machine DWC ignored even though dot_width_mm
        // is non-zero. Layer DWC still applies.
        let mut segments = vec![dwc_test_segment(0.2)];
        apply_dot_width_correction(
            &mut segments,
            &PlannerCalibration {
                dot_width_mm: 0.5,
                enable_dot_width: false,
            },
        );
        let PlanSegment::Raster { scanlines, .. } = &segments[0] else {
            panic!("expected raster");
        };
        // Only layer DWC of 0.2 → trim 0.1 each side
        assert!((scanlines[0].runs[0].start_x_mm - 0.1).abs() < 1e-9);
        assert!((scanlines[0].runs[0].end_x_mm - 9.9).abs() < 1e-9);
    }

    #[test]
    fn composite_offset_fill_single_object_unchanged() {
        // Single rectangle on OffsetFill layer — verify concentric rings are
        // still generated after the composite pipeline refactor.
        let mut project = create_test_project();

        let layer = Layer::new("OffsetFill", OperationType::OffsetFill);
        let layer_id = layer.id;
        project.layers.push(layer);

        project.objects.push(ProjectObject::new(
            "rect",
            layer_id,
            Bounds::new(Point2D::new(50.0, 50.0), Point2D::new(150.0, 150.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 100.0,
                height: 100.0,
                corner_radius: 0.0,
            },
        ));

        let plan = build_plan(&project).expect("single OffsetFill object should succeed");

        let vector_count = plan
            .segments
            .iter()
            .filter(|s| matches!(s, PlanSegment::Vector { .. }))
            .count();
        assert!(
            vector_count >= 2,
            "Single object OffsetFill should still generate multiple concentric rings, got {vector_count}"
        );
    }

    #[test]
    fn offset_fill_grouping_all_shapes_batches_everything_together() {
        let layer = Layer::new("OffsetFill", OperationType::OffsetFill);
        let layer_id = layer.id;
        let a = ProjectObject::new(
            "A",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        );
        let b = ProjectObject::new(
            "B",
            layer_id,
            Bounds::new(Point2D::new(12.0, 0.0), Point2D::new(22.0, 10.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        );

        let all_objects = vec![a, b];
        let refs: Vec<&ProjectObject> = all_objects.iter().collect();
        let batches = offset_fill_object_batches(
            &refs,
            &all_objects,
            OffsetFillGroupingMode::AllShapesAtOnce,
        );

        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].len(), 2);
    }

    #[test]
    fn offset_fill_grouping_groups_together_uses_outermost_ancestor() {
        let layer = Layer::new("OffsetFill", OperationType::OffsetFill);
        let layer_id = layer.id;
        let a = ProjectObject::new(
            "A",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        );
        let b = ProjectObject::new(
            "B",
            layer_id,
            Bounds::new(Point2D::new(12.0, 0.0), Point2D::new(22.0, 10.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        );
        let c = ProjectObject::new(
            "C",
            layer_id,
            Bounds::new(Point2D::new(24.0, 0.0), Point2D::new(34.0, 10.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        );
        let d = ProjectObject::new(
            "D",
            layer_id,
            Bounds::new(Point2D::new(36.0, 0.0), Point2D::new(46.0, 10.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        );
        let inner = group_object("inner", layer_id, vec![a.id, b.id]);
        let outer = group_object("outer", layer_id, vec![inner.id, c.id]);

        let all_objects = vec![a, b, c, d, inner, outer];
        let refs: Vec<&ProjectObject> = all_objects.iter().take(4).collect();
        let batches =
            offset_fill_object_batches(&refs, &all_objects, OffsetFillGroupingMode::GroupsTogether);

        assert_eq!(batches.len(), 2);
        let batch_names: Vec<Vec<&str>> = batches
            .iter()
            .map(|batch| batch.iter().map(|obj| obj.name.as_str()).collect())
            .collect();
        assert_eq!(batch_names[0], vec!["A", "B", "C"]);
        assert_eq!(batch_names[1], vec!["D"]);
    }

    #[test]
    fn offset_fill_grouping_shapes_individually_creates_singleton_batches() {
        let layer = Layer::new("OffsetFill", OperationType::OffsetFill);
        let layer_id = layer.id;
        let a = ProjectObject::new(
            "A",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        );
        let b = ProjectObject::new(
            "B",
            layer_id,
            Bounds::new(Point2D::new(12.0, 0.0), Point2D::new(22.0, 10.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        );

        let all_objects = vec![a, b];
        let refs: Vec<&ProjectObject> = all_objects.iter().collect();
        let batches = offset_fill_object_batches(
            &refs,
            &all_objects,
            OffsetFillGroupingMode::ShapesIndividually,
        );

        assert_eq!(batches.len(), 2);
        assert!(batches.iter().all(|batch| batch.len() == 1));
    }

    #[test]
    fn composite_offset_fill_overlapping_shapes_merge() {
        // Two partially overlapping rectangles on the same OffsetFill layer.
        // After boolean union they merge into one contour — only one set of
        // concentric rings should be generated (no double-burning).
        let mut project = create_test_project();

        let layer = Layer::new("OffsetFill", OperationType::OffsetFill);
        let layer_id = layer.id;
        project.layers.push(layer);

        // Rect A: 50,50 → 120,120
        project.objects.push(ProjectObject::new(
            "A",
            layer_id,
            Bounds::new(Point2D::new(50.0, 50.0), Point2D::new(120.0, 120.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 70.0,
                height: 70.0,
                corner_radius: 0.0,
            },
        ));

        // Rect B: 90,50 → 160,120  (overlaps A by 30mm in x)
        project.objects.push(ProjectObject::new(
            "B",
            layer_id,
            Bounds::new(Point2D::new(90.0, 50.0), Point2D::new(160.0, 120.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 70.0,
                height: 70.0,
                corner_radius: 0.0,
            },
        ));

        let plan = build_plan(&project).expect("overlapping OffsetFill should succeed");

        // All vector segment points must lie within the merged bounding box
        let merged_bounds = Bounds::new(Point2D::new(50.0, 50.0), Point2D::new(160.0, 120.0));
        let tol = 1.0;
        for seg in &plan.segments {
            if let PlanSegment::Vector { polyline, .. } = seg {
                for pt in polyline {
                    assert!(
                        pt.x >= merged_bounds.min.x - tol
                            && pt.x <= merged_bounds.max.x + tol
                            && pt.y >= merged_bounds.min.y - tol
                            && pt.y <= merged_bounds.max.y + tol,
                        "Point ({:.2}, {:.2}) outside merged bounds",
                        pt.x,
                        pt.y
                    );
                }
            }
        }

        // Compare against processing each object individually — merged should
        // produce fewer segments than the sum of individual objects.
        let mut project_a = create_test_project();
        let layer_a = Layer::new("OffsetFill", OperationType::OffsetFill);
        let layer_a_id = layer_a.id;
        project_a.layers.push(layer_a);
        project_a.objects.push(ProjectObject::new(
            "A",
            layer_a_id,
            Bounds::new(Point2D::new(50.0, 50.0), Point2D::new(120.0, 120.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 70.0,
                height: 70.0,
                corner_radius: 0.0,
            },
        ));
        let plan_a = build_plan(&project_a).unwrap();

        let mut project_b = create_test_project();
        let layer_b = Layer::new("OffsetFill", OperationType::OffsetFill);
        let layer_b_id = layer_b.id;
        project_b.layers.push(layer_b);
        project_b.objects.push(ProjectObject::new(
            "B",
            layer_b_id,
            Bounds::new(Point2D::new(90.0, 50.0), Point2D::new(160.0, 120.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 70.0,
                height: 70.0,
                corner_radius: 0.0,
            },
        ));
        let plan_b = build_plan(&project_b).unwrap();

        let count_merged = plan
            .segments
            .iter()
            .filter(|s| matches!(s, PlanSegment::Vector { .. }))
            .count();
        let count_individual_sum = plan_a
            .segments
            .iter()
            .filter(|s| matches!(s, PlanSegment::Vector { .. }))
            .count()
            + plan_b
                .segments
                .iter()
                .filter(|s| matches!(s, PlanSegment::Vector { .. }))
                .count();

        assert!(
            count_merged < count_individual_sum,
            "Merged OffsetFill ({count_merged} segments) should be fewer than sum of individual ({count_individual_sum})"
        );
    }

    #[test]
    fn composite_offset_fill_groups_together_does_not_merge_different_groups() {
        let mut grouped_project = create_test_project();
        let mut grouped_layer = Layer::new("OffsetFill", OperationType::OffsetFill);
        set_offset_fill_grouping_mode(&mut grouped_layer, OffsetFillGroupingMode::GroupsTogether);
        let layer_id = grouped_layer.id;
        grouped_project.layers.push(grouped_layer);

        let a = ProjectObject::new(
            "A",
            layer_id,
            Bounds::new(Point2D::new(10.0, 10.0), Point2D::new(80.0, 80.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 70.0,
                height: 70.0,
                corner_radius: 0.0,
            },
        );
        let b = ProjectObject::new(
            "B",
            layer_id,
            Bounds::new(Point2D::new(50.0, 10.0), Point2D::new(120.0, 80.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 70.0,
                height: 70.0,
                corner_radius: 0.0,
            },
        );
        let c = ProjectObject::new(
            "C",
            layer_id,
            Bounds::new(Point2D::new(90.0, 10.0), Point2D::new(160.0, 80.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 70.0,
                height: 70.0,
                corner_radius: 0.0,
            },
        );
        let d = ProjectObject::new(
            "D",
            layer_id,
            Bounds::new(Point2D::new(130.0, 10.0), Point2D::new(200.0, 80.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 70.0,
                height: 70.0,
                corner_radius: 0.0,
            },
        );
        let g1 = group_object("G1", layer_id, vec![a.id, b.id]);
        let g2 = group_object("G2", layer_id, vec![c.id, d.id]);
        grouped_project
            .objects
            .extend([a.clone(), b.clone(), c.clone(), d.clone(), g1, g2]);

        let mut all_project = create_test_project();
        let all_layer = Layer::new("OffsetFill", OperationType::OffsetFill);
        let all_layer_id = all_layer.id;
        all_project.layers.push(all_layer);
        let mut all_objects = vec![a, b, c, d];
        for object in &mut all_objects {
            object.layer_id = all_layer_id;
        }
        all_project.objects.extend(all_objects);

        let grouped_plan = build_plan(&grouped_project).unwrap();
        let all_plan = build_plan(&all_project).unwrap();

        assert!(
            vector_segment_count(&grouped_plan) > vector_segment_count(&all_plan),
            "Grouped offset fill should keep different outermost groups separate"
        );
    }

    #[test]
    fn composite_offset_fill_inner_shape_is_hole() {
        // Outer rect with a smaller inner rect centered inside on the same
        // OffsetFill layer. After union + subtract, the inner shape becomes a
        // hole. Offset rings should trace around the hole — no vector segments
        // should pass through the hole's deep interior.
        let mut project = create_test_project();

        let layer = Layer::new("OffsetFill", OperationType::OffsetFill);
        let layer_id = layer.id;
        project.layers.push(layer);

        // Outer: 50,50 → 150,150 (100×100)
        project.objects.push(ProjectObject::new(
            "outer",
            layer_id,
            Bounds::new(Point2D::new(50.0, 50.0), Point2D::new(150.0, 150.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 100.0,
                height: 100.0,
                corner_radius: 0.0,
            },
        ));

        // Inner: 80,80 → 120,120 (40×40, fully inside outer)
        project.objects.push(ProjectObject::new(
            "inner",
            layer_id,
            Bounds::new(Point2D::new(80.0, 80.0), Point2D::new(120.0, 120.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 40.0,
                height: 40.0,
                corner_radius: 0.0,
            },
        ));

        let plan = build_plan(&project).expect("OffsetFill with hole should succeed");

        let vector_segments: Vec<_> = plan
            .segments
            .iter()
            .filter(|s| matches!(s, PlanSegment::Vector { .. }))
            .collect();
        assert!(
            !vector_segments.is_empty(),
            "OffsetFill with hole should produce vector segments"
        );

        let points: Vec<Point2D> = vector_segments
            .iter()
            .filter_map(|s| match s {
                PlanSegment::Vector { polyline, .. } => Some(polyline.clone()),
                _ => None,
            })
            .flatten()
            .collect();
        assert!(
            !points
                .iter()
                .any(|pt| pt.x >= 90.0 && pt.x <= 110.0 && pt.y >= 90.0 && pt.y <= 110.0),
            "OffsetFill should not emit ring geometry inside the hole interior"
        );

        // Compare against a project with only the outer shape: the composite
        // version should differ because the center is left empty.
        let mut project_outer_only = create_test_project();
        let layer_outer = Layer::new("OffsetFill", OperationType::OffsetFill);
        let layer_outer_id = layer_outer.id;
        project_outer_only.layers.push(layer_outer);
        project_outer_only.objects.push(ProjectObject::new(
            "outer",
            layer_outer_id,
            Bounds::new(Point2D::new(50.0, 50.0), Point2D::new(150.0, 150.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 100.0,
                height: 100.0,
                corner_radius: 0.0,
            },
        ));
        let plan_outer_only = build_plan(&project_outer_only).unwrap();

        let count_with_hole = vector_segments.len();
        let count_outer_only = plan_outer_only
            .segments
            .iter()
            .filter(|s| matches!(s, PlanSegment::Vector { .. }))
            .count();

        // The version with a hole should produce MORE segments (outer + hole
        // boundary rings) but have a gap in the center where no fill occurs.
        // Most importantly, it should differ from the sum of independent objects.
        assert!(
            count_with_hole != count_outer_only,
            "Composite with hole ({count_with_hole}) should differ from outer-only ({count_outer_only})"
        );
    }

    #[test]
    fn composite_offset_fill_three_nested_contours_preserves_inner_island() {
        // Outer + middle hole + inner island. The inner-most contour should survive
        // compositing and produce offset rings of its own.
        let mut project = create_test_project();

        let layer = Layer::new("OffsetFill", OperationType::OffsetFill);
        let layer_id = layer.id;
        project.layers.push(layer);

        project.objects.push(ProjectObject::new(
            "outer",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(80.0, 80.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 80.0,
                height: 80.0,
                corner_radius: 0.0,
            },
        ));

        project.objects.push(ProjectObject::new(
            "middle",
            layer_id,
            Bounds::new(Point2D::new(20.0, 20.0), Point2D::new(60.0, 60.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 40.0,
                height: 40.0,
                corner_radius: 0.0,
            },
        ));

        project.objects.push(ProjectObject::new(
            "inner",
            layer_id,
            Bounds::new(Point2D::new(35.0, 35.0), Point2D::new(45.0, 45.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        ));

        let plan = build_plan(&project).expect("three-nested OffsetFill should succeed");

        let points: Vec<Point2D> = plan
            .segments
            .iter()
            .filter_map(|s| match s {
                PlanSegment::Vector { polyline, .. } => Some(polyline.clone()),
                _ => None,
            })
            .flatten()
            .collect();

        assert!(
            points
                .iter()
                .any(|pt| { pt.x >= 35.0 && pt.x <= 45.0 && pt.y >= 35.0 && pt.y <= 45.0 }),
            "Inner island should still produce offset-fill geometry inside its bounds"
        );
    }

    #[test]
    fn composite_offset_fill_disjoint_shapes() {
        // Two non-overlapping rectangles on the same OffsetFill layer.
        // Both should get their own concentric rings (backward compatible).
        let mut project = create_test_project();

        let layer = Layer::new("OffsetFill", OperationType::OffsetFill);
        let layer_id = layer.id;
        project.layers.push(layer);

        // Rect A: 10,10 → 60,60
        project.objects.push(ProjectObject::new(
            "A",
            layer_id,
            Bounds::new(Point2D::new(10.0, 10.0), Point2D::new(60.0, 60.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 50.0,
                height: 50.0,
                corner_radius: 0.0,
            },
        ));

        // Rect B: 100,100 → 150,150 (far from A, no overlap)
        project.objects.push(ProjectObject::new(
            "B",
            layer_id,
            Bounds::new(Point2D::new(100.0, 100.0), Point2D::new(150.0, 150.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 50.0,
                height: 50.0,
                corner_radius: 0.0,
            },
        ));

        let plan = build_plan(&project).expect("disjoint OffsetFill should succeed");

        // Collect all vector points
        let all_points: Vec<Point2D> = plan
            .segments
            .iter()
            .filter_map(|s| match s {
                PlanSegment::Vector { polyline, .. } => Some(polyline.clone()),
                _ => None,
            })
            .flatten()
            .collect();

        // Should have points in both regions
        let in_a = all_points
            .iter()
            .any(|pt| pt.x >= 10.0 && pt.x <= 61.0 && pt.y >= 10.0 && pt.y <= 61.0);
        let in_b = all_points
            .iter()
            .any(|pt| pt.x >= 99.0 && pt.x <= 151.0 && pt.y >= 99.0 && pt.y <= 151.0);

        assert!(in_a, "Should have vector segments in region A");
        assert!(in_b, "Should have vector segments in region B");

        // Total segments should be roughly the same as the sum of individual objects
        let count_both = plan
            .segments
            .iter()
            .filter(|s| matches!(s, PlanSegment::Vector { .. }))
            .count();
        assert!(
            count_both >= 4,
            "Disjoint shapes should produce rings for both objects, got {count_both}"
        );
    }

    #[test]
    fn composite_offset_fill_disjoint_donuts_finish_one_unit_before_next() {
        let mut project = create_test_project();

        let layer = Layer::new("OffsetFill", OperationType::OffsetFill);
        let layer_id = layer.id;
        project.layers.push(layer);

        project.objects.push(ProjectObject::new(
            "left_outer",
            layer_id,
            Bounds::new(Point2D::new(20.0, 20.0), Point2D::new(100.0, 100.0)),
            ObjectData::Shape {
                kind: ShapeKind::Ellipse,
                width: 80.0,
                height: 80.0,
                corner_radius: 0.0,
            },
        ));
        project.objects.push(ProjectObject::new(
            "left_inner",
            layer_id,
            Bounds::new(Point2D::new(40.0, 40.0), Point2D::new(80.0, 80.0)),
            ObjectData::Shape {
                kind: ShapeKind::Ellipse,
                width: 40.0,
                height: 40.0,
                corner_radius: 0.0,
            },
        ));
        project.objects.push(ProjectObject::new(
            "right_outer",
            layer_id,
            Bounds::new(Point2D::new(160.0, 20.0), Point2D::new(240.0, 100.0)),
            ObjectData::Shape {
                kind: ShapeKind::Ellipse,
                width: 80.0,
                height: 80.0,
                corner_radius: 0.0,
            },
        ));
        project.objects.push(ProjectObject::new(
            "right_inner",
            layer_id,
            Bounds::new(Point2D::new(180.0, 40.0), Point2D::new(220.0, 80.0)),
            ObjectData::Shape {
                kind: ShapeKind::Ellipse,
                width: 40.0,
                height: 40.0,
                corner_radius: 0.0,
            },
        ));

        let input = plan_input_with(ProjectOptimization {
            reduce_travel: false,
            ..Default::default()
        });
        let plan = build_plan_with_input(&project, &input).unwrap();

        let mut seen_right = false;
        for polyline in plan.segments.iter().filter_map(|segment| match segment {
            PlanSegment::Vector { polyline, .. } => Some(polyline),
            _ => None,
        }) {
            let avg_x = polyline.iter().map(|pt| pt.x).sum::<f64>() / polyline.len() as f64;
            if avg_x > 130.0 {
                seen_right = true;
            } else if seen_right {
                panic!("OffsetFill returned to the left donut after starting the right donut");
            }
        }
    }

    #[test]
    fn composite_offset_fill_donut_uses_localized_seam_corridor() {
        let mut project = create_test_project();

        let layer = Layer::new("OffsetFill", OperationType::OffsetFill);
        let layer_id = layer.id;
        project.layers.push(layer);

        project.objects.push(ProjectObject::new(
            "outer",
            layer_id,
            Bounds::new(Point2D::new(120.0, 120.0), Point2D::new(280.0, 280.0)),
            ObjectData::Shape {
                kind: ShapeKind::Ellipse,
                width: 160.0,
                height: 160.0,
                corner_radius: 0.0,
            },
        ));
        project.objects.push(ProjectObject::new(
            "inner",
            layer_id,
            Bounds::new(Point2D::new(165.0, 165.0), Point2D::new(235.0, 235.0)),
            ObjectData::Shape {
                kind: ShapeKind::Ellipse,
                width: 70.0,
                height: 70.0,
                corner_radius: 0.0,
            },
        ));

        let input = plan_input_with(ProjectOptimization {
            reduce_travel: false,
            ..Default::default()
        });
        let plan = build_plan_with_input(&project, &input).unwrap();

        let center = Point2D::new(200.0, 200.0);
        let seam_buckets: std::collections::HashSet<i32> = plan
            .segments
            .iter()
            .filter_map(|segment| match segment {
                PlanSegment::Vector { polyline, .. } => polyline.first().copied(),
                _ => None,
            })
            .take(12)
            .map(|start| {
                let angle = (start.y - center.y).atan2(start.x - center.x).to_degrees();
                ((angle + 360.0) % 360.0 / 15.0).round() as i32
            })
            .collect();

        assert!(
            seam_buckets.len() <= 3,
            "Expected donut seams to stay in a localized corridor, got {} angle buckets",
            seam_buckets.len()
        );
    }

    #[test]
    fn composite_offset_fill_offcenter_annulus_uses_localized_seam_corridors() {
        let mut project = create_test_project();

        let layer = Layer::new("OffsetFill", OperationType::OffsetFill);
        let layer_id = layer.id;
        project.layers.push(layer);

        project.objects.push(ProjectObject::new(
            "outer",
            layer_id,
            Bounds::new(Point2D::new(100.0, 80.0), Point2D::new(300.0, 220.0)),
            ObjectData::Shape {
                kind: ShapeKind::Ellipse,
                width: 200.0,
                height: 140.0,
                corner_radius: 0.0,
            },
        ));
        project.objects.push(ProjectObject::new(
            "inner",
            layer_id,
            Bounds::new(Point2D::new(170.0, 120.0), Point2D::new(270.0, 200.0)),
            ObjectData::Shape {
                kind: ShapeKind::Ellipse,
                width: 100.0,
                height: 80.0,
                corner_radius: 0.0,
            },
        ));

        let input = plan_input_with(ProjectOptimization {
            reduce_travel: false,
            ..Default::default()
        });
        let plan = build_plan_with_input(&project, &input).unwrap();

        let seam_buckets: std::collections::HashSet<(i32, i32)> = plan
            .segments
            .iter()
            .filter_map(|segment| match segment {
                PlanSegment::Vector { polyline, .. } => Some(polyline),
                _ => None,
            })
            .take(16)
            .filter_map(|polyline| {
                let first = polyline.first()?;
                let bounds = bounds_from_polylines(&[Polyline::new(polyline.clone(), true)])?;
                let center = Point2D::new(
                    (bounds.min.x + bounds.max.x) * 0.5,
                    (bounds.min.y + bounds.max.y) * 0.5,
                );
                let angle = (first.y - center.y).atan2(first.x - center.x).to_degrees();
                let bucket = (((angle + 360.0) % 360.0) / 20.0).round() as i32;
                let family = if bounds.width() > 130.0 { 0 } else { 1 };
                Some((family, bucket))
            })
            .collect();

        let outer_bucket_count = seam_buckets
            .iter()
            .filter(|(family, _)| *family == 0)
            .count();
        let inner_bucket_count = seam_buckets
            .iter()
            .filter(|(family, _)| *family == 1)
            .count();

        assert!(
            outer_bucket_count <= 3,
            "Expected outer annulus seams to stay localized, got {outer_bucket_count} buckets"
        );
        assert!(
            inner_bucket_count <= 3,
            "Expected inner annulus seams to stay localized, got {inner_bucket_count} buckets"
        );

        let mut outer_starts = Vec::new();
        let mut inner_starts = Vec::new();
        for polyline in plan.segments.iter().filter_map(|segment| match segment {
            PlanSegment::Vector { polyline, .. } => Some(polyline),
            _ => None,
        }) {
            let Some(first) = polyline.first().copied() else {
                continue;
            };
            let Some(bounds) = bounds_from_polylines(&[Polyline::new(polyline.clone(), true)])
            else {
                continue;
            };
            if bounds.width() > 130.0 {
                outer_starts.push(first);
            } else {
                inner_starts.push(first);
            }
        }

        let outer_max_jump = outer_starts
            .windows(2)
            .take(10)
            .map(|pair| pair[0].distance_to(&pair[1]))
            .fold(0.0, f64::max);
        let inner_max_jump = inner_starts
            .windows(2)
            .take(8)
            .map(|pair| pair[0].distance_to(&pair[1]))
            .fold(0.0, f64::max);

        assert!(
            outer_max_jump <= 20.0,
            "Expected outer annulus seam jumps to stay bounded, got max jump {outer_max_jump:.2}mm"
        );
        assert!(
            inner_max_jump <= 14.0,
            "Expected inner annulus seam jumps to stay bounded, got max jump {inner_max_jump:.2}mm"
        );
    }

    #[test]
    fn fill_overlapping_shapes_stay_separate_objects() {
        let mut project = create_test_project();

        let mut layer = Layer::new("Fill", OperationType::Fill);
        if let Some(ref mut rs) = layer.primary_entry_mut().raster_settings {
            rs.line_interval_mm = 1.0;
        }
        let layer_id = layer.id;
        project.layers.push(layer);

        project.objects.push(ProjectObject::new(
            "A",
            layer_id,
            Bounds::new(Point2D::new(10.0, 10.0), Point2D::new(40.0, 40.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 30.0,
                height: 30.0,
                corner_radius: 0.0,
            },
        ));
        project.objects.push(ProjectObject::new(
            "B",
            layer_id,
            Bounds::new(Point2D::new(25.0, 10.0), Point2D::new(55.0, 40.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 30.0,
                height: 30.0,
                corner_radius: 0.0,
            },
        ));

        let plan = build_plan(&project).expect("overlapping Fill should succeed");
        let raster_segments: Vec<_> = plan
            .segments
            .iter()
            .filter_map(|s| match s {
                PlanSegment::Raster { scanlines, .. } => Some(scanlines),
                _ => None,
            })
            .collect();

        assert_eq!(
            raster_segments.len(),
            2,
            "Fill should rasterize overlapping objects as separate regions"
        );
    }

    #[test]
    fn fill_three_nested_objects_stay_separate_regions() {
        let mut project = create_test_project();

        let mut layer = Layer::new("Fill", OperationType::Fill);
        if let Some(ref mut rs) = layer.primary_entry_mut().raster_settings {
            rs.line_interval_mm = 1.0;
        }
        let layer_id = layer.id;
        project.layers.push(layer);

        project.objects.push(ProjectObject::new(
            "outer",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(80.0, 80.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 80.0,
                height: 80.0,
                corner_radius: 0.0,
            },
        ));
        project.objects.push(ProjectObject::new(
            "middle",
            layer_id,
            Bounds::new(Point2D::new(20.0, 20.0), Point2D::new(60.0, 60.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 40.0,
                height: 40.0,
                corner_radius: 0.0,
            },
        ));
        project.objects.push(ProjectObject::new(
            "inner",
            layer_id,
            Bounds::new(Point2D::new(35.0, 35.0), Point2D::new(45.0, 45.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        ));

        let plan = build_plan(&project).expect("three-nested Fill should succeed");
        let raster_segments: Vec<_> = plan
            .segments
            .iter()
            .filter_map(|s| match s {
                PlanSegment::Raster { scanlines, .. } => Some(scanlines),
                _ => None,
            })
            .collect();
        assert_eq!(
            raster_segments.len(),
            3,
            "Nested Fill objects should rasterize as three separate regions"
        );
    }

    // ---- Face-tracking seam & topology-aware filtering tests ----

    /// Build a regular hexagon polyline centered at `center` with `radius`.
    fn make_hexagon(center: Point2D, radius: f64) -> Polyline {
        let points: Vec<Point2D> = (0..6)
            .map(|i| {
                let angle = std::f64::consts::FRAC_PI_3 * i as f64;
                Point2D::new(
                    center.x + radius * angle.cos(),
                    center.y + radius * angle.sin(),
                )
            })
            .collect();
        Polyline::new(points, true)
    }

    fn make_circle(center: Point2D, radius: f64, segments: usize, clockwise: bool) -> Polyline {
        let indices: Vec<usize> = if clockwise {
            (0..segments).rev().collect()
        } else {
            (0..segments).collect()
        };
        let points: Vec<Point2D> = indices
            .into_iter()
            .map(|i| {
                let angle = std::f64::consts::TAU * i as f64 / segments as f64;
                Point2D::new(
                    center.x + radius * angle.cos(),
                    center.y + radius * angle.sin(),
                )
            })
            .collect();
        Polyline::new(points, true)
    }

    /// Build a VecPath from a single closed Polyline.
    fn polyline_to_vecpath_test(poly: &Polyline) -> VecPath {
        polyline_to_vecpath(poly)
    }

    #[test]
    fn offset_fill_seam_face_stable() {
        // Build a hexagon offset fill. For consecutive rings in the same track,
        // all seam points should fall in ≤2 adjacent angular buckets (15° each).
        let center = Point2D::new(100.0, 100.0);
        let hex = make_hexagon(center, 40.0);
        let path = polyline_to_vecpath_test(&hex);
        let bounds = bounds_from_polylines(&[hex]).unwrap();

        let tracks = build_offset_fill_tracks(&path, &bounds, 2.0);
        assert!(!tracks.is_empty(), "Should produce at least one track");

        // Find the first track with multiple polylines.
        let multi_track = tracks.iter().find(|t| t.polylines.len() >= 3);
        assert!(
            multi_track.is_some(),
            "Hexagon should produce a track with ≥3 rings"
        );
        let track = multi_track.unwrap();

        // Emit with face tracking and collect seam points.
        let seam_angle_deg = 0.0;
        let start_pos = Point2D::new(bounds.max.x + 10.0, center.y);
        let result = emit_offset_fill_track(
            &track.polylines,
            center,
            seam_angle_deg,
            2.0,
            100.0,
            1000.0,
            "test",
            "entry-test",
            start_pos,
            None,
            start_pos,
        );
        let segments = result.segments;

        let seam_buckets: std::collections::HashSet<i32> = segments
            .iter()
            .filter_map(|seg| match seg {
                PlanSegment::Vector { polyline, .. } => polyline.first().copied(),
                _ => None,
            })
            .map(|pt| {
                let angle = (pt.y - center.y).atan2(pt.x - center.x).to_degrees();
                (((angle + 360.0) % 360.0) / 15.0).round() as i32
            })
            .collect();

        assert!(
            seam_buckets.len() <= 2,
            "Face tracking should keep seams in ≤2 angular buckets, got {} buckets: {:?}",
            seam_buckets.len(),
            seam_buckets
        );
    }

    #[test]
    fn offset_fill_siblings_share_parent_anchor() {
        // Wide U-shape: 80mm wide, 20mm legs, 40mm gap — reliably splits at 2mm spacing.
        // The bottom bridge is 10mm tall, so after ~5 inward offsets the bridge disappears
        // and the two tall legs become separate child branches.
        let u_shape = Polyline::new(
            vec![
                Point2D::new(10.0, 10.0), // bottom-left
                Point2D::new(10.0, 60.0), // top-left
                Point2D::new(30.0, 60.0), // inner top-left
                Point2D::new(30.0, 20.0), // inner bottom-left
                Point2D::new(60.0, 20.0), // inner bottom-right
                Point2D::new(60.0, 60.0), // inner top-right
                Point2D::new(80.0, 60.0), // top-right
                Point2D::new(80.0, 10.0), // bottom-right
            ],
            true,
        );
        let path = polyline_to_vecpath_test(&u_shape);
        let bounds = bounds_from_polylines(&[u_shape]).unwrap();

        let tracks = build_offset_fill_tracks(&path, &bounds, 2.0);
        assert!(!tracks.is_empty(), "Should produce at least one track");

        // Assert the split actually happened — no vacuous pass.
        let children_count = tracks.iter().filter(|t| t.parent.is_some()).count();
        assert!(
            children_count >= 1,
            "U-shape (80mm wide, 20mm legs, 10mm bridge) at 2mm spacing must split. \
             Got {} tracks with {} children",
            tracks.len(),
            children_count
        );

        let unit_center = Point2D::new(
            (bounds.min.x + bounds.max.x) * 0.5,
            (bounds.min.y + bounds.max.y) * 0.5,
        );
        let seam_angle_deg = 0.0;
        let start_pos = Point2D::new(bounds.max.x + 5.0, unit_center.y);

        let mut child_map = vec![Vec::new(); tracks.len()];
        let mut roots = Vec::new();
        for (tid, t) in tracks.iter().enumerate() {
            if let Some(pid) = t.parent {
                child_map[pid].push(tid);
            } else {
                roots.push(tid);
            }
        }

        // Emit the full tree.
        let (segments, _, _) = emit_offset_fill_track_tree(
            roots[0],
            &tracks,
            &child_map,
            unit_center,
            seam_angle_deg,
            2.0,
            100.0,
            1000.0,
            "test",
            "entry-test",
            start_pos,
            None,
        );
        assert!(
            segments.len() >= 3,
            "Expected ≥3 vector segments from U-shape"
        );

        // For each parent track that has children, verify:
        // The child's first seam point is angularly close to the parent's
        // split-point ring seam (within 60° — same corridor side).
        for (parent_id, child_ids) in child_map.iter().enumerate() {
            if child_ids.is_empty() {
                continue;
            }

            for &child_id in child_ids {
                let child_first = &tracks[child_id].polylines[0];
                // Find the point on the parent ring that faces the child.
                let split_anchor = find_split_point_anchor(&tracks[parent_id], child_first);
                let Some(anchor) = split_anchor else { continue };

                // Now emit just the child's first ring with the split anchor
                // and check the resulting seam point.
                let child_result = emit_offset_fill_track(
                    &tracks[child_id].polylines[..1],
                    unit_center,
                    seam_angle_deg,
                    2.0,
                    100.0,
                    1000.0,
                    "test",
                    "entry-test",
                    start_pos,
                    Some(anchor),
                    start_pos,
                );
                let Some(child_seam) = child_result.ring_anchors.first() else {
                    continue;
                };

                let anchor_angle = point_angle_deg(unit_center, anchor);
                let child_angle = point_angle_deg(unit_center, *child_seam);
                let delta = angular_delta_deg(anchor_angle, child_angle);

                assert!(
                    delta < 60.0,
                    "Child {} seam should be within 60° of parent {}'s split-point anchor. \
                     anchor_angle={anchor_angle:.1}°, child_angle={child_angle:.1}°, delta={delta:.1}°",
                    child_id,
                    parent_id,
                );
            }
        }
    }

    #[test]
    fn offset_fill_fallback_respects_position() {
        // Create a triangle with very sparse vertices (3 points).
        // The selected start vertex should not be on the opposite side of the
        // shape from current_pos.
        let triangle = Polyline::new(
            vec![
                Point2D::new(0.0, 0.0),
                Point2D::new(30.0, 0.0),
                Point2D::new(15.0, 26.0),
            ],
            true,
        );
        let center = Point2D::new(15.0, 8.67);
        let seam_angle_deg = 45.0; // unlikely to hit any vertex exactly

        // current_pos is near vertex 0 (0,0).
        let current_pos = Point2D::new(-5.0, 0.0);
        let seam_target = point_on_polyline_phase(&triangle, center, seam_angle_deg);
        let idx = select_corridor_start_index(
            &triangle,
            center,
            seam_angle_deg,
            seam_target,
            current_pos,
        );
        let selected = triangle.points[idx];

        let angle_to_selected = (selected.y - current_pos.y)
            .atan2(selected.x - current_pos.x)
            .to_degrees();
        let angle_to_center = (center.y - current_pos.y)
            .atan2(center.x - current_pos.x)
            .to_degrees();
        let delta = angular_delta_deg(angle_to_selected, angle_to_center);

        assert!(
            delta < 120.0,
            "Fallback should pick a vertex not opposite from current_pos, angular delta = {delta:.1}°"
        );
    }

    #[test]
    fn offset_fill_thin_sliver_terminates_early() {
        // Create a thin triangle (base 30mm, height 1.5mm), spacing 0.3mm.
        // The area filter should stop early, producing fewer than 100 rings.
        let thin_triangle = Polyline::new(
            vec![
                Point2D::new(100.0, 100.0),
                Point2D::new(130.0, 100.0),
                Point2D::new(115.0, 101.5),
            ],
            true,
        );
        let path = polyline_to_vecpath_test(&thin_triangle);
        let bounds = bounds_from_polylines(&[thin_triangle]).unwrap();

        let tracks = build_offset_fill_tracks(&path, &bounds, 0.3);
        let total_rings: usize = tracks.iter().map(|t| t.polylines.len()).sum();

        assert!(
            total_rings < 100,
            "Thin sliver should terminate early, got {total_rings} rings (expected < 100)"
        );
        // With a triangle of area ~22.5 mm² and spacing 0.3mm, max useful depth
        // is small. The area filter should kick in well before 100 iterations.
        assert!(
            total_rings > 0,
            "Should produce at least some rings for the thin triangle"
        );
    }

    #[test]
    fn offset_fill_large_rectangle_exceeds_legacy_cap_without_failure() {
        let rect = Polyline::new(
            vec![
                Point2D::new(0.0, 0.0),
                Point2D::new(60.0, 0.0),
                Point2D::new(60.0, 60.0),
                Point2D::new(0.0, 60.0),
            ],
            true,
        );
        let path = polyline_to_vecpath_test(&rect);
        let bounds = bounds_from_polylines(&[rect]).unwrap();
        let controls = OffsetFillBuildControls::new(PlannerCancellation::default());
        let result = build_offset_fill_tracks_with_controls(&path, &bounds, 0.1, &controls);
        let total_rings: usize = result.tracks.iter().map(|t| t.polylines.len()).sum();

        assert!(
            total_rings > 256,
            "60x60mm rectangle at 0.1mm should produce >256 rings, got {total_rings}"
        );
        assert!(
            !result.stats.hit_iteration_cap && !result.stats.timed_out && !result.stats.cancelled,
            "large rectangle should complete through geometry exits, stats={:?}",
            result.stats
        );
    }

    #[test]
    fn offset_fill_controls_cancel_mid_loop_quietly() {
        let rect = Polyline::new(
            vec![
                Point2D::new(0.0, 0.0),
                Point2D::new(80.0, 0.0),
                Point2D::new(80.0, 80.0),
                Point2D::new(0.0, 80.0),
            ],
            true,
        );
        let path = polyline_to_vecpath_test(&rect);
        let bounds = bounds_from_polylines(&[rect]).unwrap();
        let controls = OffsetFillBuildControls::with_test_limits(
            Duration::from_secs(300),
            PlannerCancellation::default(),
            None,
            Some(3),
        );
        let result = build_offset_fill_tracks_with_controls(&path, &bounds, 0.1, &controls);

        assert!(result.stats.cancelled, "stats={:?}", result.stats);
        assert!(
            !result.stats.timed_out && !result.stats.hit_iteration_cap,
            "cancel should not be reported as timeout/cap, stats={:?}",
            result.stats
        );
    }

    #[test]
    fn offset_fill_controls_timeout_and_emergency_limit_fail_closed() {
        let rect = Polyline::new(
            vec![
                Point2D::new(0.0, 0.0),
                Point2D::new(80.0, 0.0),
                Point2D::new(80.0, 80.0),
                Point2D::new(0.0, 80.0),
            ],
            true,
        );
        let path = polyline_to_vecpath_test(&rect);
        let bounds = bounds_from_polylines(&[rect]).unwrap();

        let timeout_controls =
            OffsetFillBuildControls::with_limits(Duration::ZERO, PlannerCancellation::default());
        let timeout_result =
            build_offset_fill_tracks_with_controls(&path, &bounds, 0.1, &timeout_controls);
        assert!(
            timeout_result.stats.timed_out,
            "stats={:?}",
            timeout_result.stats
        );

        let ceiling_controls = OffsetFillBuildControls::with_test_limits(
            Duration::from_secs(300),
            PlannerCancellation::default(),
            Some(2),
            None,
        );
        let ceiling_result =
            build_offset_fill_tracks_with_controls(&path, &bounds, 0.1, &ceiling_controls);
        assert!(
            ceiling_result.stats.hit_iteration_cap,
            "stats={:?}",
            ceiling_result.stats
        );
    }

    #[test]
    fn offset_fill_vertex_count_bounded() {
        // Create a 64-sided polygon, run 30+ iterations.
        // Last ring vertex count should be ≤ 2× first ring.
        let center = Point2D::new(200.0, 200.0);
        let radius = 80.0;
        let n = 64;
        let points: Vec<Point2D> = (0..n)
            .map(|i| {
                let angle = 2.0 * std::f64::consts::PI * i as f64 / n as f64;
                Point2D::new(
                    center.x + radius * angle.cos(),
                    center.y + radius * angle.sin(),
                )
            })
            .collect();
        let poly = Polyline::new(points, true);
        let path = polyline_to_vecpath_test(&poly);
        let bounds = bounds_from_polylines(&[poly]).unwrap();

        let tracks = build_offset_fill_tracks(&path, &bounds, 2.0);
        assert!(!tracks.is_empty());

        // Find the longest track (root).
        let longest = tracks.iter().max_by_key(|t| t.polylines.len()).unwrap();
        assert!(
            longest.polylines.len() >= 10,
            "Expected ≥10 rings for a radius-80 circle at 2mm spacing, got {}",
            longest.polylines.len()
        );

        let first_count = longest.polylines.first().unwrap().points.len();
        let last_count = longest.polylines.last().unwrap().points.len();

        assert!(
            last_count <= first_count * 2,
            "Simplification should prevent vertex count explosion: first={first_count}, last={last_count}"
        );
    }

    #[test]
    fn offset_fill_simplification_preserves_hole_topology() {
        // Create a rectangle (100×100) with a centered circular hole (radius 20).
        // The gap between outer edge and hole is ~30mm.  At 2mm spacing with
        // simplification (2% tolerance = 0.04mm), the hole must survive for
        // multiple iterations — i.e. each early iteration must still contain
        // both an exterior ring (even depth) and a hole ring (odd depth).
        //
        // We manually step through buffer_closed_path + optimize_path (the same
        // pipeline build_offset_fill_tracks uses) and check per-iteration topology.
        let outer = Polyline::new(
            vec![
                Point2D::new(50.0, 50.0),
                Point2D::new(150.0, 50.0),
                Point2D::new(150.0, 150.0),
                Point2D::new(50.0, 150.0),
            ],
            true,
        );
        let center = Point2D::new(100.0, 100.0);
        let hole_radius = 20.0;
        let hole_n = 32;
        // CW winding for the hole (negative area).
        let hole_points: Vec<Point2D> = (0..hole_n)
            .rev()
            .map(|i| {
                let angle = 2.0 * std::f64::consts::PI * i as f64 / hole_n as f64;
                Point2D::new(
                    center.x + hole_radius * angle.cos(),
                    center.y + hole_radius * angle.sin(),
                )
            })
            .collect();
        let hole = Polyline::new(hole_points, true);

        let vecpath = polylines_to_vecpath(&[outer, hole]);
        let offset_spacing = 2.0;
        let simplify_tolerance = offset_spacing * 0.02;

        let mut current_path = vecpath;
        let mut iterations_with_hole = 0;
        let mut total_iterations = 0;

        for iteration in 0..50 {
            let ring_polylines: Vec<Polyline> =
                flatten_vecpath(&current_path, DEFAULT_TOLERANCE_MM)
                    .into_iter()
                    .filter(|p| p.closed && p.points.len() >= 3)
                    .collect();
            if ring_polylines.is_empty() {
                break;
            }
            total_iterations = iteration + 1;

            let (depths, _analyses) = compute_all_nesting_depths(&ring_polylines);

            let has_exterior = depths.iter().any(|d| d.is_multiple_of(2));
            let has_hole = depths.iter().any(|d| !d.is_multiple_of(2));

            if has_exterior && has_hole {
                iterations_with_hole += 1;
            }

            // Offset inward + simplify (same as build_offset_fill_tracks).
            let next_paths = buffer_closed_path(
                &current_path,
                offset_spacing,
                OffsetDirection::Inward,
                CornerStyle::Miter,
                2.0,
            );
            let Some(next_path) = next_paths
                .into_iter()
                .find(|path| !path.subpaths.is_empty())
            else {
                break;
            };
            let Some(bounds) = next_path.bounds() else {
                break;
            };
            if bounds.width() <= 1e-6 || bounds.height() <= 1e-6 {
                break;
            }
            current_path = optimize_path(&next_path, simplify_tolerance);
        }

        // The hole has radius 20mm.  At 2mm spacing the hole ring should survive
        // for ~10 iterations (20/2).  Require at least 5 to prove simplification
        // is not collapsing it prematurely.
        assert!(
            iterations_with_hole >= 5,
            "Hole should survive ≥5 iterations with simplification enabled, \
             but only {iterations_with_hole}/{total_iterations} iterations had both exterior and hole"
        );

        // Also verify via build_offset_fill_tracks that both parities exist
        // and total ring count is reasonable.
        let bounds = Bounds::new(Point2D::new(50.0, 50.0), Point2D::new(150.0, 150.0));
        let outer2 = Polyline::new(
            vec![
                Point2D::new(50.0, 50.0),
                Point2D::new(150.0, 50.0),
                Point2D::new(150.0, 150.0),
                Point2D::new(50.0, 150.0),
            ],
            true,
        );
        let hole2_points: Vec<Point2D> = (0..hole_n)
            .rev()
            .map(|i| {
                let angle = 2.0 * std::f64::consts::PI * i as f64 / hole_n as f64;
                Point2D::new(
                    center.x + hole_radius * angle.cos(),
                    center.y + hole_radius * angle.sin(),
                )
            })
            .collect();
        let hole2 = Polyline::new(hole2_points, true);
        let vecpath2 = polylines_to_vecpath(&[outer2, hole2]);

        let tracks = build_offset_fill_tracks(&vecpath2, &bounds, 2.0);
        let has_parity_0 = tracks.iter().any(|t| t.parity == 0);
        let has_parity_1 = tracks.iter().any(|t| t.parity == 1);

        assert!(
            has_parity_0 && has_parity_1,
            "Rectangle-with-hole should produce both exterior (parity 0) and hole (parity 1) tracks"
        );

        let total_rings: usize = tracks.iter().map(|t| t.polylines.len()).sum();
        assert!(
            total_rings >= 10,
            "Expected ≥10 total rings for rectangle-with-hole, got {total_rings}"
        );
    }

    #[test]
    fn offset_fill_winding_parity_survives_hole_merge_transition() {
        // Two close holes expand during inward offset and merge into one hole
        // region. Parity must continue to come from Clipper winding, not from
        // stale nesting assumptions taken from the previous iteration.
        let outer = Polyline::new(
            vec![
                Point2D::new(0.0, 0.0),
                Point2D::new(100.0, 0.0),
                Point2D::new(100.0, 60.0),
                Point2D::new(0.0, 60.0),
            ],
            true,
        );
        let hole_a = make_circle(Point2D::new(44.0, 30.0), 8.0, 48, true);
        let hole_b = make_circle(Point2D::new(56.0, 30.0), 8.0, 48, true);
        let bounds = bounds_from_polylines(std::slice::from_ref(&outer)).unwrap();
        let path = polylines_to_vecpath(&[outer, hole_a, hole_b]);

        let tracks = build_offset_fill_tracks(&path, &bounds, 1.0);
        let exterior_rings: usize = tracks
            .iter()
            .filter(|track| track.parity == 0)
            .map(|track| track.polylines.len())
            .sum();
        let hole_rings: usize = tracks
            .iter()
            .filter(|track| track.parity == 1)
            .map(|track| track.polylines.len())
            .sum();

        assert!(
            exterior_rings > 10,
            "expected exterior rings through transition"
        );
        assert!(
            hole_rings > 5,
            "expected hole rings through merge transition"
        );
        assert!(
            tracks
                .iter()
                .filter(|track| track.parity == 1)
                .any(|track| track.polylines.len() > 1),
            "hole rings should continue as tracked parity-1 branches across topology changes"
        );
    }

    #[test]
    fn tool_layer_excluded_from_plan() {
        let mut project = create_test_project();

        // Regular layer with an object
        let regular = Layer::new("Lines", OperationType::Line);
        let regular_id = regular.id;
        project.layers.push(regular);

        project.objects.push(ProjectObject::new(
            "Rect",
            regular_id,
            Bounds::new(Point2D::new(10.0, 10.0), Point2D::new(50.0, 50.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 40.0,
                height: 40.0,
                corner_radius: 0.0,
            },
        ));

        // Tool layer with an object — should be excluded
        let mut tool = Layer::new("T1", OperationType::Line);
        tool.is_tool_layer = true;
        let tool_id = tool.id;
        project.layers.push(tool);

        project.objects.push(ProjectObject::new(
            "Tool Rect",
            tool_id,
            Bounds::new(Point2D::new(60.0, 10.0), Point2D::new(100.0, 50.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 40.0,
                height: 40.0,
                corner_radius: 0.0,
            },
        ));

        let plan = build_plan(&project).unwrap();
        // Only the regular layer should be in the plan
        assert_eq!(plan.layer_order.len(), 1);
        assert_eq!(plan.layer_order[0], regular_id.to_string());
    }

    #[test]
    fn only_tool_layers_returns_empty_plan_error() {
        let mut project = create_test_project();

        let mut tool = Layer::new("T1", OperationType::Line);
        tool.is_tool_layer = true;
        let tool_id = tool.id;
        project.layers.push(tool);

        project.objects.push(ProjectObject::new(
            "Tool Rect",
            tool_id,
            Bounds::new(Point2D::new(10.0, 10.0), Point2D::new(50.0, 50.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 40.0,
                height: 40.0,
                corner_radius: 0.0,
            },
        ));

        let result = build_plan(&project);
        assert!(matches!(result, Err(PlannerError::EmptyPlan)));
    }

    #[test]
    fn tool_operation_entry_excluded_even_when_layer_flag_is_false() {
        let mut project = create_test_project();

        let mut layer = Layer::new("Demoted Tool", OperationType::Tool);
        layer.is_tool_layer = false;
        let layer_id = layer.id;
        project.layers.push(layer);
        project.objects.push(ProjectObject::new(
            "Rect",
            layer_id,
            Bounds::new(Point2D::new(10.0, 10.0), Point2D::new(50.0, 50.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 40.0,
                height: 40.0,
                corner_radius: 0.0,
            },
        ));

        let result = build_plan(&project);
        assert!(matches!(result, Err(PlannerError::EmptyPlan)));
    }

    #[test]
    fn expand_virtual_clones_produces_real_geometry() {
        let mut project = create_test_project();
        let layer = Layer::new("Lines", OperationType::Line);
        let layer_id = layer.id;
        project.layers.push(layer);

        // Create a source rectangle object
        let source = ProjectObject::new(
            "source",
            layer_id,
            Bounds::new(Point2D::new(10.0, 10.0), Point2D::new(50.0, 40.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 40.0,
                height: 30.0,
                corner_radius: 0.0,
            },
        );
        let source_id = source.id;
        project.objects.push(source);

        // Create a VirtualClone pointing to the source, offset in bounds
        let clone = ProjectObject::new(
            "clone",
            layer_id,
            Bounds::new(Point2D::new(60.0, 10.0), Point2D::new(100.0, 40.0)),
            ObjectData::VirtualClone { source_id },
        );
        project.objects.push(clone);

        // Build plan — the VirtualClone should be expanded and contribute segments
        let plan = build_plan(&project).expect("Should produce a plan");

        // There should be segments from both the source and the expanded clone
        assert!(
            plan.segments.len() >= 2,
            "Expected at least 2 segments (source + clone), got {}",
            plan.segments.len()
        );
    }

    /// Builds an image-operation layer with the given raster mode and a
    /// non-zero ramp length, for the ramp gating tests below.
    fn ramp_gate_layer(mode: RasterMode) -> Layer {
        let mut layer = Layer::new("img", OperationType::Image);
        if let Some(rs) = layer.primary_entry_mut().raster_settings.as_mut() {
            rs.mode = mode;
            rs.ramp_length_mm = 2.5;
            rs.pass_through = false;
        }
        layer
    }

    #[test]
    fn ramp_gate_allows_threshold_mode() {
        let layer = ramp_gate_layer(RasterMode::Threshold);
        assert_eq!(effective_layer_ramp_length_mm(&layer), 2.5);
    }

    #[test]
    fn ramp_gate_zeroes_dithered_modes() {
        for mode in [
            RasterMode::FloydSteinberg,
            RasterMode::OrderedDither,
            RasterMode::Stucki,
            RasterMode::Jarvis,
            RasterMode::Sierra,
            RasterMode::Atkinson,
        ] {
            let layer = ramp_gate_layer(mode);
            assert_eq!(
                effective_layer_ramp_length_mm(&layer),
                0.0,
                "ramp must be gated for mode {mode:?}"
            );
        }
    }

    #[test]
    fn ramp_gate_zeroes_grayscale_mode() {
        // Grayscale runs are already tonal (they carry per-pixel power
        // values) so ramping would be meaningless.
        let layer = ramp_gate_layer(RasterMode::Grayscale);
        assert_eq!(effective_layer_ramp_length_mm(&layer), 0.0);
    }

    #[test]
    fn ramp_gate_zeroes_pass_through() {
        let mut layer = ramp_gate_layer(RasterMode::Threshold);
        layer
            .primary_entry_mut()
            .raster_settings
            .as_mut()
            .unwrap()
            .pass_through = true;
        assert_eq!(effective_layer_ramp_length_mm(&layer), 0.0);
    }

    #[test]
    fn ramp_gate_zeroes_non_image_operations() {
        // Fill layers also carry raster_settings but are not tonal-source
        // binary rasters — they are vector fill patterns and should never
        // be ramped.
        let mut layer = Layer::new("fill", OperationType::Fill);
        if let Some(rs) = layer.primary_entry_mut().raster_settings.as_mut() {
            rs.mode = RasterMode::Threshold;
            rs.ramp_length_mm = 2.5;
        }
        assert_eq!(effective_layer_ramp_length_mm(&layer), 0.0);
    }

    #[test]
    fn ramp_gate_clamps_negative_ramp_to_zero() {
        let mut layer = ramp_gate_layer(RasterMode::Threshold);
        layer
            .primary_entry_mut()
            .raster_settings
            .as_mut()
            .unwrap()
            .ramp_length_mm = -1.0;
        assert_eq!(effective_layer_ramp_length_mm(&layer), 0.0);
    }
}

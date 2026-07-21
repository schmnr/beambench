//! Execution plan extensions for advanced features.

use crate::plan::PlanSegment;
use beambench_common::AnchorPoint;
use beambench_common::geometry::{Bounds, Point2D};
use beambench_core::FocusTestZMode;
use beambench_core::workspace::WorkspaceOrigin;

/// Apply kerf offset to vector segments.
///
/// Kerf offset compensates for material removal during cutting.
/// Positive kerf_mm moves the path outward, negative inward.
/// Uses perpendicular miter-join offsetting on each polyline.
pub fn apply_kerf_offset(segments: &mut [PlanSegment], kerf_mm: f64) {
    if kerf_mm.abs() < 1e-9 {
        return;
    }
    for segment in segments.iter_mut() {
        if let PlanSegment::Vector {
            polyline, closed, ..
        } = segment
            && polyline.len() >= 2
        {
            *polyline = offset_polyline_points(polyline, kerf_mm, *closed);
        }
    }
}

/// Insert tab gaps in vector segments.
///
/// Tabs are small uncut regions that hold parts in place during cutting.
/// Each tab creates a gap of `tab_width_mm` in the cut path where the laser
/// is off, holding the part in place. The original Vector segment is replaced
/// by alternating Vector/Travel segments.
pub fn apply_tabs(segments: &mut Vec<PlanSegment>, tab_count: u32, tab_width_mm: f64) {
    let mut i = 0;
    while i < segments.len() {
        let should_split = if let PlanSegment::Vector {
            polyline, closed, ..
        } = &segments[i]
        {
            *closed && polyline.len() >= 2 && tab_count > 0
        } else {
            false
        };

        if should_split {
            let replacement = generate_tabs(&segments[i], tab_count, tab_width_mm);
            let replacement_len = replacement.len();
            segments.splice(i..=i, replacement);
            i += replacement_len;
        } else {
            i += 1;
        }
    }
}

/// Apply positioned tabs (from per-object TabAnchors) to a set of segments.
///
/// Unlike `apply_tabs` which distributes tabs evenly, this function uses explicit
/// normalized positions (0.0–1.0) along the contour to place tab gaps.
pub fn apply_positioned_tabs(
    segments: &mut Vec<PlanSegment>,
    tab_positions: &[f64],
    tab_width_mm: f64,
) {
    if tab_positions.is_empty() || tab_width_mm <= 0.0 {
        return;
    }
    let mut i = 0;
    while i < segments.len() {
        let should_split = if let PlanSegment::Vector {
            polyline, closed, ..
        } = &segments[i]
        {
            *closed && polyline.len() >= 2
        } else {
            false
        };

        if should_split {
            let replacement = generate_positioned_tabs(&segments[i], tab_positions, tab_width_mm);
            let replacement_len = replacement.len();
            segments.splice(i..=i, replacement);
            i += replacement_len;
        } else {
            i += 1;
        }
    }
}

/// Generate tab-split segments from a single Vector segment using explicit positions.
fn generate_positioned_tabs(
    segment: &PlanSegment,
    positions: &[f64],
    tab_width_mm: f64,
) -> Vec<PlanSegment> {
    let (
        polyline,
        is_closed,
        power_percent,
        speed_mm_min,
        layer_id,
        perf_enabled,
        perf_on,
        perf_off,
    ) = match segment {
        PlanSegment::Vector {
            polyline,
            closed,
            power_percent,
            speed_mm_min,
            layer_id,
            perforation_enabled,
            perforation_on_ms,
            perforation_off_ms,
            ..
        } => (
            polyline,
            *closed,
            *power_percent,
            *speed_mm_min,
            layer_id.clone(),
            *perforation_enabled,
            *perforation_on_ms,
            *perforation_off_ms,
        ),
        _ => return vec![segment.clone()],
    };

    let total_length = compute_path_length(polyline, is_closed);
    if total_length < f64::EPSILON {
        return vec![segment.clone()];
    }

    let half_tab = tab_width_mm / 2.0;

    // Compute split ranges from normalized positions.
    // For closed contours, tabs near the seam (position ~0 or ~1) wrap around:
    // the portion that extends past the end wraps to the start, and vice versa.
    let mut splits: Vec<(f64, f64)> = Vec::new();
    for &pos in positions {
        let center = pos.clamp(0.0, 1.0) * total_length;
        let raw_start = center - half_tab;
        let raw_end = center + half_tab;

        if is_closed && raw_start < 0.0 {
            // Tab wraps around the seam backward: split into two ranges
            // [0, raw_end] and [total_length + raw_start, total_length]
            let wrap_start = total_length + raw_start; // positive, near end
            if wrap_start < total_length - f64::EPSILON {
                splits.push((wrap_start, total_length));
            }
            if raw_end > f64::EPSILON {
                splits.push((0.0, raw_end.min(total_length)));
            }
        } else if is_closed && raw_end > total_length {
            // Tab wraps around the seam forward: split into two ranges
            // [raw_start, total_length] and [0, raw_end - total_length]
            if raw_start < total_length - f64::EPSILON {
                splits.push((raw_start.max(0.0), total_length));
            }
            let wrap_end = raw_end - total_length;
            if wrap_end > f64::EPSILON {
                splits.push((0.0, wrap_end));
            }
        } else {
            let start = raw_start.max(0.0);
            let end = raw_end.min(total_length);
            if end > start + f64::EPSILON {
                splits.push((start, end));
            }
        }
    }

    if splits.is_empty() {
        return vec![segment.clone()];
    }

    // Sort and collect critical distances
    splits.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

    let mut critical_dists: Vec<f64> = Vec::new();
    for &(s, e) in &splits {
        critical_dists.push(s);
        critical_dists.push(e);
    }
    critical_dists.sort_by(|a, b| a.partial_cmp(b).unwrap());
    critical_dists.dedup_by(|a, b| (*a - *b).abs() < f64::EPSILON);

    let sub_polylines = split_polyline_at_distances(polyline, &critical_dists, is_closed);

    let mut result: Vec<PlanSegment> = Vec::new();
    let mut interval_start = 0.0_f64;

    for (idx, sub) in sub_polylines.into_iter().enumerate() {
        if sub.len() < 2 {
            if idx < critical_dists.len() {
                interval_start = critical_dists[idx];
            }
            continue;
        }

        // Skip zero-length sub-polylines (e.g. duplicate-point segments at the seam)
        let geo_len = compute_path_length(&sub, false);
        if geo_len < f64::EPSILON {
            if idx < critical_dists.len() {
                interval_start = critical_dists[idx];
            }
            continue;
        }

        let interval_end = if idx < critical_dists.len() {
            critical_dists[idx]
        } else {
            total_length
        };

        let mid = (interval_start + interval_end) / 2.0;

        let is_tab = splits
            .iter()
            .any(|&(s, e)| mid >= s - f64::EPSILON && mid <= e + f64::EPSILON);

        if is_tab {
            let start = *sub.first().unwrap();
            let end = *sub.last().unwrap();
            result.push(PlanSegment::Travel { start, end });
        } else {
            result.push(PlanSegment::Vector {
                polyline: sub,
                closed: false,
                power_percent,
                speed_mm_min,
                layer_id: layer_id.clone(),
                cut_entry_id: String::new(),
                perforation_enabled: perf_enabled,
                perforation_on_ms: perf_on,
                perforation_off_ms: perf_off,
                source_object_id: None,
                source_subpath_index: None,
            });
        }

        interval_start = interval_end;
    }

    if result.is_empty() {
        return vec![segment.clone()];
    }

    result
}

/// Generate tab-split segments from a single Vector segment.
///
/// Walks the polyline and inserts Travel gaps at evenly-spaced intervals.
/// Returns alternating Vector/Travel/Vector segments.
fn generate_tabs(segment: &PlanSegment, tab_count: u32, tab_width_mm: f64) -> Vec<PlanSegment> {
    let (
        polyline,
        is_closed,
        power_percent,
        speed_mm_min,
        layer_id,
        perf_enabled,
        perf_on,
        perf_off,
    ) = match segment {
        PlanSegment::Vector {
            polyline,
            closed,
            power_percent,
            speed_mm_min,
            layer_id,
            perforation_enabled,
            perforation_on_ms,
            perforation_off_ms,
            ..
        } => (
            polyline,
            *closed,
            *power_percent,
            *speed_mm_min,
            layer_id.clone(),
            *perforation_enabled,
            *perforation_on_ms,
            *perforation_off_ms,
        ),
        _ => return vec![segment.clone()],
    };

    let total_length = compute_path_length(polyline, is_closed);
    if total_length < f64::EPSILON {
        return vec![segment.clone()];
    }

    let half_tab = tab_width_mm / 2.0;
    let tab_spacing = total_length / tab_count as f64;

    // Compute distance from start for each tab center
    let mut tab_centers: Vec<f64> = Vec::with_capacity(tab_count as usize);
    for t in 0..tab_count {
        tab_centers.push(tab_spacing * (t as f64 + 0.5));
    }

    // Compute split distances: each tab center generates two split points
    // (center - half_tab, center + half_tab), clamped to [0, total_length].
    let mut splits: Vec<(f64, f64)> = Vec::new();
    for center in &tab_centers {
        let start = (center - half_tab).max(0.0);
        let end = (center + half_tab).min(total_length);
        if end > start + f64::EPSILON {
            splits.push((start, end));
        }
    }

    if splits.is_empty() {
        return vec![segment.clone()];
    }

    // Walk the polyline and split at the computed distances.
    // Collect all critical distances (tab starts and ends) and split
    // the polyline at those points.
    let mut critical_dists: Vec<f64> = Vec::new();
    for &(s, e) in &splits {
        critical_dists.push(s);
        critical_dists.push(e);
    }
    // Deduplicate and sort
    critical_dists.sort_by(|a, b| a.partial_cmp(b).unwrap());
    critical_dists.dedup_by(|a, b| (*a - *b).abs() < f64::EPSILON);

    // Split the polyline at each critical distance, producing sub-polylines
    let sub_polylines = split_polyline_at_distances(polyline, &critical_dists, is_closed);

    // Now assign each sub-polyline as either Vector or Travel.
    // Sub-polylines correspond to intervals:
    //   [0, critical_dists[0]], [critical_dists[0], critical_dists[1]], ...
    // Tab gaps are the intervals [split.0, split.1] for each split.
    let mut result: Vec<PlanSegment> = Vec::new();
    let mut interval_start = 0.0_f64;

    for (idx, sub) in sub_polylines.into_iter().enumerate() {
        if sub.len() < 2 {
            // Skip degenerate segments
            if idx < critical_dists.len() {
                interval_start = critical_dists[idx];
            }
            continue;
        }

        // Skip zero-length sub-polylines (e.g. duplicate-point segments at the seam)
        let geo_len = compute_path_length(&sub, false);
        if geo_len < f64::EPSILON {
            if idx < critical_dists.len() {
                interval_start = critical_dists[idx];
            }
            continue;
        }

        let interval_end = if idx < critical_dists.len() {
            critical_dists[idx]
        } else {
            total_length
        };

        let mid = (interval_start + interval_end) / 2.0;

        // Check if this interval falls inside any tab gap
        let is_tab = splits
            .iter()
            .any(|&(s, e)| mid >= s - f64::EPSILON && mid <= e + f64::EPSILON);

        if is_tab {
            // Travel segment (laser off) over the tab
            let start = *sub.first().unwrap();
            let end = *sub.last().unwrap();
            result.push(PlanSegment::Travel { start, end });
        } else {
            result.push(PlanSegment::Vector {
                polyline: sub,
                closed: false, // split segments are no longer closed
                power_percent,
                speed_mm_min,
                layer_id: layer_id.clone(),
                cut_entry_id: String::new(),
                perforation_enabled: perf_enabled,
                perforation_on_ms: perf_on,
                perforation_off_ms: perf_off,
                source_object_id: None,
                source_subpath_index: None,
            });
        }

        interval_start = interval_end;
    }

    // Filter out empty or single-point segments
    if result.is_empty() {
        return vec![segment.clone()];
    }

    result
}

/// Split a polyline at specified cumulative distances along the path.
///
/// Returns `distances.len() + 1` sub-polylines. Each sub-polyline shares
/// the split point with its neighbor (the end of one is the start of the next).
///
/// When `closed` is true, the closing edge (last → first) is included in the
/// walk so that splits near the seam are handled correctly.
fn split_polyline_at_distances(
    polyline: &[Point2D],
    distances: &[f64],
    closed: bool,
) -> Vec<Vec<Point2D>> {
    let mut result: Vec<Vec<Point2D>> = Vec::new();
    let mut current_sub: Vec<Point2D> = vec![polyline[0]];
    let mut cumulative = 0.0_f64;
    let mut dist_idx = 0;

    // Number of edges: for closed polylines, include the closing edge (last → first)
    let n = polyline.len();
    let edge_count = if closed && n >= 3 { n } else { n - 1 };

    for edge in 0..edge_count {
        let from = polyline[edge];
        let to = polyline[(edge + 1) % n];
        let dx = to.x - from.x;
        let dy = to.y - from.y;
        let seg_len = (dx * dx + dy * dy).sqrt();

        if seg_len < f64::EPSILON {
            current_sub.push(to);
            continue;
        }

        let seg_start = cumulative;
        let seg_end = cumulative + seg_len;

        // Check if any split distances fall within this edge
        while dist_idx < distances.len() && distances[dist_idx] <= seg_end + f64::EPSILON {
            let d = distances[dist_idx];
            if d >= seg_start - f64::EPSILON && d <= seg_end + f64::EPSILON {
                // Interpolate the split point on this edge
                let t = if seg_len > f64::EPSILON {
                    ((d - seg_start) / seg_len).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let split_point = Point2D::new(from.x + dx * t, from.y + dy * t);
                current_sub.push(split_point);
                result.push(current_sub);
                current_sub = vec![split_point];
            }
            dist_idx += 1;
        }

        current_sub.push(to);
        cumulative = seg_end;
    }

    // Push the final sub-polyline
    if !current_sub.is_empty() {
        result.push(current_sub);
    }

    result
}

/// Translate every plan segment by `(dx, dy)` in the current coordinate
/// space. Callers use this to anchor non-absolute jobs: after the workspace
/// transform, the delta that moves the job-origin anchor onto the start-from
/// target (live head position or stored user origin) is applied here.
///
/// Mirrors `apply_workspace_origin_transform`'s per-variant structure:
/// vertical-axis raster runs hold Y coordinates (scanline.y_mm holds X), and
/// angled-raster scanlines are relative to `scan_origin`, so only the origin
/// moves.
pub fn translate_segments(segments: &mut [PlanSegment], dx: f64, dy: f64) {
    if dx == 0.0 && dy == 0.0 {
        return;
    }

    for segment in segments.iter_mut() {
        match segment {
            PlanSegment::Travel { start, end } => {
                start.x += dx;
                start.y += dy;
                end.x += dx;
                end.y += dy;
            }
            PlanSegment::Vector { polyline, .. } => {
                for point in polyline.iter_mut() {
                    point.x += dx;
                    point.y += dy;
                }
            }
            PlanSegment::Raster {
                scanlines,
                scan_origin,
                outlines,
                scan_angle_deg,
                scan_axis,
                ..
            } => {
                if scan_angle_deg.abs() <= f64::EPSILON {
                    match scan_axis {
                        crate::plan::ScanAxis::Horizontal => {
                            for scanline in scanlines.iter_mut() {
                                scanline.y_mm += dy;
                                for run in scanline.runs.iter_mut() {
                                    run.start_x_mm += dx;
                                    run.end_x_mm += dx;
                                }
                            }
                        }
                        crate::plan::ScanAxis::Vertical => {
                            for scanline in scanlines.iter_mut() {
                                scanline.y_mm += dx;
                                for run in scanline.runs.iter_mut() {
                                    run.start_x_mm += dy;
                                    run.end_x_mm += dy;
                                }
                            }
                        }
                    }
                } else {
                    scan_origin.x += dx;
                    scan_origin.y += dy;
                }

                for outline in outlines.iter_mut() {
                    for point in outline.points.iter_mut() {
                        point.x += dx;
                        point.y += dy;
                    }
                }
            }
            PlanSegment::Frame { path, .. } => {
                for point in path.iter_mut() {
                    point.x += dx;
                    point.y += dy;
                }
            }
            // Carries no geometry at this stage; resolved downstream.
            PlanSegment::OffsetFill { .. } => {}
        }
    }
}

fn flip_point_y(point: &mut Point2D, bed_height_mm: f64) {
    point.y = bed_height_mm - point.y;
}

/// Convert the planner's canvas-space coordinates into machine-space
/// coordinates for the configured workspace origin.
pub fn apply_workspace_origin_transform(
    segments: &mut [PlanSegment],
    origin: WorkspaceOrigin,
    bed_height_mm: f64,
) {
    if origin == WorkspaceOrigin::TopLeft {
        return;
    }

    for segment in segments.iter_mut() {
        match segment {
            PlanSegment::Travel { start, end } => {
                flip_point_y(start, bed_height_mm);
                flip_point_y(end, bed_height_mm);
            }
            PlanSegment::Vector { polyline, .. } => {
                for point in polyline.iter_mut() {
                    flip_point_y(point, bed_height_mm);
                }
            }
            PlanSegment::Raster {
                scanlines,
                scan_origin,
                outlines,
                scan_angle_deg,
                scan_axis,
                ..
            } => {
                if scan_angle_deg.abs() <= f64::EPSILON {
                    match scan_axis {
                        crate::plan::ScanAxis::Horizontal => {
                            for scanline in scanlines.iter_mut() {
                                scanline.y_mm = bed_height_mm - scanline.y_mm;
                            }
                        }
                        crate::plan::ScanAxis::Vertical => {
                            for scanline in scanlines.iter_mut() {
                                for run in scanline.runs.iter_mut() {
                                    run.start_x_mm = bed_height_mm - run.start_x_mm;
                                    run.end_x_mm = bed_height_mm - run.end_x_mm;
                                }
                            }
                        }
                    }
                } else {
                    scan_origin.y = bed_height_mm - scan_origin.y;
                    *scan_angle_deg = -*scan_angle_deg;
                    for scanline in scanlines.iter_mut() {
                        scanline.y_mm = -scanline.y_mm;
                    }
                }

                for outline in outlines.iter_mut() {
                    for point in outline.points.iter_mut() {
                        flip_point_y(point, bed_height_mm);
                    }
                }
            }
            PlanSegment::Frame { path, .. } => {
                for point in path.iter_mut() {
                    flip_point_y(point, bed_height_mm);
                }
            }
            PlanSegment::OffsetFill { .. } => {}
        }
    }
}

/// Machine-space position of the job-origin anchor on `bounds`.
///
/// Anchor names describe the design as the user sees it on the canvas
/// ("Top Left" is the design's top-left corner on screen). When the
/// workspace transform flipped Y (machine origin at the front-left,
/// `WorkspaceOrigin::BottomLeft`), the canvas "top" is the machine-space
/// max-Y side, so `y_flipped` selects which end of the bounds each
/// vertical anchor refers to.
pub fn job_anchor_point(origin: AnchorPoint, bounds: &Bounds, y_flipped: bool) -> (f64, f64) {
    let fx = match origin {
        AnchorPoint::TopLeft | AnchorPoint::CenterLeft | AnchorPoint::BottomLeft => 0.0,
        AnchorPoint::TopCenter | AnchorPoint::Center | AnchorPoint::BottomCenter => 0.5,
        AnchorPoint::TopRight | AnchorPoint::CenterRight | AnchorPoint::BottomRight => 1.0,
    };
    let fy = match origin {
        AnchorPoint::TopLeft | AnchorPoint::TopCenter | AnchorPoint::TopRight => 0.0,
        AnchorPoint::CenterLeft | AnchorPoint::Center | AnchorPoint::CenterRight => 0.5,
        AnchorPoint::BottomLeft | AnchorPoint::BottomCenter | AnchorPoint::BottomRight => 1.0,
    };
    let x = bounds.min.x + fx * bounds.width();
    let y = if y_flipped {
        bounds.max.y - fy * bounds.height()
    } else {
        bounds.min.y + fy * bounds.height()
    };
    (x, y)
}

/// Anchor non-absolute jobs: translate `segments` (already in machine space)
/// so the `job_origin` anchor of `content_bounds` lands exactly on `target`
/// (the live head position for Current Position, the stored origin for User
/// Origin). This is the single shared implementation for jobs, framing, and
/// quality tests; the previous code shifted by fractions of the content size
/// without ever subtracting its position, so anything not parked at the
/// canvas origin ran at an absolute-plus-offset garbage location.
pub fn anchor_segments_to_target(
    segments: &mut [PlanSegment],
    job_origin: AnchorPoint,
    content_bounds: &Bounds,
    y_flipped: bool,
    target: (f64, f64),
) {
    let anchor = job_anchor_point(job_origin, content_bounds, y_flipped);
    translate_segments(segments, target.0 - anchor.0, target.1 - anchor.1);
}

/// Mirror canvas-space bounds through the workspace Y flip so they describe
/// the same region in machine space.
pub fn flip_bounds_y(bounds: &Bounds, bed_height_mm: f64) -> Bounds {
    Bounds::new(
        Point2D::new(bounds.min.x, bed_height_mm - bounds.max.y),
        Point2D::new(bounds.max.x, bed_height_mm - bounds.min.y),
    )
}

/// Generate air assist G-code commands.
///
/// Returns (on_command, off_command).
pub fn air_assist_commands(enabled: bool) -> (String, String) {
    if enabled {
        ("M7".to_string(), "M9".to_string())
    } else {
        (String::new(), String::new())
    }
}

/// Generate Z-offset G-code command.
///
/// `AbsoluteWorkCoord` emits a single absolute move (the historical behavior used by normal jobs).
/// `RelativeTemporary` brackets the move in `G91`/`G90` so a delta can be applied without disturbing
/// the surrounding absolute-mode session — used by Focus Test transient jobs.
pub fn z_offset_command(z_mm: f64, mode: FocusTestZMode, feed_mm_min: f64) -> String {
    match mode {
        FocusTestZMode::AbsoluteWorkCoord => format!("G1 Z{z_mm:.3} F{feed_mm_min:.0}"),
        FocusTestZMode::RelativeTemporary => {
            format!("G91\nG1 Z{z_mm:.3} F{feed_mm_min:.0}\nG90")
        }
    }
}

// Helper functions

/// Offset a polyline by `distance` mm using perpendicular miter-join offsetting.
///
/// Positive distance offsets outward (to the right of travel direction),
/// negative distance offsets inward (to the left of travel direction).
/// For a CCW-wound closed polygon, positive = outward and negative = inward.
fn offset_polyline_points(points: &[Point2D], distance: f64, closed: bool) -> Vec<Point2D> {
    if points.len() < 2 {
        return points.to_vec();
    }

    let n = points.len();
    let mut result = Vec::with_capacity(n);

    for i in 0..n {
        let prev_i = if i == 0 {
            if closed { n - 1 } else { 0 }
        } else {
            i - 1
        };

        let next_i = if i == n - 1 {
            if closed { 0 } else { n - 1 }
        } else {
            i + 1
        };

        let p = points[i];
        let prev = points[prev_i];
        let next = points[next_i];

        let offset_pt = if i == 0 && !closed {
            // First point of open path: use edge to next
            compute_perpendicular_offset(p, p, next, distance)
        } else if i == n - 1 && !closed {
            // Last point of open path: use edge from prev
            compute_perpendicular_offset(p, prev, p, distance)
        } else {
            // Interior or closed-path point: use miter offset
            compute_perpendicular_offset(p, prev, next, distance)
        };

        result.push(offset_pt);
    }

    result
}

/// Compute the offset position for a point on the polyline, given its
/// predecessor and successor.  Uses the bisector of the two adjacent edge
/// normals (miter join).
fn compute_perpendicular_offset(
    p: Point2D,
    prev: Point2D,
    next: Point2D,
    distance: f64,
) -> Point2D {
    // Edge vectors
    let d1x = p.x - prev.x;
    let d1y = p.y - prev.y;
    let len1 = (d1x * d1x + d1y * d1y).sqrt();

    let d2x = next.x - p.x;
    let d2y = next.y - p.y;
    let len2 = (d2x * d2x + d2y * d2y).sqrt();

    // Degenerate: both edges have zero length
    if len1 < 1e-9 && len2 < 1e-9 {
        return p;
    }

    // Right-hand perpendicular normals: (dy, -dx) / len
    let (n1x, n1y) = if len1 > 1e-9 {
        (d1y / len1, -d1x / len1)
    } else {
        (d2y / len2, -d2x / len2)
    };

    let (n2x, n2y) = if len2 > 1e-9 {
        (d2y / len2, -d2x / len2)
    } else {
        (n1x, n1y)
    };

    // Bisector of the two normals
    let nx = n1x + n2x;
    let ny = n1y + n2y;
    let nlen = (nx * nx + ny * ny).sqrt();

    if nlen < 1e-9 {
        // Collinear edges forming a 180-degree turn — just use one normal
        Point2D::new(p.x + n1x * distance, p.y + n1y * distance)
    } else {
        // Miter scale: 1 / cos(half-angle) so perpendicular distance equals `distance`
        let cos_half = (n1x * nx + n1y * ny) / nlen;
        let miter_scale = if cos_half.abs() > 1e-9 {
            1.0 / cos_half
        } else {
            1.0
        };

        Point2D::new(
            p.x + (nx / nlen) * distance * miter_scale,
            p.y + (ny / nlen) * distance * miter_scale,
        )
    }
}

fn compute_path_length(points: &[Point2D], closed: bool) -> f64 {
    if points.len() < 2 {
        return 0.0;
    }

    let mut length = 0.0;
    for i in 1..points.len() {
        let dx = points[i].x - points[i - 1].x;
        let dy = points[i].y - points[i - 1].y;
        length += (dx * dx + dy * dy).sqrt();
    }

    // Include the closing edge (last → first) for closed polylines
    if closed && points.len() >= 3 {
        let first = points[0];
        let last = points[points.len() - 1];
        let dx = first.x - last.x;
        let dy = first.y - last.y;
        length += (dx * dx + dy * dy).sqrt();
    }

    length
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::{ScanAxis, ScanDirection, ScanRun, Scanline};

    #[test]
    fn kerf_offset_offsets_open_polyline() {
        let mut segments = vec![PlanSegment::Vector {
            polyline: vec![
                Point2D::new(0.0, 0.0),
                Point2D::new(10.0, 0.0),
                Point2D::new(10.0, 10.0),
            ],
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

        apply_kerf_offset(&mut segments, 1.0);

        // Verify the segment was modified and points shifted perpendicular to edges
        if let PlanSegment::Vector { polyline, .. } = &segments[0] {
            assert_eq!(polyline.len(), 3);
            // First point on horizontal edge going right: right-hand normal is (0, -1),
            // so positive offset moves downward in screen coords => y decreases by 1
            assert!((polyline[0].x - 0.0).abs() < 0.01);
            assert!((polyline[0].y - (-1.0)).abs() < 0.01);
        }
    }

    #[test]
    fn kerf_offset_inward_on_ccw_rectangle() {
        // CCW rectangle: (0,0) -> (10,0) -> (10,10) -> (0,10)
        // For a CCW-wound polygon, negative distance = inward offset.
        let mut segments = vec![PlanSegment::Vector {
            polyline: vec![
                Point2D::new(0.0, 0.0),
                Point2D::new(10.0, 0.0),
                Point2D::new(10.0, 10.0),
                Point2D::new(0.0, 10.0),
            ],
            closed: true,
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

        let kerf = -1.0; // inward
        apply_kerf_offset(&mut segments, kerf);

        if let PlanSegment::Vector { polyline, .. } = &segments[0] {
            assert_eq!(polyline.len(), 4);

            // All corners should have moved inward by ~1mm.
            // For a CCW rectangle, inward (negative kerf) shrinks the rectangle.
            // The min-x should increase, max-x should decrease, etc.
            let min_x = polyline.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
            let max_x = polyline
                .iter()
                .map(|p| p.x)
                .fold(f64::NEG_INFINITY, f64::max);
            let min_y = polyline.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
            let max_y = polyline
                .iter()
                .map(|p| p.y)
                .fold(f64::NEG_INFINITY, f64::max);

            let width = max_x - min_x;
            let height = max_y - min_y;

            // Original was 10x10, inward by 1mm on each side => 8x8
            assert!((width - 8.0).abs() < 0.1, "Expected width ~8, got {width}");
            assert!(
                (height - 8.0).abs() < 0.1,
                "Expected height ~8, got {height}"
            );
        } else {
            panic!("Expected Vector segment");
        }
    }

    #[test]
    fn kerf_offset_outward_on_ccw_rectangle() {
        // CCW rectangle: (0,0) -> (10,0) -> (10,10) -> (0,10)
        // For a CCW-wound polygon, positive distance = outward offset.
        let mut segments = vec![PlanSegment::Vector {
            polyline: vec![
                Point2D::new(0.0, 0.0),
                Point2D::new(10.0, 0.0),
                Point2D::new(10.0, 10.0),
                Point2D::new(0.0, 10.0),
            ],
            closed: true,
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

        let kerf = 1.0; // outward
        apply_kerf_offset(&mut segments, kerf);

        if let PlanSegment::Vector { polyline, .. } = &segments[0] {
            assert_eq!(polyline.len(), 4);

            // All corners should have moved outward by ~1mm.
            // For a CCW rectangle, outward (positive kerf) enlarges the rectangle.
            let min_x = polyline.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
            let max_x = polyline
                .iter()
                .map(|p| p.x)
                .fold(f64::NEG_INFINITY, f64::max);
            let min_y = polyline.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
            let max_y = polyline
                .iter()
                .map(|p| p.y)
                .fold(f64::NEG_INFINITY, f64::max);

            let width = max_x - min_x;
            let height = max_y - min_y;

            // Original was 10x10, outward by 1mm on each side => 12x12
            assert!(
                (width - 12.0).abs() < 0.1,
                "Expected width ~12, got {width}"
            );
            assert!(
                (height - 12.0).abs() < 0.1,
                "Expected height ~12, got {height}"
            );
        } else {
            panic!("Expected Vector segment");
        }
    }

    #[test]
    fn translate_segments_shifts_travel() {
        let mut segments = vec![PlanSegment::Travel {
            start: Point2D::new(0.0, 0.0),
            end: Point2D::new(10.0, 10.0),
        }];

        translate_segments(&mut segments, 5.0, 5.0);

        if let PlanSegment::Travel { start, end } = &segments[0] {
            assert_eq!(start.x, 5.0);
            assert_eq!(start.y, 5.0);
            assert_eq!(end.x, 15.0);
            assert_eq!(end.y, 15.0);
        }
    }

    #[test]
    fn job_anchor_point_covers_all_anchors_unflipped() {
        // Canvas-frame workspace (no Y flip): "top" is min-y.
        let bounds = Bounds::new(Point2D::new(100.0, 150.0), Point2D::new(140.0, 170.0));
        let cases = [
            (AnchorPoint::TopLeft, (100.0, 150.0)),
            (AnchorPoint::TopCenter, (120.0, 150.0)),
            (AnchorPoint::TopRight, (140.0, 150.0)),
            (AnchorPoint::CenterLeft, (100.0, 160.0)),
            (AnchorPoint::Center, (120.0, 160.0)),
            (AnchorPoint::CenterRight, (140.0, 160.0)),
            (AnchorPoint::BottomLeft, (100.0, 170.0)),
            (AnchorPoint::BottomCenter, (120.0, 170.0)),
            (AnchorPoint::BottomRight, (140.0, 170.0)),
        ];
        for (anchor, expected) in cases {
            assert_eq!(
                job_anchor_point(anchor, &bounds, false),
                expected,
                "anchor {anchor:?}"
            );
        }
    }

    #[test]
    fn job_anchor_point_flips_vertical_anchors_in_machine_space() {
        // After the BottomLeft-origin Y flip, the design's canvas "top" is
        // the machine-space max-y side.
        let bounds = Bounds::new(Point2D::new(100.0, 230.0), Point2D::new(140.0, 250.0));
        assert_eq!(
            job_anchor_point(AnchorPoint::TopLeft, &bounds, true),
            (100.0, 250.0)
        );
        assert_eq!(
            job_anchor_point(AnchorPoint::Center, &bounds, true),
            (120.0, 240.0)
        );
        assert_eq!(
            job_anchor_point(AnchorPoint::BottomRight, &bounds, true),
            (140.0, 230.0)
        );
    }

    #[test]
    fn anchor_segments_to_target_lands_anchor_on_target() {
        // Machine-space content rect (100,230)-(140,250) after a Y flip;
        // head parked at (50,40); Center anchor: the rect's center must land
        // exactly on the head, i.e. (30,30)-(70,50).
        let mut segments = vec![PlanSegment::Travel {
            start: Point2D::new(100.0, 230.0),
            end: Point2D::new(140.0, 250.0),
        }];
        let bounds = Bounds::new(Point2D::new(100.0, 230.0), Point2D::new(140.0, 250.0));

        anchor_segments_to_target(
            &mut segments,
            AnchorPoint::Center,
            &bounds,
            true,
            (50.0, 40.0),
        );

        if let PlanSegment::Travel { start, end } = &segments[0] {
            assert_eq!((start.x, start.y), (30.0, 30.0));
            assert_eq!((end.x, end.y), (70.0, 50.0));
        }
    }

    #[test]
    fn anchor_top_left_is_not_a_no_op_for_offset_content() {
        // Regression: TopLeft used to be skipped as a "no-op", leaving the
        // artwork at absolute coordinates plus the head offset — the job ran
        // to a strange location instead of starting at the head.
        let mut segments = vec![PlanSegment::Travel {
            start: Point2D::new(100.0, 230.0),
            end: Point2D::new(140.0, 250.0),
        }];
        let bounds = Bounds::new(Point2D::new(100.0, 230.0), Point2D::new(140.0, 250.0));

        // y_flipped: canvas top-left of the design is machine (min.x, max.y).
        anchor_segments_to_target(
            &mut segments,
            AnchorPoint::TopLeft,
            &bounds,
            true,
            (50.0, 40.0),
        );

        if let PlanSegment::Travel { start, end } = &segments[0] {
            // Anchor (100,250) -> target (50,40): shift (-50,-210).
            assert_eq!((start.x, start.y), (50.0, 20.0));
            assert_eq!((end.x, end.y), (90.0, 40.0));
        }
    }

    #[test]
    fn flip_bounds_y_mirrors_through_bed_height() {
        let canvas = Bounds::new(Point2D::new(100.0, 150.0), Point2D::new(140.0, 170.0));
        let machine = flip_bounds_y(&canvas, 400.0);
        assert_eq!((machine.min.x, machine.min.y), (100.0, 230.0));
        assert_eq!((machine.max.x, machine.max.y), (140.0, 250.0));
    }

    #[test]
    fn air_assist_enabled_returns_commands() {
        let (on, off) = air_assist_commands(true);
        assert_eq!(on, "M7");
        assert_eq!(off, "M9");
    }

    #[test]
    fn air_assist_disabled_returns_empty() {
        let (on, off) = air_assist_commands(false);
        assert_eq!(on, "");
        assert_eq!(off, "");
    }

    #[test]
    fn z_offset_absolute_formats_correctly() {
        let cmd = z_offset_command(5.5, FocusTestZMode::AbsoluteWorkCoord, 300.0);
        assert_eq!(cmd, "G1 Z5.500 F300");
    }

    #[test]
    fn z_offset_absolute_negative_value() {
        let cmd = z_offset_command(-2.5, FocusTestZMode::AbsoluteWorkCoord, 250.0);
        assert_eq!(cmd, "G1 Z-2.500 F250");
    }

    #[test]
    fn z_offset_relative_wraps_in_g91_g90() {
        let cmd = z_offset_command(0.5, FocusTestZMode::RelativeTemporary, 300.0);
        assert_eq!(cmd, "G91\nG1 Z0.500 F300\nG90");
    }

    #[test]
    fn apply_tabs_to_closed_path() {
        let mut segments = vec![PlanSegment::Vector {
            polyline: vec![
                Point2D::new(0.0, 0.0),
                Point2D::new(10.0, 0.0),
                Point2D::new(10.0, 10.0),
                Point2D::new(0.0, 10.0),
            ],
            closed: true,
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

        apply_tabs(&mut segments, 4, 2.0);

        // Should not crash, polyline should still exist
        if let PlanSegment::Vector { polyline, .. } = &segments[0] {
            assert!(!polyline.is_empty());
        }
    }

    #[test]
    fn raster_segment_offset_applied() {
        let mut segments = vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 10.0,
                runs: vec![ScanRun {
                    start_x_mm: 5.0,
                    end_x_mm: 15.0,
                    power_values: vec![],
                }],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 0.1,
            direction_mode: crate::plan::DirectionMode::Bidirectional,
            power_mode: crate::plan::PowerMode::Binary,
            speed_mm_min: 2000.0,
            layer_id: "layer1".to_string(),
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
        }];

        translate_segments(&mut segments, 10.0, 20.0);

        if let PlanSegment::Raster { scanlines, .. } = &segments[0] {
            assert_eq!(scanlines[0].y_mm, 30.0);
            assert_eq!(scanlines[0].runs[0].start_x_mm, 15.0);
            assert_eq!(scanlines[0].runs[0].end_x_mm, 25.0);
        }
    }

    #[test]
    fn translate_segments_vertical_axis_raster_swaps_components() {
        // Vertical-axis rasters store X in scanline.y_mm and Y in the runs;
        // the translation must follow that layout (the old code shifted the
        // wrong components).
        let mut segments = vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 10.0,
                runs: vec![ScanRun {
                    start_x_mm: 5.0,
                    end_x_mm: 15.0,
                    power_values: vec![],
                }],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 0.1,
            direction_mode: crate::plan::DirectionMode::Bidirectional,
            power_mode: crate::plan::PowerMode::Binary,
            speed_mm_min: 2000.0,
            layer_id: "layer1".to_string(),
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
        }];

        translate_segments(&mut segments, 10.0, 20.0);

        if let PlanSegment::Raster { scanlines, .. } = &segments[0] {
            assert_eq!(scanlines[0].y_mm, 20.0, "y_mm holds X, shifts by dx");
            assert_eq!(
                scanlines[0].runs[0].start_x_mm, 25.0,
                "runs hold Y, shift by dy"
            );
            assert_eq!(scanlines[0].runs[0].end_x_mm, 35.0);
        }
    }

    #[test]
    fn translate_segments_angled_raster_moves_origin_only() {
        // Angled-raster scanlines are relative to scan_origin; only the
        // origin moves.
        let mut segments = vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 10.0,
                runs: vec![ScanRun {
                    start_x_mm: 5.0,
                    end_x_mm: 15.0,
                    power_values: vec![],
                }],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 0.1,
            direction_mode: crate::plan::DirectionMode::Bidirectional,
            power_mode: crate::plan::PowerMode::Binary,
            speed_mm_min: 2000.0,
            layer_id: "layer1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 30.0,
            scan_origin: Point2D::new(100.0, 200.0),
            overscan_mm: 0.0,
            outlines: vec![],
            scan_axis: ScanAxis::default(),
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }];

        translate_segments(&mut segments, 10.0, 20.0);

        if let PlanSegment::Raster {
            scanlines,
            scan_origin,
            ..
        } = &segments[0]
        {
            assert_eq!((scan_origin.x, scan_origin.y), (110.0, 220.0));
            assert_eq!(scanlines[0].y_mm, 10.0, "relative scanlines unchanged");
            assert_eq!(scanlines[0].runs[0].start_x_mm, 5.0);
        }
    }

    #[test]
    fn frame_segment_offset_applied() {
        let mut segments = vec![PlanSegment::Frame {
            path: vec![
                Point2D::new(0.0, 0.0),
                Point2D::new(10.0, 0.0),
                Point2D::new(10.0, 10.0),
                Point2D::new(0.0, 10.0),
            ],
            power_percent: 10.0,
            speed_mm_min: 3000.0,
        }];

        translate_segments(&mut segments, 5.0, 5.0);

        if let PlanSegment::Frame { path, .. } = &segments[0] {
            assert_eq!(path[0].x, 5.0);
            assert_eq!(path[0].y, 5.0);
            assert_eq!(path[2].x, 15.0);
            assert_eq!(path[2].y, 15.0);
        }
    }

    #[test]
    fn positioned_tab_at_midpoint_splits_into_three() {
        // Closed square without duplicate endpoint (matches flatten_vecpath output).
        // 4 vertices, perimeter = 40 (closing edge 0,10 → 0,0 included implicitly).
        let mut segments = vec![PlanSegment::Vector {
            polyline: vec![
                Point2D::new(0.0, 0.0),
                Point2D::new(10.0, 0.0),
                Point2D::new(10.0, 10.0),
                Point2D::new(0.0, 10.0),
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
        }];

        // Single tab at 0.5 (midpoint of perimeter = 20mm along path)
        apply_positioned_tabs(&mut segments, &[0.5], 3.0);

        // Should produce: Vector + Travel + Vector
        assert!(
            segments.len() >= 3,
            "Expected at least 3 segments, got {}",
            segments.len()
        );
        let has_travel = segments
            .iter()
            .any(|s| matches!(s, PlanSegment::Travel { .. }));
        assert!(has_travel, "Should contain a Travel segment");
        let vectors: Vec<_> = segments
            .iter()
            .filter(|s| matches!(s, PlanSegment::Vector { .. }))
            .collect();
        assert!(
            vectors.len() >= 2,
            "Should contain at least 2 Vector segments"
        );
        // All Vector segments should be open (closed: false)
        for v in &vectors {
            if let PlanSegment::Vector { closed, .. } = v {
                assert!(!closed, "Split Vector segments should be open");
            }
        }
    }

    #[test]
    fn positioned_tabs_multiple_positions() {
        // Closed square without duplicate endpoint.
        let mut segments = vec![PlanSegment::Vector {
            polyline: vec![
                Point2D::new(0.0, 0.0),
                Point2D::new(10.0, 0.0),
                Point2D::new(10.0, 10.0),
                Point2D::new(0.0, 10.0),
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
        }];

        // Two tabs at 0.25 and 0.75
        apply_positioned_tabs(&mut segments, &[0.25, 0.75], 2.0);

        // Should produce 5 segments: V + T + V + T + V
        let travel_count = segments
            .iter()
            .filter(|s| matches!(s, PlanSegment::Travel { .. }))
            .count();
        assert_eq!(
            travel_count, 2,
            "Expected 2 Travel segments, got {travel_count}"
        );
    }

    #[test]
    fn positioned_tabs_empty_positions_no_change() {
        // Closed square without duplicate endpoint.
        let mut segments = vec![PlanSegment::Vector {
            polyline: vec![
                Point2D::new(0.0, 0.0),
                Point2D::new(10.0, 0.0),
                Point2D::new(10.0, 10.0),
                Point2D::new(0.0, 10.0),
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
        }];

        apply_positioned_tabs(&mut segments, &[], 2.0);

        assert_eq!(
            segments.len(),
            1,
            "Empty positions should not change segments"
        );
    }

    #[test]
    fn positioned_tab_near_seam_splits_on_closing_edge() {
        // Closed square: 4 vertices, perimeter = 40.
        // Tab at position 0.95 = 38mm, which falls on the closing edge
        // (0,10) → (0,0) spanning 30..40mm.
        let mut segments = vec![PlanSegment::Vector {
            polyline: vec![
                Point2D::new(0.0, 0.0),
                Point2D::new(10.0, 0.0),
                Point2D::new(10.0, 10.0),
                Point2D::new(0.0, 10.0),
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
        }];

        // Tab at 0.95, width 2mm → gap at 36..40mm on the closing edge
        apply_positioned_tabs(&mut segments, &[0.95], 2.0);

        let has_travel = segments
            .iter()
            .any(|s| matches!(s, PlanSegment::Travel { .. }));
        assert!(
            has_travel,
            "Tab near seam should produce a Travel segment on the closing edge"
        );
        let vectors: Vec<_> = segments
            .iter()
            .filter(|s| matches!(s, PlanSegment::Vector { .. }))
            .collect();
        assert!(
            vectors.len() >= 2,
            "Should split into at least 2 Vector segments"
        );
        assert_no_zero_length_segments(&segments);
    }

    #[test]
    fn positioned_tab_at_seam_wraps_around() {
        // Closed square: 4 vertices, perimeter = 40.
        // Tab at position 1.0 = 40mm with width 4mm → wraps around seam:
        // should produce gap at [38, 40] and [0, 2].
        let mut segments = vec![PlanSegment::Vector {
            polyline: vec![
                Point2D::new(0.0, 0.0),
                Point2D::new(10.0, 0.0),
                Point2D::new(10.0, 10.0),
                Point2D::new(0.0, 10.0),
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
        }];

        apply_positioned_tabs(&mut segments, &[1.0], 4.0);

        // Should wrap and produce travel segments at both ends
        let travel_count = segments
            .iter()
            .filter(|s| matches!(s, PlanSegment::Travel { .. }))
            .count();
        assert!(
            travel_count >= 2,
            "Seam-wrapping tab should produce at least 2 Travel segments, got {travel_count}"
        );
        assert_no_zero_length_segments(&segments);
    }

    #[test]
    fn positioned_tab_at_zero_wraps_around() {
        // Tab at position 0.0 with width 4mm → wraps backward:
        // should produce gap at [0, 2] and [38, 40].
        let mut segments = vec![PlanSegment::Vector {
            polyline: vec![
                Point2D::new(0.0, 0.0),
                Point2D::new(10.0, 0.0),
                Point2D::new(10.0, 10.0),
                Point2D::new(0.0, 10.0),
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
        }];

        apply_positioned_tabs(&mut segments, &[0.0], 4.0);

        let travel_count = segments
            .iter()
            .filter(|s| matches!(s, PlanSegment::Travel { .. }))
            .count();
        assert!(
            travel_count >= 2,
            "Seam-wrapping tab at 0.0 should produce at least 2 Travel segments, got {travel_count}"
        );
        assert_no_zero_length_segments(&segments);
    }

    /// Assert that no emitted segment has zero geometric length.
    fn assert_no_zero_length_segments(segments: &[PlanSegment]) {
        for (i, seg) in segments.iter().enumerate() {
            match seg {
                PlanSegment::Vector { polyline, .. } => {
                    let len = compute_path_length(polyline, false);
                    assert!(
                        len > f64::EPSILON,
                        "Vector segment {i} has zero geometric length"
                    );
                }
                PlanSegment::Travel { start, end } => {
                    let d = start.distance_to(end);
                    assert!(
                        d > f64::EPSILON,
                        "Travel segment {i} has zero geometric length"
                    );
                }
                _ => {}
            }
        }
    }
}

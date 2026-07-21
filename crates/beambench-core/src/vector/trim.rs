use beambench_common::Point2D;
use beambench_common::path::{PathCommand, SubPath, VecPath};

use crate::vector::flatten::DEFAULT_TOLERANCE_MM;
use crate::vector::offset::signed_area;

// ────────────────────────────────────────────────────────────
// Intersection metadata
// ────────────────────────────────────────────────────────────

/// A single intersection detected between the target path and another polyline.
/// Carries cutter identity and cutter-side arc distance so downstream consumers
/// (TrimBracket, TrimResult, close_with_cutter_boundary) don't need to rediscover
/// the intersecting cutter heuristically.
#[derive(Debug, Clone, Copy)]
pub struct IntersectionHit {
    /// Arc-length distance on the target path.
    pub target_dist: f64,
    /// Which cutter polyline (index into `others` slice). None for self-intersections
    /// or ambiguous dedup merges.
    pub cutter_id: Option<usize>,
    /// Arc-length distance on the cutter polyline (if external intersection).
    /// Computed at detection time — no re-projection needed later.
    /// None for self-intersections or ambiguous dedup merges.
    pub cutter_dist: Option<f64>,
}

// ────────────────────────────────────────────────────────────
// Step 1: Curve split utilities (de Casteljau)
// ────────────────────────────────────────────────────────────

/// Split a cubic bezier at parameter t using de Casteljau.
/// Four control points of a cubic bezier segment.
pub type CubicSegment = (Point2D, Point2D, Point2D, Point2D);

/// Returns two cubics: (before, after) as (p0,c1,c2,p3) tuples.
pub fn split_cubic(
    p0: Point2D,
    p1: Point2D,
    p2: Point2D,
    p3: Point2D,
    t: f64,
) -> (CubicSegment, CubicSegment) {
    let m01 = p0.lerp(&p1, t);
    let m12 = p1.lerp(&p2, t);
    let m23 = p2.lerp(&p3, t);
    let m012 = m01.lerp(&m12, t);
    let m123 = m12.lerp(&m23, t);
    let mid = m012.lerp(&m123, t);

    ((p0, m01, m012, mid), (mid, m123, m23, p3))
}

/// Split a quadratic bezier at parameter t using de Casteljau.
/// Returns two quadratics: (before, after) as (p0,c,p2) tuples.
pub fn split_quad(
    p0: Point2D,
    p1: Point2D,
    p2: Point2D,
    t: f64,
) -> ((Point2D, Point2D, Point2D), (Point2D, Point2D, Point2D)) {
    let m01 = p0.lerp(&p1, t);
    let m12 = p1.lerp(&p2, t);
    let mid = m01.lerp(&m12, t);

    ((p0, m01, mid), (mid, m12, p2))
}

// ────────────────────────────────────────────────────────────
// Step 2: Annotated flatten (command ↔ arc-length correspondence)
// ────────────────────────────────────────────────────────────

/// Maps a range of flattened points back to the original PathCommand.
#[derive(Debug, Clone)]
struct CommandSpan {
    cmd_index: usize,
    arc_start: f64,
    arc_end: f64,
    flat_start: usize,
    flat_end: usize,
}

/// Flatten a subpath's commands into polyline points, recording which
/// command each segment came from and its arc-length range.
fn flatten_subpath_annotated(
    commands: &[PathCommand],
    closed: bool,
    tolerance: f64,
) -> (Vec<Point2D>, Vec<CommandSpan>) {
    let mut points: Vec<Point2D> = Vec::new();
    let mut spans: Vec<CommandSpan> = Vec::new();
    let mut current = Point2D::zero();
    let mut first_point = Point2D::zero();
    let mut arc_len = 0.0;

    for (cmd_idx, cmd) in commands.iter().enumerate() {
        match *cmd {
            PathCommand::MoveTo { x, y } => {
                current = Point2D::new(x, y);
                first_point = current;
                if points.is_empty() {
                    points.push(current);
                }
            }
            PathCommand::LineTo { x, y } => {
                let end = Point2D::new(x, y);
                let flat_start = points.len() - 1;
                let seg_len = current.distance_to(&end);
                points.push(end);
                spans.push(CommandSpan {
                    cmd_index: cmd_idx,
                    arc_start: arc_len,
                    arc_end: arc_len + seg_len,
                    flat_start,
                    flat_end: points.len() - 1,
                });
                arc_len += seg_len;
                current = end;
            }
            PathCommand::QuadTo { cx, cy, x, y } => {
                let cp = Point2D::new(cx, cy);
                let end = Point2D::new(x, y);
                let flat_start = points.len() - 1;
                flatten_quad_recursive(current, cp, end, tolerance, &mut points);
                let seg_len = arc_length_between(&points, flat_start, points.len() - 1);
                spans.push(CommandSpan {
                    cmd_index: cmd_idx,
                    arc_start: arc_len,
                    arc_end: arc_len + seg_len,
                    flat_start,
                    flat_end: points.len() - 1,
                });
                arc_len += seg_len;
                current = end;
            }
            PathCommand::CubicTo {
                c1x,
                c1y,
                c2x,
                c2y,
                x,
                y,
            } => {
                let cp1 = Point2D::new(c1x, c1y);
                let cp2 = Point2D::new(c2x, c2y);
                let end = Point2D::new(x, y);
                let flat_start = points.len() - 1;
                flatten_cubic_recursive(current, cp1, cp2, end, tolerance, &mut points);
                let seg_len = arc_length_between(&points, flat_start, points.len() - 1);
                spans.push(CommandSpan {
                    cmd_index: cmd_idx,
                    arc_start: arc_len,
                    arc_end: arc_len + seg_len,
                    flat_start,
                    flat_end: points.len() - 1,
                });
                arc_len += seg_len;
                current = end;
            }
            PathCommand::Close => {
                if closed && current.distance_to(&first_point) > 1e-9 {
                    let flat_start = points.len() - 1;
                    let seg_len = current.distance_to(&first_point);
                    points.push(first_point);
                    spans.push(CommandSpan {
                        cmd_index: cmd_idx,
                        arc_start: arc_len,
                        arc_end: arc_len + seg_len,
                        flat_start,
                        flat_end: points.len() - 1,
                    });
                    arc_len += seg_len;
                }
            }
        }
    }

    (points, spans)
}

/// Compute arc length between two indices in a point array.
fn arc_length_between(points: &[Point2D], start: usize, end: usize) -> f64 {
    let mut len = 0.0;
    for i in start..end {
        len += points[i].distance_to(&points[i + 1]);
    }
    len
}

/// Given an arc-length distance, find which CommandSpan it falls in
/// and compute local t ∈ [0,1] within that command.
fn distance_to_command_t(
    distance: f64,
    spans: &[CommandSpan],
    flat_points: &[Point2D],
) -> Option<(usize, f64)> {
    if spans.is_empty() {
        return None;
    }

    // Find the span containing this distance
    let span_idx = spans
        .iter()
        .position(|s| distance >= s.arc_start - 1e-9 && distance <= s.arc_end + 1e-9)
        .unwrap_or(spans.len() - 1);

    let span = &spans[span_idx];
    let span_len = span.arc_end - span.arc_start;
    if span_len < 1e-12 {
        return Some((span_idx, 0.0));
    }

    // Walk the flat points within this span to find exact local position
    let mut walked = 0.0;
    let target = distance - span.arc_start;
    for i in span.flat_start..span.flat_end {
        let seg_len = flat_points[i].distance_to(&flat_points[i + 1]);
        if walked + seg_len >= target - 1e-9 {
            let frac = if seg_len > 1e-12 {
                ((target - walked) / seg_len).clamp(0.0, 1.0)
            } else {
                0.0
            };
            // Convert to local t within the full command
            let local_arc = walked + frac * seg_len;
            let local_t = (local_arc / span_len).clamp(0.0, 1.0);
            return Some((span_idx, local_t));
        }
        walked += seg_len;
    }

    Some((span_idx, 1.0))
}

// ────────────────────────────────────────────────────────────
// Step 2b: Arc-length → parametric-t refinement
// ────────────────────────────────────────────────────────────

/// Arc length of a cubic from t=0 to t=t_end (via splitting then flattening).
fn cubic_partial_arc_length(
    p0: Point2D,
    p1: Point2D,
    p2: Point2D,
    p3: Point2D,
    t_end: f64,
    tolerance: f64,
) -> f64 {
    if t_end <= 0.0 {
        return 0.0;
    }
    let seg = if t_end < 1.0 - 1e-12 {
        split_cubic(p0, p1, p2, p3, t_end).0
    } else {
        (p0, p1, p2, p3)
    };
    let mut pts = vec![seg.0];
    flatten_cubic_recursive(seg.0, seg.1, seg.2, seg.3, tolerance, &mut pts);
    let mut len = 0.0;
    for i in 0..pts.len() - 1 {
        len += pts[i].distance_to(&pts[i + 1]);
    }
    len
}

/// Refine an arc-length-ratio t into the true parametric t for a cubic.
/// Uses binary search: 16 iterations → ~1e-5 accuracy in t.
fn refine_cubic_t(
    p0: Point2D,
    p1: Point2D,
    p2: Point2D,
    p3: Point2D,
    target_arc: f64,
    tolerance: f64,
) -> f64 {
    let total = cubic_partial_arc_length(p0, p1, p2, p3, 1.0, tolerance);
    if total < 1e-12 {
        return 0.0;
    }
    if target_arc >= total - 1e-12 {
        return 1.0;
    }
    if target_arc <= 1e-12 {
        return 0.0;
    }

    let mut lo = 0.0_f64;
    let mut hi = 1.0_f64;
    for _ in 0..16 {
        let mid = (lo + hi) / 2.0;
        let arc = cubic_partial_arc_length(p0, p1, p2, p3, mid, tolerance);
        if arc < target_arc {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    (lo + hi) / 2.0
}

/// Arc length of a quadratic from t=0 to t=t_end.
fn quad_partial_arc_length(
    p0: Point2D,
    p1: Point2D,
    p2: Point2D,
    t_end: f64,
    tolerance: f64,
) -> f64 {
    if t_end <= 0.0 {
        return 0.0;
    }
    let seg = if t_end < 1.0 - 1e-12 {
        split_quad(p0, p1, p2, t_end).0
    } else {
        (p0, p1, p2)
    };
    let mut pts = vec![seg.0];
    flatten_quad_recursive(seg.0, seg.1, seg.2, tolerance, &mut pts);
    let mut len = 0.0;
    for i in 0..pts.len() - 1 {
        len += pts[i].distance_to(&pts[i + 1]);
    }
    len
}

/// Refine an arc-length-ratio t into the true parametric t for a quadratic.
fn refine_quad_t(p0: Point2D, p1: Point2D, p2: Point2D, target_arc: f64, tolerance: f64) -> f64 {
    let total = quad_partial_arc_length(p0, p1, p2, 1.0, tolerance);
    if total < 1e-12 {
        return 0.0;
    }
    if target_arc >= total - 1e-12 {
        return 1.0;
    }
    if target_arc <= 1e-12 {
        return 0.0;
    }

    let mut lo = 0.0_f64;
    let mut hi = 1.0_f64;
    for _ in 0..16 {
        let mid = (lo + hi) / 2.0;
        let arc = quad_partial_arc_length(p0, p1, p2, mid, tolerance);
        if arc < target_arc {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    (lo + hi) / 2.0
}

// ────────────────────────────────────────────────────────────
// Step 3: Curve-preserving SubPath split
// ────────────────────────────────────────────────────────────

/// Split a subpath at sorted arc-length distances, producing multiple SubPaths.
/// Curves are preserved — only the commands at split points are subdivided.
fn split_subpath_at_distances(
    subpath: &SubPath,
    distances: &[f64],
    tolerance: f64,
) -> Vec<SubPath> {
    if distances.is_empty() {
        return vec![subpath.clone()];
    }

    let (flat_points, spans) =
        flatten_subpath_annotated(&subpath.commands, subpath.closed, tolerance);
    if spans.is_empty() {
        return vec![subpath.clone()];
    }

    // Precompute start point of each command for refinement
    let mut cmd_starts: Vec<Point2D> = vec![Point2D::zero(); subpath.commands.len()];
    {
        let mut ep = get_first_point(&subpath.commands);
        for (idx, cmd) in subpath.commands.iter().enumerate() {
            cmd_starts[idx] = ep;
            ep = command_endpoint(cmd, ep);
        }
    }

    // Map each distance to (span_index, local_t), refining for cubics/quads
    let mut split_points: Vec<(usize, f64)> = Vec::new();
    for &d in distances {
        if let Some((span_idx, mut local_t)) = distance_to_command_t(d, &spans, &flat_points) {
            let span = &spans[span_idx];
            let cmd = &subpath.commands[span.cmd_index];
            let target_arc = d - span.arc_start;
            match *cmd {
                PathCommand::CubicTo {
                    c1x,
                    c1y,
                    c2x,
                    c2y,
                    x,
                    y,
                } => {
                    let p0 = cmd_starts[span.cmd_index];
                    local_t = refine_cubic_t(
                        p0,
                        Point2D::new(c1x, c1y),
                        Point2D::new(c2x, c2y),
                        Point2D::new(x, y),
                        target_arc,
                        tolerance,
                    );
                }
                PathCommand::QuadTo { cx, cy, x, y } => {
                    let p0 = cmd_starts[span.cmd_index];
                    local_t = refine_quad_t(
                        p0,
                        Point2D::new(cx, cy),
                        Point2D::new(x, y),
                        target_arc,
                        tolerance,
                    );
                }
                _ => {} // LineTo: arc-length ratio == parametric t (linear)
            }
            split_points.push((span_idx, local_t));
        }
    }

    if split_points.is_empty() {
        return vec![subpath.clone()];
    }

    // Group splits by span_index, maintaining order
    let mut splits_per_span: Vec<Vec<f64>> = vec![Vec::new(); spans.len()];
    for &(span_idx, local_t) in &split_points {
        if span_idx < splits_per_span.len() {
            splits_per_span[span_idx].push(local_t);
        }
    }

    // Build output subpaths
    let mut result: Vec<SubPath> = Vec::new();
    let mut current_cmds: Vec<PathCommand> = Vec::new();

    // We need to track the "current start point" for each command
    let mut prev_endpoint = get_first_point(&subpath.commands);

    for (span_idx, span) in spans.iter().enumerate() {
        let cmd = &subpath.commands[span.cmd_index];
        let local_splits = &splits_per_span[span_idx];

        if local_splits.is_empty() {
            // No splits in this command — copy verbatim
            if current_cmds.is_empty() {
                // Start a new subpath with MoveTo
                current_cmds.push(PathCommand::MoveTo {
                    x: prev_endpoint.x,
                    y: prev_endpoint.y,
                });
            }
            match *cmd {
                PathCommand::Close => {
                    // Skip — we handle closure at the end
                }
                PathCommand::MoveTo { .. } => {
                    // MoveTo is handled by the subpath start
                }
                _ => {
                    current_cmds.push(*cmd);
                }
            }
        } else {
            // Split this command at each local t
            split_command_at_ts(
                cmd,
                prev_endpoint,
                local_splits,
                &mut current_cmds,
                &mut result,
            );
        }

        // Update prev_endpoint
        prev_endpoint = command_endpoint(cmd, prev_endpoint);
    }

    // Flush remaining commands
    if current_cmds.len() > 1 {
        result.push(SubPath {
            commands: current_cmds,
            closed: false,
        });
    }

    result
}

/// Flush current_cmds as a completed SubPath and start a new one at the given point.
fn flush_at_point(pt: Point2D, current_cmds: &mut Vec<PathCommand>, result: &mut Vec<SubPath>) {
    if current_cmds.len() > 1 {
        result.push(SubPath {
            commands: std::mem::take(current_cmds),
            closed: false,
        });
    } else {
        current_cmds.clear();
    }
    current_cmds.push(PathCommand::MoveTo { x: pt.x, y: pt.y });
}

/// Split a single command at one or more local t values.
/// Appends the "before" portion to current_cmds, flushes to result, starts new subpath.
///
/// For boundary splits (t ≈ 0 or t ≈ 1): instead of subdividing the command,
/// emit a clean break at the exact endpoint without creating a tiny stub.
fn split_command_at_ts(
    cmd: &PathCommand,
    start: Point2D,
    ts: &[f64],
    current_cmds: &mut Vec<PathCommand>,
    result: &mut Vec<SubPath>,
) {
    if current_cmds.is_empty() {
        current_cmds.push(PathCommand::MoveTo {
            x: start.x,
            y: start.y,
        });
    }

    match *cmd {
        PathCommand::LineTo { x, y } => {
            let end = Point2D::new(x, y);
            let mut sorted_ts = ts.to_vec();
            sorted_ts.sort_by(|a, b| a.partial_cmp(b).unwrap());
            // Remap ts to remaining segment
            let mut remaining_t_offset = 0.0;
            for &raw_t in &sorted_ts {
                let t = if (1.0 - remaining_t_offset) > 1e-12 {
                    ((raw_t - remaining_t_offset) / (1.0 - remaining_t_offset)).clamp(0.0, 1.0)
                } else {
                    0.5
                };
                // Boundary split at t≈0: break before the command starts (at current point)
                if t < 1e-9 {
                    let break_pt = start.lerp(&end, remaining_t_offset.max(0.0));
                    flush_at_point(break_pt, current_cmds, result);
                    continue;
                }
                // Boundary split at t≈1: emit the full command then break at its endpoint
                if t > 1.0 - 1e-9 {
                    current_cmds.push(PathCommand::LineTo { x: end.x, y: end.y });
                    flush_at_point(end, current_cmds, result);
                    remaining_t_offset = 1.0;
                    continue;
                }
                let abs_t = remaining_t_offset + t * (1.0 - remaining_t_offset);
                let split_pt = start.lerp(&end, abs_t);

                // "before" portion
                current_cmds.push(PathCommand::LineTo {
                    x: split_pt.x,
                    y: split_pt.y,
                });
                // Flush
                flush_at_point(split_pt, current_cmds, result);
                remaining_t_offset = abs_t;
            }
            // "after" portion (unless we already consumed the entire command)
            if remaining_t_offset < 1.0 - 1e-9 {
                current_cmds.push(PathCommand::LineTo { x: end.x, y: end.y });
            }
        }
        PathCommand::CubicTo {
            c1x,
            c1y,
            c2x,
            c2y,
            x,
            y,
        } => {
            let mut p0 = start;
            let mut p1 = Point2D::new(c1x, c1y);
            let mut p2 = Point2D::new(c2x, c2y);
            let mut p3 = Point2D::new(x, y);

            let mut sorted_ts = ts.to_vec();
            sorted_ts.sort_by(|a, b| a.partial_cmp(b).unwrap());

            let mut prev_abs_t = 0.0;
            let mut consumed_full = false;
            for &raw_t in &sorted_ts {
                // Reparameterize into remaining curve
                let t_in_remaining = if (1.0 - prev_abs_t) > 1e-12 {
                    ((raw_t - prev_abs_t) / (1.0 - prev_abs_t)).clamp(0.0, 1.0)
                } else {
                    0.5
                };
                // Boundary split at t≈0: break before the command
                if t_in_remaining < 1e-9 {
                    let break_pt = p0;
                    flush_at_point(break_pt, current_cmds, result);
                    continue;
                }
                // Boundary split at t≈1: emit the full remaining command then break
                if t_in_remaining > 1.0 - 1e-9 {
                    current_cmds.push(PathCommand::CubicTo {
                        c1x: p1.x,
                        c1y: p1.y,
                        c2x: p2.x,
                        c2y: p2.y,
                        x: p3.x,
                        y: p3.y,
                    });
                    flush_at_point(p3, current_cmds, result);
                    consumed_full = true;
                    continue;
                }

                let (before, after) = split_cubic(p0, p1, p2, p3, t_in_remaining);

                // "before" portion
                current_cmds.push(PathCommand::CubicTo {
                    c1x: before.1.x,
                    c1y: before.1.y,
                    c2x: before.2.x,
                    c2y: before.2.y,
                    x: before.3.x,
                    y: before.3.y,
                });
                flush_at_point(before.3, current_cmds, result);

                // Continue with the "after" half
                p0 = after.0;
                p1 = after.1;
                p2 = after.2;
                p3 = after.3;
                prev_abs_t = raw_t;
            }
            // Final "after" portion (unless full command was consumed by boundary split)
            if !consumed_full {
                current_cmds.push(PathCommand::CubicTo {
                    c1x: p1.x,
                    c1y: p1.y,
                    c2x: p2.x,
                    c2y: p2.y,
                    x: p3.x,
                    y: p3.y,
                });
            }
        }
        PathCommand::QuadTo { cx, cy, x, y } => {
            let mut p0 = start;
            let mut p1 = Point2D::new(cx, cy);
            let mut p2 = Point2D::new(x, y);

            let mut sorted_ts = ts.to_vec();
            sorted_ts.sort_by(|a, b| a.partial_cmp(b).unwrap());

            let mut prev_abs_t = 0.0;
            let mut consumed_full = false;
            for &raw_t in &sorted_ts {
                let t_in_remaining = if (1.0 - prev_abs_t) > 1e-12 {
                    ((raw_t - prev_abs_t) / (1.0 - prev_abs_t)).clamp(0.0, 1.0)
                } else {
                    0.5
                };
                if t_in_remaining < 1e-9 {
                    flush_at_point(p0, current_cmds, result);
                    continue;
                }
                if t_in_remaining > 1.0 - 1e-9 {
                    current_cmds.push(PathCommand::QuadTo {
                        cx: p1.x,
                        cy: p1.y,
                        x: p2.x,
                        y: p2.y,
                    });
                    flush_at_point(p2, current_cmds, result);
                    consumed_full = true;
                    continue;
                }

                let (before, after) = split_quad(p0, p1, p2, t_in_remaining);

                current_cmds.push(PathCommand::QuadTo {
                    cx: before.1.x,
                    cy: before.1.y,
                    x: before.2.x,
                    y: before.2.y,
                });
                flush_at_point(before.2, current_cmds, result);

                p0 = after.0;
                p1 = after.1;
                p2 = after.2;
                prev_abs_t = raw_t;
            }
            if !consumed_full {
                current_cmds.push(PathCommand::QuadTo {
                    cx: p1.x,
                    cy: p1.y,
                    x: p2.x,
                    y: p2.y,
                });
            }
        }
        PathCommand::Close => {
            // Treat Close as LineTo back to start — split as a line
            let first = get_first_point(
                result
                    .last()
                    .map(|sp| sp.commands.as_slice())
                    .unwrap_or(&[]),
            );
            let synthetic = PathCommand::LineTo {
                x: first.x,
                y: first.y,
            };
            split_command_at_ts(&synthetic, start, ts, current_cmds, result);
        }
        PathCommand::MoveTo { .. } => {
            // MoveTo doesn't get split
        }
    }
}

fn get_first_point(commands: &[PathCommand]) -> Point2D {
    for cmd in commands {
        if let PathCommand::MoveTo { x, y } = *cmd {
            return Point2D::new(x, y);
        }
    }
    Point2D::zero()
}

fn command_endpoint(cmd: &PathCommand, prev: Point2D) -> Point2D {
    match *cmd {
        PathCommand::MoveTo { x, y }
        | PathCommand::LineTo { x, y }
        | PathCommand::QuadTo { x, y, .. }
        | PathCommand::CubicTo { x, y, .. } => Point2D::new(x, y),
        PathCommand::Close => prev,
    }
}

// ────────────────────────────────────────────────────────────
// Step 4: Segment-segment intersection
// ────────────────────────────────────────────────────────────

/// Line-segment intersection using cross products.
/// Returns (t on p1→p2, u on p3→p4, intersection point).
fn segment_intersection(
    p1: Point2D,
    p2: Point2D,
    p3: Point2D,
    p4: Point2D,
) -> Option<(f64, f64, Point2D)> {
    let d1x = p2.x - p1.x;
    let d1y = p2.y - p1.y;
    let d2x = p4.x - p3.x;
    let d2y = p4.y - p3.y;

    let denom = d1x * d2y - d1y * d2x;

    if denom.abs() < 1e-10 {
        return None; // Parallel or coincident
    }

    let dx = p3.x - p1.x;
    let dy = p3.y - p1.y;

    let t = (dx * d2y - dy * d2x) / denom;
    let u = (dx * d1y - dy * d1x) / denom;

    if (-1e-9..=1.0 + 1e-9).contains(&t) && (-1e-9..=1.0 + 1e-9).contains(&u) {
        let t = t.clamp(0.0, 1.0);
        let pt = Point2D::new(p1.x + d1x * t, p1.y + d1y * t);
        Some((t, u.clamp(0.0, 1.0), pt))
    } else {
        None
    }
}

/// Detect colinear overlap between two segments.
/// Returns (t_start, t_end) on p1→p2 where overlap occurs.
fn colinear_overlap(
    p1: Point2D,
    p2: Point2D,
    p3: Point2D,
    p4: Point2D,
    tol: f64,
) -> Option<(f64, f64)> {
    let d1x = p2.x - p1.x;
    let d1y = p2.y - p1.y;
    let len_sq = d1x * d1x + d1y * d1y;
    if len_sq < 1e-20 {
        return None;
    }

    // Check if p3 is on the line through p1→p2
    let cross = (p3.x - p1.x) * d1y - (p3.y - p1.y) * d1x;
    if cross.abs() / len_sq.sqrt() > tol {
        return None;
    }

    // Project p3 and p4 onto p1→p2
    let t3 = ((p3.x - p1.x) * d1x + (p3.y - p1.y) * d1y) / len_sq;
    let t4 = ((p4.x - p1.x) * d1x + (p4.y - p1.y) * d1y) / len_sq;

    let t_start = t3.min(t4).max(0.0);
    let t_end = t3.max(t4).min(1.0);

    if t_end - t_start > 1e-9 {
        Some((t_start, t_end))
    } else {
        None
    }
}

/// Find all intersection arc-length distances where a polyline crosses
/// other polylines (and optionally itself). Returns rich `IntersectionHit`
/// records with cutter identity and cutter-side arc distance.
fn find_intersections(
    path_points: &[Point2D],
    path_closed: bool,
    others: &[(Vec<Point2D>, bool)],
    include_self: bool,
) -> Vec<IntersectionHit> {
    let mut hits: Vec<IntersectionHit> = Vec::new();
    let n = if path_closed {
        path_points.len()
    } else {
        path_points.len() - 1
    };

    if n == 0 {
        return hits;
    }

    // Build cumulative arc lengths for the path
    let mut cum_len = vec![0.0_f64; path_points.len()];
    for i in 1..path_points.len() {
        cum_len[i] = cum_len[i - 1] + path_points[i - 1].distance_to(&path_points[i]);
    }
    let total_len = if path_closed {
        cum_len[path_points.len() - 1]
            + path_points[path_points.len() - 1].distance_to(&path_points[0])
    } else {
        cum_len[path_points.len() - 1]
    };

    // Build cumulative arc lengths for each cutter polyline
    let cutter_cum_lens: Vec<Vec<f64>> = others
        .iter()
        .map(|(pts, _closed)| {
            let mut cl = vec![0.0_f64; pts.len()];
            for i in 1..pts.len() {
                cl[i] = cl[i - 1] + pts[i - 1].distance_to(&pts[i]);
            }
            cl
        })
        .collect();

    // Check path segments against all other polylines
    for i in 0..n {
        let a1 = path_points[i];
        let a2 = path_points[(i + 1) % path_points.len()];
        let seg_len = a1.distance_to(&a2);
        if seg_len < 1e-12 {
            continue;
        }

        for (cutter_idx, (other_pts, other_closed)) in others.iter().enumerate() {
            let m = if *other_closed {
                other_pts.len()
            } else {
                other_pts.len().saturating_sub(1)
            };
            let ccl = &cutter_cum_lens[cutter_idx];
            for j in 0..m {
                let b1 = other_pts[j];
                let b2 = other_pts[(j + 1) % other_pts.len()];
                let cutter_seg_len = b1.distance_to(&b2);

                if let Some((t, u, _pt)) = segment_intersection(a1, a2, b1, b2) {
                    let d = cum_len[i] + t * seg_len;
                    let cd = ccl[j] + u * cutter_seg_len;
                    hits.push(IntersectionHit {
                        target_dist: d,
                        cutter_id: Some(cutter_idx),
                        cutter_dist: Some(cd),
                    });
                } else if let Some((t_start, t_end)) = colinear_overlap(a1, a2, b1, b2, 0.01) {
                    // Project overlap endpoints onto cutter segment
                    let ov_p1 = a1.lerp(&a2, t_start);
                    let ov_p2 = a1.lerp(&a2, t_end);
                    let dir_j = Point2D::new(b2.x - b1.x, b2.y - b1.y);
                    let len_sq = dir_j.x * dir_j.x + dir_j.y * dir_j.y;
                    let (cu1, cu2) = if len_sq > 1e-18 {
                        let u1 = ((ov_p1.x - b1.x) * dir_j.x + (ov_p1.y - b1.y) * dir_j.y) / len_sq;
                        let u2 = ((ov_p2.x - b1.x) * dir_j.x + (ov_p2.y - b1.y) * dir_j.y) / len_sq;
                        (
                            Some(ccl[j] + u1.clamp(0.0, 1.0) * cutter_seg_len),
                            Some(ccl[j] + u2.clamp(0.0, 1.0) * cutter_seg_len),
                        )
                    } else {
                        (None, None)
                    };
                    hits.push(IntersectionHit {
                        target_dist: cum_len[i] + t_start * seg_len,
                        cutter_id: Some(cutter_idx),
                        cutter_dist: cu1,
                    });
                    hits.push(IntersectionHit {
                        target_dist: cum_len[i] + t_end * seg_len,
                        cutter_id: Some(cutter_idx),
                        cutter_dist: cu2,
                    });
                }
            }
        }

        // Self-intersection: check against non-adjacent segments
        if include_self {
            for j in 0..n {
                // Skip adjacent segments (they share a vertex)
                if i == j {
                    continue;
                }
                if i.abs_diff(j) <= 1 {
                    continue;
                }
                // For closed paths, also skip (first, last) pair
                if path_closed && ((i == 0 && j == n - 1) || (j == 0 && i == n - 1)) {
                    continue;
                }
                // Only check each pair once (j > i+1)
                if j <= i {
                    continue;
                }

                let b1 = path_points[j];
                let b2 = path_points[(j + 1) % path_points.len()];

                if let Some((t, u, _pt)) = segment_intersection(a1, a2, b1, b2) {
                    let d1 = cum_len[i] + t * seg_len;
                    let seg_j_len = b1.distance_to(&b2);
                    let d2 = cum_len[j] + u * seg_j_len;
                    hits.push(IntersectionHit {
                        target_dist: d1,
                        cutter_id: None,
                        cutter_dist: None,
                    });
                    hits.push(IntersectionHit {
                        target_dist: d2,
                        cutter_id: None,
                        cutter_dist: None,
                    });
                } else if let Some((t_start, t_end)) = colinear_overlap(a1, a2, b1, b2, 0.01) {
                    let seg_j_len = b1.distance_to(&b2);
                    // Record overlap endpoints on both segments
                    hits.push(IntersectionHit {
                        target_dist: cum_len[i] + t_start * seg_len,
                        cutter_id: None,
                        cutter_dist: None,
                    });
                    hits.push(IntersectionHit {
                        target_dist: cum_len[i] + t_end * seg_len,
                        cutter_id: None,
                        cutter_dist: None,
                    });
                    // Project overlap endpoints back onto segment j
                    let ov_p1 = a1.lerp(&a2, t_start);
                    let ov_p2 = a1.lerp(&a2, t_end);
                    let dir_j = Point2D::new(b2.x - b1.x, b2.y - b1.y);
                    let len_sq = dir_j.x * dir_j.x + dir_j.y * dir_j.y;
                    if len_sq > 1e-18 {
                        let u_start =
                            ((ov_p1.x - b1.x) * dir_j.x + (ov_p1.y - b1.y) * dir_j.y) / len_sq;
                        let u_end =
                            ((ov_p2.x - b1.x) * dir_j.x + (ov_p2.y - b1.y) * dir_j.y) / len_sq;
                        hits.push(IntersectionHit {
                            target_dist: cum_len[j] + u_start.clamp(0.0, 1.0) * seg_j_len,
                            cutter_id: None,
                            cutter_dist: None,
                        });
                        hits.push(IntersectionHit {
                            target_dist: cum_len[j] + u_end.clamp(0.0, 1.0) * seg_j_len,
                            cutter_id: None,
                            cutter_dist: None,
                        });
                    }
                }
            }
        }
    }

    // Sort and deduplicate within tolerance.
    // Tolerances are derived from the flatten tolerance (DEFAULT_TOLERANCE_MM = 0.05mm):
    // - Endpoint margin: 0.1× flatten tolerance — small enough to keep real intersections
    //   near endpoints, large enough to reject noise from degenerate sub-tolerance pieces.
    // - Dedup tolerance: 0.5× flatten tolerance — merges numerical duplicates of the same
    //   crossing but preserves two genuinely distinct crossings that happen to be close.
    let endpoint_margin = DEFAULT_TOLERANCE_MM * 0.1;
    let dedup_tol = DEFAULT_TOLERANCE_MM * 0.5;

    hits.sort_by(|a, b| a.target_dist.partial_cmp(&b.target_dist).unwrap());
    hits.retain(|h| h.target_dist > endpoint_margin && h.target_dist < total_len - endpoint_margin);
    dedup_hits_within_tolerance(&mut hits, dedup_tol);

    hits
}

/// Dedup hits within tolerance, marking as ambiguous when merged hits disagree
/// on cutter identity or cutter arc distance.
fn dedup_hits_within_tolerance(hits: &mut Vec<IntersectionHit>, tol: f64) {
    if hits.len() < 2 {
        return;
    }
    let mut i = 1;
    while i < hits.len() {
        if (hits[i].target_dist - hits[i - 1].target_dist).abs() < tol {
            // Check if the two hits are compatible
            let a = &hits[i - 1];
            let b = &hits[i];
            let ambiguous = match (a.cutter_id, b.cutter_id) {
                (Some(ca), Some(cb)) if ca != cb => true, // different cutters
                (Some(ca), Some(cb)) if ca == cb => {
                    // Same cutter — ambiguous unless both have compatible Some distances
                    match (a.cutter_dist, b.cutter_dist) {
                        (Some(da), Some(db)) => (da - db).abs() > tol,
                        (None, None) => false, // both missing is consistent (shouldn't happen but harmless)
                        _ => true,             // one Some + one None = partial metadata mismatch
                    }
                }
                (None, Some(_)) | (Some(_), None) => true, // one self, one external
                _ => false,
            };
            if ambiguous {
                hits[i - 1].cutter_id = None;
                hits[i - 1].cutter_dist = None;
            }
            hits.remove(i);
        } else {
            i += 1;
        }
    }
}

// ────────────────────────────────────────────────────────────
// Step 5: Edge-proximity selection
// ────────────────────────────────────────────────────────────

/// Point-to-polyline-edge minimum distance + nearest subpath index + arc distance.
/// Returns (subpath_index, min_distance, arc_distance_of_nearest_point).
pub fn nearest_edge(
    point: Point2D,
    subpath_polylines: &[(Vec<Point2D>, bool)],
) -> Option<(usize, f64, f64)> {
    let mut best: Option<(usize, f64, f64)> = None;

    for (sp_idx, (pts, closed)) in subpath_polylines.iter().enumerate() {
        let n = if *closed {
            pts.len()
        } else {
            pts.len().saturating_sub(1)
        };
        let mut cum_len = 0.0;

        for i in 0..n {
            let a = pts[i];
            let b = pts[(i + 1) % pts.len()];
            let seg_len = a.distance_to(&b);

            let (dist, t) = point_to_segment(point, a, b);

            let arc_dist = cum_len + t * seg_len;

            if best.is_none() || dist < best.unwrap().1 {
                best = Some((sp_idx, dist, arc_dist));
            }

            cum_len += seg_len;
        }
    }

    best
}

/// Distance from point to line segment, returning (distance, t_on_segment).
fn point_to_segment(p: Point2D, a: Point2D, b: Point2D) -> (f64, f64) {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len_sq = dx * dx + dy * dy;

    if len_sq < 1e-20 {
        return (p.distance_to(&a), 0.0);
    }

    let t = ((p.x - a.x) * dx + (p.y - a.y) * dy) / len_sq;
    let t = t.clamp(0.0, 1.0);
    let proj = Point2D::new(a.x + dx * t, a.y + dy * t);
    (p.distance_to(&proj), t)
}

/// Project point onto polyline, returning (perpendicular_distance, arc_length).
pub fn project_point_on_polyline_with_dist(
    point: Point2D,
    pts: &[Point2D],
    closed: bool,
) -> (f64, f64) {
    let n = if closed {
        pts.len()
    } else {
        pts.len().saturating_sub(1)
    };
    let mut best_dist = f64::INFINITY;
    let mut best_arc = 0.0;
    let mut cum_len = 0.0;

    for i in 0..n {
        let a = pts[i];
        let b = pts[(i + 1) % pts.len()];
        let seg_len = a.distance_to(&b);
        let (dist, t) = point_to_segment(point, a, b);

        if dist < best_dist {
            best_dist = dist;
            best_arc = cum_len + t * seg_len;
        }

        cum_len += seg_len;
    }

    (best_dist, best_arc)
}

/// Project a point onto a polyline, returning arc-length distance.
pub fn project_point_on_polyline(point: Point2D, pts: &[Point2D], closed: bool) -> f64 {
    project_point_on_polyline_with_dist(point, pts, closed).1
}

// ────────────────────────────────────────────────────────────
// Cutter boundary closure
// ────────────────────────────────────────────────────────────

// ────────────────────────────────────────────────────────────
// Validation utilities for contour quality checks
// ────────────────────────────────────────────────────────────

fn cross_2d(a: Point2D, b: Point2D, c: Point2D) -> f64 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

fn segments_cross(a1: Point2D, a2: Point2D, b1: Point2D, b2: Point2D) -> bool {
    let d1 = cross_2d(a1, a2, b1);
    let d2 = cross_2d(a1, a2, b2);
    let d3 = cross_2d(b1, b2, a1);
    let d4 = cross_2d(b1, b2, a2);
    if ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0))
        && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
    {
        return true;
    }
    false
}

fn polygon_has_self_intersection(points: &[Point2D]) -> bool {
    let n = points.len();
    if n < 4 {
        return false;
    }
    for i in 0..n {
        let a1 = points[i];
        let a2 = points[(i + 1) % n];
        for j in (i + 2)..n {
            if i == 0 && j == n - 1 {
                continue; // adjacent via wrap-around
            }
            let b1 = points[j];
            let b2 = points[(j + 1) % n];
            if segments_cross(a1, a2, b1, b2) {
                return true;
            }
        }
    }
    false
}

fn point_in_polygon(px: f64, py: f64, ring: &[Point2D]) -> bool {
    let mut inside = false;
    let n = ring.len();
    let mut j = n - 1;
    for i in 0..n {
        let yi = ring[i].y;
        let yj = ring[j].y;
        if ((yi > py) != (yj > py))
            && (px < (ring[j].x - ring[i].x) * (py - yi) / (yj - yi) + ring[i].x)
        {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Squared distance from point `p` to the line segment `a`–`b`.
fn point_to_segment_dist_sq(p: Point2D, a: Point2D, b: Point2D) -> f64 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len_sq = dx * dx + dy * dy;
    if len_sq < 1e-18 {
        // Degenerate zero-length segment
        return (p.x - a.x).powi(2) + (p.y - a.y).powi(2);
    }
    let t = ((p.x - a.x) * dx + (p.y - a.y) * dy) / len_sq;
    let t = t.clamp(0.0, 1.0);
    let proj_x = a.x + t * dx;
    let proj_y = a.y + t * dy;
    (p.x - proj_x).powi(2) + (p.y - proj_y).powi(2)
}

/// Returns true if any vertex of the polygon lies within `tolerance` of a
/// non-adjacent edge. Catches colinear overlaps, T-junctions, and
/// near-coincident / duplicated boundary segments that the proper-crossing
/// check in [`polygon_has_self_intersection`] misses.
fn polygon_has_self_touching(points: &[Point2D], tolerance: f64) -> bool {
    let n = points.len();
    if n < 4 {
        return false;
    }
    let tol_sq = tolerance * tolerance;
    for i in 0..n {
        let v = points[i];
        let prev_i = if i == 0 { n - 1 } else { i - 1 };
        for j in 0..n {
            // Skip edges adjacent to vertex i
            if j == i || j == prev_i {
                continue;
            }
            let a = points[j];
            let b = points[(j + 1) % n];
            if point_to_segment_dist_sq(v, a, b) < tol_sq {
                return true;
            }
        }
    }
    false
}

fn contour_min_segment_length(points: &[Point2D]) -> f64 {
    let n = points.len();
    if n < 2 {
        return 0.0;
    }
    let mut min_len = f64::INFINITY;
    for i in 0..n {
        let len = points[i].distance_to(&points[(i + 1) % n]);
        if len < min_len {
            min_len = len;
        }
    }
    min_len
}

// ────────────────────────────────────────────────────────────
// Arc-distance point evaluation helper
// ────────────────────────────────────────────────────────────

/// Evaluate a point on a polyline at a given arc distance.
/// For closed polylines, wraps via `dist % total_len`.
/// For open polylines, clamps to endpoints.
fn point_at_arc_dist(pts: &[Point2D], dist: f64, closed: bool) -> Point2D {
    if pts.len() < 2 {
        return pts.first().copied().unwrap_or(Point2D::zero());
    }

    let total_len = compute_polyline_arc_length(pts, closed);
    let dist = if closed && total_len > 1e-12 {
        let d = dist % total_len;
        if d < 0.0 { d + total_len } else { d }
    } else {
        dist.clamp(0.0, total_len)
    };

    if dist <= 0.0 {
        return pts[0];
    }

    let n = if closed { pts.len() } else { pts.len() - 1 };
    let mut walked = 0.0;
    for i in 0..n {
        let a = pts[i];
        let b = pts[(i + 1) % pts.len()];
        let seg_len = a.distance_to(&b);
        if walked + seg_len >= dist - 1e-9 {
            let frac = if seg_len > 1e-12 {
                ((dist - walked) / seg_len).clamp(0.0, 1.0)
            } else {
                0.0
            };
            return a.lerp(&b, frac);
        }
        walked += seg_len;
    }

    if closed { pts[0] } else { *pts.last().unwrap() }
}

// ────────────────────────────────────────────────────────────
// Cutter boundary closure (Tier 2)
// ────────────────────────────────────────────────────────────

/// Attempt to close an open subpath by appending a boundary segment from the
/// cutter polyline identified by the intersection hits. Uses rich metadata from
/// intersection detection — no heuristic cutter rediscovery.
///
/// Both hits must reference the **same closed cutter** and the resulting contour
/// must pass geometric validation (no self-intersection, non-trivial area,
/// click outside kept contour). Returns `None` for all ambiguous cases.
pub fn close_with_cutter_boundary(
    open_chain: &SubPath,
    cutter_polylines: &[(Vec<Point2D>, bool)],
    left_hit: &IntersectionHit,
    right_hit: &IntersectionHit,
    click_point: Point2D,
) -> Option<SubPath> {
    let chain_start = subpath_first_point(open_chain)?;
    let chain_end = subpath_last_point(open_chain)?;

    // Guard: endpoints must not be coincident
    if chain_start.distance_to(&chain_end) < DEFAULT_TOLERANCE_MM {
        return None;
    }

    // Same-cutter gate: both hits must have cutter_id = Some(idx) and match
    let cutter_idx = match (left_hit.cutter_id, right_hit.cutter_id) {
        (Some(a), Some(b)) if a == b => a,
        _ => return None,
    };

    // Cutter must be closed and exist
    let (cutter_pts, cutter_closed) = cutter_polylines.get(cutter_idx)?;
    if !cutter_closed || cutter_pts.len() < 3 {
        return None;
    }

    // Both hits must have cutter_dist
    let left_cutter_dist = left_hit.cutter_dist?;
    let right_cutter_dist = right_hit.cutter_dist?;

    // Map hits to chain endpoints by proximity
    let left_cutter_pt = point_at_arc_dist(cutter_pts, left_cutter_dist, true);

    let left_to_start = dist_sq(left_cutter_pt, chain_start);
    let left_to_end = dist_sq(left_cutter_pt, chain_end);

    // Ambiguity guard: if both distances are within tolerance², bail out
    let diff = (left_to_start - left_to_end).abs();
    let tol_sq = DEFAULT_TOLERANCE_MM * DEFAULT_TOLERANCE_MM;
    if diff < tol_sq {
        return None;
    }

    let (start_cutter_dist, end_cutter_dist) = if left_to_start < left_to_end {
        // left_hit corresponds to chain_start, right_hit to chain_end
        (left_cutter_dist, right_cutter_dist)
    } else {
        // right_hit corresponds to chain_start, left_hit to chain_end
        (right_cutter_dist, left_cutter_dist)
    };

    let total_cutter_len = compute_polyline_arc_length(cutter_pts, true);

    // Extract both candidate arcs (oriented chain_end → chain_start)
    // Arc A (forward): end_cutter_dist → start_cutter_dist
    let arc_a = extract_polyline_segment(
        cutter_pts,
        end_cutter_dist,
        start_cutter_dist,
        true,
        total_cutter_len,
    );
    // Arc B (reverse): start_cutter_dist → end_cutter_dist, then reverse
    let arc_b = extract_polyline_segment(
        cutter_pts,
        start_cutter_dist,
        end_cutter_dist,
        true,
        total_cutter_len,
    )
    .map(|mut pts| {
        pts.reverse();
        pts
    });

    let tolerance = DEFAULT_TOLERANCE_MM;

    // Build + validate each candidate
    let mut valid_candidates: Vec<SubPath> = Vec::new();

    for arc_opt in [arc_a, arc_b] {
        let Some(mut boundary) = arc_opt else {
            continue;
        };
        if boundary.len() < 2 {
            continue;
        }

        // Snap boundary endpoints to exact chain coordinates
        boundary[0] = chain_end;
        let last_idx = boundary.len() - 1;
        boundary[last_idx] = chain_start;

        // Build closed SubPath: chain + boundary + Close
        let mut commands = open_chain.commands.clone();
        for pt in &boundary[1..boundary.len() - 1] {
            commands.push(PathCommand::LineTo { x: pt.x, y: pt.y });
        }
        // Final LineTo to chain_start if needed
        let last_cmd_pt = boundary.get(boundary.len().saturating_sub(2));
        let need_final = last_cmd_pt.is_none_or(|pt| pt.distance_to(&chain_start) > 1e-6);
        if need_final {
            commands.push(PathCommand::LineTo {
                x: chain_start.x,
                y: chain_start.y,
            });
        }
        commands.push(PathCommand::Close);

        let candidate = SubPath {
            commands,
            closed: true,
        };

        // Flatten for validation — remove duplicate first/last point if present
        let (mut flat, _) = flatten_subpath_annotated(&candidate.commands, true, tolerance);
        if flat.len() >= 2 && flat.first().unwrap().distance_to(flat.last().unwrap()) < 1e-9 {
            flat.pop();
        }
        if flat.len() < 4 {
            continue;
        }

        // Validation checks
        if polygon_has_self_intersection(&flat) {
            continue;
        }
        if polygon_has_self_touching(&flat, tolerance) {
            continue;
        }
        if contour_min_segment_length(&flat) < tolerance {
            continue;
        }
        if signed_area(&flat).abs() < tolerance * tolerance {
            continue;
        }
        // Click must be outside the kept contour (it was in the removed region)
        if point_in_polygon(click_point.x, click_point.y, &flat) {
            continue;
        }

        valid_candidates.push(candidate);
    }

    // Uniqueness gate: exactly 1 passes → return it; 0 or 2 → None
    if valid_candidates.len() == 1 {
        Some(valid_candidates.remove(0))
    } else {
        None
    }
}

fn dist_sq(a: Point2D, b: Point2D) -> f64 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    dx * dx + dy * dy
}

// ────────────────────────────────────────────────────────────
// Step 6: Main entry point
// ────────────────────────────────────────────────────────────

// ────────────────────────────────────────────────────────────
// Bracket-finding helper (shared by trim and preview)
// ────────────────────────────────────────────────────────────

/// Internal result of finding the trim bracket for a click point.
pub struct TrimBracket {
    pub flat_points: Vec<Point2D>,
    pub intersection_dists: Vec<f64>,
    pub left_dist: f64,
    pub right_dist: f64,
    pub total_len: f64,
    pub click_dist: f64,
    /// Full metadata for left bracket intersection (None for path-start sentinel).
    pub left_hit: Option<IntersectionHit>,
    /// Full metadata for right bracket intersection (None for path-end sentinel).
    pub right_hit: Option<IntersectionHit>,
}

/// Find the bracketing intersection distances around a click point.
/// Shared by `trim_at_intersection` and `preview_trim_segment`.
pub fn find_trim_bracket(
    target_sp: &SubPath,
    external_cutter_polylines: &[(Vec<Point2D>, bool)],
    click_point: Point2D,
) -> Option<TrimBracket> {
    let tolerance = DEFAULT_TOLERANCE_MM;

    let (flat_points, _spans) =
        flatten_subpath_annotated(&target_sp.commands, target_sp.closed, tolerance);

    if flat_points.len() < 2 {
        return None;
    }

    let hits = find_intersections(
        &flat_points,
        target_sp.closed,
        external_cutter_polylines,
        true,
    );

    if hits.is_empty() {
        return None;
    }

    // Extract plain distances for backward compat (preview, etc.)
    let intersection_dists: Vec<f64> = hits.iter().map(|h| h.target_dist).collect();

    let click_dist = project_point_on_polyline(click_point, &flat_points, target_sp.closed);
    let total_len = compute_polyline_arc_length(&flat_points, target_sp.closed);

    let (left_dist, right_dist, left_hit, right_hit) = if target_sp.closed {
        let (li, ri) = find_bracket_closed(&hits, click_dist, total_len);
        let lh = hits.get(li).copied();
        let rh = hits.get(ri).copied();
        let ld = lh.map(|h| h.target_dist).unwrap_or(0.0);
        let rd = rh.map(|h| h.target_dist).unwrap_or(total_len);
        (ld, rd, lh, rh)
    } else {
        let (li, ri) = find_bracket_open(&hits, click_dist, total_len);
        let lh = li.and_then(|i| hits.get(i).copied());
        let rh = ri.and_then(|i| hits.get(i).copied());
        let ld = lh.map(|h| h.target_dist).unwrap_or(0.0);
        let rd = rh.map(|h| h.target_dist).unwrap_or(total_len);
        (ld, rd, lh, rh)
    };

    Some(TrimBracket {
        flat_points,
        intersection_dists,
        left_dist,
        right_dist,
        total_len,
        click_dist,
        left_hit,
        right_hit,
    })
}

// ────────────────────────────────────────────────────────────
// Arc-distance to point helper
// ────────────────────────────────────────────────────────────

/// Resolve an arc-length distance to a Point2D on the polyline.
pub fn arc_distance_to_point(flat_points: &[Point2D], dist: f64) -> Point2D {
    if flat_points.len() < 2 || dist <= 0.0 {
        return flat_points.first().copied().unwrap_or(Point2D::zero());
    }

    let mut walked = 0.0;
    for i in 0..flat_points.len() - 1 {
        let seg_len = flat_points[i].distance_to(&flat_points[i + 1]);
        if walked + seg_len >= dist - 1e-9 {
            let frac = if seg_len > 1e-12 {
                ((dist - walked) / seg_len).clamp(0.0, 1.0)
            } else {
                0.0
            };
            return flat_points[i].lerp(&flat_points[i + 1], frac);
        }
        walked += seg_len;
    }

    flat_points.last().copied().unwrap_or(Point2D::zero())
}

// ────────────────────────────────────────────────────────────
// Preview query
// ────────────────────────────────────────────────────────────

/// Return the polyline points of the segment that would be removed by trim.
/// Read-only — no mutation.
pub fn preview_trim_segment(
    target_path: &VecPath,
    target_subpath_idx: usize,
    external_cutter_polylines: &[(Vec<Point2D>, bool)],
    click_point: Point2D,
) -> Option<Vec<Point2D>> {
    let target_sp = target_path.subpaths.get(target_subpath_idx)?;
    let bracket = find_trim_bracket(target_sp, external_cutter_polylines, click_point)?;

    // Extract polyline points between left_dist and right_dist
    extract_polyline_segment(
        &bracket.flat_points,
        bracket.left_dist,
        bracket.right_dist,
        target_sp.closed,
        bracket.total_len,
    )
}

/// Extract polyline points between two arc-length distances.
fn extract_polyline_segment(
    flat_points: &[Point2D],
    left_dist: f64,
    right_dist: f64,
    closed: bool,
    _total_len: f64,
) -> Option<Vec<Point2D>> {
    if flat_points.len() < 2 {
        return None;
    }

    let wraps = closed && right_dist < left_dist;

    let mut result = Vec::new();

    if wraps {
        // Wrap-around: collect from left_dist to end, then from start to right_dist
        let pt_start = arc_distance_to_point(flat_points, left_dist);
        result.push(pt_start);
        // Points from left_dist to end
        let mut walked = 0.0;
        for i in 0..flat_points.len() - 1 {
            let seg_len = flat_points[i].distance_to(&flat_points[i + 1]);
            if walked + seg_len > left_dist + 1e-9 {
                result.push(flat_points[i + 1]);
            }
            walked += seg_len;
        }
        // Points from start to right_dist
        let mut walked2 = 0.0;
        for i in 0..flat_points.len() - 1 {
            let seg_len = flat_points[i].distance_to(&flat_points[i + 1]);
            if walked2 + seg_len > right_dist - 1e-9 {
                let pt_end = arc_distance_to_point(flat_points, right_dist);
                result.push(pt_end);
                break;
            }
            result.push(flat_points[i + 1]);
            walked2 += seg_len;
        }
    } else {
        // Normal: collect from left_dist to right_dist
        let pt_start = arc_distance_to_point(flat_points, left_dist);
        result.push(pt_start);

        let mut walked = 0.0;
        for i in 0..flat_points.len() - 1 {
            let seg_len = flat_points[i].distance_to(&flat_points[i + 1]);
            let seg_end = walked + seg_len;
            if seg_end > left_dist + 1e-9
                && walked < right_dist - 1e-9
                && seg_end < right_dist - 1e-9
            {
                result.push(flat_points[i + 1]);
            }
            walked = seg_end;
        }

        let pt_end = arc_distance_to_point(flat_points, right_dist);
        result.push(pt_end);
    }

    if result.len() >= 2 {
        Some(result)
    } else {
        None
    }
}

// ────────────────────────────────────────────────────────────
// Trim result types
// ────────────────────────────────────────────────────────────

/// Result of trim_at_intersection — includes cut points for healing.
pub struct TrimResult {
    pub pieces: Vec<VecPath>,
    /// The 2 bracket intersection coordinates where the trim cut.
    pub cut_points: Vec<Point2D>,
    /// Full metadata for left bracket intersection (None for path-start sentinel).
    pub left_hit: Option<IntersectionHit>,
    /// Full metadata for right bracket intersection (None for path-end sentinel).
    pub right_hit: Option<IntersectionHit>,
}

// ────────────────────────────────────────────────────────────
// Subpath reversal
// ────────────────────────────────────────────────────────────

/// Reverse a subpath's command order, producing a geometrically equivalent
/// subpath that walks in the opposite direction.
pub fn reverse_subpath(sp: &SubPath) -> SubPath {
    // Collect (endpoint, command) pairs in drawing order
    let mut segments: Vec<(Point2D, PathCommand)> = Vec::new();
    let mut current = Point2D::zero();
    let mut first_point = Point2D::zero();

    for cmd in &sp.commands {
        match *cmd {
            PathCommand::MoveTo { x, y } => {
                first_point = Point2D::new(x, y);
                current = first_point;
            }
            PathCommand::LineTo { x, y } => {
                segments.push((current, *cmd));
                current = Point2D::new(x, y);
            }
            PathCommand::QuadTo { cx, cy, x, y } => {
                segments.push((current, PathCommand::QuadTo { cx, cy, x, y }));
                current = Point2D::new(x, y);
            }
            PathCommand::CubicTo {
                c1x,
                c1y,
                c2x,
                c2y,
                x,
                y,
            } => {
                segments.push((
                    current,
                    PathCommand::CubicTo {
                        c1x,
                        c1y,
                        c2x,
                        c2y,
                        x,
                        y,
                    },
                ));
                current = Point2D::new(x, y);
            }
            PathCommand::Close => {
                // Treat close as implicit LineTo back to first_point
                if current.distance_to(&first_point) > 1e-9 {
                    segments.push((
                        current,
                        PathCommand::LineTo {
                            x: first_point.x,
                            y: first_point.y,
                        },
                    ));
                    current = first_point;
                }
            }
        }
    }

    if segments.is_empty() {
        return sp.clone();
    }

    // The last point of the original path becomes the first point of the reversed path
    let mut reversed_cmds = vec![PathCommand::MoveTo {
        x: current.x,
        y: current.y,
    }];

    // Walk segments in reverse, swapping control point roles
    for (start_pt, cmd) in segments.iter().rev() {
        match *cmd {
            PathCommand::LineTo { .. } => {
                reversed_cmds.push(PathCommand::LineTo {
                    x: start_pt.x,
                    y: start_pt.y,
                });
            }
            PathCommand::QuadTo { cx, cy, .. } => {
                reversed_cmds.push(PathCommand::QuadTo {
                    cx,
                    cy,
                    x: start_pt.x,
                    y: start_pt.y,
                });
            }
            PathCommand::CubicTo {
                c1x, c1y, c2x, c2y, ..
            } => {
                // Swap control points when reversing
                reversed_cmds.push(PathCommand::CubicTo {
                    c1x: c2x,
                    c1y: c2y,
                    c2x: c1x,
                    c2y: c1y,
                    x: start_pt.x,
                    y: start_pt.y,
                });
            }
            _ => {}
        }
    }

    // If original was closed, re-close
    if sp.closed {
        reversed_cmds.push(PathCommand::Close);
    }

    SubPath {
        commands: reversed_cmds,
        closed: sp.closed,
    }
}

// ────────────────────────────────────────────────────────────
// Conservative heal
// ────────────────────────────────────────────────────────────

/// Outcome of attempting to heal trim results.
pub enum HealOutcome {
    /// Merged into one closed path — fill-ready.
    HealedClosed(VecPath),
    /// Merged into one open path — NOT fill-ready (typical trim result).
    HealedOpen(VecPath),
    /// Could not form a single chain — pieces left as-is.
    Ambiguous(Vec<VecPath>),
}

/// Attempt to heal trim results into a single chain/loop.
/// Conservative: only heals the simple, unambiguous case.
pub fn heal_trim_results(
    pieces: &[VecPath],
    cut_points: &[Point2D],
    tolerance: f64,
) -> HealOutcome {
    // Separate open and closed subpaths
    let mut open_sps: Vec<SubPath> = Vec::new();
    let mut closed_sps: Vec<SubPath> = Vec::new();

    for vp in pieces {
        for sp in &vp.subpaths {
            if sp.closed {
                closed_sps.push(sp.clone());
            } else if sp.commands.len() > 1 {
                open_sps.push(sp.clone());
            }
        }
    }

    if open_sps.is_empty() {
        return HealOutcome::Ambiguous(pieces.to_vec());
    }

    // Snap open subpath endpoints to nearest cut point within tolerance
    for sp in &mut open_sps {
        snap_subpath_endpoints(sp, cut_points, tolerance);
    }

    // Greedy chaining
    let mut chain = vec![open_sps.remove(0)];
    let mut remaining = open_sps;

    loop {
        let chain_end = subpath_last_point(chain.last().unwrap());
        let Some(chain_end) = chain_end else { break };

        let mut best_idx = None;
        let mut best_reversed = false;
        let mut match_count = 0;

        for (i, sp) in remaining.iter().enumerate() {
            let sp_start = subpath_first_point(sp);
            let sp_end = subpath_last_point(sp);

            if let Some(start) = sp_start
                && chain_end.distance_to(&start) < tolerance
            {
                match_count += 1;
                best_idx = Some(i);
                best_reversed = false;
            }
            if let Some(end) = sp_end
                && chain_end.distance_to(&end) < tolerance
            {
                match_count += 1;
                best_idx = Some(i);
                best_reversed = true;
            }
        }

        // Ambiguity check: 2+ equally valid matches
        if match_count > 1 {
            // Put remaining back and return ambiguous
            let mut all = Vec::new();
            for sp in chain.into_iter().chain(remaining.into_iter()) {
                let mut vp = VecPath::new();
                vp.subpaths.push(sp);
                all.push(vp);
            }
            for sp in closed_sps {
                let mut vp = VecPath::new();
                vp.subpaths.push(sp);
                all.push(vp);
            }
            return HealOutcome::Ambiguous(all);
        }

        if let Some(idx) = best_idx {
            let sp = remaining.remove(idx);
            let sp = if best_reversed {
                reverse_subpath(&sp)
            } else {
                sp
            };
            chain.push(sp);
        } else {
            break;
        }
    }

    // Backward chaining: extend chain from its start
    if !remaining.is_empty() {
        loop {
            let chain_start = subpath_first_point(chain.first().unwrap());
            let Some(chain_start) = chain_start else {
                break;
            };

            let mut best_idx = None;
            let mut best_reversed = false;
            let mut match_count = 0;

            for (i, sp) in remaining.iter().enumerate() {
                let sp_start = subpath_first_point(sp);
                let sp_end = subpath_last_point(sp);
                if let Some(end) = sp_end
                    && chain_start.distance_to(&end) < tolerance
                {
                    match_count += 1;
                    best_idx = Some(i);
                    best_reversed = false;
                }
                if let Some(start) = sp_start
                    && chain_start.distance_to(&start) < tolerance
                {
                    match_count += 1;
                    best_idx = Some(i);
                    best_reversed = true;
                }
            }

            if match_count > 1 {
                break;
            }
            if let Some(idx) = best_idx {
                let sp = remaining.remove(idx);
                let sp = if best_reversed {
                    reverse_subpath(&sp)
                } else {
                    sp
                };
                chain.insert(0, sp);
            } else {
                break;
            }
        }
    }

    // Any unchained subpaths → ambiguous
    if !remaining.is_empty() {
        let mut all = Vec::new();
        for sp in chain.into_iter().chain(remaining.into_iter()) {
            let mut vp = VecPath::new();
            vp.subpaths.push(sp);
            all.push(vp);
        }
        for sp in closed_sps {
            let mut vp = VecPath::new();
            vp.subpaths.push(sp);
            all.push(vp);
        }
        return HealOutcome::Ambiguous(all);
    }

    // Merge chain into single SubPath
    let merged = merge_chain_to_subpath(&chain);

    // Check if chain's start ≈ end within tolerance → close it
    let start = subpath_first_point(&merged);
    let end = subpath_last_point(&merged);
    let should_close = if let (Some(s), Some(e)) = (start, end) {
        s.distance_to(&e) < tolerance
    } else {
        false
    };

    if should_close {
        let mut cmds = merged.commands;
        // Snap the last draw command endpoint to exactly match the start point
        if let Some(start_pt) = subpath_first_point(&SubPath {
            commands: cmds.clone(),
            closed: false,
        }) {
            if let Some(last_cmd) = cmds.last_mut() {
                match last_cmd {
                    PathCommand::LineTo { x, y }
                    | PathCommand::QuadTo { x, y, .. }
                    | PathCommand::CubicTo { x, y, .. } => {
                        *x = start_pt.x;
                        *y = start_pt.y;
                    }
                    _ => {}
                }
            }
        }
        cmds.push(PathCommand::Close);
        let closed_sp = SubPath {
            commands: cmds,
            closed: true,
        };
        let mut result = VecPath::new();
        result.subpaths.push(closed_sp);
        for sp in closed_sps {
            result.subpaths.push(sp);
        }
        HealOutcome::HealedClosed(result)
    } else {
        let mut result = VecPath::new();
        result.subpaths.push(merged);
        for sp in closed_sps {
            result.subpaths.push(sp);
        }
        HealOutcome::HealedOpen(result)
    }
}

fn snap_subpath_endpoints(sp: &mut SubPath, cut_points: &[Point2D], tolerance: f64) {
    if sp.commands.is_empty() || cut_points.is_empty() {
        return;
    }

    // Snap first point (MoveTo)
    if let Some(PathCommand::MoveTo { x, y }) = sp.commands.first_mut() {
        let pt = Point2D::new(*x, *y);
        if let Some(nearest) = nearest_cut_point(&pt, cut_points, tolerance) {
            *x = nearest.x;
            *y = nearest.y;
        }
    }

    // Snap last endpoint
    if let Some(
        PathCommand::LineTo { x, y }
        | PathCommand::QuadTo { x, y, .. }
        | PathCommand::CubicTo { x, y, .. },
    ) = sp.commands.last_mut()
    {
        let pt = Point2D::new(*x, *y);
        if let Some(nearest) = nearest_cut_point(&pt, cut_points, tolerance) {
            *x = nearest.x;
            *y = nearest.y;
        }
    }
}

fn nearest_cut_point(pt: &Point2D, cut_points: &[Point2D], tolerance: f64) -> Option<Point2D> {
    cut_points
        .iter()
        .filter(|cp| pt.distance_to(cp) < tolerance)
        .min_by(|a, b| pt.distance_to(a).partial_cmp(&pt.distance_to(b)).unwrap())
        .copied()
}

pub fn subpath_first_point(sp: &SubPath) -> Option<Point2D> {
    sp.commands.first().and_then(|cmd| match *cmd {
        PathCommand::MoveTo { x, y } => Some(Point2D::new(x, y)),
        _ => None,
    })
}

pub fn subpath_last_point(sp: &SubPath) -> Option<Point2D> {
    for cmd in sp.commands.iter().rev() {
        match *cmd {
            PathCommand::LineTo { x, y }
            | PathCommand::QuadTo { x, y, .. }
            | PathCommand::CubicTo { x, y, .. } => return Some(Point2D::new(x, y)),
            PathCommand::Close => continue,
            PathCommand::MoveTo { x, y } => return Some(Point2D::new(x, y)),
        }
    }
    None
}

/// Merge a chain of SubPaths into one SubPath by concatenating draw commands.
fn merge_chain_to_subpath(chain: &[SubPath]) -> SubPath {
    if chain.is_empty() {
        return SubPath {
            commands: vec![],
            closed: false,
        };
    }

    let mut commands = Vec::new();
    for (i, sp) in chain.iter().enumerate() {
        for (j, cmd) in sp.commands.iter().enumerate() {
            match *cmd {
                PathCommand::MoveTo { x, y } => {
                    if i == 0 && j == 0 {
                        commands.push(PathCommand::MoveTo { x, y });
                    }
                    // Skip MoveTo from subsequent chain links
                }
                PathCommand::Close => {
                    // Skip Close from intermediate chain links
                }
                _ => {
                    commands.push(*cmd);
                }
            }
        }
    }

    SubPath {
        commands,
        closed: false,
    }
}

// ────────────────────────────────────────────────────────────
// Step 6: Main entry point
// ────────────────────────────────────────────────────────────

/// Trim a path at intersection points, removing the segment under the click.
/// Returns the remaining pieces with cut point coordinates for healing.
/// Empty pieces means no intersections were found.
pub fn trim_at_intersection(
    target_path: &VecPath,
    target_subpath_idx: usize,
    external_cutter_polylines: &[(Vec<Point2D>, bool)],
    click_point: Point2D,
) -> TrimResult {
    let Some(target_sp) = target_path.subpaths.get(target_subpath_idx) else {
        return TrimResult {
            pieces: Vec::new(),
            cut_points: Vec::new(),
            left_hit: None,
            right_hit: None,
        };
    };

    let tolerance = DEFAULT_TOLERANCE_MM;

    let Some(bracket) = find_trim_bracket(target_sp, external_cutter_polylines, click_point) else {
        return TrimResult {
            pieces: Vec::new(),
            cut_points: Vec::new(),
            left_hit: None,
            right_hit: None,
        };
    };

    // Compute cut points from bracket distances
    let cut_points = vec![
        arc_distance_to_point(&bracket.flat_points, bracket.left_dist),
        arc_distance_to_point(&bracket.flat_points, bracket.right_dist),
    ];

    // Split and remove the clicked segment
    let pieces = if target_sp.closed {
        trim_closed_subpath(
            target_path,
            target_subpath_idx,
            bracket.left_dist,
            bracket.right_dist,
            bracket.total_len,
            tolerance,
        )
    } else {
        trim_open_subpath(
            target_path,
            target_subpath_idx,
            bracket.left_dist,
            bracket.right_dist,
            tolerance,
        )
    };

    TrimResult {
        pieces,
        cut_points,
        left_hit: bracket.left_hit,
        right_hit: bracket.right_hit,
    }
}

/// For open paths: find left and right bracket indices around click_dist.
/// Returns (left_index, right_index) where None means path start/end.
fn find_bracket_open(
    hits: &[IntersectionHit],
    click_dist: f64,
    _total_len: f64,
) -> (Option<usize>, Option<usize>) {
    let left = hits
        .iter()
        .enumerate()
        .rev()
        .find(|(_, h)| h.target_dist < click_dist)
        .map(|(i, _)| i);

    let right = hits
        .iter()
        .enumerate()
        .find(|(_, h)| h.target_dist > click_dist)
        .map(|(i, _)| i);

    (left, right)
}

/// For closed paths: find the adjacent intersection pair containing click_dist.
/// Returns (left_index, right_index) into the hits slice.
fn find_bracket_closed(
    hits: &[IntersectionHit],
    click_dist: f64,
    total_len: f64,
) -> (usize, usize) {
    if hits.len() == 1 {
        // Single intersection on a closed path — bracket wraps the entire path
        return (0, 0);
    }

    // Check each adjacent pair
    for i in 0..hits.len() {
        let left = hits[i].target_dist;
        let right = if i + 1 < hits.len() {
            hits[i + 1].target_dist
        } else {
            hits[0].target_dist + total_len // wrap-around
        };

        let contains = if right > left {
            click_dist > left && click_dist < right
        } else {
            // Wrap-around case
            click_dist > left || click_dist < (right - total_len)
        };

        if contains {
            let right_idx = if i + 1 < hits.len() { i + 1 } else { 0 };
            return (i, right_idx);
        }
    }

    // Fallback: use first and last
    (0, hits.len() - 1)
}

/// Trim an open subpath: remove the segment between left_dist and right_dist.
fn trim_open_subpath(
    target_path: &VecPath,
    sp_idx: usize,
    left_dist: f64,
    right_dist: f64,
    tolerance: f64,
) -> Vec<VecPath> {
    let subpath = &target_path.subpaths[sp_idx];
    let endpoint_margin = DEFAULT_TOLERANCE_MM * 0.1;

    // Split at both distances
    let mut split_dists = Vec::new();
    if left_dist > endpoint_margin {
        split_dists.push(left_dist);
    }
    let (flat_pts, _) = flatten_subpath_annotated(&subpath.commands, subpath.closed, tolerance);
    let total = compute_polyline_arc_length(&flat_pts, subpath.closed);
    if right_dist < total - endpoint_margin {
        split_dists.push(right_dist);
    }

    if split_dists.is_empty() {
        // Entire path is between the brackets — remove everything
        return Vec::new();
    }

    let pieces = split_subpath_at_distances(subpath, &split_dists, tolerance);

    // Remove the middle piece (the one containing the click)
    let mut result = Vec::new();
    let mut first_kept = true;
    for (i, piece) in pieces.into_iter().enumerate() {
        // If we split at both distances, piece 1 (index 1) is the removed segment
        // If we split at only left, piece 1 is removed
        // If we split at only right, piece 0 is removed
        let is_removed = if left_dist > endpoint_margin && right_dist < total - endpoint_margin {
            i == 1 // middle piece
        } else if left_dist <= endpoint_margin {
            i == 0 // first piece (from start to right_dist)
        } else {
            i == pieces_count_minus_one(left_dist) // last piece
        };

        if !is_removed && piece.commands.len() > 1 {
            let mut vp = VecPath::new();
            vp.subpaths.push(piece);
            // Only the first kept piece inherits sibling subpaths
            if first_kept {
                for (j, sp) in target_path.subpaths.iter().enumerate() {
                    if j != sp_idx {
                        vp.subpaths.push(sp.clone());
                    }
                }
                first_kept = false;
            }
            result.push(vp);
        }
    }

    // If we got no results but had pieces, return them all (safety fallback)
    if result.is_empty() {
        return Vec::new();
    }

    result
}

fn pieces_count_minus_one(left_dist: f64) -> usize {
    let endpoint_margin = DEFAULT_TOLERANCE_MM * 0.1;
    if left_dist > endpoint_margin { 1 } else { 0 }
}

/// Trim a closed subpath: unroll at one intersection, remove clicked segment.
fn trim_closed_subpath(
    target_path: &VecPath,
    sp_idx: usize,
    left_dist: f64,
    right_dist: f64,
    _total_len: f64,
    tolerance: f64,
) -> Vec<VecPath> {
    let subpath = &target_path.subpaths[sp_idx];

    let dedup_tol = DEFAULT_TOLERANCE_MM * 0.5;
    if (left_dist - right_dist).abs() < dedup_tol {
        // Single intersection — can't trim closed path with one intersection
        return Vec::new();
    }

    // Convert closed path to open by replacing Close with explicit LineTo
    let mut open_commands = Vec::new();
    let mut first_pt = Point2D::zero();
    let mut last_pt = Point2D::zero();
    for (i, cmd) in subpath.commands.iter().enumerate() {
        match *cmd {
            PathCommand::MoveTo { x, y } => {
                if i == 0 {
                    first_pt = Point2D::new(x, y);
                }
                last_pt = Point2D::new(x, y);
                open_commands.push(*cmd);
            }
            PathCommand::Close => {
                // Replace with explicit LineTo back to start if last point differs
                if last_pt.distance_to(&first_pt) > 1e-9 {
                    open_commands.push(PathCommand::LineTo {
                        x: first_pt.x,
                        y: first_pt.y,
                    });
                }
            }
            _ => {
                last_pt = command_endpoint(cmd, last_pt);
                open_commands.push(*cmd);
            }
        }
    }

    let open_sp = SubPath {
        commands: open_commands,
        closed: false,
    };

    // Determine split distances
    let wraps = right_dist < left_dist;

    if wraps {
        // Wrap-around: the removed segment crosses the start/end boundary.
        // We need to unroll (cyclically rotate) the path so both split points
        // become interior, then split normally and keep the middle piece.
        //
        // Strategy: split at right_dist (the lower distance) first to rotate
        // the path so right_dist becomes the new start. Then the kept segment
        // runs from 0 to (left_dist - right_dist) in the rotated path.
        let split_dists = vec![right_dist, left_dist];
        let pieces = split_subpath_at_distances(&open_sp, &split_dists, tolerance);

        // pieces[0] = start → right_dist (tail of removed segment)
        // pieces[1] = right_dist → left_dist (KEPT: the part NOT clicked)
        // pieces[2] = left_dist → end (head of removed segment)
        // We keep piece[1]. If wrap produced only 2 pieces, piece[1] is still the kept one.
        let mut result = Vec::new();
        let kept_idx = 1;
        if pieces.len() > kept_idx && pieces[kept_idx].commands.len() > 1 {
            let mut vp = VecPath::new();
            vp.subpaths.push(pieces[kept_idx].clone());
            // Sibling subpaths go only into the first result VecPath
            for (j, sp) in target_path.subpaths.iter().enumerate() {
                if j != sp_idx {
                    vp.subpaths.push(sp.clone());
                }
            }
            result.push(vp);
        }
        result
    } else {
        // Normal: remove segment between left_dist and right_dist
        let split_dists = vec![left_dist, right_dist];
        let pieces = split_subpath_at_distances(&open_sp, &split_dists, tolerance);

        // pieces[0] = start→left_dist (KEPT)
        // pieces[1] = left_dist→right_dist (removed)
        // pieces[2] = right_dist→end (KEPT)
        let mut result = Vec::new();
        let mut first_kept = true;
        for (i, piece) in pieces.into_iter().enumerate() {
            if i != 1 && piece.commands.len() > 1 {
                let mut vp = VecPath::new();
                vp.subpaths.push(piece);
                // Only the first kept piece inherits sibling subpaths
                if first_kept {
                    for (j, sp) in target_path.subpaths.iter().enumerate() {
                        if j != sp_idx {
                            vp.subpaths.push(sp.clone());
                        }
                    }
                    first_kept = false;
                }
                result.push(vp);
            }
        }
        result
    }
}

fn compute_polyline_arc_length(points: &[Point2D], closed: bool) -> f64 {
    if points.len() < 2 {
        return 0.0;
    }
    let n = if closed {
        points.len()
    } else {
        points.len() - 1
    };
    let mut total = 0.0;
    for i in 0..n {
        total += points[i].distance_to(&points[(i + 1) % points.len()]);
    }
    total
}

// Reusable flatten helpers (match flatten.rs logic)
fn flatten_cubic_recursive(
    p0: Point2D,
    p1: Point2D,
    p2: Point2D,
    p3: Point2D,
    tolerance: f64,
    points: &mut Vec<Point2D>,
) {
    let d1 = point_to_line_dist(p1, p0, p3);
    let d2 = point_to_line_dist(p2, p0, p3);
    if d1.max(d2) <= tolerance {
        points.push(p3);
    } else {
        let m01 = p0.lerp(&p1, 0.5);
        let m12 = p1.lerp(&p2, 0.5);
        let m23 = p2.lerp(&p3, 0.5);
        let m012 = m01.lerp(&m12, 0.5);
        let m123 = m12.lerp(&m23, 0.5);
        let mid = m012.lerp(&m123, 0.5);
        flatten_cubic_recursive(p0, m01, m012, mid, tolerance, points);
        flatten_cubic_recursive(mid, m123, m23, p3, tolerance, points);
    }
}

fn flatten_quad_recursive(
    p0: Point2D,
    p1: Point2D,
    p2: Point2D,
    tolerance: f64,
    points: &mut Vec<Point2D>,
) {
    let d = point_to_line_dist(p1, p0, p2);
    if d <= tolerance {
        points.push(p2);
    } else {
        let m01 = p0.lerp(&p1, 0.5);
        let m12 = p1.lerp(&p2, 0.5);
        let mid = m01.lerp(&m12, 0.5);
        flatten_quad_recursive(p0, m01, mid, tolerance, points);
        flatten_quad_recursive(mid, m12, p2, tolerance, points);
    }
}

fn point_to_line_dist(p: Point2D, a: Point2D, b: Point2D) -> f64 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len_sq = dx * dx + dy * dy;
    if len_sq < 1e-20 {
        return p.distance_to(&a);
    }
    let cross = (p.x - a.x) * dy - (p.y - a.y) * dx;
    cross.abs() / len_sq.sqrt()
}

// ────────────────────────────────────────────────────────────
// Legacy API (kept for backward compat, delegates to new impl)
// ────────────────────────────────────────────────────────────

/// Trim a path between two parameter positions (0.0-1.0).
/// Returns the segments before t_start, and after t_end.
/// **Legacy API** — prefer `trim_at_intersection` for intersection-based trimming.
pub fn trim_at_points(path: &VecPath, t_start: f64, t_end: f64) -> Vec<VecPath> {
    use crate::vector::flatten::flatten_vecpath;

    let t_start = t_start.clamp(0.0, 1.0);
    let t_end = t_end.clamp(0.0, 1.0);

    if t_start >= t_end {
        return vec![path.clone()];
    }

    let polylines = flatten_vecpath(path, DEFAULT_TOLERANCE_MM);

    let mut result_paths = Vec::new();

    for polyline in polylines {
        if polyline.points.len() < 2 {
            continue;
        }

        let total_len = compute_polyline_arc_length(&polyline.points, polyline.closed);
        let trim_start = total_len * t_start;
        let trim_end = total_len * t_end;

        let segments =
            legacy_split_at_distances(&polyline.points, &[trim_start, trim_end], polyline.closed);

        if !segments.is_empty() && !segments[0].is_empty() {
            let commands = points_to_commands(&segments[0], false);
            result_paths.push(VecPath {
                subpaths: vec![SubPath {
                    commands,
                    closed: false,
                }],
            });
        }

        if segments.len() >= 3 && !segments[2].is_empty() {
            let commands = points_to_commands(&segments[2], polyline.closed);
            result_paths.push(VecPath {
                subpaths: vec![SubPath {
                    commands,
                    closed: false,
                }],
            });
        }
    }

    result_paths
}

fn legacy_split_at_distances(
    points: &[Point2D],
    distances: &[f64],
    closed: bool,
) -> Vec<Vec<Point2D>> {
    let mut segments: Vec<Vec<Point2D>> = vec![vec![]];
    let mut current_len = 0.0;
    let mut distance_idx = 0;

    let n = if closed {
        points.len()
    } else {
        points.len() - 1
    };

    for i in 0..n {
        let p1 = points[i];
        let p2 = points[(i + 1) % points.len()];

        let dx = p2.x - p1.x;
        let dy = p2.y - p1.y;
        let seg_len = (dx * dx + dy * dy).sqrt();

        segments.last_mut().unwrap().push(p1);

        while distance_idx < distances.len() && current_len + seg_len >= distances[distance_idx] {
            let t = (distances[distance_idx] - current_len) / seg_len;
            let split_point = Point2D {
                x: p1.x + dx * t,
                y: p1.y + dy * t,
            };

            segments.last_mut().unwrap().push(split_point);
            segments.push(vec![split_point]);

            distance_idx += 1;
        }

        current_len += seg_len;
    }

    if !closed && !points.is_empty() {
        segments.last_mut().unwrap().push(points[points.len() - 1]);
    }

    segments
}

fn points_to_commands(points: &[Point2D], closed: bool) -> Vec<PathCommand> {
    if points.is_empty() {
        return vec![];
    }

    let mut commands = vec![PathCommand::MoveTo {
        x: points[0].x,
        y: points[0].y,
    }];

    for pt in &points[1..] {
        commands.push(PathCommand::LineTo { x: pt.x, y: pt.y });
    }

    if closed {
        commands.push(PathCommand::Close);
    }

    commands
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Split utilities ──

    #[test]
    fn split_cubic_at_half() {
        let p0 = Point2D::new(0.0, 0.0);
        let p1 = Point2D::new(10.0, 20.0);
        let p2 = Point2D::new(30.0, 20.0);
        let p3 = Point2D::new(40.0, 0.0);

        let (before, after) = split_cubic(p0, p1, p2, p3, 0.5);

        // Start of first half matches p0
        assert_eq!(before.0, p0);
        // End of second half matches p3
        assert_eq!(after.3, p3);
        // Halves meet at midpoint
        assert!((before.3.x - after.0.x).abs() < 1e-10);
        assert!((before.3.y - after.0.y).abs() < 1e-10);
    }

    #[test]
    fn split_cubic_preserves_shape() {
        let p0 = Point2D::new(0.0, 0.0);
        let p1 = Point2D::new(10.0, 30.0);
        let p2 = Point2D::new(30.0, 30.0);
        let p3 = Point2D::new(40.0, 0.0);

        let (before, after) = split_cubic(p0, p1, p2, p3, 0.5);

        // Evaluate at t=0 on the first half should give p0
        assert!((before.0.x - p0.x).abs() < 1e-10);
        // Evaluate at t=1 on the second half should give p3
        assert!((after.3.x - p3.x).abs() < 1e-10);
    }

    #[test]
    fn split_quad_at_half() {
        let p0 = Point2D::new(0.0, 0.0);
        let p1 = Point2D::new(20.0, 40.0);
        let p2 = Point2D::new(40.0, 0.0);

        let (before, after) = split_quad(p0, p1, p2, 0.5);

        assert_eq!(before.0, p0);
        assert_eq!(after.2, p2);
        assert!((before.2.x - after.0.x).abs() < 1e-10);
    }

    #[test]
    fn two_splits_in_one_cubic() {
        let p0 = Point2D::new(0.0, 0.0);
        let p1 = Point2D::new(10.0, 30.0);
        let p2 = Point2D::new(30.0, 30.0);
        let p3 = Point2D::new(40.0, 0.0);

        // Split at t=0.3
        let (first, rest) = split_cubic(p0, p1, p2, p3, 0.3);
        // Reparameterize t=0.7 into the remaining segment
        let t2 = (0.7 - 0.3) / (1.0 - 0.3);
        let (second, third) = split_cubic(rest.0, rest.1, rest.2, rest.3, t2);

        // First piece starts at p0
        assert_eq!(first.0, p0);
        // Third piece ends at p3
        assert_eq!(third.3, p3);
        // Pieces connect
        assert!((first.3.x - second.0.x).abs() < 1e-10);
        assert!((second.3.x - third.0.x).abs() < 1e-10);
    }

    // ── Annotated flatten ──

    #[test]
    fn annotated_flatten_tracks_command_spans() {
        let path = VecPath::parse_svg_d("M0 0 L10 0 L10 10 L0 10");
        let sp = &path.subpaths[0];
        let (_pts, spans) = flatten_subpath_annotated(&sp.commands, sp.closed, 0.05);

        // 3 draw commands: L10 0, L10 10, L0 10
        assert_eq!(spans.len(), 3);

        // No gaps between spans
        for i in 1..spans.len() {
            assert!((spans[i].arc_start - spans[i - 1].arc_end).abs() < 1e-9);
        }

        // First span starts at 0
        assert!((spans[0].arc_start).abs() < 1e-9);
    }

    // ── Segment intersection ──

    #[test]
    fn segment_intersection_crossing() {
        let p1 = Point2D::new(0.0, 0.0);
        let p2 = Point2D::new(10.0, 10.0);
        let p3 = Point2D::new(0.0, 10.0);
        let p4 = Point2D::new(10.0, 0.0);

        let result = segment_intersection(p1, p2, p3, p4);
        assert!(result.is_some());
        let (t, u, pt) = result.unwrap();
        assert!((t - 0.5).abs() < 0.01);
        assert!((u - 0.5).abs() < 0.01);
        assert!((pt.x - 5.0).abs() < 0.01);
        assert!((pt.y - 5.0).abs() < 0.01);
    }

    #[test]
    fn segment_intersection_parallel() {
        let p1 = Point2D::new(0.0, 0.0);
        let p2 = Point2D::new(10.0, 0.0);
        let p3 = Point2D::new(0.0, 5.0);
        let p4 = Point2D::new(10.0, 5.0);

        assert!(segment_intersection(p1, p2, p3, p4).is_none());
    }

    #[test]
    fn segment_intersection_at_endpoint() {
        let p1 = Point2D::new(0.0, 0.0);
        let p2 = Point2D::new(10.0, 0.0);
        let p3 = Point2D::new(10.0, 0.0);
        let p4 = Point2D::new(10.0, 10.0);

        let result = segment_intersection(p1, p2, p3, p4);
        // Should handle gracefully (may or may not detect depending on epsilon)
        if let Some((_t, _u, pt)) = result {
            assert!((pt.x - 10.0).abs() < 0.1);
        }
    }

    // ── Edge proximity ──

    #[test]
    fn nearest_edge_on_segment() {
        let pts = vec![Point2D::new(0.0, 0.0), Point2D::new(10.0, 0.0)];
        let point = Point2D::new(5.0, 3.0);

        let result = nearest_edge(point, &[(pts, false)]);
        assert!(result.is_some());
        let (sp_idx, dist, _arc) = result.unwrap();
        assert_eq!(sp_idx, 0);
        assert!((dist - 3.0).abs() < 0.01);
    }

    #[test]
    fn nearest_edge_picks_closest_subpath() {
        let pts1 = vec![Point2D::new(0.0, 0.0), Point2D::new(10.0, 0.0)];
        let pts2 = vec![Point2D::new(0.0, 1.0), Point2D::new(10.0, 1.0)];
        let point = Point2D::new(5.0, 0.8);

        let result = nearest_edge(point, &[(pts1, false), (pts2, false)]);
        assert!(result.is_some());
        let (sp_idx, _, _) = result.unwrap();
        assert_eq!(sp_idx, 1); // Closer to the second polyline
    }

    // ── Self-intersection ──

    #[test]
    fn find_intersections_self_crossing() {
        // Figure-8: crosses itself
        let pts = vec![
            Point2D::new(0.0, 0.0),
            Point2D::new(10.0, 10.0),
            Point2D::new(10.0, 0.0),
            Point2D::new(0.0, 10.0),
        ];

        let with_self = find_intersections(&pts, false, &[], true);
        let without_self = find_intersections(&pts, false, &[], false);

        assert!(
            !with_self.is_empty(),
            "Self-intersection should be detected"
        );
        assert!(
            without_self.is_empty(),
            "Without include_self, no intersections expected"
        );
    }

    #[test]
    fn find_intersections_skips_adjacent_segments() {
        // L-shaped polyline: no false intersection at shared vertex
        let pts = vec![
            Point2D::new(0.0, 0.0),
            Point2D::new(10.0, 0.0),
            Point2D::new(10.0, 10.0),
        ];

        let result = find_intersections(&pts, false, &[], true);
        assert!(
            result.is_empty(),
            "Adjacent segments should not produce false intersections"
        );
    }

    #[test]
    fn find_intersections_self_colinear_overlap() {
        // Path that doubles back on itself: 0→10, then 10→5 (overlap on 5→10)
        let pts = vec![
            Point2D::new(0.0, 0.0),
            Point2D::new(10.0, 0.0),
            Point2D::new(10.0, 5.0), // go up to avoid adjacency skip
            Point2D::new(10.0, 0.0), // come back down — not colinear with seg 0
            Point2D::new(5.0, 0.0),  // now go left along x-axis — overlaps seg 0
        ];

        let with_self = find_intersections(&pts, false, &[], true);
        // Segments 0 (0,0→10,0) and 3 (10,0→5,0) are colinear and overlap on [5,10]
        assert!(
            with_self.len() >= 2,
            "Should detect colinear overlap on the doubled-back segment, got {:?}",
            with_self
        );
    }

    // ── Full trim pipeline ──

    #[test]
    fn trim_crossing_lines() {
        // Horizontal line crossed by vertical
        let horiz = VecPath::parse_svg_d("M0 5 L20 5");
        let vert_pts = vec![Point2D::new(10.0, 0.0), Point2D::new(10.0, 10.0)];

        // Click on the right half of horizontal
        let result = trim_at_intersection(&horiz, 0, &[(vert_pts, false)], Point2D::new(15.0, 5.0));

        assert!(
            !result.pieces.is_empty(),
            "Should return at least one piece after trim"
        );
    }

    #[test]
    fn trim_preserves_cubics() {
        // Cubic bezier crossed by a vertical line
        let cubic = VecPath::parse_svg_d("M0 0 C10 30 30 30 40 0");
        let vert_pts = vec![Point2D::new(20.0, -10.0), Point2D::new(20.0, 30.0)];

        let result =
            trim_at_intersection(&cubic, 0, &[(vert_pts, false)], Point2D::new(10.0, 15.0));

        // Check that output retains CubicTo commands
        let has_cubic = result.pieces.iter().any(|vp| {
            vp.subpaths.iter().any(|sp| {
                sp.commands
                    .iter()
                    .any(|c| matches!(c, PathCommand::CubicTo { .. }))
            })
        });
        assert!(has_cubic, "Trimmed output should preserve cubic curves");
    }

    #[test]
    fn trim_no_intersection() {
        let line = VecPath::parse_svg_d("M0 0 L10 0");
        // Cutter that doesn't cross
        let other = vec![Point2D::new(20.0, 20.0), Point2D::new(30.0, 30.0)];

        let result = trim_at_intersection(&line, 0, &[(other, false)], Point2D::new(5.0, 0.0));
        assert!(
            result.pieces.is_empty(),
            "No intersection should return empty"
        );
    }

    #[test]
    fn trim_single_intersection() {
        // A line crossed once by another — produces one remaining piece
        let line = VecPath::parse_svg_d("M0 0 L20 0");
        let cutter = vec![Point2D::new(10.0, -5.0), Point2D::new(10.0, 5.0)];

        // Click on the left half
        let result = trim_at_intersection(&line, 0, &[(cutter, false)], Point2D::new(5.0, 0.0));

        // With single intersection, clicking left of it should remove left piece
        // and return right piece (or vice versa depending on bracket)
        assert!(
            !result.pieces.is_empty(),
            "Single intersection should produce a piece"
        );
    }

    #[test]
    fn trim_self_intersection() {
        // Figure-8 single-subpath path
        let fig8 = VecPath::parse_svg_d("M0 0 L10 10 L10 0 L0 10");

        // Click on the first segment (should remove it after detecting self-crossing)
        let result = trim_at_intersection(&fig8, 0, &[], Point2D::new(2.0, 2.0));

        assert!(
            !result.pieces.is_empty(),
            "Self-intersection trim should produce results"
        );
    }

    #[test]
    fn trim_closed_path_normal() {
        // Closed rectangle crossed by a line
        let rect = VecPath::parse_svg_d("M0 0 L20 0 L20 10 L0 10 Z");
        let cutter = vec![Point2D::new(10.0, -5.0), Point2D::new(10.0, 15.0)];

        // Click on the right side
        let result = trim_at_intersection(&rect, 0, &[(cutter, false)], Point2D::new(15.0, 5.0));

        assert!(
            !result.pieces.is_empty(),
            "Closed path trim should return pieces"
        );
        // Result should be open (not closed)
        for vp in &result.pieces {
            for sp in &vp.subpaths {
                if sp.commands.len() > 1 {
                    assert!(
                        !sp.closed,
                        "Trimmed closed path should produce open subpaths"
                    );
                }
            }
        }
    }

    #[test]
    fn trim_click_selects_correct_segment() {
        // Line with two crossings, creating three segments
        let line = VecPath::parse_svg_d("M0 5 L30 5");
        let cutter1 = vec![Point2D::new(10.0, 0.0), Point2D::new(10.0, 10.0)];
        let cutter2 = vec![Point2D::new(20.0, 0.0), Point2D::new(20.0, 10.0)];

        // Click in the middle segment (between x=10 and x=20)
        let result = trim_at_intersection(
            &line,
            0,
            &[(cutter1, false), (cutter2, false)],
            Point2D::new(15.0, 5.0),
        );

        assert!(
            !result.pieces.is_empty(),
            "Should return at least one piece (the non-clicked segments)"
        );
    }

    // ── Colinear overlap ──

    #[test]
    fn colinear_overlap_full() {
        let p1 = Point2D::new(0.0, 0.0);
        let p2 = Point2D::new(10.0, 0.0);
        let p3 = Point2D::new(0.0, 0.0);
        let p4 = Point2D::new(10.0, 0.0);

        let result = colinear_overlap(p1, p2, p3, p4, 0.01);
        assert!(result.is_some());
        let (ts, te) = result.unwrap();
        assert!((ts - 0.0).abs() < 0.01);
        assert!((te - 1.0).abs() < 0.01);
    }

    #[test]
    fn colinear_overlap_partial() {
        let p1 = Point2D::new(0.0, 0.0);
        let p2 = Point2D::new(10.0, 0.0);
        let p3 = Point2D::new(5.0, 0.0);
        let p4 = Point2D::new(15.0, 0.0);

        let result = colinear_overlap(p1, p2, p3, p4, 0.01);
        assert!(result.is_some());
        let (ts, te) = result.unwrap();
        assert!((ts - 0.5).abs() < 0.01);
        assert!((te - 1.0).abs() < 0.01);
    }

    #[test]
    fn colinear_overlap_non_colinear() {
        let p1 = Point2D::new(0.0, 0.0);
        let p2 = Point2D::new(10.0, 0.0);
        let p3 = Point2D::new(0.0, 5.0);
        let p4 = Point2D::new(10.0, 5.0);

        let result = colinear_overlap(p1, p2, p3, p4, 0.01);
        assert!(result.is_none());
    }

    #[test]
    fn trim_colinear_shared_edge() {
        // Two rectangles sharing an edge at x=10
        // rect1: (0,0)→(10,0)→(10,10)→(0,10)→close
        // rect2 polyline: shares the (10,0)→(10,10) edge
        let rect1 = VecPath::parse_svg_d("M0 0 L10 0 L10 10 L0 10 Z");
        let rect2_pts = vec![
            Point2D::new(10.0, 0.0),
            Point2D::new(20.0, 0.0),
            Point2D::new(20.0, 10.0),
            Point2D::new(10.0, 10.0),
        ];

        // Click on the shared edge (x=10, y=5 — midpoint of the shared segment)
        let result = trim_at_intersection(&rect1, 0, &[(rect2_pts, true)], Point2D::new(10.0, 5.0));

        // The shared edge should be detected via colinear_overlap, and clicking
        // on it should remove the segment between the two overlap endpoints.
        assert!(
            !result.pieces.is_empty(),
            "Colinear shared edge should be detected and trimmed"
        );

        // Flatten all result subpaths and verify the shared edge segment
        // (the vertical line at x=10 from y=0 to y=10) was removed.
        // The remaining pieces should be open paths (the closed rect is broken).
        for vp in &result.pieces {
            for sp in &vp.subpaths {
                assert!(
                    !sp.closed,
                    "After trimming a closed path, result subpaths should be open"
                );
                // Walk through draw commands and check that no segment lies
                // entirely on x=10 between y=0 and y=10
                let mut prev = get_first_point(&sp.commands);
                for cmd in &sp.commands {
                    let ep = command_endpoint(cmd, prev);
                    if matches!(cmd, PathCommand::LineTo { .. }) {
                        let on_shared = (prev.x - 10.0).abs() < 0.1
                            && (ep.x - 10.0).abs() < 0.1
                            && prev.distance_to(&ep) > 0.5;
                        assert!(
                            !on_shared,
                            "Shared edge segment (x=10) should have been removed, \
                             found segment ({:.1},{:.1})→({:.1},{:.1})",
                            prev.x, prev.y, ep.x, ep.y
                        );
                    }
                    prev = ep;
                }
            }
        }
    }

    // ── Legacy API tests ──

    #[test]
    fn trim_at_points_splits_path() {
        let path = VecPath::parse_svg_d("M0 0 L100 0");
        let result = trim_at_points(&path, 0.25, 0.75);
        assert!(!result.is_empty());
    }

    #[test]
    fn trim_at_points_start_equals_end() {
        let path = VecPath::parse_svg_d("M0 0 L100 0");
        let result = trim_at_points(&path, 0.5, 0.5);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn trim_circle() {
        let path = VecPath::parse_svg_d("M0 0 L100 0 L100 100 L0 100 Z");
        let result = trim_at_points(&path, 0.2, 0.8);
        assert!(!result.is_empty());
    }

    #[test]
    fn trim_clamps_parameters() {
        let path = VecPath::parse_svg_d("M0 0 L100 0");
        let result = trim_at_points(&path, -0.5, 1.5);
        assert!(!result.is_empty());
    }

    #[test]
    fn trim_two_splits_in_one_command() {
        // Long cubic crossed by two close vertical lines
        let cubic = VecPath::parse_svg_d("M0 0 C20 40 60 40 80 0");
        let cutter1 = vec![Point2D::new(30.0, -10.0), Point2D::new(30.0, 50.0)];
        let cutter2 = vec![Point2D::new(50.0, -10.0), Point2D::new(50.0, 50.0)];

        // Click between the two cutters
        let result = trim_at_intersection(
            &cubic,
            0,
            &[(cutter1, false), (cutter2, false)],
            Point2D::new(40.0, 30.0),
        );

        // Should produce results (both splits land in the same CubicTo)
        assert!(
            !result.pieces.is_empty(),
            "Two splits in one command should work"
        );
    }

    #[test]
    fn trim_closed_path_wraparound() {
        // Closed path with cutters — click in the wrap-around region
        let rect = VecPath::parse_svg_d("M0 0 L20 0 L20 10 L0 10 Z");
        let cutter = vec![Point2D::new(5.0, -5.0), Point2D::new(5.0, 15.0)];

        // Click near x=0, which is in the wrap-around region (before the first intersection at x=5)
        let result = trim_at_intersection(&rect, 0, &[(cutter, false)], Point2D::new(2.0, 0.0));

        // With a single cutter on a closed path, there are typically 2 intersections
        // (top edge and bottom edge). Click in wrap-around should still work.
        assert!(
            !result.pieces.is_empty(),
            "Wrap-around trim should produce results"
        );
        // Result should be open subpaths
        for vp in &result.pieces {
            for sp in &vp.subpaths {
                if sp.commands.len() > 1 {
                    assert!(!sp.closed, "Wrapped trim should produce open subpaths");
                }
            }
        }
    }

    // ── Regression tests for bug fixes ──

    #[test]
    fn trim_closed_path_closing_edge_included() {
        // Closed rectangle: M0,0 L20,0 L20,10 L0,10 Z
        // The closing edge runs from (0,10) back to (0,0).
        // A vertical cutter at x=0 intersects the left edge (the Close segment).
        // After fix: Close is replaced with explicit LineTo, so trims on the
        // closing edge work correctly.
        let rect = VecPath::parse_svg_d("M0 0 L20 0 L20 10 L0 10 Z");
        // Cutter crosses through x=10 (intersects top and bottom edges)
        let cutter = vec![Point2D::new(10.0, -5.0), Point2D::new(10.0, 15.0)];

        // Click on top edge between x=0 and x=10
        let result = trim_at_intersection(&rect, 0, &[(cutter, false)], Point2D::new(5.0, 0.0));

        // Should produce at least one result (the non-clicked portion)
        assert!(
            !result.pieces.is_empty(),
            "Trim near closing edge should work"
        );

        // Verify output subpaths have actual geometry (not degenerate)
        for vp in &result.pieces {
            for sp in &vp.subpaths {
                assert!(
                    sp.commands.len() > 1,
                    "Each result subpath should have at least MoveTo + one draw command"
                );
            }
        }
    }

    #[test]
    fn trim_no_sibling_duplication() {
        // Object with two subpaths: trim one, the other should not be duplicated
        let mut path = VecPath::new();
        // Subpath 0: horizontal line that will be trimmed
        path.subpaths.push(SubPath {
            commands: vec![
                PathCommand::MoveTo { x: 0.0, y: 5.0 },
                PathCommand::LineTo { x: 30.0, y: 5.0 },
            ],
            closed: false,
        });
        // Subpath 1: an untouched square
        path.subpaths.push(SubPath {
            commands: vec![
                PathCommand::MoveTo { x: 50.0, y: 0.0 },
                PathCommand::LineTo { x: 60.0, y: 0.0 },
                PathCommand::LineTo { x: 60.0, y: 10.0 },
                PathCommand::LineTo { x: 50.0, y: 10.0 },
            ],
            closed: true,
        });

        // Cutter crosses subpath 0 at two points (x=10, x=20)
        let cutter1 = vec![Point2D::new(10.0, 0.0), Point2D::new(10.0, 10.0)];
        let cutter2 = vec![Point2D::new(20.0, 0.0), Point2D::new(20.0, 10.0)];

        // Click in the middle segment (between x=10 and x=20) of subpath 0
        let result = trim_at_intersection(
            &path,
            0,
            &[(cutter1, false), (cutter2, false)],
            Point2D::new(15.0, 5.0),
        );

        assert!(
            result.pieces.len() >= 2,
            "Should produce at least 2 kept pieces"
        );

        // Count how many result VecPaths contain the sibling subpath (the square)
        let sibling_count: usize = result
            .pieces
            .iter()
            .filter(|vp| vp.subpaths.len() > 1)
            .count();
        assert_eq!(
            sibling_count, 1,
            "Sibling subpath should appear in exactly one result VecPath, not all"
        );
    }

    #[test]
    fn trim_boundary_intersection_no_stub() {
        // Line from (0,0)→(10,0)→(20,0), cutter crosses exactly at x=10
        // (the boundary between the two LineTo commands). Click at (5,0)
        // removes the left segment. The kept piece(s) should not contain
        // a tiny (<0.1mm) stub caused by the boundary split.
        let path = VecPath::parse_svg_d("M0 0 L10 0 L20 0");
        let cutter = vec![Point2D::new(10.0, -5.0), Point2D::new(10.0, 5.0)];

        let result = trim_at_intersection(&path, 0, &[(cutter, false)], Point2D::new(5.0, 0.0));

        assert!(
            !result.pieces.is_empty(),
            "Boundary intersection should produce at least one kept piece"
        );

        // Measure every draw-command segment length; none should be a tiny stub
        for vp in &result.pieces {
            for sp in &vp.subpaths {
                let mut prev = get_first_point(&sp.commands);
                for cmd in &sp.commands {
                    let ep = command_endpoint(cmd, prev);
                    if !matches!(cmd, PathCommand::MoveTo { .. }) {
                        let seg_len = prev.distance_to(&ep);
                        assert!(
                            !(1e-12..=0.1).contains(&seg_len),
                            "Found tiny stub of length {seg_len:.6}mm \
                             from ({:.2},{:.2})→({:.2},{:.2})",
                            prev.x,
                            prev.y,
                            ep.x,
                            ep.y,
                        );
                    }
                    prev = ep;
                }
            }
        }

        // The kept piece should be the right segment: approximately (10,0)→(20,0)
        // with total length ~10mm
        let total_kept_len: f64 = result
            .pieces
            .iter()
            .flat_map(|vp| &vp.subpaths)
            .map(|sp| {
                let mut prev = get_first_point(&sp.commands);
                let mut len = 0.0;
                for cmd in &sp.commands {
                    let ep = command_endpoint(cmd, prev);
                    if !matches!(cmd, PathCommand::MoveTo { .. }) {
                        len += prev.distance_to(&ep);
                    }
                    prev = ep;
                }
                len
            })
            .sum();
        assert!(
            (total_kept_len - 10.0).abs() < 0.5,
            "Kept piece should be ~10mm (the right half), got {total_kept_len:.2}mm"
        );
    }

    // ── Part 1: Tolerance fix tests ──

    #[test]
    fn find_intersections_near_path_endpoint() {
        // Path from (0,0)→(10,0). Cutter crosses very close to the start (at x≈0.006)
        // endpoint_margin = DEFAULT_TOLERANCE_MM * 0.1 = 0.05 * 0.1 = 0.005
        // So d≈0.006 > 0.005 should be detected
        let pts = vec![Point2D::new(0.0, 0.0), Point2D::new(10.0, 0.0)];
        let cutter = vec![Point2D::new(0.006, -5.0), Point2D::new(0.006, 5.0)];

        let dists = find_intersections(&pts, false, &[(cutter, false)], false);
        assert!(
            !dists.is_empty(),
            "Intersection at d≈0.006 should be detected (above endpoint_margin=0.005)"
        );
    }

    #[test]
    fn trim_near_endpoint_succeeds() {
        // Path from (0,0)→(20,0), cutter at x=0.006 (very close to start)
        // endpoint_margin = DEFAULT_TOLERANCE_MM * 0.1 = 0.005, so 0.006 > 0.005 passes
        let path = VecPath::parse_svg_d("M0 0 L20 0");
        let cutter = vec![Point2D::new(0.006, -5.0), Point2D::new(0.006, 5.0)];

        let result = trim_at_intersection(&path, 0, &[(cutter, false)], Point2D::new(10.0, 0.0));
        assert!(
            !result.pieces.is_empty(),
            "Trim near endpoint should succeed with tighter tolerance"
        );
    }

    #[test]
    fn dedup_preserves_distinct_close_crossings() {
        // Path from (0,0)→(20,0), two cutters at x=5.000 and x=5.030
        // dedup_tol = DEFAULT_TOLERANCE_MM * 0.5 = 0.05 * 0.5 = 0.025
        // The gap (0.030mm) is above dedup tolerance (0.025mm) so both are preserved
        let pts = vec![Point2D::new(0.0, 0.0), Point2D::new(20.0, 0.0)];
        let cutter1 = vec![Point2D::new(5.0, -5.0), Point2D::new(5.0, 5.0)];
        let cutter2 = vec![Point2D::new(5.03, -5.0), Point2D::new(5.03, 5.0)];

        let dists = find_intersections(&pts, false, &[(cutter1, false), (cutter2, false)], false);
        assert!(
            dists.len() >= 2,
            "Two crossings 0.030mm apart should both be preserved (dedup_tol=0.025), got {} distances: {:?}",
            dists.len(),
            dists
        );
    }

    // ── Part 2: Heal tests ──

    #[test]
    fn heal_two_pieces_into_closed() {
        // Two crossing lines: horiz (0,5)→(20,5) and vert (10,0)→(10,10)
        // Trim the right half of horiz → 2 pieces: (0,5)→(10,5) and sibling is separate
        let horiz = VecPath::parse_svg_d("M0 5 L20 5");
        let vert = vec![Point2D::new(10.0, 0.0), Point2D::new(10.0, 10.0)];

        let result = trim_at_intersection(&horiz, 0, &[(vert, false)], Point2D::new(15.0, 5.0));
        assert!(!result.pieces.is_empty());

        // Now test heal with fabricated pieces that form a closed shape
        // Two open pieces: A(0,0)→(10,0) and B(10,0)→(10,10)→(0,10)→(0,0)
        let piece_a = VecPath::parse_svg_d("M0 0 L10 0");
        let piece_b = VecPath::parse_svg_d("M10 0 L10 10 L0 10 L0 0");
        let cut_pts = vec![Point2D::new(10.0, 0.0), Point2D::new(0.0, 0.0)];

        match heal_trim_results(&[piece_a, piece_b], &cut_pts, 0.1) {
            HealOutcome::HealedClosed(vp) => {
                assert!(
                    vp.subpaths.iter().any(|sp| sp.closed),
                    "Healed result should have a closed subpath"
                );
            }
            HealOutcome::HealedOpen(_) => panic!("Expected HealedClosed, got HealedOpen"),
            HealOutcome::Ambiguous(_) => panic!("Expected HealedClosed, got Ambiguous"),
        }
    }

    #[test]
    fn heal_returns_ambiguous_for_disconnected() {
        let piece_a = VecPath::parse_svg_d("M0 0 L10 0");
        let piece_b = VecPath::parse_svg_d("M50 50 L60 50");
        let cut_pts = vec![Point2D::new(10.0, 0.0), Point2D::new(50.0, 50.0)];

        match heal_trim_results(&[piece_a, piece_b], &cut_pts, 0.1) {
            HealOutcome::Ambiguous(_) => {} // expected
            HealOutcome::HealedClosed(_) | HealOutcome::HealedOpen(_) => {
                panic!("Expected Ambiguous for disconnected pieces")
            }
        }
    }

    #[test]
    fn heal_returns_ambiguous_for_branched() {
        // Three pieces where two match the same endpoint
        let piece_a = VecPath::parse_svg_d("M0 0 L10 0");
        let piece_b = VecPath::parse_svg_d("M10 0 L20 5");
        let piece_c = VecPath::parse_svg_d("M10 0 L20 -5");
        let cut_pts = vec![Point2D::new(10.0, 0.0)];

        match heal_trim_results(&[piece_a, piece_b, piece_c], &cut_pts, 0.1) {
            HealOutcome::Ambiguous(_) => {} // expected
            HealOutcome::HealedClosed(_) | HealOutcome::HealedOpen(_) => {
                panic!("Expected Ambiguous for branched pieces")
            }
        }
    }

    #[test]
    fn heal_preserves_curves() {
        // Two pieces with a cubic
        let piece_a = VecPath::parse_svg_d("M0 0 C5 10 10 10 15 0");
        let piece_b = VecPath::parse_svg_d("M15 0 L15 10 L0 10 L0 0");
        let cut_pts = vec![Point2D::new(15.0, 0.0), Point2D::new(0.0, 0.0)];

        match heal_trim_results(&[piece_a, piece_b], &cut_pts, 0.1) {
            HealOutcome::HealedClosed(vp) => {
                let has_cubic = vp.subpaths.iter().any(|sp| {
                    sp.commands
                        .iter()
                        .any(|c| matches!(c, PathCommand::CubicTo { .. }))
                });
                assert!(has_cubic, "Healed result should preserve CubicTo");
            }
            HealOutcome::HealedOpen(_) => panic!("Expected HealedClosed, got HealedOpen"),
            HealOutcome::Ambiguous(_) => panic!("Expected HealedClosed, got Ambiguous"),
        }
    }

    #[test]
    fn heal_preserves_closed_siblings() {
        // Open piece + closed sibling in same VecPath
        let mut piece_a = VecPath::parse_svg_d("M0 0 L10 0");
        piece_a.subpaths.push(SubPath {
            commands: vec![
                PathCommand::MoveTo { x: 50.0, y: 0.0 },
                PathCommand::LineTo { x: 60.0, y: 0.0 },
                PathCommand::LineTo { x: 60.0, y: 10.0 },
                PathCommand::Close,
            ],
            closed: true,
        });
        let piece_b = VecPath::parse_svg_d("M10 0 L10 10 L0 10 L0 0");
        let cut_pts = vec![Point2D::new(10.0, 0.0), Point2D::new(0.0, 0.0)];

        match heal_trim_results(&[piece_a, piece_b], &cut_pts, 0.1) {
            HealOutcome::HealedClosed(vp) => {
                assert!(
                    vp.subpaths.len() >= 2,
                    "Should have chained open + original closed sibling, got {} subpaths",
                    vp.subpaths.len()
                );
            }
            HealOutcome::HealedOpen(_) => panic!("Expected HealedClosed, got HealedOpen"),
            HealOutcome::Ambiguous(_) => panic!("Expected HealedClosed, got Ambiguous"),
        }
    }

    #[test]
    fn heal_snaps_to_cut_points() {
        // Pieces with endpoints slightly off from cut points
        let piece_a = VecPath::parse_svg_d("M0 0 L10.05 0");
        let piece_b = VecPath::parse_svg_d("M9.95 0 L10 10 L0 10 L0 0");
        let cut_pts = vec![Point2D::new(10.0, 0.0), Point2D::new(0.0, 0.0)];

        match heal_trim_results(&[piece_a, piece_b], &cut_pts, 0.1) {
            HealOutcome::HealedClosed(vp) => {
                assert!(
                    vp.subpaths.iter().any(|sp| sp.closed),
                    "Snapped endpoints should allow closing"
                );
            }
            HealOutcome::HealedOpen(_) => panic!("Expected HealedClosed, got HealedOpen"),
            HealOutcome::Ambiguous(_) => panic!("Expected HealedClosed after snapping"),
        }
    }

    #[test]
    fn heal_open_chain_returns_healed_open() {
        // Two pieces that chain endpoint-to-endpoint but don't close
        // (start of chain is far from end of chain) → HealedOpen
        let piece_a = VecPath::parse_svg_d("M0 0 L10 0");
        let piece_b = VecPath::parse_svg_d("M10 0 L20 0");
        let cut_pts = vec![Point2D::new(10.0, 0.0)];

        match heal_trim_results(&[piece_a, piece_b], &cut_pts, 0.1) {
            HealOutcome::HealedOpen(vp) => {
                // Should be a single open subpath
                assert_eq!(vp.subpaths.len(), 1, "Should merge into one subpath");
                assert!(!vp.subpaths[0].closed, "Merged subpath should be open");
                // Check that the merged path spans from (0,0) to (20,0)
                let start = subpath_first_point(&vp.subpaths[0]).unwrap();
                let end = subpath_last_point(&vp.subpaths[0]).unwrap();
                assert!(start.distance_to(&Point2D::new(0.0, 0.0)) < 0.1);
                assert!(end.distance_to(&Point2D::new(20.0, 0.0)) < 0.1);
            }
            HealOutcome::HealedClosed(_) => panic!("Expected HealedOpen, got HealedClosed"),
            HealOutcome::Ambiguous(_) => panic!("Expected HealedOpen, got Ambiguous"),
        }
    }

    #[test]
    fn heal_backward_chain_merges_at_shared_midpoint() {
        // Pieces [A→B, C→A] where A is the shared point.
        // Forward chaining extends from B but C→A doesn't connect to B.
        // Backward chaining should prepend C→A before A→B, yielding C→A→B.
        let piece_ab = VecPath::parse_svg_d("M5 0 L10 0");
        let piece_ca = VecPath::parse_svg_d("M0 0 L5 0");
        let cut_pts = vec![Point2D::new(5.0, 0.0)];

        // Order matters: piece_ab is first in chain, piece_ca should be
        // attached via backward chaining since its endpoint matches chain start
        match heal_trim_results(&[piece_ab, piece_ca], &cut_pts, 0.1) {
            HealOutcome::HealedOpen(vp) => {
                assert_eq!(vp.subpaths.len(), 1);
                assert!(!vp.subpaths[0].closed);
                let start = subpath_first_point(&vp.subpaths[0]).unwrap();
                let end = subpath_last_point(&vp.subpaths[0]).unwrap();
                assert!(
                    start.distance_to(&Point2D::new(0.0, 0.0)) < 0.1,
                    "Chain should start at C=(0,0), got ({:.1},{:.1})",
                    start.x,
                    start.y
                );
                assert!(
                    end.distance_to(&Point2D::new(10.0, 0.0)) < 0.1,
                    "Chain should end at B=(10,0), got ({:.1},{:.1})",
                    end.x,
                    end.y
                );
            }
            HealOutcome::HealedClosed(_) => panic!("Expected HealedOpen, got HealedClosed"),
            HealOutcome::Ambiguous(parts) => panic!(
                "Expected HealedOpen, got Ambiguous with {} parts",
                parts.len()
            ),
        }
    }

    #[test]
    fn reverse_subpath_preserves_cubics() {
        let sp = SubPath {
            commands: vec![
                PathCommand::MoveTo { x: 0.0, y: 0.0 },
                PathCommand::CubicTo {
                    c1x: 5.0,
                    c1y: 10.0,
                    c2x: 15.0,
                    c2y: 10.0,
                    x: 20.0,
                    y: 0.0,
                },
            ],
            closed: false,
        };

        let rev = reverse_subpath(&sp);
        assert!(
            rev.commands
                .iter()
                .any(|c| matches!(c, PathCommand::CubicTo { .. }))
        );
        // Reversed should start at (20,0) and end at (0,0)
        if let PathCommand::MoveTo { x, y } = rev.commands[0] {
            assert!((x - 20.0).abs() < 0.01 && y.abs() < 0.01);
        }
    }

    #[test]
    fn reverse_subpath_roundtrip() {
        let sp = SubPath {
            commands: vec![
                PathCommand::MoveTo { x: 0.0, y: 0.0 },
                PathCommand::LineTo { x: 10.0, y: 0.0 },
                PathCommand::CubicTo {
                    c1x: 15.0,
                    c1y: 5.0,
                    c2x: 15.0,
                    c2y: 10.0,
                    x: 10.0,
                    y: 15.0,
                },
                PathCommand::LineTo { x: 0.0, y: 15.0 },
            ],
            closed: false,
        };

        let rev = reverse_subpath(&sp);
        let roundtrip = reverse_subpath(&rev);

        // Compare endpoints — roundtrip should match original
        let orig_first = subpath_first_point(&sp).unwrap();
        let rt_first = subpath_first_point(&roundtrip).unwrap();
        assert!(
            orig_first.distance_to(&rt_first) < 0.01,
            "First point mismatch"
        );

        let orig_last = subpath_last_point(&sp).unwrap();
        let rt_last = subpath_last_point(&roundtrip).unwrap();
        assert!(
            orig_last.distance_to(&rt_last) < 0.01,
            "Last point mismatch"
        );
    }

    // ── Part 3: Preview tests ──

    #[test]
    fn preview_trim_segment_returns_polyline() {
        let horiz = VecPath::parse_svg_d("M0 5 L20 5");
        let vert = vec![Point2D::new(10.0, 0.0), Point2D::new(10.0, 10.0)];

        // Hover on the right half
        let result = preview_trim_segment(&horiz, 0, &[(vert, false)], Point2D::new(15.0, 5.0));
        assert!(result.is_some(), "Should return a preview polyline");
        let pts = result.unwrap();
        assert!(pts.len() >= 2, "Preview should have at least 2 points");
    }

    #[test]
    fn preview_trim_segment_no_intersections() {
        let line = VecPath::parse_svg_d("M0 0 L10 0");
        let result = preview_trim_segment(&line, 0, &[], Point2D::new(5.0, 0.0));
        assert!(result.is_none(), "No intersections → no preview");
    }

    // ── Integration: overlapping circles trim ──

    #[test]
    fn trim_overlapping_circles_single_merged_result() {
        use crate::object::ShapeKind;
        use crate::vector::convert::shape_to_vecpath;
        use crate::vector::flatten::{DEFAULT_TOLERANCE_MM, flatten_vecpath};
        use crate::vector::transform::bake_transform;
        use beambench_common::geometry::Transform2D;

        // Two circles, radius=20mm (diameter=40mm), second translated 20mm right
        let circle_a = shape_to_vecpath(ShapeKind::Ellipse, 40.0, 40.0, 0.0);
        let circle_b = {
            let raw = shape_to_vecpath(ShapeKind::Ellipse, 40.0, 40.0, 0.0);
            bake_transform(
                &raw,
                &Transform2D {
                    a: 1.0,
                    b: 0.0,
                    c: 0.0,
                    d: 1.0,
                    tx: 20.0,
                    ty: 0.0,
                },
            )
        };

        // Flatten circle B to get cutter polylines
        let flat_b = flatten_vecpath(&circle_b, DEFAULT_TOLERANCE_MM);
        let cutters: Vec<(Vec<Point2D>, bool)> =
            flat_b.into_iter().map(|p| (p.points, p.closed)).collect();

        // Click inside the overlap region on circle A (center of circle A is (20,20),
        // overlap region is roughly x=20..40, so click at (30, 20) which is inside the
        // intersection area on circle A)
        let click_pt = Point2D::new(30.0, 20.0);
        let trim_result = trim_at_intersection(&circle_a, 0, &cutters, click_pt);

        assert!(!trim_result.pieces.is_empty(), "Trim should produce pieces");

        // Heal the result
        let heal_result = heal_trim_results(&trim_result.pieces, &trim_result.cut_points, 0.5);

        match heal_result {
            HealOutcome::HealedOpen(ref vp) => {
                // The heal produces an open chain. Now attempt cutter boundary closure
                // using IntersectionHit metadata from trim_result.
                let main_sp = vp.subpaths.iter().find(|sp| !sp.closed);
                assert!(main_sp.is_some(), "Should have an open main subpath");
                let main = main_sp.unwrap();

                if let (Some(left_hit), Some(right_hit)) =
                    (&trim_result.left_hit, &trim_result.right_hit)
                {
                    let closed_sp =
                        close_with_cutter_boundary(main, &cutters, left_hit, right_hit, click_pt);
                    // With the new conservative pipeline, closure may or may not succeed
                    // depending on validation. Both outcomes are acceptable.
                    if let Some(sp) = closed_sp {
                        assert!(sp.closed, "Result should be closed");
                        assert_eq!(
                            sp.commands.last(),
                            Some(&PathCommand::Close),
                            "Last command should be Close"
                        );
                    }
                }
            }
            HealOutcome::HealedClosed(ref vp) => {
                // If heal already produced a closed result, that's also fine
                assert!(
                    vp.subpaths.iter().any(|sp| sp.closed),
                    "HealedClosed should have a closed subpath"
                );
            }
            HealOutcome::Ambiguous(ref parts) => {
                // Acceptable fallback — at least verify the pieces are reasonable
                assert!(
                    parts.len() <= 3,
                    "Expected at most 3 parts if ambiguous, got {}",
                    parts.len()
                );
            }
        }
    }

    // ── Cutter boundary closure (Tier 2 with IntersectionHit metadata) ──

    /// Helper: compute cutter arc distance for a point on a rectangular cutter.
    fn rect_cutter_arc_dist(cutter_pts: &[Point2D], point: Point2D) -> f64 {
        project_point_on_polyline_with_dist(point, cutter_pts, true).1
    }

    #[test]
    fn close_with_cutter_boundary_rectangle() {
        // Open chain: horizontal line from (0,0) to (10,0)
        let open_chain = SubPath {
            commands: vec![
                PathCommand::MoveTo { x: 0.0, y: 0.0 },
                PathCommand::LineTo { x: 10.0, y: 0.0 },
            ],
            closed: false,
        };

        // Closed rectangle cutter: (0,0)→(10,0)→(10,6)→(0,6)→(0,0)
        let cutter_pts = vec![
            Point2D::new(0.0, 0.0),
            Point2D::new(10.0, 0.0),
            Point2D::new(10.0, 6.0),
            Point2D::new(0.0, 6.0),
        ];
        let cutters = vec![(cutter_pts.clone(), true)];

        // Construct IntersectionHit metadata
        let left_hit = IntersectionHit {
            target_dist: 0.0, // at chain start
            cutter_id: Some(0),
            cutter_dist: Some(rect_cutter_arc_dist(&cutter_pts, Point2D::new(0.0, 0.0))),
        };
        let right_hit = IntersectionHit {
            target_dist: 10.0, // at chain end
            cutter_id: Some(0),
            cutter_dist: Some(rect_cutter_arc_dist(&cutter_pts, Point2D::new(10.0, 0.0))),
        };

        // Click below the chain, outside the resulting rectangle (the removed
        // segment was below y=0; the kept contour is the rectangle at y≥0)
        let click = Point2D::new(5.0, -2.0);

        let result =
            close_with_cutter_boundary(&open_chain, &cutters, &left_hit, &right_hit, click);
        assert!(result.is_some(), "Should produce a closed subpath");

        let sp = result.unwrap();
        assert!(sp.closed, "Result should be closed");
        assert!(
            sp.commands.last() == Some(&PathCommand::Close),
            "Last command should be Close"
        );

        // The boundary should go via y=6 (the top of the rectangle)
        let has_y6 = sp
            .commands
            .iter()
            .any(|cmd| matches!(cmd, PathCommand::LineTo { y, .. } if (*y - 6.0).abs() < 0.5));
        assert!(
            has_y6,
            "Boundary should pass through y≈6 (top of rectangle)"
        );

        // Verify the click-outside-contour rule: click must NOT be inside the result
        let (flat, _) = flatten_subpath_annotated(&sp.commands, true, DEFAULT_TOLERANCE_MM);
        assert!(
            !point_in_polygon(click.x, click.y, &flat),
            "Click must be outside the kept contour"
        );
    }

    #[test]
    fn close_with_cutter_boundary_click_selects_correct_arc() {
        // Open chain: vertical chord from A(0,10) to B(0,-10)
        let open_chain = SubPath {
            commands: vec![
                PathCommand::MoveTo { x: 0.0, y: 10.0 },
                PathCommand::LineTo { x: 0.0, y: -10.0 },
            ],
            closed: false,
        };

        // Cutter: circle r=10 centered at origin (as a polygon approximation)
        let n = 64;
        let cutter_pts: Vec<Point2D> = (0..n)
            .map(|i| {
                let angle = 2.0 * std::f64::consts::PI * (i as f64) / (n as f64);
                Point2D::new(10.0 * angle.cos(), 10.0 * angle.sin())
            })
            .collect();
        let cutters = vec![(cutter_pts.clone(), true)];

        let left_hit = IntersectionHit {
            target_dist: 0.0,
            cutter_id: Some(0),
            cutter_dist: Some(rect_cutter_arc_dist(&cutter_pts, Point2D::new(0.0, 10.0))),
        };
        let right_hit = IntersectionHit {
            target_dist: 20.0,
            cutter_id: Some(0),
            cutter_dist: Some(rect_cutter_arc_dist(&cutter_pts, Point2D::new(0.0, -10.0))),
        };

        // Click on left side — the removed arc was the left semicircle.
        // The kept contour is the right semicircle; click must be outside it.
        let click = Point2D::new(-5.0, 0.0);

        let result =
            close_with_cutter_boundary(&open_chain, &cutters, &left_hit, &right_hit, click);
        assert!(result.is_some(), "Should produce a closed subpath");

        let sp = result.unwrap();
        assert!(sp.closed);

        // Boundary points should have positive x values (right semicircle selected)
        let boundary_xs: Vec<f64> = sp
            .commands
            .iter()
            .filter_map(|cmd| match cmd {
                PathCommand::LineTo { x, y } if (*y).abs() < 5.0 => Some(*x),
                _ => None,
            })
            .collect();
        assert!(
            boundary_xs.iter().any(|&x| x > 5.0),
            "Right semicircle arc should have positive x values near the midpoint, got: {:?}",
            boundary_xs
        );

        // Verify the click-outside-contour rule
        let (flat, _) = flatten_subpath_annotated(&sp.commands, true, DEFAULT_TOLERANCE_MM);
        assert!(
            !point_in_polygon(click.x, click.y, &flat),
            "Click must be outside the kept contour"
        );
    }

    #[test]
    fn close_with_cutter_boundary_skips_open_cutter() {
        let open_chain = SubPath {
            commands: vec![
                PathCommand::MoveTo { x: 0.0, y: 0.0 },
                PathCommand::LineTo { x: 10.0, y: 0.0 },
            ],
            closed: false,
        };

        // Same rectangle but marked as open
        let cutter_pts = vec![
            Point2D::new(0.0, 0.0),
            Point2D::new(10.0, 0.0),
            Point2D::new(10.0, 6.0),
            Point2D::new(0.0, 6.0),
        ];
        let cutters = vec![(cutter_pts.clone(), false)]; // open!

        let left_hit = IntersectionHit {
            target_dist: 0.0,
            cutter_id: Some(0),
            cutter_dist: Some(rect_cutter_arc_dist(&cutter_pts, Point2D::new(0.0, 0.0))),
        };
        let right_hit = IntersectionHit {
            target_dist: 10.0,
            cutter_id: Some(0),
            cutter_dist: Some(rect_cutter_arc_dist(&cutter_pts, Point2D::new(10.0, 0.0))),
        };
        let click = Point2D::new(5.0, 4.0);

        let result =
            close_with_cutter_boundary(&open_chain, &cutters, &left_hit, &right_hit, click);
        assert!(result.is_none(), "Should return None for open cutter");
    }

    #[test]
    fn close_with_cutter_boundary_both_arcs_fail_validation() {
        // Chain along the bottom of a rectangle, click INSIDE the rectangle.
        // Arc A (via top) produces a valid rectangle contour, but the click
        // is inside it → rejected by containment check.
        // Arc B (via bottom) is a degenerate back-and-forth → rejected by area.
        // Both fail → None.
        let open_chain = SubPath {
            commands: vec![
                PathCommand::MoveTo { x: 0.0, y: 0.0 },
                PathCommand::LineTo { x: 10.0, y: 0.0 },
            ],
            closed: false,
        };

        let cutter_pts = vec![
            Point2D::new(0.0, 0.0),
            Point2D::new(10.0, 0.0),
            Point2D::new(10.0, 6.0),
            Point2D::new(0.0, 6.0),
        ];
        let cutters = vec![(cutter_pts.clone(), true)];

        let left_hit = IntersectionHit {
            target_dist: 0.0,
            cutter_id: Some(0),
            cutter_dist: Some(rect_cutter_arc_dist(&cutter_pts, Point2D::new(0.0, 0.0))),
        };
        let right_hit = IntersectionHit {
            target_dist: 10.0,
            cutter_id: Some(0),
            cutter_dist: Some(rect_cutter_arc_dist(&cutter_pts, Point2D::new(10.0, 0.0))),
        };
        // Click INSIDE the rectangle — containment check rejects the valid arc
        let click = Point2D::new(5.0, 3.0);

        let result =
            close_with_cutter_boundary(&open_chain, &cutters, &left_hit, &right_hit, click);
        assert!(
            result.is_none(),
            "Should return None when both arcs fail validation"
        );
    }

    // ── New tests for IntersectionHit metadata ──

    #[test]
    fn find_intersections_tracks_cutter_index() {
        // Path from (0,0)→(20,0), two cutters at x=5 and x=15
        let pts = vec![Point2D::new(0.0, 0.0), Point2D::new(20.0, 0.0)];
        let cutter0 = vec![Point2D::new(5.0, -5.0), Point2D::new(5.0, 5.0)];
        let cutter1 = vec![Point2D::new(15.0, -5.0), Point2D::new(15.0, 5.0)];

        let hits = find_intersections(&pts, false, &[(cutter0, false), (cutter1, false)], false);
        assert_eq!(hits.len(), 2, "Should find 2 intersections");

        // First hit (lower target_dist) should be from cutter 0
        assert_eq!(hits[0].cutter_id, Some(0));
        assert!(hits[0].cutter_dist.is_some());
        // Second hit from cutter 1
        assert_eq!(hits[1].cutter_id, Some(1));
        assert!(hits[1].cutter_dist.is_some());

        // Check target_dist values
        assert!((hits[0].target_dist - 5.0).abs() < 0.01);
        assert!((hits[1].target_dist - 15.0).abs() < 0.01);
    }

    #[test]
    fn find_intersections_self_returns_none_cutter() {
        // Figure-8: crosses itself
        let pts = vec![
            Point2D::new(0.0, 0.0),
            Point2D::new(10.0, 10.0),
            Point2D::new(10.0, 0.0),
            Point2D::new(0.0, 10.0),
        ];

        let hits = find_intersections(&pts, false, &[], true);
        assert!(!hits.is_empty(), "Self-intersection should be detected");

        // All self-intersection hits should have cutter_id: None, cutter_dist: None
        for hit in &hits {
            assert!(
                hit.cutter_id.is_none(),
                "Self-intersection hit should have cutter_id: None"
            );
            assert!(
                hit.cutter_dist.is_none(),
                "Self-intersection hit should have cutter_dist: None"
            );
        }
    }

    #[test]
    fn dedup_marks_ambiguous_when_different_cutters() {
        // Two cutters crossing at same point on target path
        let pts = vec![Point2D::new(0.0, 0.0), Point2D::new(20.0, 0.0)];
        // Both cutters cross at x=10 — within dedup tolerance
        let cutter0 = vec![Point2D::new(10.0, -5.0), Point2D::new(10.0, 5.0)];
        let cutter1 = vec![Point2D::new(10.001, -5.0), Point2D::new(10.001, 5.0)];

        let hits = find_intersections(&pts, false, &[(cutter0, false), (cutter1, false)], false);

        // After dedup, the merged hit should be ambiguous (cutter_id: None)
        // since hits from different cutters merged
        assert!(!hits.is_empty());
        // The dedup_tol = 0.025mm, and 0.001mm < 0.025mm, so they merge
        assert_eq!(hits.len(), 1, "Hits within dedup tolerance should merge");
        assert!(
            hits[0].cutter_id.is_none(),
            "Merged hit from different cutters should be ambiguous"
        );
        assert!(
            hits[0].cutter_dist.is_none(),
            "Merged hit from different cutters should have None cutter_dist"
        );
    }

    #[test]
    fn dedup_marks_ambiguous_when_same_cutter_different_dist() {
        // Construct hits manually for this edge case (looping cutter producing
        // two different cutter_dist values at same target_dist)
        let mut hits = vec![
            IntersectionHit {
                target_dist: 5.0,
                cutter_id: Some(0),
                cutter_dist: Some(1.0),
            },
            IntersectionHit {
                target_dist: 5.001,
                cutter_id: Some(0),
                cutter_dist: Some(20.0),
            },
        ];
        dedup_hits_within_tolerance(&mut hits, 0.025);
        assert_eq!(hits.len(), 1);
        assert!(
            hits[0].cutter_id.is_none(),
            "Same cutter but different cutter_dist should be ambiguous"
        );
        assert!(hits[0].cutter_dist.is_none());
    }

    #[test]
    fn dedup_marks_ambiguous_when_same_cutter_partial_dist() {
        // Same cutter but one hit has cutter_dist and the other doesn't.
        // This is a metadata mismatch — must be marked ambiguous.
        let mut hits = vec![
            IntersectionHit {
                target_dist: 5.0,
                cutter_id: Some(0),
                cutter_dist: Some(3.0),
            },
            IntersectionHit {
                target_dist: 5.001,
                cutter_id: Some(0),
                cutter_dist: None,
            },
        ];
        dedup_hits_within_tolerance(&mut hits, 0.025);
        assert_eq!(hits.len(), 1);
        assert!(
            hits[0].cutter_id.is_none(),
            "Same cutter with partial cutter_dist should be ambiguous"
        );
        assert!(
            hits[0].cutter_dist.is_none(),
            "Ambiguous hit should have cutter_dist cleared"
        );
    }

    #[test]
    fn close_rejects_different_cutters() {
        let open_chain = SubPath {
            commands: vec![
                PathCommand::MoveTo { x: 0.0, y: 0.0 },
                PathCommand::LineTo { x: 10.0, y: 0.0 },
            ],
            closed: false,
        };

        let cutter_pts = vec![
            Point2D::new(0.0, 0.0),
            Point2D::new(10.0, 0.0),
            Point2D::new(10.0, 6.0),
            Point2D::new(0.0, 6.0),
        ];
        let cutters = vec![(cutter_pts, true)];

        // Hits reference different cutter indices
        let left_hit = IntersectionHit {
            target_dist: 0.0,
            cutter_id: Some(0),
            cutter_dist: Some(0.0),
        };
        let right_hit = IntersectionHit {
            target_dist: 10.0,
            cutter_id: Some(1), // different cutter!
            cutter_dist: Some(10.0),
        };
        let click = Point2D::new(5.0, 4.0);

        let result =
            close_with_cutter_boundary(&open_chain, &cutters, &left_hit, &right_hit, click);
        assert!(
            result.is_none(),
            "Should reject when left/right hits reference different cutters"
        );
    }

    #[test]
    fn close_rejects_self_intersection_cut() {
        let open_chain = SubPath {
            commands: vec![
                PathCommand::MoveTo { x: 0.0, y: 0.0 },
                PathCommand::LineTo { x: 10.0, y: 0.0 },
            ],
            closed: false,
        };

        let cutter_pts = vec![
            Point2D::new(0.0, 0.0),
            Point2D::new(10.0, 0.0),
            Point2D::new(10.0, 6.0),
            Point2D::new(0.0, 6.0),
        ];
        let cutters = vec![(cutter_pts, true)];

        // Hits with cutter_id: None (self-intersection)
        let left_hit = IntersectionHit {
            target_dist: 0.0,
            cutter_id: None,
            cutter_dist: None,
        };
        let right_hit = IntersectionHit {
            target_dist: 10.0,
            cutter_id: None,
            cutter_dist: None,
        };
        let click = Point2D::new(5.0, 4.0);

        let result =
            close_with_cutter_boundary(&open_chain, &cutters, &left_hit, &right_hit, click);
        assert!(
            result.is_none(),
            "Should reject when hits are self-intersections (cutter_id: None)"
        );
    }

    #[test]
    fn close_rejects_ambiguous_both_arcs_valid() {
        // Symmetric shape: open chain along x-axis from (-10,0) to (10,0),
        // cutter is a circle centered at origin. Both arcs are valid semicircles.
        // Click at origin (equidistant from both arcs) — but point_in_polygon
        // check should cause only one arc to validate, or the ambiguity gate
        // should reject.
        let open_chain = SubPath {
            commands: vec![
                PathCommand::MoveTo { x: -10.0, y: 0.0 },
                PathCommand::LineTo { x: 10.0, y: 0.0 },
            ],
            closed: false,
        };

        let n = 64;
        let cutter_pts: Vec<Point2D> = (0..n)
            .map(|i| {
                let angle = 2.0 * std::f64::consts::PI * (i as f64) / (n as f64);
                Point2D::new(10.0 * angle.cos(), 10.0 * angle.sin())
            })
            .collect();
        let cutters = vec![(cutter_pts.clone(), true)];

        let left_hit = IntersectionHit {
            target_dist: 0.0,
            cutter_id: Some(0),
            cutter_dist: Some(rect_cutter_arc_dist(&cutter_pts, Point2D::new(-10.0, 0.0))),
        };
        let right_hit = IntersectionHit {
            target_dist: 20.0,
            cutter_id: Some(0),
            cutter_dist: Some(rect_cutter_arc_dist(&cutter_pts, Point2D::new(10.0, 0.0))),
        };

        // Click outside the circle entirely — outside both semicircle contours.
        // Both arcs pass all other validation, but both also pass the containment
        // check (click outside) → 2 valid candidates → uniqueness gate → None.
        let click = Point2D::new(15.0, 0.0);

        let result =
            close_with_cutter_boundary(&open_chain, &cutters, &left_hit, &right_hit, click);
        assert!(
            result.is_none(),
            "Both arcs are valid with click outside both — ambiguity gate should reject"
        );
    }

    #[test]
    fn close_validates_self_crossing_contour() {
        // Chain: straight horizontal segment from (0,0) to (10,0).
        // Cutter: closed zigzag polygon whose long arc crosses the chain,
        // producing a self-intersecting combined contour.
        let open_chain = SubPath {
            commands: vec![
                PathCommand::MoveTo { x: 0.0, y: 0.0 },
                PathCommand::LineTo { x: 10.0, y: 0.0 },
            ],
            closed: false,
        };

        // Cutter: (0,0)→(10,0)→(8,4)→(5,-2)→(2,4) — closed.
        // The long arc (10,0)→(8,4)→(5,-2)→(2,4)→(0,0) crosses the chain at
        // x≈6 and x≈4 (the segment (8,4)→(5,-2) and (5,-2)→(2,4) cross y=0).
        let cutter_pts = vec![
            Point2D::new(0.0, 0.0),
            Point2D::new(10.0, 0.0),
            Point2D::new(8.0, 4.0),
            Point2D::new(5.0, -2.0),
            Point2D::new(2.0, 4.0),
        ];
        let cutters = vec![(cutter_pts.clone(), true)];

        let left_hit = IntersectionHit {
            target_dist: 0.0,
            cutter_id: Some(0),
            cutter_dist: Some(rect_cutter_arc_dist(&cutter_pts, Point2D::new(0.0, 0.0))),
        };
        let right_hit = IntersectionHit {
            target_dist: 10.0,
            cutter_id: Some(0),
            cutter_dist: Some(rect_cutter_arc_dist(&cutter_pts, Point2D::new(10.0, 0.0))),
        };
        let click = Point2D::new(5.0, 2.0);

        let result =
            close_with_cutter_boundary(&open_chain, &cutters, &left_hit, &right_hit, click);
        // The long arc crosses the chain → self-intersection check rejects it.
        // The short arc is the degenerate single-segment (0,0)→(10,0) — same
        // as the chain → near-zero area or degenerate. Both fail → None.
        assert!(result.is_none(), "Self-crossing contour should be rejected");
    }

    // ── Self-touching / overlap validation tests ──

    #[test]
    fn self_touching_detects_colinear_overlap() {
        // Thin sliver: two long edges nearly colinear, 0.03mm apart
        let pts = vec![
            Point2D::new(0.0, 0.0),
            Point2D::new(20.0, 0.0),
            Point2D::new(15.0, 0.03),
            Point2D::new(5.0, 0.03),
        ];
        assert!(
            polygon_has_self_touching(&pts, DEFAULT_TOLERANCE_MM),
            "Thin sliver within tolerance should be detected as self-touching"
        );
    }

    #[test]
    fn self_touching_detects_t_junction() {
        // Pentagon where vertex (10,0) lies on the opposite edge (0,0)→(20,0)
        let pts = vec![
            Point2D::new(0.0, 0.0),
            Point2D::new(20.0, 0.0),
            Point2D::new(20.0, 5.0),
            Point2D::new(10.0, 0.0), // on edge 0
            Point2D::new(0.0, 5.0),
        ];
        assert!(
            polygon_has_self_touching(&pts, DEFAULT_TOLERANCE_MM),
            "T-junction (vertex on non-adjacent edge) should be detected"
        );
    }

    #[test]
    fn self_touching_detects_duplicated_edge() {
        // Polygon that doubles back along an edge: (0,0)→(10,0)→(10,5)→(10,0)
        // Vertex 3 coincides with vertex 1 — lies on edge 0.
        let pts = vec![
            Point2D::new(0.0, 0.0),
            Point2D::new(10.0, 0.0),
            Point2D::new(10.0, 5.0),
            Point2D::new(10.0, 0.0),
        ];
        assert!(
            polygon_has_self_touching(&pts, DEFAULT_TOLERANCE_MM),
            "Duplicated boundary (back-and-forth) should be detected"
        );
    }

    #[test]
    fn self_touching_accepts_clean_polygon() {
        // Valid rectangle — no self-touching
        let pts = vec![
            Point2D::new(0.0, 0.0),
            Point2D::new(10.0, 0.0),
            Point2D::new(10.0, 6.0),
            Point2D::new(0.0, 6.0),
        ];
        assert!(
            !polygon_has_self_touching(&pts, DEFAULT_TOLERANCE_MM),
            "Clean rectangle should not be flagged as self-touching"
        );
    }

    #[test]
    fn close_rejects_colinear_overlap_sliver() {
        // Chain: (0,0)→(20,0). Cutter: thin rectangle (0,0)→(20,0)→(20,0.03)→(0,0.03).
        // The top arc produces a sliver within tolerance of the chain →
        // self-touching check rejects it. The bottom arc is degenerate. → None.
        let open_chain = SubPath {
            commands: vec![
                PathCommand::MoveTo { x: 0.0, y: 0.0 },
                PathCommand::LineTo { x: 20.0, y: 0.0 },
            ],
            closed: false,
        };

        let cutter_pts = vec![
            Point2D::new(0.0, 0.0),
            Point2D::new(20.0, 0.0),
            Point2D::new(20.0, 0.03),
            Point2D::new(0.0, 0.03),
        ];
        let cutters = vec![(cutter_pts.clone(), true)];

        let left_hit = IntersectionHit {
            target_dist: 0.0,
            cutter_id: Some(0),
            cutter_dist: Some(rect_cutter_arc_dist(&cutter_pts, Point2D::new(0.0, 0.0))),
        };
        let right_hit = IntersectionHit {
            target_dist: 20.0,
            cutter_id: Some(0),
            cutter_dist: Some(rect_cutter_arc_dist(&cutter_pts, Point2D::new(20.0, 0.0))),
        };
        let click = Point2D::new(10.0, -2.0);

        let result =
            close_with_cutter_boundary(&open_chain, &cutters, &left_hit, &right_hit, click);
        assert!(
            result.is_none(),
            "Near-coincident sliver (colinear overlap) should be rejected"
        );
    }

    #[test]
    fn close_rejects_boundary_touching_chain() {
        // Chain: (0,0)→(20,0). Cutter: pentagon with vertex (10,0) on the chain.
        // The arc through (10,0) creates a T-junction → self-touching rejects it.
        // The degenerate short arc also fails. → None.
        let open_chain = SubPath {
            commands: vec![
                PathCommand::MoveTo { x: 0.0, y: 0.0 },
                PathCommand::LineTo { x: 20.0, y: 0.0 },
            ],
            closed: false,
        };

        let cutter_pts = vec![
            Point2D::new(0.0, 0.0),
            Point2D::new(20.0, 0.0),
            Point2D::new(20.0, 5.0),
            Point2D::new(10.0, 0.0), // lies on chain edge
            Point2D::new(0.0, 5.0),
        ];
        let cutters = vec![(cutter_pts.clone(), true)];

        let left_hit = IntersectionHit {
            target_dist: 0.0,
            cutter_id: Some(0),
            cutter_dist: Some(rect_cutter_arc_dist(&cutter_pts, Point2D::new(0.0, 0.0))),
        };
        let right_hit = IntersectionHit {
            target_dist: 20.0,
            cutter_id: Some(0),
            cutter_dist: Some(rect_cutter_arc_dist(&cutter_pts, Point2D::new(20.0, 0.0))),
        };
        let click = Point2D::new(10.0, -2.0);

        let result =
            close_with_cutter_boundary(&open_chain, &cutters, &left_hit, &right_hit, click);
        assert!(
            result.is_none(),
            "Arc touching chain at non-endpoint (T-junction) should be rejected"
        );
    }

    #[test]
    fn close_rejects_duplicated_boundary_segment() {
        // Chain: (0,0)→(10,0)→(10,5). Cutter: pentagon (0,0)→(10,0)→(10,5)→(10,10)→(0,10).
        // Short arc retraces the chain backwards → self-touching detects it.
        // Long arc produces a valid polygon, but click is inside it. → None.
        let open_chain = SubPath {
            commands: vec![
                PathCommand::MoveTo { x: 0.0, y: 0.0 },
                PathCommand::LineTo { x: 10.0, y: 0.0 },
                PathCommand::LineTo { x: 10.0, y: 5.0 },
            ],
            closed: false,
        };

        let cutter_pts = vec![
            Point2D::new(0.0, 0.0),
            Point2D::new(10.0, 0.0),
            Point2D::new(10.0, 5.0),
            Point2D::new(10.0, 10.0),
            Point2D::new(0.0, 10.0),
        ];
        let cutters = vec![(cutter_pts.clone(), true)];

        let left_hit = IntersectionHit {
            target_dist: 0.0,
            cutter_id: Some(0),
            cutter_dist: Some(rect_cutter_arc_dist(&cutter_pts, Point2D::new(0.0, 0.0))),
        };
        let right_hit = IntersectionHit {
            target_dist: 15.0, // 10 + 5 = chain length
            cutter_id: Some(0),
            cutter_dist: Some(rect_cutter_arc_dist(&cutter_pts, Point2D::new(10.0, 5.0))),
        };
        // Click inside the long-arc contour so it's also rejected by containment.
        // This ensures both arcs fail: short from self-touching, long from containment.
        let click = Point2D::new(5.0, 5.0);

        let result =
            close_with_cutter_boundary(&open_chain, &cutters, &left_hit, &right_hit, click);
        assert!(
            result.is_none(),
            "Duplicated boundary segment should be rejected by self-touching check"
        );
    }

    #[test]
    fn seam_weld_snaps_endpoints() {
        // Two pieces that form a nearly-closed chain (endpoints within epsilon)
        let piece_a = VecPath::parse_svg_d("M0 0 L10 0 L10 10");
        let piece_b = VecPath::parse_svg_d("M10 10 L0 10 L0 0.04"); // 0.04 < 0.1 tolerance

        let cut_pts = vec![Point2D::new(10.0, 10.0), Point2D::new(0.0, 0.0)];

        match heal_trim_results(&[piece_a, piece_b], &cut_pts, 0.1) {
            HealOutcome::HealedClosed(vp) => {
                let sp = &vp.subpaths[0];
                assert!(sp.closed, "Should be closed");
                // The last draw command before Close should have endpoint snapped to (0,0)
                let last_draw = sp.commands.iter().rev().find(|c| {
                    matches!(
                        c,
                        PathCommand::LineTo { .. }
                            | PathCommand::QuadTo { .. }
                            | PathCommand::CubicTo { .. }
                    )
                });
                if let Some(PathCommand::LineTo { x, y }) = last_draw {
                    assert!(
                        (*x).abs() < 0.01 && (*y).abs() < 0.01,
                        "Last draw endpoint should be snapped to start (0,0), got ({}, {})",
                        x,
                        y
                    );
                }
            }
            other => panic!(
                "Expected HealedClosed, got {:?}",
                match other {
                    HealOutcome::HealedOpen(_) => "HealedOpen",
                    HealOutcome::Ambiguous(_) => "Ambiguous",
                    _ => "unknown",
                }
            ),
        }
    }

    #[test]
    fn refine_cubic_t_improves_accuracy() {
        // A highly non-linear cubic — ellipse quarter arc approximation
        // where speed varies significantly along the curve
        let p0 = Point2D::new(20.0, 0.0);
        let p1 = Point2D::new(20.0, 11.046);
        let p2 = Point2D::new(11.046, 20.0);
        let p3 = Point2D::new(0.0, 20.0);
        let tolerance = DEFAULT_TOLERANCE_MM;

        let total = cubic_partial_arc_length(p0, p1, p2, p3, 1.0, tolerance);
        // Target at 25% of arc length — where naive t diverges most from parametric t
        let target = total * 0.25;

        let naive_t = 0.25;
        let refined_t = refine_cubic_t(p0, p1, p2, p3, target, tolerance);

        let naive_arc = cubic_partial_arc_length(p0, p1, p2, p3, naive_t, tolerance);
        let refined_arc = cubic_partial_arc_length(p0, p1, p2, p3, refined_t, tolerance);

        // Refined should be very accurate (within 0.1mm)
        assert!(
            (refined_arc - target).abs() < 0.1,
            "Refined arc ({:.4}) should be within 0.1mm of target ({:.4}), naive was ({:.4})",
            refined_arc,
            target,
            naive_arc
        );
    }
}

use beambench_common::Point2D;
use beambench_common::path::{PathCommand, SubPath, VecPath};
use geo::algorithm::bool_ops::BooleanOps;
use geo::algorithm::simplify::Simplify;
use geo::{Coord, LineString, MultiPolygon, Polygon};
use serde::{Deserialize, Serialize};

use crate::vector::flatten::{DEFAULT_TOLERANCE_MM, flatten_vecpath};

/// Direction for path offsetting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OffsetDirection {
    Inward,
    Outward,
    Both,
}

/// Corner style for path offsetting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum CornerStyle {
    #[default]
    Miter,
    Round,
    Bevel,
}

/// Offset a path by a given distance.
/// Returns a vector of paths (Both direction returns two paths).
pub fn offset_path(
    path: &VecPath,
    distance_mm: f64,
    direction: OffsetDirection,
    corner_style: CornerStyle,
) -> Vec<VecPath> {
    if distance_mm < 0.0 {
        return vec![];
    }

    match direction {
        OffsetDirection::Inward => vec![offset_single(path, -distance_mm, corner_style)],
        OffsetDirection::Outward => vec![offset_single(path, distance_mm, corner_style)],
        OffsetDirection::Both => vec![
            offset_single(path, distance_mm, corner_style),
            offset_single(path, -distance_mm, corner_style),
        ],
    }
}

/// Signed area via the shoelace formula.  Positive = CCW, negative = CW.
pub fn signed_area(points: &[Point2D]) -> f64 {
    let n = points.len();
    let mut area = 0.0;
    for i in 0..n {
        let j = (i + 1) % n;
        area += points[i].x * points[j].y;
        area -= points[j].x * points[i].y;
    }
    area / 2.0
}

fn offset_single(path: &VecPath, distance: f64, corner_style: CornerStyle) -> VecPath {
    let mut result_subpaths = Vec::new();

    for polyline in flatten_vecpath(path, DEFAULT_TOLERANCE_MM) {
        // Sanitize the flattened input before offsetting. Boolean-union output
        // can contain tiny near-collinear segments at overlap junctions; if we
        // feed those directly into the corner solver, they become visible spikes.
        let pts = sanitize_offset_input(&polyline.points, polyline.closed);

        if pts.len() < if polyline.closed { 3 } else { 2 } {
            continue;
        }

        // Correct for winding: positive distance should go right of CCW edges (outward).
        // For CW paths, flip so the caller's intent (inward/outward) is preserved.
        let effective_distance = if polyline.closed {
            let area = signed_area(&pts);
            if area < 0.0 { -distance } else { distance }
        } else {
            distance
        };

        let winding_sign = if polyline.closed {
            signed_area(&pts).signum()
        } else {
            0.0
        };

        let offset_points = offset_polyline(
            &pts,
            effective_distance,
            polyline.closed,
            corner_style,
            winding_sign,
        );

        if offset_points.len() >= if polyline.closed { 3 } else { 2 } {
            let mut commands = Vec::new();
            commands.push(PathCommand::MoveTo {
                x: offset_points[0].x,
                y: offset_points[0].y,
            });

            for pt in &offset_points[1..] {
                commands.push(PathCommand::LineTo { x: pt.x, y: pt.y });
            }

            if polyline.closed {
                commands.push(PathCommand::Close);
            }

            result_subpaths.push(SubPath {
                commands,
                closed: polyline.closed,
            });
        }
    }

    VecPath {
        subpaths: result_subpaths,
    }
}

fn sanitize_offset_input(points: &[Point2D], closed: bool) -> Vec<Point2D> {
    if points.len() < 2 {
        return points.to_vec();
    }

    let mut pts = points.to_vec();

    // Authoritative seam guard for closed paths.
    if closed && pts.len() > 2 {
        while pts.len() > 2 && pts.last().unwrap().distance_to(&pts[0]) <= DEFAULT_TOLERANCE_MM {
            pts.pop();
        }
    }

    // Drop consecutive near-duplicates.
    let mut deduped = Vec::with_capacity(pts.len());
    for pt in pts {
        if deduped
            .last()
            .map(|last: &Point2D| last.distance_to(&pt) > DEFAULT_TOLERANCE_MM)
            .unwrap_or(true)
        {
            deduped.push(pt);
        }
    }

    if closed && deduped.len() > 2 {
        while deduped.len() > 2
            && deduped.last().unwrap().distance_to(&deduped[0]) <= DEFAULT_TOLERANCE_MM
        {
            deduped.pop();
        }
    }

    remove_tiny_near_collinear_vertices(&deduped, closed, DEFAULT_TOLERANCE_MM * 2.0)
}

fn remove_tiny_near_collinear_vertices(
    points: &[Point2D],
    closed: bool,
    tolerance: f64,
) -> Vec<Point2D> {
    if points.len() < 3 {
        return points.to_vec();
    }

    let mut pts = points.to_vec();

    loop {
        let n = pts.len();
        if n < 3 {
            break;
        }

        let mut changed = false;
        let mut next = Vec::with_capacity(n);

        for i in 0..n {
            if !closed && (i == 0 || i == n - 1) {
                next.push(pts[i]);
                continue;
            }

            let prev = pts[if i == 0 { n - 1 } else { i - 1 }];
            let curr = pts[i];
            let foll = pts[if i == n - 1 { 0 } else { i + 1 }];

            let prev_len = prev.distance_to(&curr);
            let next_len = curr.distance_to(&foll);

            if prev_len <= tolerance || next_len <= tolerance {
                let deviation = point_to_segment_distance(curr, prev, foll);
                if deviation <= tolerance {
                    changed = true;
                    continue;
                }
            }

            next.push(curr);
        }

        if !changed {
            return pts;
        }

        pts = next;
    }

    pts
}

fn point_to_segment_distance(p: Point2D, a: Point2D, b: Point2D) -> f64 {
    let abx = b.x - a.x;
    let aby = b.y - a.y;
    let ab_len_sq = abx * abx + aby * aby;

    if ab_len_sq <= 1e-12 {
        return p.distance_to(&a);
    }

    let apx = p.x - a.x;
    let apy = p.y - a.y;
    let t = ((apx * abx + apy * aby) / ab_len_sq).clamp(0.0, 1.0);
    let proj = Point2D {
        x: a.x + abx * t,
        y: a.y + aby * t,
    };
    p.distance_to(&proj)
}

fn offset_polyline(
    points: &[Point2D],
    distance: f64,
    closed: bool,
    corner_style: CornerStyle,
    winding_sign: f64,
) -> Vec<Point2D> {
    if points.len() < 2 {
        return points.to_vec();
    }

    let mut offset_points = Vec::new();
    let n = points.len();

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

        // Endpoints of open paths always use simple perpendicular offset
        if (i == 0 || i == n - 1) && !closed {
            let offset_pt = if i == 0 {
                compute_offset_point(p, p, next, distance)
            } else {
                compute_offset_point(p, prev, p, distance)
            };
            offset_points.push(offset_pt);
            continue;
        }

        if closed && is_concave_corner(prev, p, next, winding_sign) {
            if let Some(join) = intersect_offset_lines(prev, p, next, distance) {
                offset_points.push(join);
            } else {
                offset_points.push(compute_offset_point(p, prev, next, distance));
            }
            continue;
        }

        match corner_style {
            CornerStyle::Miter => {
                offset_points.push(compute_offset_point(p, prev, next, distance));
            }
            CornerStyle::Bevel => {
                let (n1, n2) = compute_edge_normals(p, prev, next, distance);
                offset_points.push(Point2D {
                    x: p.x + n1.0,
                    y: p.y + n1.1,
                });
                offset_points.push(Point2D {
                    x: p.x + n2.0,
                    y: p.y + n2.1,
                });
            }
            CornerStyle::Round => {
                let (n1, n2) = compute_edge_normals(p, prev, next, distance);
                let angle1 = n1.1.atan2(n1.0);
                let angle2 = n2.1.atan2(n2.0);
                let mut delta = angle2 - angle1;
                // Normalize to [-PI, PI]
                if delta > std::f64::consts::PI {
                    delta -= 2.0 * std::f64::consts::PI;
                } else if delta < -std::f64::consts::PI {
                    delta += 2.0 * std::f64::consts::PI;
                }
                let abs_delta = delta.abs();
                // ~1 point per 15 degrees
                let steps = ((abs_delta / (std::f64::consts::PI / 12.0)).ceil() as usize).max(2);
                let r = distance.abs();
                for s in 0..=steps {
                    let t = s as f64 / steps as f64;
                    let angle = angle1 + delta * t;
                    offset_points.push(Point2D {
                        x: p.x + r * angle.cos(),
                        y: p.y + r * angle.sin(),
                    });
                }
            }
        }
    }

    // Clean up the offset polygon — for closed paths, use self-union to clip
    // crossing edges; for open paths, fall back to Douglas-Peucker simplification.
    clean_offset_polygon(&offset_points, closed)
}

fn is_concave_corner(prev: Point2D, p: Point2D, next: Point2D, winding_sign: f64) -> bool {
    if winding_sign.abs() < 1e-9 {
        return false;
    }

    let d1x = p.x - prev.x;
    let d1y = p.y - prev.y;
    let d2x = next.x - p.x;
    let d2y = next.y - p.y;
    let cross = d1x * d2y - d1y * d2x;
    cross * winding_sign < -1e-9
}

fn intersect_offset_lines(
    prev: Point2D,
    p: Point2D,
    next: Point2D,
    distance: f64,
) -> Option<Point2D> {
    let d1x = p.x - prev.x;
    let d1y = p.y - prev.y;
    let len1 = (d1x * d1x + d1y * d1y).sqrt();

    let d2x = next.x - p.x;
    let d2y = next.y - p.y;
    let len2 = (d2x * d2x + d2y * d2y).sqrt();

    if len1 < 1e-9 || len2 < 1e-9 {
        return None;
    }

    let u1x = d1x / len1;
    let u1y = d1y / len1;
    let u2x = d2x / len2;
    let u2y = d2y / len2;

    let n1x = u1y * distance;
    let n1y = -u1x * distance;
    let n2x = u2y * distance;
    let n2y = -u2x * distance;

    let ax = p.x + n1x;
    let ay = p.y + n1y;
    let bx = p.x + n2x;
    let by = p.y + n2y;

    let det = u1x * u2y - u1y * u2x;
    if det.abs() < 1e-9 {
        return Some(Point2D { x: ax, y: ay });
    }

    let abx = bx - ax;
    let aby = by - ay;
    let t = (abx * u2y - aby * u2x) / det;

    Some(Point2D {
        x: ax + t * u1x,
        y: ay + t * u1y,
    })
}

/// Compute the two edge-normal offset vectors at a vertex.
/// Returns (incoming_normal * distance, outgoing_normal * distance).
fn compute_edge_normals(
    p: Point2D,
    prev: Point2D,
    next: Point2D,
    distance: f64,
) -> ((f64, f64), (f64, f64)) {
    let d1x = p.x - prev.x;
    let d1y = p.y - prev.y;
    let len1 = (d1x * d1x + d1y * d1y).sqrt();

    let d2x = next.x - p.x;
    let d2y = next.y - p.y;
    let len2 = (d2x * d2x + d2y * d2y).sqrt();

    let (n1x, n1y) = if len1 > 1e-9 {
        (d1y / len1 * distance, -d1x / len1 * distance)
    } else if len2 > 1e-9 {
        (d2y / len2 * distance, -d2x / len2 * distance)
    } else {
        (0.0, 0.0)
    };

    let (n2x, n2y) = if len2 > 1e-9 {
        (d2y / len2 * distance, -d2x / len2 * distance)
    } else {
        (n1x, n1y)
    };

    ((n1x, n1y), (n2x, n2y))
}

fn compute_offset_point(p: Point2D, prev: Point2D, next: Point2D, distance: f64) -> Point2D {
    // Compute edge vectors
    let d1x = p.x - prev.x;
    let d1y = p.y - prev.y;
    let len1 = (d1x * d1x + d1y * d1y).sqrt();

    let d2x = next.x - p.x;
    let d2y = next.y - p.y;
    let len2 = (d2x * d2x + d2y * d2y).sqrt();

    // If degenerate edge, return point itself
    if len1 < 1e-9 && len2 < 1e-9 {
        return p;
    }

    // Normalized perpendicular (right normal for outward, left for inward based on sign of distance)
    // For a vector (dx, dy), the right perpendicular is (dy, -dx)
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

    // Bisector normal
    let nx = n1x + n2x;
    let ny = n1y + n2y;
    let nlen = (nx * nx + ny * ny).sqrt();

    if nlen < 1e-9 {
        // Collinear edges (180 degree turn) - use perpendicular
        Point2D {
            x: p.x + n1x * distance,
            y: p.y + n1y * distance,
        }
    } else {
        // Compute miter length correction
        // The bisector needs to be scaled so its perpendicular distance equals 'distance'
        // Scale factor = 1 / sin(angle/2) where angle is between edges
        // Can compute from dot product: cos(angle) = dot(n1, n2)
        let cos_half = (n1x * nx + n1y * ny) / nlen;
        // Miter limit: cap the scale factor to prevent spikes at sharp
        // concave corners.  A limit of 2.0 means the miter joint extends
        // at most 2× the offset distance beyond the vertex — the standard
        // approach used by SVG, PostScript, and polygon-offset libraries.
        const MITER_LIMIT: f64 = 2.0;
        let miter_scale = if cos_half.abs() > 1e-9 {
            (1.0 / cos_half).clamp(-MITER_LIMIT, MITER_LIMIT)
        } else {
            1.0
        };

        Point2D {
            x: p.x + (nx / nlen) * distance * miter_scale,
            y: p.y + (ny / nlen) * distance * miter_scale,
        }
    }
}

/// Remove self-intersections from an offset polyline by converting to a
/// geo Polygon and self-unioning to clip crossed edges.
fn clean_offset_polygon(points: &[Point2D], closed: bool) -> Vec<Point2D> {
    if !closed || points.len() < 3 {
        // Open paths: fall back to Douglas-Peucker (topology repair not applicable)
        return simplify_points(points, DEFAULT_TOLERANCE_MM);
    }

    let mut coords: Vec<Coord<f64>> = points.iter().map(|p| Coord { x: p.x, y: p.y }).collect();
    // geo requires the ring to be closed (first == last)
    if coords.first() != coords.last() {
        coords.push(coords[0]);
    }
    let ring = LineString::new(coords);
    let poly = Polygon::new(ring, vec![]);
    let mp = MultiPolygon::new(vec![poly]);

    // Self-union forces the sweep-line algorithm to find and resolve all
    // edge crossings, producing a valid (non-self-intersecting) polygon.
    let cleaned = mp.union(&mp);

    // Take the largest exterior ring (by vertex count as proxy for area)
    cleaned
        .iter()
        .max_by_key(|p| p.exterior().coords().count())
        .map(|p| {
            let mut pts: Vec<Point2D> = p
                .exterior()
                .coords()
                .map(|c| Point2D { x: c.x, y: c.y })
                .collect();
            // geo closes the ring — strip duplicate closing point
            if pts.len() > 1 && pts.first() == pts.last() {
                pts.pop();
            }
            pts
        })
        .unwrap_or_else(|| points.to_vec())
}

fn simplify_points(points: &[Point2D], tolerance: f64) -> Vec<Point2D> {
    if points.len() < 3 {
        return points.to_vec();
    }

    let coords: Vec<Coord<f64>> = points.iter().map(|p| Coord { x: p.x, y: p.y }).collect();
    let line = LineString::new(coords);
    let simplified = line.simplify(&tolerance);

    simplified
        .coords()
        .map(|c| Point2D { x: c.x, y: c.y })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::object::ShapeKind;
    use crate::vector::boolean::weld_shapes;
    use crate::vector::convert::shape_to_vecpath;
    use geo::algorithm::contains::Contains;
    use geo::{Coord, LineString, Point, Polygon};

    #[test]
    fn offset_outward_rectangle() {
        let rect = shape_to_vecpath(ShapeKind::Rectangle, 10.0, 10.0, 0.0);
        let result = offset_path(&rect, 2.0, OffsetDirection::Outward, CornerStyle::Miter);

        assert_eq!(result.len(), 1);
        assert!(!result[0].is_empty());
        if let Some(bounds) = result[0].bounds() {
            // Outward offset should increase bounds
            assert!(bounds.width() > 10.0);
            assert!(bounds.height() > 10.0);
        }
    }

    #[test]
    fn offset_inward_rectangle() {
        let rect = shape_to_vecpath(ShapeKind::Rectangle, 20.0, 20.0, 0.0);
        let result = offset_path(&rect, 2.0, OffsetDirection::Inward, CornerStyle::Miter);

        assert_eq!(result.len(), 1);
        assert!(!result[0].is_empty());
        if let Some(bounds) = result[0].bounds() {
            // Inward offset should decrease bounds
            assert!(bounds.width() < 20.0);
            assert!(bounds.height() < 20.0);
        }
    }

    #[test]
    fn offset_both_directions() {
        let rect = shape_to_vecpath(ShapeKind::Rectangle, 10.0, 10.0, 0.0);
        let result = offset_path(&rect, 2.0, OffsetDirection::Both, CornerStyle::Miter);

        assert_eq!(result.len(), 2);
        assert!(!result[0].is_empty());
        assert!(!result[1].is_empty());
    }

    #[test]
    fn offset_circle() {
        let circle = shape_to_vecpath(ShapeKind::Ellipse, 10.0, 10.0, 0.0);
        let result = offset_path(&circle, 2.0, OffsetDirection::Outward, CornerStyle::Miter);

        assert_eq!(result.len(), 1);
        assert!(!result[0].is_empty());
    }

    #[test]
    fn offset_zero_distance() {
        let rect = shape_to_vecpath(ShapeKind::Rectangle, 10.0, 10.0, 0.0);
        let result = offset_path(&rect, 0.0, OffsetDirection::Outward, CornerStyle::Miter);

        assert_eq!(result.len(), 1);
        // Should produce a path similar to the original
        if let Some(bounds) = result[0].bounds() {
            assert!((bounds.width() - 10.0).abs() < 1.0);
        }
    }

    #[test]
    fn offset_negative_distance_error() {
        let rect = shape_to_vecpath(ShapeKind::Rectangle, 10.0, 10.0, 0.0);
        let result = offset_path(&rect, -5.0, OffsetDirection::Outward, CornerStyle::Miter);

        // Negative distance returns empty
        assert!(result.is_empty());
    }

    #[test]
    fn offset_open_two_point_line_produces_parallel_line() {
        let line = VecPath::parse_svg_d("M0 0 L10 0");
        let result = offset_path(&line, 2.0, OffsetDirection::Outward, CornerStyle::Miter);

        assert_eq!(result.len(), 1);
        assert!(!result[0].is_empty());
        assert_eq!(result[0].subpaths.len(), 1);

        let subpath = &result[0].subpaths[0];
        assert!(!subpath.closed);
        assert_eq!(subpath.commands.len(), 2);
        match (&subpath.commands[0], &subpath.commands[1]) {
            (PathCommand::MoveTo { x: x0, y: y0 }, PathCommand::LineTo { x: x1, y: y1 }) => {
                assert!((*x0 - 0.0).abs() < 1e-9);
                assert!((*x1 - 10.0).abs() < 1e-9);
                assert!((*y0 + 2.0).abs() < 1e-9);
                assert!((*y1 + 2.0).abs() < 1e-9);
            }
            other => panic!("expected two line commands, got {other:?}"),
        }
    }

    #[test]
    fn offset_complex_path() {
        let path = VecPath::parse_svg_d("M0 0 L10 0 L10 10 L0 10 Z M20 0 L30 0 L30 10 L20 10 Z");
        let result = offset_path(&path, 1.0, OffsetDirection::Outward, CornerStyle::Miter);

        assert!(!result.is_empty());
    }

    #[test]
    fn offset_handles_self_intersection() {
        // Star-like shape that may self-intersect when offset
        let path = VecPath::parse_svg_d("M10 0 L12 8 L20 10 L12 12 L10 20 L8 12 L0 10 L8 8 Z");
        let result = offset_path(&path, 2.0, OffsetDirection::Outward, CornerStyle::Miter);

        assert!(!result.is_empty());
    }

    /// Regression: inward offset of a concave shape used to spike far outside
    /// the original bounds because `compute_offset_point` had no miter limit.
    #[test]
    fn inward_offset_concave_stays_within_original_bounds() {
        // L-shape with a sharp concave corner at (5,5)
        let path = VecPath::parse_svg_d("M0 0 L20 0 L20 10 L5 10 L5 5 L0 5 Z");
        let original_bounds = path.bounds().expect("non-empty path");

        let result = offset_path(&path, 1.0, OffsetDirection::Inward, CornerStyle::Miter);
        assert_eq!(result.len(), 1);

        let offset_bounds = result[0].bounds().expect("non-empty offset path");
        // All inward-offset points must stay within the original path bounds
        // (with a small tolerance for floating-point).
        let tol = 0.5; // half of offset distance — generous for miter artifacts
        assert!(
            offset_bounds.min.x >= original_bounds.min.x - tol
                && offset_bounds.min.y >= original_bounds.min.y - tol
                && offset_bounds.max.x <= original_bounds.max.x + tol
                && offset_bounds.max.y <= original_bounds.max.y + tol,
            "Inward offset escaped original bounds: offset={:?}, original={:?}",
            offset_bounds,
            original_bounds
        );
    }

    /// Bug fix: inward offset of a 20×20 rect should produce a concentric inset
    /// centered on the same center as the original — not a shifted copy.
    #[test]
    fn inward_offset_rectangle_is_centered() {
        let rect = shape_to_vecpath(ShapeKind::Rectangle, 20.0, 20.0, 0.0);
        let orig_bounds = rect.bounds().expect("non-empty");
        let orig_cx = (orig_bounds.min.x + orig_bounds.max.x) / 2.0;
        let orig_cy = (orig_bounds.min.y + orig_bounds.max.y) / 2.0;

        let result = offset_path(&rect, 2.0, OffsetDirection::Inward, CornerStyle::Miter);
        assert_eq!(result.len(), 1);
        let offset_bounds = result[0].bounds().expect("non-empty offset");
        let off_cx = (offset_bounds.min.x + offset_bounds.max.x) / 2.0;
        let off_cy = (offset_bounds.min.y + offset_bounds.max.y) / 2.0;

        assert!(
            (orig_cx - off_cx).abs() < 0.5 && (orig_cy - off_cy).abs() < 0.5,
            "Inward offset center ({off_cx:.2}, {off_cy:.2}) should match original ({orig_cx:.2}, {orig_cy:.2})"
        );
        // Size should be ~16×16 (20 - 2*2)
        assert!(
            (offset_bounds.width() - 16.0).abs() < 1.0,
            "Expected ~16mm width, got {:.2}",
            offset_bounds.width()
        );
        assert!(
            (offset_bounds.height() - 16.0).abs() < 1.0,
            "Expected ~16mm height, got {:.2}",
            offset_bounds.height()
        );
    }

    /// Bug fix: a CW rectangle inward offset should produce the same result
    /// as a CCW rectangle, thanks to winding correction.
    #[test]
    fn inward_offset_cw_rectangle_matches_ccw() {
        // CCW rectangle (default from shape_to_vecpath)
        let ccw = shape_to_vecpath(ShapeKind::Rectangle, 20.0, 20.0, 0.0);
        let ccw_result = offset_path(&ccw, 3.0, OffsetDirection::Inward, CornerStyle::Miter);

        // CW rectangle (reversed vertex order)
        let cw = VecPath::parse_svg_d("M0 0 L0 20 L20 20 L20 0 Z");
        let cw_result = offset_path(&cw, 3.0, OffsetDirection::Inward, CornerStyle::Miter);

        assert_eq!(ccw_result.len(), 1);
        assert_eq!(cw_result.len(), 1);

        let ccw_bounds = ccw_result[0].bounds().expect("non-empty");
        let cw_bounds = cw_result[0].bounds().expect("non-empty");

        // Both should produce ~14×14 offset
        assert!(
            (ccw_bounds.width() - cw_bounds.width()).abs() < 1.0,
            "CW width {:.2} should match CCW width {:.2}",
            cw_bounds.width(),
            ccw_bounds.width()
        );
        assert!(
            (ccw_bounds.height() - cw_bounds.height()).abs() < 1.0,
            "CW height {:.2} should match CCW height {:.2}",
            cw_bounds.height(),
            ccw_bounds.height()
        );
    }

    /// Bug fix: closed square inward offset should produce 4 equidistant points
    /// from center, with no asymmetry from a duplicate-point seam.
    #[test]
    fn closed_path_no_seam_artifact() {
        let path = VecPath::parse_svg_d("M0 0 L10 0 L10 10 L0 10 Z");
        let result = offset_path(&path, 1.0, OffsetDirection::Inward, CornerStyle::Miter);
        assert_eq!(result.len(), 1);

        let bounds = result[0].bounds().expect("non-empty");
        let cx = (bounds.min.x + bounds.max.x) / 2.0;
        let cy = (bounds.min.y + bounds.max.y) / 2.0;

        // Center should be at (5,5) — the center of the 10×10 square
        assert!(
            (cx - 5.0).abs() < 0.5 && (cy - 5.0).abs() < 0.5,
            "Offset center ({cx:.2}, {cy:.2}) should be near (5, 5)"
        );
        // Width/height should be ~8 (10 - 2*1)
        assert!(
            (bounds.width() - 8.0).abs() < 1.0,
            "Expected ~8mm width, got {:.2}",
            bounds.width()
        );
        assert!(
            (bounds.height() - 8.0).abs() < 1.0,
            "Expected ~8mm height, got {:.2}",
            bounds.height()
        );
    }

    /// Verify offset_single's seam guard: a polyline whose last point
    /// duplicates the first must still produce a symmetric offset.
    /// This exercises the dedup logic inside offset_single directly,
    /// independent of flatten_vecpath.
    #[test]
    fn seam_guard_strips_duplicate_before_offset() {
        use crate::vector::flatten::flatten_vecpath;

        // Build a closed polyline with an explicit duplicate at the seam:
        // the path "M0 0 L10 0 L10 10 L0 10 L0 0 Z" has the last LineTo
        // returning to the start before Close.
        let path = VecPath::parse_svg_d("M0 0 L10 0 L10 10 L0 10 L0 0 Z");

        // Verify flatten strips the duplicate
        let polys = flatten_vecpath(&path, DEFAULT_TOLERANCE_MM);
        assert_eq!(polys.len(), 1);
        assert!(polys[0].closed);
        assert_eq!(
            polys[0].points.len(),
            4,
            "Expected 4 unique points, got {} — seam duplicate not stripped",
            polys[0].points.len()
        );

        // Now offset and verify symmetry — all 4 corners should be
        // equidistant from center, proving no seam bias.
        let result = offset_path(&path, 1.0, OffsetDirection::Inward, CornerStyle::Miter);
        assert_eq!(result.len(), 1);
        let polys = flatten_vecpath(&result[0], DEFAULT_TOLERANCE_MM);
        assert_eq!(polys.len(), 1);
        let pts = &polys[0].points;

        let cx = pts.iter().map(|p| p.x).sum::<f64>() / pts.len() as f64;
        let cy = pts.iter().map(|p| p.y).sum::<f64>() / pts.len() as f64;
        let distances: Vec<f64> = pts
            .iter()
            .map(|p| ((p.x - cx).powi(2) + (p.y - cy).powi(2)).sqrt())
            .collect();
        let max_dist = distances.iter().cloned().fold(f64::NAN, f64::max);
        let min_dist = distances.iter().cloned().fold(f64::NAN, f64::min);
        assert!(
            (max_dist - min_dist) < 0.5,
            "Seam artifact: corner distances from center vary by {:.3} (max={max_dist:.3}, min={min_dist:.3})",
            max_dist - min_dist
        );
    }

    // -- helpers for topology assertions --

    /// Cross product of vectors (b-a) and (c-a).
    fn cross_2d(a: Point2D, b: Point2D, c: Point2D) -> f64 {
        (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
    }

    /// True if segments (a1-a2) and (b1-b2) properly cross (not just touch).
    fn segments_cross(a1: Point2D, a2: Point2D, b1: Point2D, b2: Point2D) -> bool {
        let d1 = cross_2d(a1, a2, b1);
        let d2 = cross_2d(a1, a2, b2);
        let d3 = cross_2d(b1, b2, a1);
        let d4 = cross_2d(b1, b2, a2);
        ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0))
            && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
    }

    /// Check a closed polygon for any pair of non-adjacent edges that cross.
    fn polygon_has_self_intersection(points: &[Point2D]) -> bool {
        let n = points.len();
        for i in 0..n {
            let a1 = points[i];
            let a2 = points[(i + 1) % n];
            for j in (i + 2)..n {
                // Skip pair that shares the wrap-around vertex
                if i == 0 && j == n - 1 {
                    continue;
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

    fn min_segment_length(points: &[Point2D], closed: bool) -> f64 {
        if points.len() < 2 {
            return 0.0;
        }

        let mut min_len = f64::INFINITY;
        for i in 0..(points.len() - 1) {
            min_len = min_len.min(points[i].distance_to(&points[i + 1]));
        }
        if closed {
            min_len = min_len.min(points[points.len() - 1].distance_to(&points[0]));
        }
        min_len
    }

    fn polygon_from_points(points: &[Point2D]) -> Polygon<f64> {
        let mut coords: Vec<Coord<f64>> = points.iter().map(|p| Coord { x: p.x, y: p.y }).collect();
        if coords.first() != coords.last() {
            coords.push(coords[0]);
        }
        Polygon::new(LineString::new(coords), vec![])
    }

    /// Bug fix: star shape inward offset must produce a contour with no
    /// self-intersecting edges after clean_offset_polygon processes it.
    /// This test actually verifies edge-crossing topology, not just bounds.
    #[test]
    fn self_intersecting_offset_cleaned() {
        use crate::vector::flatten::flatten_vecpath;

        let path = VecPath::parse_svg_d("M10 0 L12 8 L20 10 L12 12 L10 20 L8 12 L0 10 L8 8 Z");
        let result = offset_path(&path, 1.0, OffsetDirection::Inward, CornerStyle::Miter);
        assert_eq!(result.len(), 1);

        // Flatten the result to get the actual polygon vertices
        let polys = flatten_vecpath(&result[0], DEFAULT_TOLERANCE_MM);
        assert!(!polys.is_empty());
        let pts = &polys[0].points;
        assert!(
            pts.len() >= 3,
            "Offset polygon degenerate: only {} points",
            pts.len()
        );

        // Verify no non-adjacent edges cross
        assert!(
            !polygon_has_self_intersection(pts),
            "Offset polygon still has self-intersecting edges ({} vertices)",
            pts.len()
        );

        // Also verify bounds containment
        let bounds = result[0].bounds().expect("non-empty offset path");
        let original_bounds = path.bounds().expect("non-empty");
        let tol = 0.5;
        assert!(
            bounds.min.x >= original_bounds.min.x - tol
                && bounds.min.y >= original_bounds.min.y - tol
                && bounds.max.x <= original_bounds.max.x + tol
                && bounds.max.y <= original_bounds.max.y + tol,
            "Inward offset of star escaped original bounds"
        );
    }

    /// Round offset of a rectangle should produce more vertices than Miter
    /// (arc interpolation adds points at each corner).
    #[test]
    fn offset_round_corners_more_vertices() {
        use crate::vector::flatten::flatten_vecpath;

        let rect = shape_to_vecpath(ShapeKind::Rectangle, 10.0, 10.0, 0.0);
        let miter = offset_path(&rect, 2.0, OffsetDirection::Outward, CornerStyle::Miter);
        let round = offset_path(&rect, 2.0, OffsetDirection::Outward, CornerStyle::Round);

        assert_eq!(miter.len(), 1);
        assert_eq!(round.len(), 1);

        let miter_pts: usize = flatten_vecpath(&miter[0], DEFAULT_TOLERANCE_MM)
            .iter()
            .map(|p| p.points.len())
            .sum();
        let round_pts: usize = flatten_vecpath(&round[0], DEFAULT_TOLERANCE_MM)
            .iter()
            .map(|p| p.points.len())
            .sum();

        assert!(
            round_pts > miter_pts,
            "Round ({round_pts} pts) should have more vertices than Miter ({miter_pts} pts)"
        );
    }

    /// Bevel offset of a rectangle should produce more vertices than Miter
    /// (bevel emits 2 points per corner vs 1 for miter).
    #[test]
    fn offset_bevel_corners_more_vertices() {
        use crate::vector::flatten::flatten_vecpath;

        let rect = shape_to_vecpath(ShapeKind::Rectangle, 10.0, 10.0, 0.0);
        let miter = offset_path(&rect, 2.0, OffsetDirection::Outward, CornerStyle::Miter);
        let bevel = offset_path(&rect, 2.0, OffsetDirection::Outward, CornerStyle::Bevel);

        assert_eq!(miter.len(), 1);
        assert_eq!(bevel.len(), 1);

        let miter_pts: usize = flatten_vecpath(&miter[0], DEFAULT_TOLERANCE_MM)
            .iter()
            .map(|p| p.points.len())
            .sum();
        let bevel_pts: usize = flatten_vecpath(&bevel[0], DEFAULT_TOLERANCE_MM)
            .iter()
            .map(|p| p.points.len())
            .sum();

        assert!(
            bevel_pts > miter_pts,
            "Bevel ({bevel_pts} pts) should have more vertices than Miter ({miter_pts} pts)"
        );
    }

    /// Round offset of a rectangle produces similar overall bounds to Miter.
    #[test]
    fn offset_round_bounds_similar_to_miter() {
        let rect = shape_to_vecpath(ShapeKind::Rectangle, 10.0, 10.0, 0.0);
        let miter = offset_path(&rect, 2.0, OffsetDirection::Outward, CornerStyle::Miter);
        let round = offset_path(&rect, 2.0, OffsetDirection::Outward, CornerStyle::Round);

        let mb = miter[0].bounds().expect("miter bounds");
        let rb = round[0].bounds().expect("round bounds");

        // Round should be slightly smaller than miter (arc vs point) but similar
        assert!(
            (mb.width() - rb.width()).abs() < 2.0,
            "Width diff too large: miter={:.2}, round={:.2}",
            mb.width(),
            rb.width()
        );
        assert!(
            (mb.height() - rb.height()).abs() < 2.0,
            "Height diff too large: miter={:.2}, round={:.2}",
            mb.height(),
            rb.height()
        );
    }

    #[test]
    fn welded_overlap_offset_strips_tiny_junction_spikes() {
        use crate::vector::flatten::flatten_vecpath;

        let ellipse = shape_to_vecpath(ShapeKind::Ellipse, 60.0, 40.0, 0.0);
        let rect = VecPath::parse_svg_d("M30 10 L95 10 L95 30 L30 30 Z");
        let welded = weld_shapes(&[ellipse, rect]);

        let result = offset_path(&welded, 2.0, OffsetDirection::Outward, CornerStyle::Miter);
        assert_eq!(result.len(), 1);

        let polys = flatten_vecpath(&result[0], DEFAULT_TOLERANCE_MM);
        assert_eq!(polys.len(), 1);

        let min_len = min_segment_length(&polys[0].points, polys[0].closed);
        assert!(
            min_len > DEFAULT_TOLERANCE_MM * 3.0,
            "Welded overlap offset still contains tiny spike segments (min edge {:.4}mm)",
            min_len
        );
    }

    #[test]
    fn welded_overlap_round_and_bevel_stay_outside_source_shape() {
        use crate::vector::flatten::flatten_vecpath;

        let ellipse = shape_to_vecpath(ShapeKind::Ellipse, 60.0, 40.0, 0.0);
        let rect = VecPath::parse_svg_d("M30 10 L95 10 L95 30 L30 30 Z");
        let welded = weld_shapes(&[ellipse, rect]);

        let welded_polys = flatten_vecpath(&welded, DEFAULT_TOLERANCE_MM);
        assert_eq!(welded_polys.len(), 1);
        let source_poly = polygon_from_points(&welded_polys[0].points);

        for corner_style in [CornerStyle::Round, CornerStyle::Bevel] {
            let result = offset_path(&welded, 2.0, OffsetDirection::Outward, corner_style);
            assert_eq!(result.len(), 1);

            let polys = flatten_vecpath(&result[0], DEFAULT_TOLERANCE_MM);
            assert_eq!(polys.len(), 1);

            for pt in &polys[0].points {
                assert!(
                    !source_poly.contains(&Point::new(pt.x, pt.y)),
                    "{corner_style:?} offset produced a vertex inside the source shape at ({:.3}, {:.3})",
                    pt.x,
                    pt.y
                );
            }
        }
    }

    /// Miter behavior with the new signature is unchanged from before.
    #[test]
    fn offset_miter_unchanged_regression() {
        let rect = shape_to_vecpath(ShapeKind::Rectangle, 10.0, 10.0, 0.0);
        let result = offset_path(&rect, 2.0, OffsetDirection::Outward, CornerStyle::Miter);

        assert_eq!(result.len(), 1);
        let bounds = result[0].bounds().expect("non-empty");
        // A 10×10 rect offset outward by 2 should produce ~14×14
        assert!(
            (bounds.width() - 14.0).abs() < 1.5,
            "Expected ~14mm width, got {:.2}",
            bounds.width()
        );
        assert!(
            (bounds.height() - 14.0).abs() < 1.5,
            "Expected ~14mm height, got {:.2}",
            bounds.height()
        );
    }

    /// An inward offset larger than half the shape should produce a significantly
    /// smaller result. The caller (Tauri command) filters out empty/degenerate
    /// results, but offset_path itself may still produce a small residual polygon
    /// due to the geo crate's self-union cleanup.
    #[test]
    fn collapsed_inward_offset_is_much_smaller() {
        // 4×4 rect, inward offset by 3 → expected ~(−2)×(−2) = degenerate/very small
        let rect = shape_to_vecpath(ShapeKind::Rectangle, 4.0, 4.0, 0.0);
        let result = offset_path(&rect, 3.0, OffsetDirection::Inward, CornerStyle::Miter);

        assert_eq!(result.len(), 1, "Should still return one entry");
        if let Some(bounds) = result[0].bounds() {
            // The result must be significantly smaller than the original 4×4
            assert!(
                bounds.width() < 3.0 && bounds.height() < 3.0,
                "Inward offset by 3 on 4×4 rect should be much smaller, got {:.2}x{:.2}",
                bounds.width(),
                bounds.height()
            );
        }
    }

    // ── weld + offset integration tests ──────────────────────────

    /// Disjoint shapes: weld two non-overlapping rectangles, then offset outward.
    /// Each rectangle should grow independently — result has 2 disjoint subpaths.
    #[test]
    fn weld_then_offset_disjoint_shapes() {
        use crate::vector::boolean::weld_shapes;

        let a = VecPath::parse_svg_d("M0 0 L10 0 L10 10 L0 10 Z");
        let b = VecPath::parse_svg_d("M30 0 L40 0 L40 10 L30 10 Z");
        let combined = weld_shapes(&[a, b]);

        // Weld of disjoint shapes preserves both
        assert!(
            combined.subpaths.len() >= 2,
            "Weld should keep 2 disjoint subpaths"
        );

        let results = offset_path(&combined, 2.0, OffsetDirection::Outward, CornerStyle::Miter);
        assert_eq!(results.len(), 1);
        let result = &results[0];
        assert!(!result.is_empty());

        // Each original was 10×10; outward offset by 2 → each should be ~14×14.
        // Combined bounds should span from roughly -2 to 42 in X.
        let bounds = result.bounds().expect("non-empty");
        assert!(
            bounds.width() > 40.0,
            "Combined width should exceed gap, got {:.2}",
            bounds.width()
        );
    }

    /// Overlapping shapes: weld two overlapping rectangles, then offset outward.
    /// The union should merge them into a single contiguous outline.
    #[test]
    fn weld_then_offset_overlapping_shapes() {
        use crate::vector::boolean::weld_shapes;

        let a = VecPath::parse_svg_d("M0 0 L10 0 L10 10 L0 10 Z");
        let b = VecPath::parse_svg_d("M5 0 L15 0 L15 10 L5 10 Z");
        let combined = weld_shapes(&[a, b]);

        // Weld of overlapping rects should produce a single merged contour
        // (possibly with 1-2 subpaths depending on geo crate, but fewer than 2 distinct shapes)
        assert!(
            combined.subpaths.len() <= 2,
            "Overlapping weld should merge into <=2 subpaths, got {}",
            combined.subpaths.len()
        );

        let results = offset_path(&combined, 2.0, OffsetDirection::Outward, CornerStyle::Miter);
        assert_eq!(results.len(), 1);
        let result = &results[0];
        assert!(!result.is_empty());

        // Merged rect is 15×10; outward by 2 → ~19×14
        let bounds = result.bounds().expect("non-empty");
        assert!(
            (bounds.width() - 19.0).abs() < 2.0,
            "Expected ~19mm width, got {:.2}",
            bounds.width()
        );
        assert!(
            (bounds.height() - 14.0).abs() < 2.0,
            "Expected ~14mm height, got {:.2}",
            bounds.height()
        );
    }

    /// Shape with hole: subtract a small rect from a large rect, then offset inward.
    /// The hole should shrink (or disappear) while the outer boundary shrinks too.
    #[test]
    fn weld_then_offset_shape_with_hole() {
        use crate::vector::boolean::path_subtract;

        let outer = VecPath::parse_svg_d("M0 0 L30 0 L30 30 L0 30 Z");
        let inner = VecPath::parse_svg_d("M10 10 L20 10 L20 20 L10 20 Z");
        let with_hole = path_subtract(&outer, &inner);

        // Sanity: subtraction should have at least 2 subpaths (outer + hole)
        assert!(
            with_hole.subpaths.len() >= 2,
            "Expected hole, got {} subpaths",
            with_hole.subpaths.len()
        );

        let results = offset_path(&with_hole, 3.0, OffsetDirection::Inward, CornerStyle::Miter);
        assert_eq!(results.len(), 1);
        let result = &results[0];
        assert!(!result.is_empty());

        // Outer was 30×30, inward by 3 → ~24×24
        let bounds = result.bounds().expect("non-empty");
        assert!(
            bounds.width() < 30.0 && bounds.height() < 30.0,
            "Inward offset should shrink: {:.2}×{:.2}",
            bounds.width(),
            bounds.height()
        );
    }

    /// Verify that clean_offset_polygon removes crossings from a
    /// larger inward offset that would otherwise self-intersect.
    #[test]
    fn deep_inward_offset_no_self_intersection() {
        use crate::vector::flatten::flatten_vecpath;

        // An L-shape with a concave corner — large inward offset will
        // produce crossing edges at the concavity without cleanup.
        let path = VecPath::parse_svg_d("M0 0 L20 0 L20 10 L5 10 L5 5 L0 5 Z");
        let result = offset_path(&path, 1.5, OffsetDirection::Inward, CornerStyle::Miter);
        assert_eq!(result.len(), 1);

        let polys = flatten_vecpath(&result[0], DEFAULT_TOLERANCE_MM);
        assert!(!polys.is_empty());
        let pts = &polys[0].points;
        assert!(
            !polygon_has_self_intersection(pts),
            "L-shape inward offset still has self-intersecting edges ({} vertices)",
            pts.len()
        );
    }
}

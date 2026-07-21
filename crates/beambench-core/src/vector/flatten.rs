use beambench_common::geometry::Point2D;
use beambench_common::path::{PathCommand, Polyline, VecPath};

/// Default flattening tolerance in mm.
/// At typical laser resolution (~0.1mm), 0.05mm yields sub-pixel accuracy.
pub const DEFAULT_TOLERANCE_MM: f64 = 0.05;

/// Flatten a VecPath into polylines by converting all bezier curves
/// to line segments within the given tolerance.
pub fn flatten_vecpath(path: &VecPath, tolerance: f64) -> Vec<Polyline> {
    let mut polylines = Vec::new();

    for subpath in &path.subpaths {
        let mut points: Vec<Point2D> = Vec::new();
        let mut current = Point2D::zero();

        for cmd in &subpath.commands {
            match *cmd {
                PathCommand::MoveTo { x, y } => {
                    if points.len() > 1 {
                        polylines.push(Polyline::new(std::mem::take(&mut points), false));
                    } else {
                        points.clear();
                    }
                    current = Point2D::new(x, y);
                    points.push(current);
                }
                PathCommand::LineTo { x, y } => {
                    current = Point2D::new(x, y);
                    points.push(current);
                }
                PathCommand::QuadTo { cx, cy, x, y } => {
                    let cp = Point2D::new(cx, cy);
                    let end = Point2D::new(x, y);
                    flatten_quad(current, cp, end, tolerance, &mut points);
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
                    flatten_cubic(current, cp1, cp2, end, tolerance, &mut points);
                    current = end;
                }
                PathCommand::Close => {
                    // The `closed` flag on the Polyline tells consumers
                    // (offset_polyline, etc.) to wrap from the last point
                    // back to the first.  We do NOT push the start point —
                    // that would create a duplicate vertex with a zero-length
                    // edge at the seam.
                    //
                    // Strip duplicate if the path explicitly returned to
                    // the start via LineTo/CurveTo before Close.
                    if points.len() > 1
                        && points.last().unwrap().distance_to(&points[0]) <= tolerance
                    {
                        points.pop();
                    }
                }
            }
        }

        if points.len() > 1 {
            polylines.push(Polyline::new(points, subpath.closed));
        }
    }

    polylines
}

/// Recursively flatten a quadratic bezier curve using de Casteljau subdivision.
fn flatten_quad(p0: Point2D, p1: Point2D, p2: Point2D, tolerance: f64, points: &mut Vec<Point2D>) {
    // Check if the curve is flat enough: distance from control point to line p0-p2
    let flatness = point_to_line_distance(p1, p0, p2);

    if flatness <= tolerance {
        points.push(p2);
    } else {
        // Subdivide at t=0.5
        let m01 = p0.lerp(&p1, 0.5);
        let m12 = p1.lerp(&p2, 0.5);
        let mid = m01.lerp(&m12, 0.5);

        flatten_quad(p0, m01, mid, tolerance, points);
        flatten_quad(mid, m12, p2, tolerance, points);
    }
}

/// Recursively flatten a cubic bezier curve using de Casteljau subdivision.
fn flatten_cubic(
    p0: Point2D,
    p1: Point2D,
    p2: Point2D,
    p3: Point2D,
    tolerance: f64,
    points: &mut Vec<Point2D>,
) {
    // Check flatness: max distance of control points from line p0-p3
    let d1 = point_to_line_distance(p1, p0, p3);
    let d2 = point_to_line_distance(p2, p0, p3);
    let flatness = d1.max(d2);

    if flatness <= tolerance {
        points.push(p3);
    } else {
        // De Casteljau subdivision at t=0.5
        let m01 = p0.lerp(&p1, 0.5);
        let m12 = p1.lerp(&p2, 0.5);
        let m23 = p2.lerp(&p3, 0.5);
        let m012 = m01.lerp(&m12, 0.5);
        let m123 = m12.lerp(&m23, 0.5);
        let mid = m012.lerp(&m123, 0.5);

        flatten_cubic(p0, m01, m012, mid, tolerance, points);
        flatten_cubic(mid, m123, m23, p3, tolerance, points);
    }
}

/// Distance from point `p` to the line defined by `a` and `b`.
fn point_to_line_distance(p: Point2D, a: Point2D, b: Point2D) -> f64 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len_sq = dx * dx + dy * dy;

    if len_sq < 1e-20 {
        // Degenerate line (a ≈ b), just return distance to a
        return p.distance_to(&a);
    }

    let cross = (p.x - a.x) * dy - (p.y - a.y) * dx;
    cross.abs() / len_sq.sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flatten_line_path() {
        let path = VecPath::parse_svg_d("M0 0 L10 0 L10 10");
        let polys = flatten_vecpath(&path, DEFAULT_TOLERANCE_MM);
        assert_eq!(polys.len(), 1);
        assert_eq!(polys[0].points.len(), 3);
        assert!(!polys[0].closed);
    }

    #[test]
    fn flatten_closed_path() {
        let path = VecPath::parse_svg_d("M0 0 L10 0 L10 10 Z");
        let polys = flatten_vecpath(&path, DEFAULT_TOLERANCE_MM);
        assert_eq!(polys.len(), 1);
        assert!(polys[0].closed);
        assert!(polys[0].points.len() >= 3);
    }

    #[test]
    fn flatten_cubic_produces_polyline() {
        let path = VecPath::parse_svg_d("M0 0 C10 20 30 40 50 0");
        let polys = flatten_vecpath(&path, DEFAULT_TOLERANCE_MM);
        assert_eq!(polys.len(), 1);
        // Cubic should produce more points than just start+end
        assert!(
            polys[0].points.len() > 2,
            "Expected >2 points, got {}",
            polys[0].points.len()
        );
    }

    #[test]
    fn flatten_quad_produces_polyline() {
        let path = VecPath::parse_svg_d("M0 0 Q25 50 50 0");
        let polys = flatten_vecpath(&path, DEFAULT_TOLERANCE_MM);
        assert_eq!(polys.len(), 1);
        assert!(polys[0].points.len() > 2);
    }

    #[test]
    fn flatten_within_tolerance() {
        // A cubic curve should be approximated within tolerance
        let path = VecPath::parse_svg_d("M0 0 C0 100 100 100 100 0");
        let tolerance = 0.1;
        let polys = flatten_vecpath(&path, tolerance);
        assert_eq!(polys.len(), 1);

        // Start and end should be exact
        let pts = &polys[0].points;
        assert!((pts[0].x - 0.0).abs() < 1e-10);
        assert!((pts[0].y - 0.0).abs() < 1e-10);
        assert!((pts.last().unwrap().x - 100.0).abs() < 1e-10);
        assert!((pts.last().unwrap().y - 0.0).abs() < 1e-10);
    }

    #[test]
    fn tighter_tolerance_produces_more_points() {
        let path = VecPath::parse_svg_d("M0 0 C0 100 100 100 100 0");
        let loose = flatten_vecpath(&path, 1.0);
        let tight = flatten_vecpath(&path, 0.01);
        assert!(
            tight[0].points.len() > loose[0].points.len(),
            "Tighter tolerance should produce more points: {} vs {}",
            tight[0].points.len(),
            loose[0].points.len()
        );
    }

    #[test]
    fn flatten_ellipse_shape() {
        use crate::object::ShapeKind;
        use crate::vector::convert::shape_to_vecpath;

        let path = shape_to_vecpath(ShapeKind::Ellipse, 100.0, 80.0, 0.0);
        let polys = flatten_vecpath(&path, DEFAULT_TOLERANCE_MM);
        assert_eq!(polys.len(), 1);
        assert!(polys[0].closed);
        // Ellipse should produce many points
        assert!(polys[0].points.len() > 10);
    }

    #[test]
    fn flatten_empty_path() {
        let path = VecPath::new();
        let polys = flatten_vecpath(&path, DEFAULT_TOLERANCE_MM);
        assert!(polys.is_empty());
    }

    #[test]
    fn multiple_subpaths_produce_multiple_polylines() {
        let path = VecPath::parse_svg_d("M0 0 L10 10 M20 20 L30 30");
        let polys = flatten_vecpath(&path, DEFAULT_TOLERANCE_MM);
        assert_eq!(polys.len(), 2);
    }

    #[test]
    fn closed_path_no_duplicate_endpoint() {
        // A closed triangle: M0 0 L10 0 L10 10 Z
        // Should produce exactly 3 points, not 4 (no duplicate closing point).
        let path = VecPath::parse_svg_d("M0 0 L10 0 L10 10 Z");
        let polys = flatten_vecpath(&path, DEFAULT_TOLERANCE_MM);
        assert_eq!(polys.len(), 1);
        assert!(polys[0].closed);
        assert_eq!(
            polys[0].points.len(),
            3,
            "Expected 3 points for closed triangle, got {} — duplicate closing point not stripped",
            polys[0].points.len()
        );
        // First point should NOT equal last point (since `closed` flag handles wrap)
        assert_ne!(
            polys[0].points[0],
            *polys[0].points.last().unwrap(),
            "First and last points should be different for a closed polyline"
        );
    }

    #[test]
    fn closed_rectangle_no_duplicate_endpoint() {
        let path = VecPath::parse_svg_d("M0 0 L10 0 L10 10 L0 10 Z");
        let polys = flatten_vecpath(&path, DEFAULT_TOLERANCE_MM);
        assert_eq!(polys.len(), 1);
        assert!(polys[0].closed);
        assert_eq!(
            polys[0].points.len(),
            4,
            "Expected 4 points for closed rectangle, got {}",
            polys[0].points.len()
        );
    }
}

// SPDX-FileCopyrightText: 2001-2019 Peter Selinger
// SPDX-FileCopyrightText: 2026 Beam Bench contributors
// SPDX-License-Identifier: GPL-2.0-or-later
//
// This file is a Rust port and modification of Potrace 1.16.
// Modified for Beam Bench on 2026-04-16: translated to Rust and adapted to
// Beam Bench's internal bitmap, path, and numeric representations.

use super::types::{InternalCurve, InternalPath, Point, SegmentTag};
#[allow(dead_code)]
use beambench_common::path::{PathCommand, SubPath, VecPath};

/// Convert one InternalCurve to one closed SubPath.
pub fn curve_to_subpath(curve: &InternalCurve) -> SubPath {
    let m = curve.n();
    let mut sp = SubPath::new();
    if m == 0 {
        return sp;
    }

    let start = &curve.segments[m - 1].c[2];
    sp.commands.push(PathCommand::MoveTo {
        x: start.x,
        y: start.y,
    });

    for seg in &curve.segments {
        match seg.tag {
            SegmentTag::Curve => {
                sp.commands.push(PathCommand::CubicTo {
                    c1x: seg.c[0].x,
                    c1y: seg.c[0].y,
                    c2x: seg.c[1].x,
                    c2y: seg.c[1].y,
                    x: seg.c[2].x,
                    y: seg.c[2].y,
                });
            }
            SegmentTag::Corner => {
                sp.commands.push(PathCommand::LineTo {
                    x: seg.c[1].x,
                    y: seg.c[1].y,
                });
                sp.commands.push(PathCommand::LineTo {
                    x: seg.c[2].x,
                    y: seg.c[2].y,
                });
            }
        }
    }

    sp.commands.push(PathCommand::Close);
    sp.closed = true;
    sp
}

/// Group traced paths into compound VecPaths.
/// Outer contours (sign=true) become the primary subpath.
/// Inner holes (sign=false) contained within an outer are additional subpaths.
pub fn paths_to_compound_vecpaths(paths: &[InternalPath]) -> Vec<VecPath> {
    let outers: Vec<usize> = paths
        .iter()
        .enumerate()
        .filter(|(_, p)| p.sign && p.fcurve.is_some())
        .map(|(i, _)| i)
        .collect();
    let inners: Vec<usize> = paths
        .iter()
        .enumerate()
        .filter(|(_, p)| !p.sign && p.fcurve.is_some())
        .map(|(i, _)| i)
        .collect();

    let mut results = Vec::new();
    let mut used_inners = vec![false; paths.len()];

    // For each inner, find the smallest-area outer that contains it.
    // This ensures deep holes (e.g. outer→hole→island→deep_hole) attach
    // to their immediate parent outer, not the top-level one.
    let mut inner_owner: Vec<Option<usize>> = vec![None; paths.len()];
    for &ii in &inners {
        let mut best_outer: Option<usize> = None;
        let mut best_area = i64::MAX;
        for &oi in &outers {
            if is_contained_in(&paths[ii], &paths[oi]) {
                let area = paths[oi].area.abs();
                if area < best_area {
                    best_area = area;
                    best_outer = Some(oi);
                }
            }
        }
        inner_owner[ii] = best_outer;
    }

    for &oi in &outers {
        let outer_sp = curve_to_subpath(paths[oi].fcurve.as_ref().unwrap());
        let mut subpaths = vec![outer_sp];

        for &ii in &inners {
            if inner_owner[ii] == Some(oi) {
                let hole_sp = curve_to_subpath(paths[ii].fcurve.as_ref().unwrap());
                subpaths.push(hole_sp);
                used_inners[ii] = true;
            }
        }
        results.push(VecPath { subpaths });
    }

    // Unmatched inner contours become standalone paths
    for &ii in &inners {
        if !used_inners[ii] {
            if let Some(ref fc) = paths[ii].fcurve {
                let sp = curve_to_subpath(fc);
                results.push(VecPath { subpaths: vec![sp] });
            }
        }
    }

    results
}

/// Test if inner path is geometrically contained within outer path.
/// Uses bounding-box containment as primary check (robust for integer
/// pixel-edge coordinates where point-in-polygon can fail on shared edges),
/// then verifies with a point-in-polygon test on the centroid.
fn is_contained_in(inner: &InternalPath, outer: &InternalPath) -> bool {
    if inner.pt.is_empty() || outer.pt.is_empty() {
        return false;
    }

    // Compute bounding boxes
    let (mut imin_x, mut imin_y, mut imax_x, mut imax_y) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for p in &inner.pt {
        imin_x = imin_x.min(p.x);
        imin_y = imin_y.min(p.y);
        imax_x = imax_x.max(p.x);
        imax_y = imax_y.max(p.y);
    }
    let (mut omin_x, mut omin_y, mut omax_x, mut omax_y) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for p in &outer.pt {
        omin_x = omin_x.min(p.x);
        omin_y = omin_y.min(p.y);
        omax_x = omax_x.max(p.x);
        omax_y = omax_y.max(p.y);
    }

    // Inner bbox must be strictly within outer bbox
    if imin_x < omin_x || imin_y < omin_y || imax_x > omax_x || imax_y > omax_y {
        return false;
    }

    // Verify with point-in-polygon on inner centroid
    let centroid = Point::new((imin_x + imax_x) / 2.0, (imin_y + imax_y) / 2.0);
    point_in_polygon(&centroid, &outer.pt)
}

/// Ray-casting point-in-polygon test.
fn point_in_polygon(p: &Point, polygon: &[Point]) -> bool {
    let mut inside = false;
    let n = polygon.len();
    if n == 0 {
        return false;
    }
    let mut j = n - 1;
    for i in 0..n {
        let pi = &polygon[i];
        let pj = &polygon[j];
        if ((pi.y > p.y) != (pj.y > p.y))
            && (p.x < (pj.x - pi.x) * (p.y - pi.y) / (pj.y - pi.y) + pi.x)
        {
            inside = !inside;
        }
        j = i;
    }
    inside
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::potrace::types::*;
    use beambench_common::path::PathCommand;

    fn make_simple_curve(corners: &[(f64, f64)]) -> InternalCurve {
        let m = corners.len();
        let mut curve = InternalCurve::new(m);
        for (i, &(x, y)) in corners.iter().enumerate() {
            curve.segments[i].tag = SegmentTag::Corner;
            curve.segments[i].c[1] = Point::new(x, y);
            // Endpoint = midpoint between this vertex and next vertex
            let next = corners[(i + 1) % m];
            curve.segments[i].c[2] = Point::new((x + next.0) / 2.0, (y + next.1) / 2.0);
            curve.segments[i].vertex = Point::new(x, y);
        }
        curve.alphacurve = true;
        curve
    }

    fn make_test_path_with_curve(sign: bool, corners: &[(f64, f64)]) -> InternalPath {
        let pts: Vec<Point> = corners.iter().map(|&(x, y)| Point::new(x, y)).collect();
        // Compute shoelace area so smallest-enclosing logic works correctly
        let n = corners.len();
        let mut area2: f64 = 0.0;
        for i in 0..n {
            let (x1, y1) = corners[i];
            let (x2, y2) = corners[(i + 1) % n];
            area2 += x1 * y2 - x2 * y1;
        }
        let area = (area2 / 2.0).round() as i64;
        let signed_area = if sign { area.abs() } else { -area.abs() };
        let mut path = InternalPath::new(pts, signed_area, sign);
        path.fcurve = Some(make_simple_curve(corners));
        path
    }

    #[test]
    fn corner_segment_produces_lineto() {
        let curve = make_simple_curve(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]);
        let sp = curve_to_subpath(&curve);
        assert!(sp.closed);
        assert!(
            sp.commands
                .iter()
                .any(|c| matches!(c, PathCommand::LineTo { .. }))
        );
        assert!(matches!(sp.commands[0], PathCommand::MoveTo { .. }));
        assert!(matches!(sp.commands.last().unwrap(), PathCommand::Close));
    }

    #[test]
    fn curve_segment_produces_cubicto() {
        let mut curve = InternalCurve::new(1);
        curve.segments[0].tag = SegmentTag::Curve;
        curve.segments[0].c[0] = Point::new(1.0, 2.0);
        curve.segments[0].c[1] = Point::new(3.0, 4.0);
        curve.segments[0].c[2] = Point::new(5.0, 0.0);
        curve.alphacurve = true;
        let sp = curve_to_subpath(&curve);
        assert!(
            sp.commands
                .iter()
                .any(|c| matches!(c, PathCommand::CubicTo { .. }))
        );
    }

    #[test]
    fn compound_groups_hole_into_outer() {
        let outer =
            make_test_path_with_curve(true, &[(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0)]);
        let hole =
            make_test_path_with_curve(false, &[(5.0, 5.0), (15.0, 5.0), (15.0, 15.0), (5.0, 15.0)]);
        let result = paths_to_compound_vecpaths(&[outer, hole]);
        assert_eq!(
            result.len(),
            1,
            "outer + hole should produce 1 compound VecPath"
        );
        assert_eq!(
            result[0].subpaths.len(),
            2,
            "compound should have 2 subpaths"
        );
    }

    #[test]
    fn multilevel_nesting_groups_correctly() {
        // outer+ (0-40) → hole- (5-35) → island+ (10-30) → deep_hole- (15-25)
        let outer =
            make_test_path_with_curve(true, &[(0.0, 0.0), (40.0, 0.0), (40.0, 40.0), (0.0, 40.0)]);
        let hole =
            make_test_path_with_curve(false, &[(5.0, 5.0), (35.0, 5.0), (35.0, 35.0), (5.0, 35.0)]);
        let island = make_test_path_with_curve(
            true,
            &[(10.0, 10.0), (30.0, 10.0), (30.0, 30.0), (10.0, 30.0)],
        );
        let deep_hole = make_test_path_with_curve(
            false,
            &[(15.0, 15.0), (25.0, 15.0), (25.0, 25.0), (15.0, 25.0)],
        );
        let result = paths_to_compound_vecpaths(&[outer, hole, island, deep_hole]);
        // Should produce 2 compound VecPaths: outer+hole and island+deep_hole
        assert_eq!(result.len(), 2, "should produce 2 compound VecPaths");
        // Outer gets 1 hole, island gets 1 deep_hole
        let outer_vp = &result[0];
        let island_vp = &result[1];
        assert_eq!(
            outer_vp.subpaths.len(),
            2,
            "outer should have 1 outer + 1 hole"
        );
        assert_eq!(
            island_vp.subpaths.len(),
            2,
            "island should have 1 outer + 1 deep_hole"
        );
    }

    #[test]
    fn separate_shapes_stay_separate() {
        let shape1 =
            make_test_path_with_curve(true, &[(0.0, 0.0), (5.0, 0.0), (5.0, 5.0), (0.0, 5.0)]);
        let shape2 = make_test_path_with_curve(
            true,
            &[(50.0, 50.0), (55.0, 50.0), (55.0, 55.0), (50.0, 55.0)],
        );
        let result = paths_to_compound_vecpaths(&[shape1, shape2]);
        assert_eq!(result.len(), 2, "disjoint shapes should produce 2 VecPaths");
        assert_eq!(result[0].subpaths.len(), 1);
        assert_eq!(result[1].subpaths.len(), 1);
    }

    #[test]
    fn point_in_polygon_works() {
        let polygon = vec![
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(10.0, 10.0),
            Point::new(0.0, 10.0),
        ];
        assert!(point_in_polygon(&Point::new(5.0, 5.0), &polygon));
        assert!(!point_in_polygon(&Point::new(15.0, 5.0), &polygon));
    }
}

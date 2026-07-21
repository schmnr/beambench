//! Clipper2-based polygon buffer for the interactive offset tool.
//!
//! Uses [`clipper2::Paths::inflate`] with proper ring-winding normalization
//! so that holes produced by boolean subtraction are correctly contracted
//! (not expanded as solids) during outward buffering.
//!
//! Only processes **closed** subpaths. Open subpaths should be routed through
//! [`super::offset::offset_path`] instead.

use beambench_common::path::{PathCommand, SubPath, VecPath};
use clipper2::{EndType, JoinType, Milli, Paths};

use crate::vector::flatten::{DEFAULT_TOLERANCE_MM, flatten_vecpath};
use crate::vector::offset::{CornerStyle, OffsetDirection, signed_area};

fn point_in_ring(px: f64, py: f64, ring: &[(f64, f64)]) -> bool {
    let n = ring.len();
    if n < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = ring[i];
        let (xj, yj) = ring[j];
        if ((yi > py) != (yj > py)) && (px < (xj - xi) * (py - yi) / (yj - yi) + xi) {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn ring_contained_in(inner: &[(f64, f64)], outer: &[(f64, f64)]) -> bool {
    if inner.is_empty() {
        return false;
    }
    inner.iter().all(|&(x, y)| point_in_ring(x, y, outer))
}

/// Buffer all **closed** rings in `path` using Clipper2's polygon inflate.
///
/// Open subpaths are silently ignored — callers should route those through
/// [`super::offset::offset_path`].
///
/// Returns one `VecPath` per direction (1 for Inward/Outward, 2 for Both).
pub fn buffer_closed_path(
    path: &VecPath,
    distance_mm: f64,
    direction: OffsetDirection,
    corner_style: CornerStyle,
    miter_limit: f64,
) -> Vec<VecPath> {
    if distance_mm < 0.0 {
        return vec![];
    }

    let join = match corner_style {
        CornerStyle::Miter => JoinType::Miter,
        CornerStyle::Round => JoinType::Round,
        CornerStyle::Bevel => JoinType::Bevel,
    };

    // Flatten beziers → polylines, keep only closed rings with ≥3 points.
    let polylines = flatten_vecpath(path, DEFAULT_TOLERANCE_MM);
    let closed_rings: Vec<_> = polylines
        .into_iter()
        .filter(|pl| pl.closed && pl.points.len() >= 3)
        .collect();

    if closed_rings.is_empty() {
        return vec![];
    }

    // Normalize ring winding for Clipper2's NonZero fill convention:
    //   even nesting depth  => exterior => CCW
    //   odd nesting depth   => hole     => CW
    //
    // This preserves multiple exteriors, holes, and islands instead of forcing
    // a single "largest ring" exterior heuristic.
    let mut ring_data: Vec<(f64, Vec<(f64, f64)>)> = Vec::new();
    for ring in &closed_rings {
        let area = signed_area(&ring.points);
        if area.abs() < 1e-12 {
            continue; // degenerate
        }
        let pts: Vec<(f64, f64)> = ring.points.iter().map(|p| (p.x, p.y)).collect();
        ring_data.push((area, pts));
    }

    if ring_data.is_empty() {
        return vec![];
    }

    let mut all_ring_points: Vec<Vec<(f64, f64)>> = Vec::new();
    for i in 0..ring_data.len() {
        let (area, mut pts) = ring_data[i].clone();
        let depth = ring_data
            .iter()
            .enumerate()
            .filter(|(j, (_, outer))| i != *j && ring_contained_in(&pts, outer))
            .count();
        let should_be_ccw = depth % 2 == 0;

        if should_be_ccw {
            if area < 0.0 {
                pts.reverse();
            }
        } else if area > 0.0 {
            pts.reverse();
        }
        all_ring_points.push(pts);
    }

    if all_ring_points.is_empty() {
        return vec![];
    }

    let clipper_paths: Paths<Milli> = all_ring_points.into();

    match direction {
        OffsetDirection::Outward => {
            vec![inflate_to_vecpath(
                &clipper_paths,
                distance_mm,
                join,
                miter_limit,
            )]
        }
        OffsetDirection::Inward => {
            vec![inflate_to_vecpath(
                &clipper_paths,
                -distance_mm,
                join,
                miter_limit,
            )]
        }
        OffsetDirection::Both => {
            vec![
                inflate_to_vecpath(&clipper_paths, distance_mm, join, miter_limit),
                inflate_to_vecpath(&clipper_paths, -distance_mm, join, miter_limit),
            ]
        }
    }
}

/// Run `inflate()` and convert result back to `VecPath`.
fn inflate_to_vecpath(
    paths: &Paths<Milli>,
    delta: f64,
    join: JoinType,
    miter_limit: f64,
) -> VecPath {
    let result = paths.inflate(delta, join, EndType::Polygon, miter_limit);

    let mut subpaths = Vec::new();
    for path in result.iter() {
        let pts: Vec<(f64, f64)> = path.iter().map(|p| (p.x(), p.y())).collect();
        if pts.len() < 3 {
            continue;
        }

        let mut commands = Vec::with_capacity(pts.len() + 2);
        commands.push(PathCommand::MoveTo {
            x: pts[0].0,
            y: pts[0].1,
        });
        for &(x, y) in &pts[1..] {
            commands.push(PathCommand::LineTo { x, y });
        }
        commands.push(PathCommand::Close);

        subpaths.push(SubPath {
            commands,
            closed: true,
        });
    }

    VecPath { subpaths }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::object::ShapeKind;
    use crate::vector::boolean::{path_subtract, weld_shapes};
    use crate::vector::convert::shape_to_vecpath;
    use crate::vector::flatten::flatten_vecpath;

    /// Helper: compute minimum edge length across all subpaths.
    fn min_edge_length(vp: &VecPath) -> f64 {
        let mut min_len = f64::INFINITY;
        for pl in flatten_vecpath(vp, DEFAULT_TOLERANCE_MM) {
            let pts = &pl.points;
            for i in 0..pts.len() {
                let j = if i + 1 < pts.len() {
                    i + 1
                } else if pl.closed {
                    0
                } else {
                    continue;
                };
                let d = pts[i].distance_to(&pts[j]);
                if d < min_len {
                    min_len = d;
                }
            }
        }
        min_len
    }

    #[test]
    fn buffer_rect_outward() {
        let rect = shape_to_vecpath(ShapeKind::Rectangle, 10.0, 10.0, 0.0);
        let results = buffer_closed_path(
            &rect,
            2.0,
            OffsetDirection::Outward,
            CornerStyle::Miter,
            2.0,
        );
        assert_eq!(results.len(), 1);
        assert!(!results[0].is_empty());
        let bounds = results[0].bounds().expect("non-empty");
        // 10 + 2*2 = 14 expected
        assert!(
            (bounds.width() - 14.0).abs() < 1.0,
            "Expected ~14mm width, got {:.2}",
            bounds.width()
        );
        assert!(
            (bounds.height() - 14.0).abs() < 1.0,
            "Expected ~14mm height, got {:.2}",
            bounds.height()
        );
    }

    #[test]
    fn buffer_rect_inward() {
        let rect = shape_to_vecpath(ShapeKind::Rectangle, 20.0, 20.0, 0.0);
        let results =
            buffer_closed_path(&rect, 2.0, OffsetDirection::Inward, CornerStyle::Miter, 2.0);
        assert_eq!(results.len(), 1);
        assert!(!results[0].is_empty());
        let bounds = results[0].bounds().expect("non-empty");
        // 20 - 2*2 = 16 expected
        assert!(
            (bounds.width() - 16.0).abs() < 1.0,
            "Expected ~16mm width, got {:.2}",
            bounds.width()
        );
        assert!(
            (bounds.height() - 16.0).abs() < 1.0,
            "Expected ~16mm height, got {:.2}",
            bounds.height()
        );
    }

    #[test]
    fn buffer_both_directions() {
        let rect = shape_to_vecpath(ShapeKind::Rectangle, 10.0, 10.0, 0.0);
        let results =
            buffer_closed_path(&rect, 2.0, OffsetDirection::Both, CornerStyle::Miter, 2.0);
        assert_eq!(results.len(), 2);
        assert!(!results[0].is_empty());
        assert!(!results[1].is_empty());

        let outer_bounds = results[0].bounds().expect("outer");
        let inner_bounds = results[1].bounds().expect("inner");
        assert!(outer_bounds.width() > inner_bounds.width());
    }

    #[test]
    fn buffer_with_hole() {
        // Create a shape with a hole: 30×30 rect minus 10×10 inner rect
        let outer = VecPath::parse_svg_d("M0 0 L30 0 L30 30 L0 30 Z");
        let inner = VecPath::parse_svg_d("M10 10 L20 10 L20 20 L10 20 Z");
        let with_hole = path_subtract(&outer, &inner);

        // Should have at least 2 subpaths (outer + hole)
        assert!(
            with_hole.subpaths.len() >= 2,
            "Expected hole, got {} subpaths",
            with_hole.subpaths.len()
        );

        // Inward buffer by 3mm: outer shrinks, hole grows (or they meet/collapse)
        let results = buffer_closed_path(
            &with_hole,
            3.0,
            OffsetDirection::Inward,
            CornerStyle::Miter,
            2.0,
        );
        assert_eq!(results.len(), 1);
        let bounds = results[0].bounds().expect("non-empty");
        // Outer was 30×30, inward by 3 → ~24×24
        assert!(
            bounds.width() < 30.0 && bounds.height() < 30.0,
            "Inward offset should shrink: {:.2}×{:.2}",
            bounds.width(),
            bounds.height()
        );
    }

    #[test]
    fn buffer_welded_overlap_no_spikes() {
        let ellipse = shape_to_vecpath(ShapeKind::Ellipse, 60.0, 40.0, 0.0);
        let rect = VecPath::parse_svg_d("M30 10 L95 10 L95 30 L30 30 Z");
        let welded = weld_shapes(&[ellipse, rect]);

        let results = buffer_closed_path(
            &welded,
            2.0,
            OffsetDirection::Outward,
            CornerStyle::Miter,
            2.0,
        );
        assert_eq!(results.len(), 1);
        let min_len = min_edge_length(&results[0]);
        assert!(
            min_len > 0.15,
            "Welded offset has tiny spike segments (min edge {:.4}mm)",
            min_len
        );
    }

    #[test]
    fn buffer_round_more_vertices_than_miter() {
        let rect = shape_to_vecpath(ShapeKind::Rectangle, 10.0, 10.0, 0.0);
        let miter = buffer_closed_path(
            &rect,
            2.0,
            OffsetDirection::Outward,
            CornerStyle::Miter,
            2.0,
        );
        let round = buffer_closed_path(
            &rect,
            2.0,
            OffsetDirection::Outward,
            CornerStyle::Round,
            2.0,
        );

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

    #[test]
    fn buffer_ignores_open_subpaths() {
        // Build a VecPath with one closed and one open subpath
        let path = VecPath {
            subpaths: vec![
                SubPath {
                    commands: vec![
                        PathCommand::MoveTo { x: 0.0, y: 0.0 },
                        PathCommand::LineTo { x: 10.0, y: 0.0 },
                        PathCommand::LineTo { x: 10.0, y: 10.0 },
                        PathCommand::LineTo { x: 0.0, y: 10.0 },
                        PathCommand::Close,
                    ],
                    closed: true,
                },
                SubPath {
                    commands: vec![
                        PathCommand::MoveTo { x: 20.0, y: 0.0 },
                        PathCommand::LineTo { x: 30.0, y: 0.0 },
                        PathCommand::LineTo { x: 30.0, y: 10.0 },
                    ],
                    closed: false,
                },
            ],
        };

        let results = buffer_closed_path(
            &path,
            2.0,
            OffsetDirection::Outward,
            CornerStyle::Miter,
            2.0,
        );
        assert_eq!(results.len(), 1);

        // Result should only contain the closed rect (not the open path)
        let bounds = results[0].bounds().expect("non-empty");
        // The 10×10 rect grows to ~14×14; the open path at x=20..30 is not included
        assert!(
            bounds.max.x < 20.0,
            "Open path was incorrectly buffered: max_x={:.2}",
            bounds.max.x
        );
    }

    #[test]
    fn buffer_welded_round_stays_outside_source() {
        use geo::algorithm::contains::Contains;
        use geo::{Coord, LineString, Point, Polygon};

        let ellipse = shape_to_vecpath(ShapeKind::Ellipse, 60.0, 40.0, 0.0);
        let rect = VecPath::parse_svg_d("M30 10 L95 10 L95 30 L30 30 Z");
        let welded = weld_shapes(&[ellipse, rect]);

        // Build source polygon for containment check
        let welded_polys = flatten_vecpath(&welded, DEFAULT_TOLERANCE_MM);
        assert!(!welded_polys.is_empty());
        let mut coords: Vec<Coord<f64>> = welded_polys[0]
            .points
            .iter()
            .map(|p| Coord { x: p.x, y: p.y })
            .collect();
        if coords.first() != coords.last() {
            coords.push(coords[0]);
        }
        let source_poly = Polygon::new(LineString::new(coords), vec![]);

        let results = buffer_closed_path(
            &welded,
            2.0,
            OffsetDirection::Outward,
            CornerStyle::Round,
            2.0,
        );
        assert_eq!(results.len(), 1);

        for pl in flatten_vecpath(&results[0], DEFAULT_TOLERANCE_MM) {
            for pt in &pl.points {
                assert!(
                    !source_poly.contains(&Point::new(pt.x, pt.y)),
                    "Round outward buffer produced a vertex inside the source at ({:.3}, {:.3})",
                    pt.x,
                    pt.y
                );
            }
        }
    }

    #[test]
    fn buffer_hole_not_treated_as_solid() {
        use geo::algorithm::contains::Contains;
        use geo::{Coord, LineString, Point, Polygon};

        // Create shape with hole: 40×40 outer minus 20×20 inner.
        let outer = VecPath::parse_svg_d("M0 0 L40 0 L40 40 L0 40 Z");
        let inner = VecPath::parse_svg_d("M10 10 L30 10 L30 30 L10 30 Z");
        let with_hole = path_subtract(&outer, &inner);

        // Outward buffer: outer ring grows, hole ring contracts (material grows inward
        // into the hole), so center of the shape (20,20) — which was inside the hole —
        // should still NOT be solid if the hole is big enough to survive.
        let results = buffer_closed_path(
            &with_hole,
            2.0,
            OffsetDirection::Outward,
            CornerStyle::Miter,
            2.0,
        );
        assert_eq!(results.len(), 1);

        let bounds = results[0].bounds().expect("non-empty");
        // Outer should expand: 40 + 2*2 = ~44
        assert!(
            bounds.width() > 40.0,
            "Outer ring should expand: {:.2}",
            bounds.width()
        );

        // The result should still have at least 2 subpaths (outer + hole)
        let polys = flatten_vecpath(&results[0], DEFAULT_TOLERANCE_MM);
        assert!(
            polys.len() >= 2,
            "Hole should survive outward buffer: got {} subpaths",
            polys.len()
        );

        // Build polygons from all result rings and check: the center of the
        // original hole (20,20) should NOT be inside any of the rings treated
        // as individual polygons. If the hole were incorrectly treated as a
        // solid, (20,20) would be contained in the expanded "hole" ring.
        //
        // Use Clipper2's result: outer ring contains center, but hole ring
        // excludes it. We verify by checking the material coverage: the point
        // (20,20) should NOT be covered by the buffered shape (it's inside
        // the shrunken hole). The hole originally spanned 10..30, and with
        // 2mm outward buffer on the donut, the hole shrinks to ~12..28.
        // So (20,20) is still inside the hole.
        //
        // To do a proper inside-donut check, build the outer ring as a polygon
        // with the hole ring as an interior.
        // Find outer (largest) and hole (smaller) ring.
        let mut sorted_polys = polys.clone();
        sorted_polys.sort_by(|a, b| {
            let area_a = signed_area(&a.points).abs();
            let area_b = signed_area(&b.points).abs();
            area_b.partial_cmp(&area_a).unwrap()
        });

        // Largest = outer ring
        let outer_pts = &sorted_polys[0].points;
        let mut outer_coords: Vec<Coord<f64>> =
            outer_pts.iter().map(|p| Coord { x: p.x, y: p.y }).collect();
        if outer_coords.first() != outer_coords.last() {
            outer_coords.push(outer_coords[0]);
        }

        // Second largest = hole ring
        let hole_pts = &sorted_polys[1].points;
        let mut hole_coords: Vec<Coord<f64>> =
            hole_pts.iter().map(|p| Coord { x: p.x, y: p.y }).collect();
        if hole_coords.first() != hole_coords.last() {
            hole_coords.push(hole_coords[0]);
        }

        let donut = Polygon::new(
            LineString::new(outer_coords),
            vec![LineString::new(hole_coords)],
        );

        // Center of the original hole should NOT be inside the donut
        assert!(
            !donut.contains(&Point::new(20.0, 20.0)),
            "Center of hole (20,20) should not be inside the buffered donut — \
             hole was incorrectly treated as a solid"
        );

        // But a point on the material ring (e.g. (5,5)) SHOULD be inside
        assert!(
            donut.contains(&Point::new(5.0, 5.0)),
            "Material point (5,5) should be inside the buffered donut"
        );
    }

    /// CW exterior ring (e.g. from imported SVG) should still offset outward
    /// correctly — the winding normalization must reverse it to CCW before
    /// passing to Clipper2.
    #[test]
    fn buffer_cw_exterior_offsets_outward_correctly() {
        // CW 10×10 rectangle (reversed vertex order compared to CCW default)
        let cw_rect = VecPath::parse_svg_d("M0 0 L0 10 L10 10 L10 0 Z");

        // Verify it's actually CW (negative area)
        let polys = flatten_vecpath(&cw_rect, DEFAULT_TOLERANCE_MM);
        assert_eq!(polys.len(), 1);
        let area = signed_area(&polys[0].points);
        assert!(
            area < 0.0,
            "Test setup: expected CW (negative area), got {area:.2}"
        );

        // Outward buffer should expand — not collapse to nothing (which would
        // happen if the CW ring were treated as a hole)
        let results = buffer_closed_path(
            &cw_rect,
            2.0,
            OffsetDirection::Outward,
            CornerStyle::Miter,
            2.0,
        );
        assert_eq!(results.len(), 1);
        assert!(
            !results[0].is_empty(),
            "CW exterior should produce output, not be treated as a hole"
        );

        let bounds = results[0].bounds().expect("non-empty");
        // 10 + 2*2 = 14 expected — same as a CCW rect
        assert!(
            (bounds.width() - 14.0).abs() < 1.0,
            "CW rect outward: expected ~14mm width, got {:.2}",
            bounds.width()
        );
        assert!(
            (bounds.height() - 14.0).abs() < 1.0,
            "CW rect outward: expected ~14mm height, got {:.2}",
            bounds.height()
        );

        // Also test inward: should shrink
        let inward = buffer_closed_path(
            &cw_rect,
            2.0,
            OffsetDirection::Inward,
            CornerStyle::Miter,
            2.0,
        );
        assert_eq!(inward.len(), 1);
        let ib = inward[0].bounds().expect("non-empty");
        assert!(
            (ib.width() - 6.0).abs() < 1.0,
            "CW rect inward: expected ~6mm width, got {:.2}",
            ib.width()
        );
    }

    #[test]
    fn buffer_disjoint_exteriors_preserves_both_islands() {
        let a = VecPath::parse_svg_d("M0 0 L10 0 L10 10 L0 10 Z");
        let b = VecPath::parse_svg_d("M30 0 L40 0 L40 10 L30 10 Z");
        let both = VecPath {
            subpaths: [a.subpaths, b.subpaths].concat(),
        };

        let results = buffer_closed_path(
            &both,
            2.0,
            OffsetDirection::Outward,
            CornerStyle::Miter,
            2.0,
        );
        assert_eq!(results.len(), 1);
        let polys = flatten_vecpath(&results[0], DEFAULT_TOLERANCE_MM);
        assert!(
            polys.len() >= 2,
            "Disjoint exteriors should remain two buffered islands, got {}",
            polys.len()
        );
    }

    #[test]
    fn buffer_three_nested_contours_preserves_inner_island() {
        let outer = VecPath::parse_svg_d("M0 0 L80 0 L80 80 L0 80 Z");
        let middle = VecPath::parse_svg_d("M20 20 L60 20 L60 60 L20 60 Z");
        let inner = VecPath::parse_svg_d("M35 35 L45 35 L45 45 L35 45 Z");
        let combined = VecPath {
            subpaths: [outer.subpaths, middle.subpaths, inner.subpaths].concat(),
        };

        let results = buffer_closed_path(
            &combined,
            2.0,
            OffsetDirection::Inward,
            CornerStyle::Miter,
            2.0,
        );
        assert_eq!(results.len(), 1);
        let polys = flatten_vecpath(&results[0], DEFAULT_TOLERANCE_MM);
        assert!(
            polys.len() >= 3,
            "Three nested contours should preserve an inner island after buffering, got {} rings",
            polys.len()
        );
    }
}

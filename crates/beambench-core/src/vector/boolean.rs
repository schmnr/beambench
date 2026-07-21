use beambench_common::Bounds;
use beambench_common::path::{PathCommand, SubPath, VecPath};
use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::filter::ContourFilter;
use i_overlay::float::overlay::FloatOverlay;

use crate::vector::flatten::{DEFAULT_TOLERANCE_MM, flatten_vecpath};
use crate::vector::path_ops::optimize_path;

pub const OFFSET_FILL_BOOLEAN_TOLERANCE_MM: f64 = 0.02;

/// Compute the union of two vector paths.
pub fn path_union(a: &VecPath, b: &VecPath) -> VecPath {
    path_union_with_tolerance(a, b, DEFAULT_TOLERANCE_MM)
}

pub fn path_union_with_tolerance(a: &VecPath, b: &VecPath, tolerance: f64) -> VecPath {
    binary_overlay_with_tolerance(a, b, tolerance, OverlayRule::Union)
}

/// Subtract path `b` from path `a`.
pub fn path_subtract(a: &VecPath, b: &VecPath) -> VecPath {
    path_subtract_with_tolerance(a, b, DEFAULT_TOLERANCE_MM)
}

pub fn path_subtract_with_tolerance(a: &VecPath, b: &VecPath, tolerance: f64) -> VecPath {
    binary_overlay_with_tolerance(a, b, tolerance, OverlayRule::Difference)
}

/// Compute the symmetric difference (XOR / exclude) of two vector paths.
pub fn path_exclude(a: &VecPath, b: &VecPath) -> VecPath {
    path_exclude_with_tolerance(a, b, DEFAULT_TOLERANCE_MM)
}

pub fn path_exclude_with_tolerance(a: &VecPath, b: &VecPath, tolerance: f64) -> VecPath {
    binary_overlay_with_tolerance(a, b, tolerance, OverlayRule::Xor)
}

/// Compute the intersection of two vector paths.
pub fn path_intersection(a: &VecPath, b: &VecPath) -> VecPath {
    path_intersection_with_tolerance(a, b, DEFAULT_TOLERANCE_MM)
}

pub fn path_intersection_with_tolerance(a: &VecPath, b: &VecPath, tolerance: f64) -> VecPath {
    binary_overlay_with_tolerance(a, b, tolerance, OverlayRule::Intersect)
}

/// Compute the union of multiple vector paths.
pub fn weld_shapes(paths: &[VecPath]) -> VecPath {
    weld_shapes_with_tolerance(paths, DEFAULT_TOLERANCE_MM)
}

pub fn weld_shapes_with_tolerance(paths: &[VecPath], tolerance: f64) -> VecPath {
    if paths.is_empty() {
        return VecPath { subpaths: vec![] };
    }

    let mut result = paths[0].clone();
    for path in &paths[1..] {
        result = path_union_with_tolerance(&result, path, tolerance);
    }
    result
}

/// Normalize a collection of closed shapes under even-odd fill in one boolean pass.
pub fn normalize_subject_evenodd_with_tolerance(
    paths: &[VecPath],
    flatten_tolerance: f64,
    simplify_tolerance: f64,
) -> VecPath {
    let subject_contours: Vec<Vec<[f64; 2]>> = paths
        .iter()
        .flat_map(|path| vecpath_to_overlay_contours(path, flatten_tolerance))
        .collect();

    if subject_contours.is_empty() {
        return VecPath { subpaths: vec![] };
    }

    let shapes = FloatOverlay::with_subj(&subject_contours).overlay_with_filter_and_solver(
        OverlayRule::Subject,
        FillRule::EvenOdd,
        ContourFilter {
            min_area: 0.0,
            simplify: false,
        },
        Default::default(),
    );

    overlay_shapes_to_vecpath(&shapes, simplify_tolerance)
}

/// Cut all earlier paths by the last path.
///
/// The final selected path is the cutter. Each earlier path contributes up to
/// two results: the portion inside the cutter and the portion outside it.
pub fn cut_shapes(paths: &[VecPath]) -> Vec<VecPath> {
    if paths.len() < 2 {
        return vec![];
    }

    let cutter = paths.last().expect("paths has at least two entries");
    let mut results = Vec::new();
    for subject in &paths[..paths.len() - 1] {
        let inside = path_intersection(subject, cutter);
        if !inside.is_empty() {
            results.push(inside);
        }
        let outside = path_subtract(subject, cutter);
        if !outside.is_empty() {
            results.push(outside);
        }
    }
    results
}

/// Apply a vector mask to image bounds.
/// Returns the intersection of the image rectangle with the mask path.
pub fn apply_mask(image_bounds: &Bounds, mask_path: &VecPath) -> VecPath {
    let rect_path = bounds_to_vecpath(image_bounds);
    path_intersection(&rect_path, mask_path)
}

fn bounds_to_vecpath(bounds: &Bounds) -> VecPath {
    let commands = vec![
        PathCommand::MoveTo {
            x: bounds.min.x,
            y: bounds.min.y,
        },
        PathCommand::LineTo {
            x: bounds.max.x,
            y: bounds.min.y,
        },
        PathCommand::LineTo {
            x: bounds.max.x,
            y: bounds.max.y,
        },
        PathCommand::LineTo {
            x: bounds.min.x,
            y: bounds.max.y,
        },
        PathCommand::Close,
    ];

    VecPath {
        subpaths: vec![SubPath {
            commands,
            closed: true,
        }],
    }
}

fn binary_overlay_with_tolerance(
    subject: &VecPath,
    clip: &VecPath,
    tolerance: f64,
    rule: OverlayRule,
) -> VecPath {
    let subject_contours = vecpath_to_boolean_contours(subject, tolerance);
    let clip_contours = vecpath_to_boolean_contours(clip, tolerance);

    if subject_contours.is_empty() && clip_contours.is_empty() {
        return VecPath { subpaths: vec![] };
    }

    if subject_contours.is_empty() {
        return match rule {
            OverlayRule::Union | OverlayRule::Xor | OverlayRule::InverseDifference => {
                overlay_contours_to_vecpath(&clip_contours, tolerance)
            }
            _ => VecPath { subpaths: vec![] },
        };
    }

    if clip_contours.is_empty() {
        return match rule {
            OverlayRule::Union
            | OverlayRule::Difference
            | OverlayRule::Xor
            | OverlayRule::Subject => overlay_contours_to_vecpath(&subject_contours, tolerance),
            _ => VecPath { subpaths: vec![] },
        };
    }

    let shapes = FloatOverlay::with_subj_and_clip(&subject_contours, &clip_contours)
        .overlay_with_filter_and_solver(
            rule,
            FillRule::EvenOdd,
            ContourFilter {
                min_area: 0.0,
                simplify: false,
            },
            Default::default(),
        );

    overlay_shapes_to_vecpath(&shapes, tolerance)
}

fn overlay_contours_to_vecpath(contours: &[Vec<[f64; 2]>], tolerance: f64) -> VecPath {
    let shapes: Vec<Vec<Vec<[f64; 2]>>> = contours
        .iter()
        .map(|contour| vec![contour.clone()])
        .collect();
    overlay_shapes_to_vecpath(&shapes, tolerance)
}

fn vecpath_to_boolean_contours(path: &VecPath, tolerance: f64) -> Vec<Vec<[f64; 2]>> {
    flatten_vecpath(path, tolerance)
        .into_iter()
        .filter(|poly| poly.points.len() >= 3)
        .map(|poly| poly.points.iter().map(|p| [p.x, p.y]).collect())
        .collect()
}

fn vecpath_to_overlay_contours(path: &VecPath, tolerance: f64) -> Vec<Vec<[f64; 2]>> {
    flatten_vecpath(path, tolerance)
        .into_iter()
        .filter(|poly| poly.closed && poly.points.len() >= 3)
        .map(|poly| poly.points.iter().map(|p| [p.x, p.y]).collect())
        .collect()
}

fn overlay_shapes_to_vecpath(shapes: &[Vec<Vec<[f64; 2]>>], simplify_tolerance: f64) -> VecPath {
    let mut subpaths = Vec::new();

    for shape in shapes {
        for contour in shape {
            if contour.len() < 3 {
                continue;
            }
            let mut commands = Vec::with_capacity(contour.len() + 1);
            commands.push(PathCommand::MoveTo {
                x: contour[0][0],
                y: contour[0][1],
            });
            for point in &contour[1..] {
                commands.push(PathCommand::LineTo {
                    x: point[0],
                    y: point[1],
                });
            }
            commands.push(PathCommand::Close);
            subpaths.push(SubPath {
                commands,
                closed: true,
            });
        }
    }

    optimize_path(&VecPath { subpaths }, simplify_tolerance)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::object::ShapeKind;
    use crate::vector::convert::shape_to_vecpath;

    fn point_is_filled_evenodd(path: &VecPath, x: f64, y: f64) -> bool {
        let mut filled = false;
        for polyline in flatten_vecpath(path, DEFAULT_TOLERANCE_MM)
            .into_iter()
            .filter(|polyline| polyline.points.len() >= 3)
        {
            let mut inside = false;
            let points = &polyline.points;
            let mut j = points.len() - 1;
            for i in 0..points.len() {
                let pi = &points[i];
                let pj = &points[j];
                let crosses_y = (pi.y > y) != (pj.y > y);
                if crosses_y {
                    let x_at_y = (pj.x - pi.x) * (y - pi.y) / (pj.y - pi.y) + pi.x;
                    if x < x_at_y {
                        inside = !inside;
                    }
                }
                j = i;
            }
            if inside {
                filled = !filled;
            }
        }
        filled
    }

    #[test]
    fn union_of_overlapping_rectangles() {
        let a = shape_to_vecpath(ShapeKind::Rectangle, 10.0, 10.0, 0.0);
        // b is offset by (5, 0) — we need to manually create a shifted rect
        let b = VecPath::parse_svg_d("M5 0 L15 0 L15 10 L5 10 Z");

        let result = path_union(&a, &b);
        assert!(!result.is_empty(), "Union should produce non-empty path");
        assert!(result.subpaths.iter().any(|sp| sp.closed));
    }

    #[test]
    fn subtract_overlapping_rectangles() {
        let a = shape_to_vecpath(ShapeKind::Rectangle, 10.0, 10.0, 0.0);
        let b = VecPath::parse_svg_d("M5 0 L15 0 L15 10 L5 10 Z");

        let result = path_subtract(&a, &b);
        assert!(!result.is_empty(), "Subtract should produce non-empty path");
    }

    #[test]
    fn subtract_inner_square_creates_hole_not_solid_center() {
        let outer = VecPath::parse_svg_d("M0 0 L20 0 L20 20 L0 20 Z");
        let inner = VecPath::parse_svg_d("M5 5 L15 5 L15 15 L5 15 Z");

        let result = path_subtract(&outer, &inner);

        assert_eq!(result.subpaths.len(), 2);
        assert!(result.subpaths.iter().all(|sp| sp.closed));
        assert!(point_is_filled_evenodd(&result, 2.0, 2.0));
        assert!(!point_is_filled_evenodd(&result, 10.0, 10.0));
    }

    #[test]
    fn intersection_preserves_depth_three_evenodd_topology() {
        let nested = VecPath::parse_svg_d(
            "M0 0 L30 0 L30 30 L0 30 Z \
             M5 5 L25 5 L25 25 L5 25 Z \
             M10 10 L20 10 L20 20 L10 20 Z",
        );
        let clip = VecPath::parse_svg_d("M-5 -5 L35 -5 L35 35 L-5 35 Z");

        let result = path_intersection(&nested, &clip);

        assert!(point_is_filled_evenodd(&result, 2.0, 2.0));
        assert!(!point_is_filled_evenodd(&result, 7.0, 7.0));
        assert!(point_is_filled_evenodd(&result, 15.0, 15.0));
    }

    #[test]
    fn subtract_cutter_inside_existing_hole_is_noop() {
        let donut = VecPath::parse_svg_d("M0 0 L20 0 L20 20 L0 20 Z M5 5 L15 5 L15 15 L5 15 Z");
        let cutter_inside_hole = VecPath::parse_svg_d("M8 8 L12 8 L12 12 L8 12 Z");

        let result = path_subtract(&donut, &cutter_inside_hole);

        assert!(point_is_filled_evenodd(&result, 2.0, 2.0));
        assert!(!point_is_filled_evenodd(&result, 10.0, 10.0));
        assert_eq!(result.subpaths.len(), 2);
    }

    #[test]
    fn union_of_non_overlapping_shapes() {
        let a = VecPath::parse_svg_d("M0 0 L10 0 L10 10 L0 10 Z");
        let b = VecPath::parse_svg_d("M20 0 L30 0 L30 10 L20 10 Z");

        let result = path_union(&a, &b);
        // Should produce two separate closed subpaths
        assert!(
            result.subpaths.len() >= 2,
            "Non-overlapping union should keep both shapes, got {} subpaths",
            result.subpaths.len()
        );
    }

    #[test]
    fn subtract_non_overlapping_preserves_original() {
        let a = VecPath::parse_svg_d("M0 0 L10 0 L10 10 L0 10 Z");
        let b = VecPath::parse_svg_d("M20 0 L30 0 L30 10 L20 10 Z");

        let result = path_subtract(&a, &b);
        assert!(!result.is_empty());
    }

    // Intersection tests
    #[test]
    fn intersection_of_overlapping_rectangles() {
        let a = VecPath::parse_svg_d("M0 0 L10 0 L10 10 L0 10 Z");
        let b = VecPath::parse_svg_d("M5 0 L15 0 L15 10 L5 10 Z");

        let result = path_intersection(&a, &b);
        assert!(
            !result.is_empty(),
            "Intersection should produce non-empty path"
        );
        assert!(result.subpaths.iter().any(|sp| sp.closed));
    }

    #[test]
    fn intersection_rect_and_ellipse() {
        let rect = shape_to_vecpath(ShapeKind::Rectangle, 10.0, 10.0, 0.0);
        let ellipse = shape_to_vecpath(ShapeKind::Ellipse, 8.0, 8.0, 0.0);

        let result = path_intersection(&rect, &ellipse);
        assert!(
            !result.is_empty(),
            "Intersection should produce non-empty path"
        );
    }

    #[test]
    fn intersection_non_overlapping_returns_empty() {
        let a = VecPath::parse_svg_d("M0 0 L10 0 L10 10 L0 10 Z");
        let b = VecPath::parse_svg_d("M20 0 L30 0 L30 10 L20 10 Z");

        let result = path_intersection(&a, &b);
        assert!(
            result.is_empty(),
            "Non-overlapping intersection should be empty"
        );
    }

    #[test]
    fn intersection_coincident_edges() {
        let a = VecPath::parse_svg_d("M0 0 L10 0 L10 10 L0 10 Z");
        let b = VecPath::parse_svg_d("M10 0 L20 0 L20 10 L10 10 Z");

        let result = path_intersection(&a, &b);
        // Coincident edges typically produce empty or degenerate result
        assert!(result.is_empty() || result.subpaths.iter().all(|sp| sp.commands.len() <= 3));
    }

    #[test]
    fn intersection_complex_paths() {
        let a = VecPath::parse_svg_d("M0 0 L20 0 L20 20 L0 20 Z");
        let b = VecPath::parse_svg_d("M5 5 L15 5 L15 15 L5 15 Z M10 10 L25 10 L25 25 L10 25 Z");

        let result = path_intersection(&a, &b);
        assert!(!result.is_empty());
    }

    #[test]
    fn intersection_partial_overlap() {
        let a = VecPath::parse_svg_d("M0 0 L10 0 L10 10 L0 10 Z");
        let b = VecPath::parse_svg_d("M5 5 L15 5 L15 15 L5 15 Z");

        let result = path_intersection(&a, &b);
        assert!(!result.is_empty());
        if let Some(bounds) = result.bounds() {
            // Intersection should be roughly 5x5 square at (5,5)
            assert!(bounds.width() > 0.0 && bounds.height() > 0.0);
        }
    }

    #[test]
    fn weld_multiple_shapes() {
        let a = VecPath::parse_svg_d("M0 0 L10 0 L10 10 L0 10 Z");
        let b = VecPath::parse_svg_d("M5 0 L15 0 L15 10 L5 10 Z");
        let c = VecPath::parse_svg_d("M10 0 L20 0 L20 10 L10 10 Z");

        let result = weld_shapes(&[a, b, c]);
        assert!(!result.is_empty());
        // Should merge into one or two continuous shapes
        assert!(result.subpaths.len() <= 3);
    }

    #[test]
    fn weld_empty_paths() {
        let result = weld_shapes(&[]);
        assert!(result.is_empty());
    }

    #[test]
    fn cut_shapes_uses_last_path_as_cutter() {
        let a = VecPath::parse_svg_d("M0 0 L20 0 L20 20 L0 20 Z");
        let b = VecPath::parse_svg_d("M5 5 L15 5 L15 15 L5 15 Z");

        let result = cut_shapes(&[a, b]);
        assert_eq!(result.len(), 2);
        assert!(result.iter().all(|path| !path.is_empty()));
    }

    #[test]
    fn cut_shapes_cuts_each_subject_with_last_cutter() {
        let a = VecPath::parse_svg_d("M0 0 L30 0 L30 30 L0 30 Z");
        let b = VecPath::parse_svg_d("M5 5 L10 5 L10 10 L5 10 Z");
        let c = VecPath::parse_svg_d("M15 15 L20 15 L20 20 L15 20 Z");

        let result = cut_shapes(&[a, b, c]);
        assert!(result.len() >= 2);
    }

    #[test]
    fn union_with_open_path_treats_as_closed() {
        let closed_rect = VecPath::parse_svg_d("M0 0 L20 0 L20 20 L0 20 Z");
        // Open triangle (no Z) — should still work as if closed
        let open_tri = VecPath::parse_svg_d("M5 5 L15 5 L10 15");

        let result = path_union(&closed_rect, &open_tri);
        assert!(
            !result.is_empty(),
            "Union with open path should produce result"
        );
    }

    #[test]
    fn exclude_overlapping_rectangles() {
        let a = VecPath::parse_svg_d("M0 0 L10 0 L10 10 L0 10 Z");
        let b = VecPath::parse_svg_d("M5 0 L15 0 L15 10 L5 10 Z");

        let result = path_exclude(&a, &b);
        assert!(
            !result.is_empty(),
            "Exclude should produce non-empty path for overlapping rects"
        );
        // XOR of two overlapping rects should produce two disjoint regions
        assert!(result.subpaths.len() >= 2);
    }

    #[test]
    fn exclude_non_overlapping_returns_both() {
        let a = VecPath::parse_svg_d("M0 0 L10 0 L10 10 L0 10 Z");
        let b = VecPath::parse_svg_d("M20 0 L30 0 L30 10 L20 10 Z");

        let result = path_exclude(&a, &b);
        assert!(
            result.subpaths.len() >= 2,
            "Non-overlapping XOR should preserve both shapes"
        );
    }

    #[test]
    fn exclude_identical_returns_empty() {
        let a = VecPath::parse_svg_d("M0 0 L10 0 L10 10 L0 10 Z");
        let b = VecPath::parse_svg_d("M0 0 L10 0 L10 10 L0 10 Z");

        let result = path_exclude(&a, &b);
        assert!(
            result.is_empty(),
            "XOR of identical shapes should cancel out"
        );
    }

    #[test]
    fn apply_mask_to_image() {
        use beambench_common::Point2D;

        let bounds = Bounds {
            min: Point2D { x: 0.0, y: 0.0 },
            max: Point2D { x: 100.0, y: 100.0 },
        };
        let mask = VecPath::parse_svg_d("M25 25 L75 25 L75 75 L25 75 Z");

        let result = apply_mask(&bounds, &mask);

        assert!(!result.is_empty());
        if let Some(result_bounds) = result.bounds() {
            // Result should be smaller than original image
            assert!(result_bounds.width() < 100.0);
            assert!(result_bounds.height() < 100.0);
        }
    }

    #[test]
    fn apply_mask_preserves_holes_in_mask_path() {
        use beambench_common::Point2D;

        let bounds = Bounds {
            min: Point2D { x: 0.0, y: 0.0 },
            max: Point2D { x: 20.0, y: 20.0 },
        };
        let mask = VecPath::parse_svg_d("M0 0 L20 0 L20 20 L0 20 Z M5 5 L15 5 L15 15 L5 15 Z");

        let result = apply_mask(&bounds, &mask);

        assert!(point_is_filled_evenodd(&result, 2.0, 2.0));
        assert!(!point_is_filled_evenodd(&result, 10.0, 10.0));
    }

    #[test]
    fn boolean_result_visual_bounds_match_rendering_contract() {
        // Union two overlapping shapes with cubics
        let a = VecPath::parse_svg_d("M0 0 C0 100 100 100 100 0 Z");
        let b = VecPath::parse_svg_d("M50 -20 C50 80 150 80 150 -20 Z");
        let result = path_union(&a, &b);
        let visual = result.visual_bounds();
        let hull = result.bounds();
        if let (Some(vis), Some(h)) = (visual, hull) {
            // Visual should be <= hull (tighter or equal)
            assert!(vis.min.x >= h.min.x - 1e-6);
            assert!(vis.min.y >= h.min.y - 1e-6);
            assert!(vis.max.x <= h.max.x + 1e-6);
            assert!(vis.max.y <= h.max.y + 1e-6);
        }
    }

    #[test]
    fn normalize_subject_evenodd_preserves_hole_topology() {
        let outer = VecPath::parse_svg_d("M0 0 L20 0 L20 20 L0 20 Z");
        let inner = VecPath::parse_svg_d("M5 5 L15 5 L15 15 L5 15 Z");

        let result = normalize_subject_evenodd_with_tolerance(
            &[outer, inner],
            OFFSET_FILL_BOOLEAN_TOLERANCE_MM,
            OFFSET_FILL_BOOLEAN_TOLERANCE_MM,
        );

        assert_eq!(
            result.subpaths.len(),
            2,
            "evenodd subject normalization should keep one outer contour and one hole"
        );
        assert!(result.subpaths.iter().all(|sp| sp.closed));
    }
}

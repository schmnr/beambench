use beambench_common::geometry::{Bounds, Point2D};
use beambench_common::path::Polyline;
use serde::{Deserialize, Serialize};

use crate::layer::LayerId;
use crate::object::ProjectObject;
use crate::vector::cleanup::{
    dedup_consecutive_points, remove_empty_subpaths, remove_zero_length_segments,
};
use crate::vector::convert::{object_to_vecpath, object_to_world_vecpath};
use crate::vector::flatten::{DEFAULT_TOLERANCE_MM, flatten_vecpath};
use crate::vector::transform::bake_transform;

const VECTOR_PATH_FLATTEN_TOLERANCE_MM: f64 = 0.02;

/// The result of normalizing a project object into planner-ready polylines.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NormalizedVector {
    pub polylines: Vec<Polyline>,
    pub layer_id: LayerId,
    pub source_object_name: String,
}

/// Full pipeline: convert → bake transform → cleanup → flatten.
/// Returns None if the object cannot be converted to vectors (e.g., raster images).
pub fn normalize_object(obj: &ProjectObject) -> Option<NormalizedVector> {
    normalize_object_with_tolerance(obj, DEFAULT_TOLERANCE_MM)
}

/// Full pipeline with configurable tolerance.
pub fn normalize_object_with_tolerance(
    obj: &ProjectObject,
    tolerance: f64,
) -> Option<NormalizedVector> {
    let flatten_tolerance = if matches!(obj.data, crate::object::ObjectData::VectorPath { .. }) {
        tolerance.min(VECTOR_PATH_FLATTEN_TOLERANCE_MM)
    } else {
        tolerance
    };

    if matches!(obj.data, crate::object::ObjectData::Text { .. }) {
        let mut vec_path = object_to_world_vecpath(obj)?;

        remove_zero_length_segments(&mut vec_path);
        dedup_consecutive_points(&mut vec_path, flatten_tolerance * 0.1);
        remove_empty_subpaths(&mut vec_path);

        let polylines = flatten_vecpath(&vec_path, flatten_tolerance);
        if polylines.is_empty() {
            return None;
        }

        return Some(NormalizedVector {
            polylines,
            layer_id: obj.layer_id,
            source_object_name: obj.name.clone(),
        });
    }

    // Step 1: Convert to VecPath
    let mut vec_path = object_to_vecpath(&obj.data)?;

    // Step 2: Bake transform into coordinates
    if !obj.transform.is_identity() {
        vec_path = bake_transform(&vec_path, &obj.transform);
    }

    // Step 3: Cleanup (before flattening to remove degenerate commands)
    remove_zero_length_segments(&mut vec_path);
    dedup_consecutive_points(&mut vec_path, flatten_tolerance * 0.1);
    remove_empty_subpaths(&mut vec_path);

    // Step 4: Flatten curves to polylines FIRST.
    // This must happen before coordinate mapping so that we compute the
    // bounding box from actual curve points, not from control points.
    // VecPath::bounds() includes cubic/quad control points which can be
    // far from the curve, inflating the bbox and causing the mapping to
    // shrink the output.
    let mut polylines = flatten_vecpath(&vec_path, flatten_tolerance);

    if polylines.is_empty() {
        return None;
    }

    // Step 5: Map flattened polylines from their actual bbox to object bounds.
    // Uses the exact polyline bounding box (no control-point inflation).
    if let Some(poly_bbox) = compute_polylines_bounds(&polylines) {
        let path_w = poly_bbox.width();
        let path_h = poly_bbox.height();
        let bounds_w = obj.bounds.width();
        let bounds_h = obj.bounds.height();

        let sx = if path_w > 0.0 { bounds_w / path_w } else { 1.0 };
        let sy = if path_h > 0.0 { bounds_h / path_h } else { 1.0 };
        let tx = obj.bounds.min.x - poly_bbox.min.x * sx;
        let ty = obj.bounds.min.y - poly_bbox.min.y * sy;

        for polyline in &mut polylines {
            for pt in &mut polyline.points {
                pt.x = pt.x * sx + tx;
                pt.y = pt.y * sy + ty;
            }
        }
    }

    Some(NormalizedVector {
        polylines,
        layer_id: obj.layer_id,
        source_object_name: obj.name.clone(),
    })
}

/// Compute the axis-aligned bounding box of a set of polylines.
fn compute_polylines_bounds(polylines: &[Polyline]) -> Option<Bounds> {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let mut has_points = false;

    for polyline in polylines {
        for pt in &polyline.points {
            has_points = true;
            min_x = min_x.min(pt.x);
            min_y = min_y.min(pt.y);
            max_x = max_x.max(pt.x);
            max_y = max_y.max(pt.y);
        }
    }

    if has_points {
        Some(Bounds::new(
            Point2D::new(min_x, min_y),
            Point2D::new(max_x, max_y),
        ))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layer::LayerId;
    use crate::object::{ObjectData, ProjectObject, ShapeKind};
    use beambench_common::geometry::Transform2D;
    use beambench_common::{Bounds, Point2D};

    fn make_rect_object() -> ProjectObject {
        let layer_id = LayerId::new();
        ProjectObject::new(
            "test_rect",
            layer_id,
            Bounds::new(Point2D::new(10.0, 20.0), Point2D::new(60.0, 70.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 50.0,
                height: 50.0,
                corner_radius: 0.0,
            },
        )
    }

    #[test]
    fn normalize_rectangle() {
        let obj = make_rect_object();
        let result = normalize_object(&obj).unwrap();
        assert_eq!(result.polylines.len(), 1);
        assert!(result.polylines[0].closed);
        assert_eq!(result.source_object_name, "test_rect");
    }

    #[test]
    fn normalize_rectangle_applies_bounds_offset() {
        let obj = make_rect_object();
        let result = normalize_object(&obj).unwrap();
        let pts = &result.polylines[0].points;
        // First point should be at bounds.min (10, 20)
        assert!(
            (pts[0].x - 10.0).abs() < 0.1,
            "Expected x~10, got {}",
            pts[0].x
        );
        assert!(
            (pts[0].y - 20.0).abs() < 0.1,
            "Expected y~20, got {}",
            pts[0].y
        );
    }

    #[test]
    fn normalize_ellipse() {
        let layer_id = LayerId::new();
        let obj = ProjectObject::new(
            "ellipse",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(100.0, 80.0)),
            ObjectData::Shape {
                kind: ShapeKind::Ellipse,
                width: 100.0,
                height: 80.0,
                corner_radius: 0.0,
            },
        );
        let result = normalize_object(&obj).unwrap();
        assert_eq!(result.polylines.len(), 1);
        assert!(result.polylines[0].closed);
        assert!(result.polylines[0].points.len() > 10);
    }

    #[test]
    fn normalize_vector_path() {
        let layer_id = LayerId::new();
        let obj = ProjectObject::new(
            "path",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(100.0, 100.0)),
            ObjectData::VectorPath {
                path_data: "M0 0 C50 100 100 100 100 0".to_string(),
                closed: false,
                ruler_guide_axis: None,
            },
        );
        let result = normalize_object(&obj).unwrap();
        assert!(!result.polylines.is_empty());
    }

    #[test]
    fn normalize_vector_path_clamps_default_tolerance() {
        let layer_id = LayerId::new();
        let obj = ProjectObject::new(
            "dense-path",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(100.0, 100.0)),
            ObjectData::VectorPath {
                path_data: "M0 0 C20 0 40 100 60 100 C80 100 100 0 120 0".to_string(),
                closed: false,
                ruler_guide_axis: None,
            },
        );

        let clamped = normalize_object_with_tolerance(&obj, DEFAULT_TOLERANCE_MM).unwrap();
        let explicit =
            normalize_object_with_tolerance(&obj, VECTOR_PATH_FLATTEN_TOLERANCE_MM).unwrap();

        assert_eq!(
            clamped.polylines[0].points.len(),
            explicit.polylines[0].points.len(),
            "VectorPath normalization should clamp the default flatten tolerance"
        );
    }

    #[test]
    fn normalize_cubic_vecpath_fills_bounds() {
        // A cubic bezier where control points extend beyond the curve.
        // VecPath::bounds() would include control points (0,0)→(100,100),
        // but the actual curve max-y is ~75. The polyline bbox should be used
        // for coordinate mapping, so the curve fills the full obj.bounds height.
        let layer_id = LayerId::new();
        let obj = ProjectObject::new(
            "cubic",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(100.0, 100.0)),
            ObjectData::VectorPath {
                path_data: "M0 0 C50 100 100 100 100 0".to_string(),
                closed: false,
                ruler_guide_axis: None,
            },
        );
        let result = normalize_object(&obj).unwrap();
        let pts = &result.polylines[0].points;

        // With flatten-first mapping, the polylines should span the full
        // obj.bounds height (0 to 100), not be compressed to ~75% height.
        let max_y = pts.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max);
        assert!(
            max_y > 90.0,
            "Cubic curve should fill bounds height (max_y={max_y}), not be compressed by control-point bbox"
        );
    }

    #[test]
    fn normalize_raster_returns_none() {
        let layer_id = LayerId::new();
        let obj = ProjectObject::new(
            "image",
            layer_id,
            Bounds::new(Point2D::zero(), Point2D::new(100.0, 100.0)),
            ObjectData::RasterImage {
                asset_key: "test".to_string(),
                original_width_px: 100,
                original_height_px: 100,
                adjustments: None,
                masks: Vec::new(),
            },
        );
        assert!(normalize_object(&obj).is_none());
    }

    #[test]
    fn normalize_with_transform() {
        let layer_id = LayerId::new();
        let mut obj = ProjectObject::new(
            "transformed_rect",
            layer_id,
            Bounds::new(Point2D::zero(), Point2D::new(10.0, 10.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        );
        obj.transform = Transform2D::scale(2.0, 2.0);
        let result = normalize_object(&obj).unwrap();
        assert!(!result.polylines.is_empty());
    }

    #[test]
    fn normalized_vector_serializes() {
        let obj = make_rect_object();
        let result = normalize_object(&obj).unwrap();
        let json = serde_json::to_string(&result).unwrap();
        let restored: NormalizedVector = serde_json::from_str(&json).unwrap();
        assert_eq!(result.polylines.len(), restored.polylines.len());
    }
}

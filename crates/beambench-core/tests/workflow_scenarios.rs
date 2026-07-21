//! Integration tests for beambench-core workflow scenarios.
//!
//! Covers: grid_array, circular_array, offset_path (outward/inward),
//! apply_radius, set_start_point, break_apart, close_path.
//!
//! Run with:
//!   cargo test -p beambench-core --test workflow_scenarios
//!   cargo nextest run -p beambench-core -E 'test(workflow_scenarios)'

use beambench_common::path::VecPath;
use beambench_common::{Bounds, Point2D};
use beambench_core::array_ops::{
    CircularArrayConfig, GridArrayConfig, GridArraySizingMode, SpacingMode, circular_array,
    grid_array,
};
use beambench_core::layer::LayerId;
use beambench_core::object::{ObjectData, ProjectObject, ShapeKind};
use beambench_core::vector::convert::shape_to_vecpath;
use beambench_core::vector::{
    CornerStyle, OffsetDirection, apply_radius, break_apart, close_path, offset_path,
    set_start_point,
};

/// Helper: create a rectangular ProjectObject at a given position and size.
fn make_rect_object(x: f64, y: f64, w: f64, h: f64) -> ProjectObject {
    let bounds = Bounds::new(Point2D::new(x, y), Point2D::new(x + w, y + h));
    let data = ObjectData::Shape {
        kind: ShapeKind::Rectangle,
        width: w,
        height: h,
        corner_radius: 0.0,
    };
    ProjectObject::new("test-rect", LayerId::new(), bounds, data)
}

// ---------------------------------------------------------------------------
// 1. grid_array_creates_correct_count
// ---------------------------------------------------------------------------
#[test]
fn grid_array_creates_correct_count() {
    let source = make_rect_object(0.0, 0.0, 10.0, 10.0);
    let config = GridArrayConfig {
        rows: 2,
        cols: 3,
        sizing_mode_x: GridArraySizingMode::Count,
        sizing_mode_y: GridArraySizingMode::Count,
        total_width_mm: None,
        total_height_mm: None,
        h_spacing_mm: 15.0,
        v_spacing_mm: 15.0,
        spacing_mode: SpacingMode::CenterToCenter,
        mirror_alternate_cols: false,
        mirror_alternate_rows: false,
        x_col_shift_mm: 0.0,
        y_row_shift_mm: 0.0,
        half_shift: false,
        reverse_h: false,
        reverse_v: false,
        random_orientation: false,
        random_seed: 0,
        group_results: false,
        create_virtual: false,
        auto_increment_text: false,
        text_increment: 1,
    };

    let copies = grid_array(&[source], &config);

    // 2x3 = 6 positions, minus 1 original = 5 copies
    assert_eq!(
        copies.len(),
        5,
        "2x3 grid should produce 5 copies (excludes original)"
    );
}

// ---------------------------------------------------------------------------
// 2. circular_array_creates_copies
// ---------------------------------------------------------------------------
#[test]
fn circular_array_creates_copies() {
    let source = make_rect_object(0.0, 0.0, 10.0, 10.0);
    let config = CircularArrayConfig {
        count: 6,
        radius_mm: 50.0,
        rotate_copies: false,
        center_x: None,
        center_y: None,
        start_angle_deg: 0.0,
        end_angle_deg: 360.0,
        group_results: false,
        create_virtual: false,
        auto_increment_text: false,
        text_increment: 1,
    };

    let copies = circular_array(&[source], &config);

    // All 6 positions on the circle (position 0 = original moved)
    assert_eq!(
        copies.len(),
        6,
        "circular_array(count=6) should produce 6 positions"
    );
}

// ---------------------------------------------------------------------------
// 3. offset_outward_expands_bounds
// ---------------------------------------------------------------------------
#[test]
fn offset_outward_expands_bounds() {
    let rect = shape_to_vecpath(ShapeKind::Rectangle, 10.0, 10.0, 0.0);
    let result = offset_path(&rect, 2.0, OffsetDirection::Outward, CornerStyle::Miter);

    assert_eq!(
        result.len(),
        1,
        "Outward offset should produce exactly one path"
    );
    assert!(!result[0].is_empty(), "Offset path should not be empty");

    let bounds = result[0].bounds().expect("offset path should have bounds");
    assert!(
        bounds.width() > 10.0,
        "Outward offset width ({:.2}) should exceed original 10.0",
        bounds.width()
    );
    assert!(
        bounds.height() > 10.0,
        "Outward offset height ({:.2}) should exceed original 10.0",
        bounds.height()
    );
}

// ---------------------------------------------------------------------------
// 4. offset_inward_shrinks_bounds
// ---------------------------------------------------------------------------
#[test]
fn offset_inward_shrinks_bounds() {
    let rect = shape_to_vecpath(ShapeKind::Rectangle, 20.0, 20.0, 0.0);
    let result = offset_path(&rect, 2.0, OffsetDirection::Inward, CornerStyle::Miter);

    assert_eq!(
        result.len(),
        1,
        "Inward offset should produce exactly one path"
    );
    assert!(!result[0].is_empty(), "Offset path should not be empty");

    let bounds = result[0].bounds().expect("offset path should have bounds");
    assert!(
        bounds.width() < 20.0,
        "Inward offset width ({:.2}) should be less than original 20.0",
        bounds.width()
    );
    assert!(
        bounds.height() < 20.0,
        "Inward offset height ({:.2}) should be less than original 20.0",
        bounds.height()
    );
}

// ---------------------------------------------------------------------------
// 5. apply_radius_adds_curves
// ---------------------------------------------------------------------------
#[test]
fn apply_radius_adds_curves() {
    // A plain rectangle has 5 commands: MoveTo + 3 LineTo + Close
    let rect = shape_to_vecpath(ShapeKind::Rectangle, 10.0, 10.0, 0.0);
    let original_cmd_count: usize = rect.subpaths.iter().map(|sp| sp.commands.len()).sum();

    let filleted = apply_radius(&rect, 2.0);

    assert!(!filleted.is_empty(), "Filleted path should not be empty");

    // apply_radius flattens to a polyline and inserts fillet points at each
    // corner, producing more LineTo commands than the original.
    let filleted_cmd_count: usize = filleted.subpaths.iter().map(|sp| sp.commands.len()).sum();
    assert!(
        filleted_cmd_count > original_cmd_count,
        "Filleted path should have more commands ({}) than original ({})",
        filleted_cmd_count,
        original_cmd_count
    );
}

// ---------------------------------------------------------------------------
// 6. set_start_point_rotates_closed_path
// ---------------------------------------------------------------------------
#[test]
fn set_start_point_rotates_closed_path() {
    let square = VecPath::parse_svg_d("M0 0 L10 0 L10 10 L0 10 Z");
    let rotated = set_start_point(&square, 10.0, 0.0);

    assert!(!rotated.is_empty(), "Rotated path should not be empty");
    assert!(
        rotated.subpaths[0].closed,
        "First subpath should remain closed after set_start_point"
    );
}

// ---------------------------------------------------------------------------
// 7. break_apart_splits_compound_path
// ---------------------------------------------------------------------------
#[test]
fn break_apart_splits_compound_path() {
    let compound = "M0 0 L10 0 Z M20 0 L30 0 Z";
    let parts = break_apart(compound);

    assert_eq!(
        parts.len(),
        2,
        "Compound path with 2 subpaths should break into 2 paths"
    );
}

// ---------------------------------------------------------------------------
// 8. close_path_appends_z
// ---------------------------------------------------------------------------
#[test]
fn close_path_appends_z() {
    let open = "M0 0 L10 0 L10 10";
    let closed = close_path(open);

    assert!(
        closed.ends_with('Z'),
        "close_path should append 'Z'; got: {}",
        closed
    );
}

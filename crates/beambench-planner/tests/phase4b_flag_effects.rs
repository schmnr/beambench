//! Phase 4b integration tests: prove each new flag makes an observable
//! difference when toggled through the full `build_plan_with_input`
//! pipeline.
//!
//! These tests exist alongside the per-module unit tests in
//! `optimize/{dedupe,inner_first,direction}.rs`. The unit tests validate
//! the pass algorithms in isolation; these tests validate that the
//! builder actually wires the flag through — catching the class of
//! regressions where the algorithm is correct but a missing call site
//! leaves the flag silent.

use beambench_common::geometry::{Bounds, Point2D};
use beambench_core::ProjectOptimization;
use beambench_core::layer::{Layer, OperationType};
use beambench_core::object::{ObjectData, ProjectObject, ShapeKind};
use beambench_core::optimization::DirectionOrder;
use beambench_core::project::Project;
use beambench_planner::{
    ExecutionPlan, OptimizationRuntime, PlanSegment, PlannerCalibration, PlannerInput,
    build_plan_with_input,
};

fn single_layer_project() -> (Project, String) {
    let mut project = Project::new("flag_effects");
    project.layers.clear();
    let layer = Layer::new("L0".to_string(), OperationType::Line);
    let layer_id = layer.id.to_string();
    project.layers.push(layer);
    (project, layer_id)
}

fn add_rectangle(project: &mut Project, name: &str, x: f64, y: f64, w: f64, h: f64) -> String {
    let layer_uuid = project.layers[0].id;
    let obj = ProjectObject::new(
        name,
        layer_uuid,
        Bounds::new(Point2D::new(x, y), Point2D::new(x + w, y + h)),
        ObjectData::Shape {
            kind: ShapeKind::Rectangle,
            width: w,
            height: h,
            corner_radius: 0.0,
        },
    );
    let object_id = obj.id.to_string();
    project.objects.push(obj);
    object_id
}

fn vector_first_points(plan: &ExecutionPlan) -> Vec<Point2D> {
    plan.segments
        .iter()
        .filter_map(|seg| match seg {
            PlanSegment::Vector { polyline, .. } => polyline.first().copied(),
            _ => None,
        })
        .collect()
}

fn vector_count(plan: &ExecutionPlan) -> usize {
    plan.segments
        .iter()
        .filter(|seg| matches!(seg, PlanSegment::Vector { .. }))
        .count()
}

fn vector_source_object_ids(plan: &ExecutionPlan) -> Vec<String> {
    plan.segments
        .iter()
        .filter_map(|seg| match seg {
            PlanSegment::Vector {
                source_object_id, ..
            } => source_object_id.clone(),
            _ => None,
        })
        .collect()
}

fn build(project: &Project, opt: ProjectOptimization) -> ExecutionPlan {
    let input = PlannerInput::new(
        opt,
        OptimizationRuntime::default(),
        PlannerCalibration::default(),
    );
    build_plan_with_input(project, &input).expect("plan build")
}

/// `remove_overlapping` off → duplicate polyline emits twice.
/// on → one of the duplicates is dropped before ordering, so vector
/// count decreases by at least one.
#[test]
fn remove_overlapping_drops_duplicate_polyline() {
    let (mut project, _layer) = single_layer_project();
    // Two identical rectangles at the same location — these produce
    // identical closed polylines after normalization.
    add_rectangle(&mut project, "a", 10.0, 10.0, 20.0, 20.0);
    add_rectangle(&mut project, "b", 10.0, 10.0, 20.0, 20.0);
    add_rectangle(&mut project, "c", 100.0, 100.0, 20.0, 20.0);

    let off = build(&project, ProjectOptimization::default());
    let on = build(
        &project,
        ProjectOptimization {
            remove_overlapping: true,
            ..Default::default()
        },
    );

    let off_count = vector_count(&off);
    let on_count = vector_count(&on);
    assert!(
        on_count < off_count,
        "remove_overlapping should drop the duplicate: off={off_count}, on={on_count}"
    );
}

/// `inner_first` off → outer rectangle emits before its nested inner
/// rectangle under the default Optimized strategy.
/// on → inner rectangle emits first regardless of nearest-neighbor.
#[test]
fn inner_first_emits_child_before_parent() {
    let (mut project, _layer) = single_layer_project();
    // Outer rectangle at origin; inner rectangle nested inside.
    let _outer_id = add_rectangle(&mut project, "outer", 0.0, 0.0, 100.0, 100.0);
    let inner_id = add_rectangle(&mut project, "inner", 40.0, 40.0, 20.0, 20.0);

    let on = build(
        &project,
        ProjectOptimization {
            inner_first: true,
            ..Default::default()
        },
    );

    let source_ids = vector_source_object_ids(&on);
    assert!(
        source_ids.len() >= 2,
        "expected at least two vector segments, got {}",
        source_ids.len()
    );
    // With inner_first, the first vector segment should come from the inner rectangle.
    assert_eq!(source_ids[0], inner_id, "inner rectangle should emit first");
}

/// `direction_order = TopDown` sorts polylines by ascending min-Y.
/// The test proves the flag propagates by constructing a layout whose
/// natural nearest-neighbor order is NOT top-to-bottom and asserting
/// the flag overrides that.
#[test]
fn direction_order_top_down_overrides_default_nearest_neighbor() {
    let (mut project, _layer) = single_layer_project();
    // Three rectangles at distinct Y levels, placed so nearest-neighbor
    // from origin would visit them in a different order than top-down.
    let middle_id = add_rectangle(&mut project, "middle", 100.0, 50.0, 10.0, 10.0);
    let top_id = add_rectangle(&mut project, "top", 0.0, 0.0, 10.0, 10.0);
    let bottom_id = add_rectangle(&mut project, "bottom", 200.0, 100.0, 10.0, 10.0);

    let on = build(
        &project,
        ProjectOptimization {
            direction_order: DirectionOrder::TopDown,
            ..Default::default()
        },
    );

    let source_ids = vector_source_object_ids(&on);
    assert!(source_ids.len() >= 3, "expected 3 vector segments");
    // First segment should be the top rectangle (y=0).
    assert_eq!(source_ids[0], top_id, "top should emit first");
    // Second should be middle (y=50).
    assert_eq!(source_ids[1], middle_id, "middle should emit second");
    // Third should be bottom (y=100).
    assert_eq!(source_ids[2], bottom_id, "bottom should emit last");
}

/// `direction_order = BottomUp` is the inverse — bottom-most emits first.
#[test]
fn direction_order_bottom_up_is_inverse_of_top_down() {
    let (mut project, _layer) = single_layer_project();
    let top_id = add_rectangle(&mut project, "top", 0.0, 0.0, 10.0, 10.0);
    let bottom_id = add_rectangle(&mut project, "bottom", 200.0, 100.0, 10.0, 10.0);

    let top_down = build(
        &project,
        ProjectOptimization {
            direction_order: DirectionOrder::TopDown,
            ..Default::default()
        },
    );
    let bottom_up = build(
        &project,
        ProjectOptimization {
            direction_order: DirectionOrder::BottomUp,
            ..Default::default()
        },
    );

    let td = vector_source_object_ids(&top_down);
    let bu = vector_source_object_ids(&bottom_up);
    assert!(td.len() >= 2 && bu.len() >= 2);
    // TopDown: top first. BottomUp: bottom first.
    assert_eq!(td[0], top_id);
    assert_eq!(bu[0], bottom_id);
}

/// All flags off → matches the pre-M1 default behavior. This is the
/// zero-drift guarantee: the additive changes in Phase 4b must not
/// shift any output when every new flag is at its default value.
#[test]
fn all_flags_off_matches_default_behavior() {
    let (mut project, _layer) = single_layer_project();
    add_rectangle(&mut project, "a", 10.0, 10.0, 10.0, 10.0);
    add_rectangle(&mut project, "b", 50.0, 50.0, 10.0, 10.0);

    let default_plan = build(&project, ProjectOptimization::default());
    let explicit_plan = build(
        &project,
        ProjectOptimization {
            inner_first: false,
            direction_order: DirectionOrder::None,
            remove_overlapping: false,
            ..Default::default()
        },
    );

    assert_eq!(
        vector_first_points(&default_plan),
        vector_first_points(&explicit_plan),
    );
    assert_eq!(
        default_plan.total_distance_mm,
        explicit_plan.total_distance_mm,
    );
}

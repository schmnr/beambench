//! Characterization tests for the current sort behavior of two §10
//! Beam Bench optimization behaviors covered here:
//!
//! - **Order by Layer** — segments from a lower `Layer.order_index` emit
//!   before any segment of a higher one. This is structurally enforced
//!   by the outer per-layer loop in `builder.rs` and holds under every
//!   `CutOrderStrategy`.
//! - **Order by Priority** — within a layer, objects are pre-sorted by
//!   `ProjectObject.priority` ascending at `builder.rs:1538`. That sort
//!   is preserved into the emitted segment order **only when
//!   `CutOrderStrategy::AsDrawn`** — other strategies (Optimized,
//!   InsideFirst, OutsideFirst) re-sort the polylines within each layer
//!   via `order_vectors_generic`, overriding the priority seeding.
//!
//! These tests capture that behavior so changes to `optimize/sort_keys.rs`
//! can prove they preserve semantics. The tests cover `layer_partition` under
//! `ProjectOptimization::default()`, and the priority tests under
//! `order_by_priority: true, reduce_travel: false` (the post-refactor
//! equivalent of `AsDrawn`).

use beambench_common::geometry::{Bounds, Point2D};
use beambench_core::ProjectOptimization;
use beambench_core::layer::{Layer, OperationType};
use beambench_core::object::{ObjectData, ProjectObject, ShapeKind};
use beambench_core::project::Project;
use beambench_planner::{
    OptimizationRuntime, PlanSegment, PlannerCalibration, PlannerInput, build_plan,
    build_plan_with_input,
};

/// Extract the (object_id, layer_id) of every Vector segment in the plan,
/// in the order they appear. Travel/Raster/Frame/OffsetFill segments are
/// ignored — they're not part of the ordering invariant under test.
fn vector_sequence(plan: &beambench_planner::ExecutionPlan) -> Vec<(Option<String>, String)> {
    plan.segments
        .iter()
        .filter_map(|seg| match seg {
            PlanSegment::Vector {
                source_object_id,
                layer_id,
                ..
            } => Some((source_object_id.clone(), layer_id.clone())),
            _ => None,
        })
        .collect()
}

/// Build a project with N layers, each containing rectangles with the
/// given priorities. Rectangles are stamped at visually distinct
/// positions so no spatial coincidence can mask a missed sort.
///
/// `per_layer[l]` = list of (object_name, priority) for layer l.
/// Objects are added to the project in a deliberately "wrong" insertion
/// order (reverse priority within each layer, layers interleaved) so that
/// *any* sort that relies on insertion order will produce a different
/// result from the expected priority-then-layer ordering.
fn make_multi_layer_project(per_layer: &[Vec<(&str, i32)>]) -> (Project, Vec<String>) {
    let mut project = Project::new("characterization");

    // Strip the default layer — we want explicit control.
    project.layers.clear();

    let mut layer_ids = Vec::new();
    for (li, entries) in per_layer.iter().enumerate() {
        let mut layer = Layer::new(format!("L{li}"), OperationType::Line);
        layer.order_index = li as u32;
        layer_ids.push(layer.id.to_string());
        project.layers.push(layer);
        let _ = entries; // used below after layers are set
    }

    // Add objects in interleaved, reverse-priority insertion order so
    // the plan's sort must actually happen for assertions to pass.
    let max_len = per_layer.iter().map(|v| v.len()).max().unwrap_or(0);
    for i in (0..max_len).rev() {
        for (li, entries) in per_layer.iter().enumerate() {
            if i >= entries.len() {
                continue;
            }
            let (name, priority) = entries[i];
            let layer_uuid = project.layers[li].id;
            let x = 10.0 + (li as f64) * 100.0 + (i as f64) * 20.0;
            let y = 10.0 + (li as f64) * 100.0 + (i as f64) * 20.0;
            let mut obj = ProjectObject::new(
                name,
                layer_uuid,
                Bounds::new(Point2D::new(x, y), Point2D::new(x + 10.0, y + 10.0)),
                ObjectData::Shape {
                    kind: ShapeKind::Rectangle,
                    width: 10.0,
                    height: 10.0,
                    corner_radius: 0.0,
                },
            );
            obj.priority = priority;
            project.objects.push(obj);
        }
    }

    (project, layer_ids)
}

/// Map each Vector segment to the name of its source object (for readable
/// assertions). Returns None for segments without a resolvable source.
fn names_in_order(
    plan: &beambench_planner::ExecutionPlan,
    project: &Project,
) -> Vec<Option<String>> {
    vector_sequence(plan)
        .into_iter()
        .map(|(src_id, _)| {
            src_id.and_then(|id| {
                project
                    .objects
                    .iter()
                    .find(|o| o.id.to_string() == id)
                    .map(|o| o.name.clone())
            })
        })
        .collect()
}

/// Order by Layer (on-case): segments from layer 0 must all appear before
/// any segment from layer 1, regardless of insertion order.
#[test]
fn layer_partition_is_strict_under_defaults() {
    let (project, layer_ids) =
        make_multi_layer_project(&[vec![("a0", 0), ("a1", 0)], vec![("b0", 0), ("b1", 0)]]);
    let plan = build_plan(&project).expect("plan build");

    let seq = vector_sequence(&plan);
    assert!(!seq.is_empty(), "expected vector segments");

    // Find the boundary: index of first layer-1 segment.
    let first_layer1 = seq
        .iter()
        .position(|(_, lid)| lid == &layer_ids[1])
        .expect("layer 1 should emit at least one segment");

    // Everything before the boundary is layer 0; everything after is layer 1.
    for (i, (_, lid)) in seq.iter().enumerate() {
        if i < first_layer1 {
            assert_eq!(
                lid, &layer_ids[0],
                "segment {i} expected layer 0, got {lid}"
            );
        } else {
            assert_eq!(
                lid, &layer_ids[1],
                "segment {i} expected layer 1, got {lid}"
            );
        }
    }
}

/// Input that preserves the priority-sorted insertion order by
/// forcing the per-layer pipeline into `AsDrawn` mode. `inner_first:
/// true` on flat (non-nested) geometry is a no-op that triggers the
/// `force_as_drawn` gate — pure identity preservation of the
/// priority seed. Using `direction_order` here would be wrong: it
/// stable-sorts by axial key, so objects at distinct Y positions
/// would reshuffle regardless of priority.
fn as_drawn_input() -> PlannerInput {
    PlannerInput::new(
        ProjectOptimization {
            inner_first: true,
            ..Default::default()
        },
        OptimizationRuntime::default(),
        PlannerCalibration::default(),
    )
}

/// Order by Priority (on-case, AsDrawn): within a single layer, objects
/// with lower `priority` emit before objects with higher `priority`. Uses
/// `AsDrawn` so the builder's priority seed at `builder.rs:1538` survives
/// into the emitted segment order without being overwritten by
/// nearest-neighbor reordering.
#[test]
fn priority_is_ascending_within_layer_under_as_drawn() {
    let (project, _layer_ids) =
        make_multi_layer_project(&[vec![("high", 10), ("low", 0), ("mid", 5)]]);
    let input = as_drawn_input();
    let plan = build_plan_with_input(&project, &input).expect("plan build");

    let resolved: Vec<String> = names_in_order(&plan, &project)
        .into_iter()
        .flatten()
        .collect();
    assert!(
        resolved.contains(&"low".to_string())
            && resolved.contains(&"mid".to_string())
            && resolved.contains(&"high".to_string()),
        "all three objects should be represented, got {resolved:?}"
    );

    let low = resolved.iter().position(|n| n == "low").unwrap();
    let mid = resolved.iter().position(|n| n == "mid").unwrap();
    let high = resolved.iter().position(|n| n == "high").unwrap();
    assert!(
        low < mid && mid < high,
        "expected priority order low<mid<high, got positions low={low}, mid={mid}, high={high} in {resolved:?}"
    );
}

/// Combined (on-case, AsDrawn): priority sort applies **within** each
/// layer window, not across layers. Given 2 layers × 2 priorities each,
/// the expected emission is L0-low, L0-high, L1-low, L1-high —
/// never interleaved.
#[test]
fn layer_then_priority_combined_under_as_drawn() {
    let (project, _layer_ids) = make_multi_layer_project(&[
        vec![("l0_high", 5), ("l0_low", 1)],
        vec![("l1_high", 5), ("l1_low", 1)],
    ]);
    let input = as_drawn_input();
    let plan = build_plan_with_input(&project, &input).expect("plan build");

    let resolved: Vec<String> = names_in_order(&plan, &project)
        .into_iter()
        .flatten()
        .collect();

    let pos = |needle: &str| {
        resolved
            .iter()
            .position(|n| n == needle)
            .unwrap_or_else(|| panic!("{needle} missing from {resolved:?}"))
    };

    let l0_low = pos("l0_low");
    let l0_high = pos("l0_high");
    let l1_low = pos("l1_low");
    let l1_high = pos("l1_high");

    // Within L0: low before high.
    assert!(l0_low < l0_high, "L0 priority sort failed: {resolved:?}");
    // Within L1: low before high.
    assert!(l1_low < l1_high, "L1 priority sort failed: {resolved:?}");
    // All of L0 before all of L1 (layer partition).
    assert!(
        l0_high < l1_low,
        "layer partition failed; L0-high at {l0_high} should precede L1-low at {l1_low} in {resolved:?}"
    );
}

/// Order by Priority under default (Optimized) strategy is documented
/// here as a **weaker** behavior: priority seeds the input order, but
/// `order_vectors_generic` with `CutOrderStrategy::Optimized` re-sorts
/// polylines within each layer via nearest-neighbor, so priority is not
/// a strict sort key in the default output. This test pins that
/// observation so any future claim that "priority always wins" will fail
/// here and force a conscious design decision.
#[test]
fn priority_is_not_strict_under_default_optimized_strategy() {
    // Three objects at visually distinct positions with non-monotone
    // priorities vs. spatial proximity. Nearest-neighbor starts at (0,0)
    // and picks the closest polyline, which does not match priority order.
    let (project, _layer_ids) = make_multi_layer_project(&[vec![
        ("high", 10), // near origin
        ("low", 0),   // far from origin
        ("mid", 5),
    ]]);
    let plan = build_plan(&project).expect("plan build");

    let resolved: Vec<String> = names_in_order(&plan, &project)
        .into_iter()
        .flatten()
        .collect();

    // If priority were a strict sort key, "low" (priority 0) would be
    // first. With Optimized strategy, "high" sits closer to (0,0) so
    // nearest-neighbor picks it first. The exact order depends on
    // fixture geometry, but "low" should NOT be first.
    assert!(
        !resolved.is_empty(),
        "expected at least one resolved segment"
    );
    assert_ne!(
        resolved.first().map(String::as_str),
        Some("low"),
        "expected priority to be overridden by Optimized nearest-neighbor; got {resolved:?}"
    );
}

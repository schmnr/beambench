//! Integration test: verify tabs are applied during plan building.

use beambench_common::geometry::{Bounds, Point2D};
use beambench_core::layer::OperationType;
use beambench_core::project::Project;
use beambench_core::{ObjectData, ProjectObject};
use beambench_planner::{PlanSegment, build_plan};

/// Create a project with a closed rectangle vector object on a layer with tabs configured.
fn make_tabbed_project(tab_count: u32, tab_width_mm: f64) -> Project {
    let mut project = Project::new("TabTest");
    let layer_id = project.ensure_default_layer();

    // Configure the layer for vector cutting with tabs
    {
        let layer = project.find_layer_mut(layer_id).unwrap();
        layer.primary_entry_mut().operation = OperationType::Cut;
        layer.primary_entry_mut().speed_mm_min = 1000.0;
        layer.primary_entry_mut().power_percent = 80.0;
        let mut vs = layer
            .primary_entry()
            .vector_settings
            .clone()
            .unwrap_or_default();
        vs.tab_count = tab_count;
        vs.tab_width_mm = tab_width_mm;
        layer.primary_entry_mut().vector_settings = Some(vs);
    }

    // Add a closed rectangle as a VectorPath (M...L...L...L...Z)
    let path_data = "M 0 0 L 40 0 L 40 30 L 0 30 Z".to_string();
    let obj = ProjectObject::new(
        "Rectangle",
        layer_id,
        Bounds::new(Point2D::new(10.0, 10.0), Point2D::new(50.0, 40.0)),
        ObjectData::VectorPath {
            path_data,
            closed: true,
            ruler_guide_axis: None,
        },
    );
    project.add_object(obj);
    project
}

#[test]
fn tabs_are_applied_in_build_plan() {
    let project = make_tabbed_project(4, 2.0);
    let plan = build_plan(&project).unwrap();

    // With 4 tabs on a closed rectangle, the single Vector segment should be
    // split into multiple segments with Travel gaps between them.
    let travel_count = plan
        .segments
        .iter()
        .filter(|s| matches!(s, PlanSegment::Travel { .. }))
        .count();
    let vector_count = plan
        .segments
        .iter()
        .filter(|s| matches!(s, PlanSegment::Vector { .. }))
        .count();

    // With tabs, we should have more than 1 vector segment (the original is split)
    // and travel segments appear within the path (tab gaps).
    assert!(
        vector_count > 1,
        "Expected multiple vector segments after tab splitting, got {vector_count}"
    );
    assert!(
        travel_count >= 4,
        "Expected at least 4 travel segments (tab gaps), got {travel_count}"
    );
}

#[test]
fn no_tabs_when_tab_count_zero() {
    let project = make_tabbed_project(0, 2.0);
    let plan = build_plan(&project).unwrap();

    // With tab_count=0, the rectangle should produce exactly 1 vector segment
    // (plus travel to/from it).
    let vector_count = plan
        .segments
        .iter()
        .filter(|s| matches!(s, PlanSegment::Vector { .. }))
        .count();

    assert_eq!(
        vector_count, 1,
        "Expected exactly 1 vector segment with no tabs, got {vector_count}"
    );
}

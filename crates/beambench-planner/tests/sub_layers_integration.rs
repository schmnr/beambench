use beambench_common::geometry::{Bounds, Point2D};
use beambench_core::OptimizationOrderKey;
use beambench_core::layer::{CutEntry, Layer, OperationType};
use beambench_core::object::{ObjectData, ProjectObject, ShapeKind};
use beambench_core::project::Project;
use beambench_planner::{
    ExecutionPlan, OptimizationRuntime, PlanSegment, PlannerCalibration, PlannerInput,
    build_plan_with_input,
};

fn build(project: &Project) -> ExecutionPlan {
    let input = PlannerInput::new(
        project.optimization.clone(),
        OptimizationRuntime::default(),
        PlannerCalibration::default(),
    );
    build_plan_with_input(project, &input).expect("plan build")
}

fn single_layer_project() -> (Project, beambench_core::LayerId) {
    let mut project = Project::new("sub-layers");
    project.layers.clear();
    let layer = Layer::new_single_entry("L0", OperationType::Line);
    let layer_id = layer.id;
    project.layers.push(layer);
    (project, layer_id)
}

fn add_rectangle(project: &mut Project, layer_id: beambench_core::LayerId) {
    let obj = ProjectObject::new(
        "rect",
        layer_id,
        Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(20.0, 10.0)),
        ObjectData::Shape {
            kind: ShapeKind::Rectangle,
            width: 20.0,
            height: 10.0,
            corner_radius: 0.0,
        },
    );
    project.objects.push(obj);
}

fn add_prioritized_rectangle(
    project: &mut Project,
    layer_id: beambench_core::LayerId,
    name: &str,
    min_x: f64,
    priority: i32,
) -> String {
    let mut obj = ProjectObject::new(
        name,
        layer_id,
        Bounds::new(Point2D::new(min_x, 0.0), Point2D::new(min_x + 10.0, 10.0)),
        ObjectData::Shape {
            kind: ShapeKind::Rectangle,
            width: 10.0,
            height: 10.0,
            corner_radius: 0.0,
        },
    );
    obj.priority = priority;
    let id = obj.id.to_string();
    project.objects.push(obj);
    id
}

#[test]
fn line_then_fill_entries_emit_in_declared_order_with_distinct_entry_ids() {
    let (mut project, layer_id) = single_layer_project();
    add_rectangle(&mut project, layer_id);

    let layer = project
        .layers
        .iter_mut()
        .find(|layer| layer.id == layer_id)
        .unwrap();
    let line_entry_id = layer.entries[0].id.to_string();
    layer.entries[0].speed_mm_min = 1200.0;
    layer.entries[0].power_percent = 30.0;

    let mut fill_entry = CutEntry::new(OperationType::Fill);
    fill_entry.speed_mm_min = 800.0;
    fill_entry.power_percent = 55.0;
    let fill_entry_id = fill_entry.id.to_string();
    layer.entries.push(fill_entry);

    let plan = build(&project);

    let first_line_index = plan
        .segments
        .iter()
        .position(|segment| match segment {
            PlanSegment::Vector { cut_entry_id, .. } => cut_entry_id == &line_entry_id,
            _ => false,
        })
        .expect("expected line vector segment");
    let first_fill_index = plan
        .segments
        .iter()
        .position(|segment| match segment {
            PlanSegment::Raster { cut_entry_id, .. } => cut_entry_id == &fill_entry_id,
            _ => false,
        })
        .expect("expected fill raster segment");

    assert!(
        first_line_index < first_fill_index,
        "entries should emit in declared order"
    );
}

#[test]
fn disabled_entry_emits_no_segments() {
    let (mut project, layer_id) = single_layer_project();
    add_rectangle(&mut project, layer_id);

    let layer = project
        .layers
        .iter_mut()
        .find(|layer| layer.id == layer_id)
        .unwrap();
    let mut disabled = CutEntry::new(OperationType::Line);
    disabled.output_enabled = false;
    let disabled_id = disabled.id.to_string();
    layer.entries.push(disabled);

    let plan = build(&project);

    let emitted_disabled = plan.segments.iter().any(|segment| match segment {
        PlanSegment::Vector { cut_entry_id, .. } | PlanSegment::Raster { cut_entry_id, .. } => {
            cut_entry_id == &disabled_id
        }
        _ => false,
    });

    assert!(
        !emitted_disabled,
        "disabled entry should not emit plan segments"
    );
}

#[test]
fn image_entry_on_vector_geometry_warns_and_skips() {
    let (mut project, layer_id) = single_layer_project();
    add_rectangle(&mut project, layer_id);

    let layer = project
        .layers
        .iter_mut()
        .find(|layer| layer.id == layer_id)
        .unwrap();
    let image_entry = CutEntry::new(OperationType::Image);
    let image_entry_id = image_entry.id.to_string();
    layer.entries.push(image_entry);

    let plan = build(&project);

    assert!(
        plan.warnings
            .iter()
            .any(|warning| warning.message.contains("Image sub-layer")),
        "expected skip warning for image entry over vector geometry"
    );

    let emitted_image = plan.segments.iter().any(|segment| match segment {
        PlanSegment::Vector { cut_entry_id, .. } | PlanSegment::Raster { cut_entry_id, .. } => {
            cut_entry_id == &image_entry_id
        }
        _ => false,
    });
    assert!(
        !emitted_image,
        "image entry should not emit vector geometry"
    );
}

#[test]
fn image_line_offset_fill_entries_emit_in_declared_order() {
    let (mut project, layer_id) = single_layer_project();
    add_rectangle(&mut project, layer_id);

    let layer = project
        .layers
        .iter_mut()
        .find(|layer| layer.id == layer_id)
        .unwrap();
    layer.entries.clear();

    let image_entry = CutEntry::new(OperationType::Image);
    let image_entry_id = image_entry.id.to_string();
    let line_entry = CutEntry::new(OperationType::Line);
    let line_entry_id = line_entry.id.to_string();
    let offset_entry = CutEntry::new(OperationType::OffsetFill);
    let offset_entry_id = offset_entry.id.to_string();
    layer
        .entries
        .extend([image_entry, line_entry, offset_entry]);

    let plan = build(&project);

    assert!(
        plan.warnings
            .iter()
            .any(|warning| warning.message.contains(&image_entry_id)),
        "expected image entry skip warning"
    );

    let line_index = plan
        .segments
        .iter()
        .position(|segment| matches!(segment, PlanSegment::Vector { cut_entry_id, .. } if cut_entry_id == &line_entry_id))
        .expect("expected line entry vector segment");
    let offset_index = plan
        .segments
        .iter()
        .position(|segment| matches!(segment, PlanSegment::Vector { cut_entry_id, .. } if cut_entry_id == &offset_entry_id))
        .expect("expected offset fill entry vector segment");
    assert!(
        line_index < offset_index,
        "line entry should run before offset-fill entry"
    );

    let emitted_image = plan.segments.iter().any(|segment| match segment {
        PlanSegment::Vector { cut_entry_id, .. } | PlanSegment::Raster { cut_entry_id, .. } => {
            cut_entry_id == &image_entry_id
        }
        _ => false,
    });
    assert!(
        !emitted_image,
        "image entry should not emit vector geometry"
    );
}

#[test]
fn order_by_priority_applies_consistently_across_entries() {
    let (mut project, layer_id) = single_layer_project();
    project.optimization.ordering =
        vec![OptimizationOrderKey::Layer, OptimizationOrderKey::Priority];
    project.optimization.reduce_travel = false;

    let low_id = add_prioritized_rectangle(&mut project, layer_id, "low", 40.0, 10);
    let high_id = add_prioritized_rectangle(&mut project, layer_id, "high", 0.0, 0);

    let layer = project
        .layers
        .iter_mut()
        .find(|layer| layer.id == layer_id)
        .unwrap();
    let first_entry_id = layer.entries[0].id.to_string();
    let second_entry = CutEntry::new(OperationType::Line);
    let second_entry_id = second_entry.id.to_string();
    layer.entries.push(second_entry);

    let plan = build(&project);

    let order_for_entry = |entry_id: &str| -> Vec<String> {
        let mut seen = Vec::<String>::new();
        for segment in &plan.segments {
            if let PlanSegment::Vector {
                cut_entry_id,
                source_object_id: Some(source_object_id),
                ..
            } = segment
                && cut_entry_id == entry_id
                && seen.last() != Some(source_object_id)
            {
                seen.push(source_object_id.clone());
            }
        }
        seen
    };

    let first_order = order_for_entry(&first_entry_id);
    let second_order = order_for_entry(&second_entry_id);
    assert_eq!(
        first_order, second_order,
        "priority order should be shared across entries"
    );
    assert_eq!(
        first_order,
        vec![high_id, low_id],
        "higher-priority object should emit first"
    );
}

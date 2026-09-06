// Historical probes for the defects in commit 65b2b10f.
// These assert the original bugs and are not part of the acceptance suite.
// Passing regression tests live in crates/beambench-service/src/review_regressions.rs.

use beambench_common::{Bounds, Point2D};
use beambench_core::{ObjectData, Project, ProjectObject, ShapeKind};
use beambench_service::{ServiceContext, ops::{planning, persistence}};
use std::{sync::Arc, time::{Duration, Instant}};

#[test]
fn reproduce_dirty_state_missing_from_ipc_project() {
    let mut project = Project::new("dirty");
    project.dirty = true;
    let payload = serde_json::to_value(&project).unwrap();
    assert!(payload.get("dirty").is_none(), "Bug reproduced: frontend never receives authoritative dirty state");
}

#[test]
fn reproduce_save_marks_unsaved_edit_clean() {
    assert!(std::env::var_os("BEAMBENCH_CONFIG_DIR").is_some(), "Run probes with isolated config directory");
    let ctx = Arc::new(ServiceContext::new());
    *ctx.project.lock().unwrap() = Some(Project::new("original"));
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("saved.lzrproj");
    let path_lock = ctx.project_path.lock().unwrap();
    let worker_ctx = ctx.clone();
    let worker_path = path.clone();
    let worker = std::thread::spawn(move || persistence::save_project_to_path(&worker_ctx, &worker_path));
    let deadline = Instant::now() + Duration::from_secs(10);
    while !path.exists() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    {
        let mut guard = ctx.project.lock().unwrap();
        let project = guard.as_mut().unwrap();
        project.material_height_mm = Some(17.0);
        project.dirty = true;
    }
    drop(path_lock);
    worker.join().unwrap().unwrap();
    let disk = beambench_project::load_project(&path).unwrap();
    let guard = ctx.project.lock().unwrap();
    let memory = guard.as_ref().unwrap();
    assert_ne!(disk.material_height_mm, memory.material_height_mm);
    assert!(!memory.dirty, "Bug reproduced: unsaved edit is marked clean");
}

#[test]
fn reproduce_selected_clone_loses_its_source() {
    let ctx = ServiceContext::new();
    let mut project = Project::new("clone");
    let layer = project.ensure_default_layer();
    let source = ProjectObject::new("source", layer,
        Bounds::new(Point2D::new(10.0, 10.0), Point2D::new(20.0, 20.0)),
        ObjectData::Shape { kind: ShapeKind::Rectangle, width: 10.0, height: 10.0, corner_radius: 0.0 });
    let clone = ProjectObject::new("clone", layer,
        Bounds::new(Point2D::new(30.0, 10.0), Point2D::new(40.0, 20.0)),
        ObjectData::VirtualClone { source_id: source.id });
    let clone_id = clone.id.to_string();
    project.add_object(source);
    project.add_object(clone);
    let selected_pdf = beambench_core::export_pdf(&project, true, &[project.objects[1].id]);
    assert!(!String::from_utf8(selected_pdf).unwrap().contains(" RG\n"), "Bug reproduced: selected clone is omitted from PDF");
    *ctx.project.lock().unwrap() = Some(project);
    assert!(planning::generate_plan(&ctx).is_ok());
    let selected = planning::generate_plan_with_options(&ctx, &planning::SessionJobOptions {
        cut_selected_graphics: true, use_selection_origin: false, selected_object_ids: vec![clone_id],
    });
    assert!(selected.is_err() || selected.as_ref().is_ok_and(|plan| !plan.segments.iter().any(|segment| matches!(segment,
        beambench_planner::PlanSegment::Vector { .. } | beambench_planner::PlanSegment::Raster { .. }
    ))), "Bug reproduced: clone-only selection cannot generate geometry");
}

#[test]
fn reproduce_undo_after_save_restores_clean_flag_for_different_contents() {
    assert!(std::env::var_os("BEAMBENCH_CONFIG_DIR").is_some());
    let ctx = ServiceContext::new();
    let before = Project::new("before");
    ctx.push_project_undo_snapshot(&before).unwrap();
    let mut edited = before;
    edited.material_height_mm = Some(17.0);
    edited.dirty = true;
    *ctx.project.lock().unwrap() = Some(edited);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("saved.lzrproj");
    persistence::save_project_to_path(&ctx, &path).unwrap();
    let restored = beambench_service::ops::project::undo_project(&ctx).unwrap();
    let disk = beambench_project::load_project(&path).unwrap();
    assert_ne!(restored.material_height_mm, disk.material_height_mm);
    assert!(!restored.dirty, "Bug reproduced: Undo changes saved content but claims project is clean");
}

#[test]
fn reproduce_long_grbl_line_never_advances_or_fails() {
    use beambench_streamer::{StreamingEngine, ProgressTracker};
    let mut session = beambench_grbl::GrblSession::new(Box::new(beambench_serial::MockSerialTransport::new("mock")));
    session.connect().unwrap();
    let mut engine = StreamingEngine::new(vec![format!(";{}", "x".repeat(130))]);
    let mut progress = ProgressTracker::new(1);
    for _ in 0..1000 {
        assert_eq!(engine.send_tick(&mut session, &mut progress).unwrap(), 0);
    }
    assert!(!engine.all_sent());
    assert!(!engine.is_failed());
    assert_eq!(engine.bytes_in_flight(), 0);
}

#[test]
fn reproduce_dxf_drops_closing_edge_and_curve() {
    let mut project = Project::new("export");
    let layer = project.ensure_default_layer();
    project.add_object(ProjectObject::new("triangle", layer,
        Bounds::new(Point2D::new(10.0, 10.0), Point2D::new(30.0, 30.0)),
        ObjectData::VectorPath { path_data: "M10 10 L30 10 L20 30 Z".into(), closed: true, ruler_guide_axis: None }));
    let dxf = beambench_core::export_dxf(&project, false, &[]);
    assert_eq!(dxf.matches("0\nLINE\n").count(), 2, "Bug reproduced: closed triangle exports only two edges");
    project.objects.clear();
    project.add_object(ProjectObject::new("curve", layer,
        Bounds::new(Point2D::new(10.0, 10.0), Point2D::new(30.0, 30.0)),
        ObjectData::VectorPath { path_data: "M10 10 C10 30 30 30 30 10".into(), closed: false, ruler_guide_axis: None }));
    let dxf = beambench_core::export_dxf(&project, false, &[]);
    assert_eq!(dxf.matches("0\nLINE\n").count(), 1, "Bug reproduced: cubic curve exports as one chord");
}

#[test]
fn reproduce_selected_image_loses_mask() {
    use beambench_core::{Asset, AssetMediaType, Layer, OperationType};
    use beambench_core::object::{ImageMaskRef, ImageMaskPolarity};
    use image::ImageEncoder;
    let ctx = ServiceContext::new();
    let mut project = Project::new("masked");
    let layer = Layer::new("image", OperationType::Image);
    let layer_id = layer.id;
    project.layers.push(layer);
    let mut mask_layer_def = Layer::new("mask", OperationType::Line);
    mask_layer_def.enabled = false;
    let mask_layer = mask_layer_def.id;
    project.layers.push(mask_layer_def);
    let mask = ProjectObject::new("mask", mask_layer,
        Bounds::new(Point2D::new(20.0, 20.0), Point2D::new(25.0, 30.0)),
        ObjectData::Shape { kind: ShapeKind::Rectangle, width: 5.0, height: 10.0, corner_radius: 0.0 });
    let mask_id = mask.id;
    project.add_object(mask);
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png).write_image(&[0u8; 100], 10, 10, image::ExtendedColorType::L8).unwrap();
    let asset = Asset::new("black.png", AssetMediaType::Png, png.len() as u64, Some(10), Some(10));
    let asset_id = asset.id;
    project.add_asset(asset, png);
    let object = ProjectObject::new("image", layer_id,
        Bounds::new(Point2D::new(20.0, 20.0), Point2D::new(30.0, 30.0)),
        ObjectData::RasterImage { asset_key: asset_id.to_string(), original_width_px: 10, original_height_px: 10,
            adjustments: None, masks: vec![ImageMaskRef { object_id: mask_id, polarity: ImageMaskPolarity::KeepInside }] });
    let id = object.id.to_string();
    project.add_object(object);
    *ctx.project.lock().unwrap() = Some(project);
    let full = planning::generate_plan(&ctx).unwrap();
    let selected = planning::generate_plan_with_options(&ctx, &planning::SessionJobOptions {
        cut_selected_graphics: true, use_selection_origin: false, selected_object_ids: vec![id],
    }).unwrap();
    let burn_width = |plan: &beambench_planner::ExecutionPlan| -> f64 {
        plan.segments.iter().filter_map(|segment| {
            if let beambench_planner::PlanSegment::Raster { scanlines, .. } = segment {
                Some(scanlines.iter().flat_map(|row| row.runs.iter()).map(|run| (run.end_x_mm-run.start_x_mm).abs()).sum::<f64>())
            } else { None }
        }).sum()
    };
    assert!(burn_width(&selected) > burn_width(&full) * 1.5, "Bug reproduced: selected-only planning engraves masked-out pixels");
}

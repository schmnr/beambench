use crate::{
    ServiceContext,
    ops::{persistence, planning},
};
use beambench_common::{Bounds, Point2D};
use beambench_core::{ObjectData, Project, ProjectObject, ShapeKind};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

#[test]
fn ipc_exposes_dirty_but_document_hash_excludes_it() {
    let mut project = Project::new("dirty");
    project.dirty = true;
    let payload = serde_json::to_value(&project).unwrap();
    assert_eq!(payload["dirty"], true);
    let document = project.document_value().unwrap();
    assert!(document.get("dirty").is_none());
    let hash = planning::revision_hash(&project).unwrap();
    project.dirty = false;
    assert_eq!(hash, planning::revision_hash(&project).unwrap());
}

#[test]
fn save_serializes_path_commit_with_concurrent_edits() {
    let ctx = Arc::new(ServiceContext::new());
    *ctx.project.lock().unwrap() = Some(Project::new("original"));
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("saved.lzrproj");
    let path_lock = ctx.project_path.lock().unwrap();
    let worker_ctx = ctx.clone();
    let worker_path = path.clone();
    let worker =
        std::thread::spawn(move || persistence::save_project_to_path(&worker_ctx, &worker_path));
    let deadline = Instant::now() + Duration::from_secs(5);
    while ctx.project.try_lock().is_ok() {
        assert!(
            Instant::now() < deadline,
            "save must acquire the document lock"
        );
        std::thread::yield_now();
    }
    assert!(
        !path.exists(),
        "the destination must be reserved before writing"
    );
    let edit_ctx = ctx.clone();
    let edit = std::thread::spawn(move || {
        let mut guard = edit_ctx.project.lock().unwrap();
        let project = guard.as_mut().unwrap();
        project.material_height_mm = Some(17.0);
        project.dirty = true;
    });
    drop(path_lock);
    worker.join().unwrap().unwrap();
    edit.join().unwrap();
    let disk = beambench_project::load_project(&path).unwrap();
    let guard = ctx.project.lock().unwrap();
    let memory = guard.as_ref().unwrap();
    assert_ne!(disk.material_height_mm, memory.material_height_mm);
    assert!(memory.dirty);
    assert_eq!(ctx.project_path.lock().unwrap().as_ref(), Some(&path));
}

#[test]
fn selected_clone_keeps_its_source_and_exports_to_pdf() {
    let ctx = ServiceContext::new();
    let mut project = Project::new("clone");
    let layer = project.ensure_default_layer();
    let source = ProjectObject::new(
        "source",
        layer,
        Bounds::new(Point2D::new(10.0, 10.0), Point2D::new(20.0, 20.0)),
        ObjectData::Shape {
            kind: ShapeKind::Rectangle,
            width: 10.0,
            height: 10.0,
            corner_radius: 0.0,
        },
    );
    let clone = ProjectObject::new(
        "clone",
        layer,
        Bounds::new(Point2D::new(30.0, 10.0), Point2D::new(40.0, 20.0)),
        ObjectData::VirtualClone {
            source_id: source.id,
        },
    );
    let clone_id = clone.id.to_string();
    project.add_object(source);
    project.add_object(clone);
    let selected_pdf = beambench_core::export_pdf(&project, true, &[project.objects[1].id]);
    assert!(String::from_utf8(selected_pdf).unwrap().contains(" RG\n"));
    *ctx.project.lock().unwrap() = Some(project);
    assert!(planning::generate_plan(&ctx).is_ok());
    let selected = planning::generate_plan_with_options(
        &ctx,
        &planning::SessionJobOptions {
            cut_selected_graphics: true,
            use_selection_origin: false,
            selected_object_ids: vec![clone_id.clone()],
        },
    );
    let selected = selected.unwrap();
    let ids: Vec<_> = selected
        .segments
        .iter()
        .filter_map(|segment| {
            if let beambench_planner::PlanSegment::Vector {
                source_object_id, ..
            } = segment
            {
                source_object_id.clone()
            } else {
                None
            }
        })
        .collect();
    assert_eq!(ids, vec![clone_id]);
}

#[test]
fn undo_and_redo_after_save_are_unsaved() {
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
    let restored = crate::ops::project::undo_project(&ctx).unwrap();
    let disk = beambench_project::load_project(&path).unwrap();
    assert_ne!(restored.material_height_mm, disk.material_height_mm);
    assert!(restored.dirty);
    persistence::save_project_to_path(&ctx, &path).unwrap();
    let redone = crate::ops::project::redo_project(&ctx).unwrap();
    assert!(redone.dirty);
    assert_eq!(redone.material_height_mm, Some(17.0));
}

#[test]
fn long_grbl_lines_fail_instead_of_stalling() {
    use beambench_streamer::{ProgressTracker, StreamingEngine};
    let mut session = beambench_grbl::GrblSession::new(Box::new(
        beambench_serial::MockSerialTransport::new("mock"),
    ));
    session.connect().unwrap();
    let mut engine = StreamingEngine::new(vec![format!(";{}", "x".repeat(130))]);
    let mut progress = ProgressTracker::new(1);
    assert!(engine.send_tick(&mut session, &mut progress).is_err());
    assert!(engine.is_failed());
    assert_eq!(engine.bytes_in_flight(), 0);
    assert!(StreamingEngine::validate_commands(&["x".repeat(126)]).is_ok());
    assert!(StreamingEngine::validate_commands(&["x".repeat(127)]).is_err());
    assert!(StreamingEngine::validate_commands(&["G0 X1\nM3".into()]).is_err());
}

#[test]
fn dxf_preserves_closing_edges_and_flattens_curves() {
    let mut project = Project::new("export");
    let layer = project.ensure_default_layer();
    project.add_object(ProjectObject::new(
        "triangle",
        layer,
        Bounds::new(Point2D::new(10.0, 10.0), Point2D::new(30.0, 30.0)),
        ObjectData::VectorPath {
            path_data: "M10 10 L30 10 L20 30 Z".into(),
            closed: true,
            ruler_guide_axis: None,
        },
    ));
    let dxf = beambench_core::export_dxf(&project, false, &[]);
    assert_eq!(dxf.matches("0\nLINE\n").count(), 3);
    project.objects.clear();
    project.add_object(ProjectObject::new(
        "curve",
        layer,
        Bounds::new(Point2D::new(10.0, 10.0), Point2D::new(30.0, 30.0)),
        ObjectData::VectorPath {
            path_data: "M10 10 C10 30 30 30 30 10".into(),
            closed: false,
            ruler_guide_axis: None,
        },
    ));
    let dxf = beambench_core::export_dxf(&project, false, &[]);
    assert!(dxf.matches("0\nLINE\n").count() > 2);
}

#[test]
fn selected_image_preserves_mask() {
    use beambench_core::object::{ImageMaskPolarity, ImageMaskRef};
    use beambench_core::{Asset, AssetMediaType, Layer, OperationType};
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
    let mask = ProjectObject::new(
        "mask",
        mask_layer,
        Bounds::new(Point2D::new(20.0, 20.0), Point2D::new(25.0, 30.0)),
        ObjectData::Shape {
            kind: ShapeKind::Rectangle,
            width: 5.0,
            height: 10.0,
            corner_radius: 0.0,
        },
    );
    let mask_id = mask.id;
    project.add_object(mask);
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(&[0u8; 100], 10, 10, image::ExtendedColorType::L8)
        .unwrap();
    let asset = Asset::new(
        "black.png",
        AssetMediaType::Png,
        png.len() as u64,
        Some(10),
        Some(10),
    );
    let asset_id = asset.id;
    project.add_asset(asset, png);
    let object = ProjectObject::new(
        "image",
        layer_id,
        Bounds::new(Point2D::new(20.0, 20.0), Point2D::new(30.0, 30.0)),
        ObjectData::RasterImage {
            asset_key: asset_id.to_string(),
            original_width_px: 10,
            original_height_px: 10,
            adjustments: None,
            masks: vec![ImageMaskRef {
                object_id: mask_id,
                polarity: ImageMaskPolarity::KeepInside,
            }],
        },
    );
    let id = object.id.to_string();
    project.add_object(object);
    let burn_width = |plan: &beambench_planner::ExecutionPlan| -> f64 {
        plan.segments
            .iter()
            .filter_map(|segment| {
                if let beambench_planner::PlanSegment::Raster { scanlines, .. } = segment {
                    Some(
                        scanlines
                            .iter()
                            .flat_map(|row| row.runs.iter())
                            .map(|run| (run.end_x_mm - run.start_x_mm).abs())
                            .sum::<f64>(),
                    )
                } else {
                    None
                }
            })
            .sum()
    };
    for polarity in [
        ImageMaskPolarity::KeepInside,
        ImageMaskPolarity::KeepOutside,
    ] {
        if let ObjectData::RasterImage { masks, .. } = &mut project.objects[1].data {
            masks[0].polarity = polarity;
        }
        *ctx.project.lock().unwrap() = Some(project.clone());
        let full = planning::generate_plan(&ctx).unwrap();
        let selected = planning::generate_plan_with_options(
            &ctx,
            &planning::SessionJobOptions {
                cut_selected_graphics: true,
                use_selection_origin: false,
                selected_object_ids: vec![id.clone()],
            },
        )
        .unwrap();
        assert!(burn_width(&full) > 0.0);
        assert!((burn_width(&selected) - burn_width(&full)).abs() < 1e-8);
    }
    project.objects.remove(0);
    *ctx.project.lock().unwrap() = Some(project);
    let missing = planning::generate_plan(&ctx).unwrap();
    assert_eq!(burn_width(&missing), 0.0);
    assert!(missing.failed_entries.iter().any(|failure| matches!(&failure.reason, beambench_planner::PlanEntryFailureReason::ImageMaskSkipped { message, .. } if message.contains("mask is missing"))));
}

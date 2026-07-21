// Run with: cargo test -p beambench-service image_scenarios
//       or: cargo nextest run -p beambench-service -E 'test(image_scenarios)'

use super::imports::{
    TraceBoundaryPx, TraceImageInput, set_trace_preview_cancel_check_hook, trace_image_preview,
    trace_raster_image,
};
use super::project::update_object_data;
use crate::ServiceContext;
use beambench_common::{Bounds, Point2D, RasterAdjustments};
use beambench_core::{Asset, AssetMediaType, ObjectData, ObjectId, Project, ProjectObject};
use beambench_planner::ExecutionPlan;
use chrono::Utc;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use uuid::Uuid;

/// Create a test PNG image (64x64 grayscale with a horizontal gradient)
/// and return (png_bytes, Asset).
/// The gradient ranges from black (0) on the left to white (255) on the right,
/// ensuring different thresholds produce different trace results.
fn make_test_image() -> (Vec<u8>, Asset) {
    let img = image::GrayImage::from_fn(64, 64, |x, _y| {
        let val = ((x as f64 / 63.0) * 255.0).round() as u8;
        image::Luma([val])
    });
    let mut png_bytes = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(std::io::Cursor::new(&mut png_bytes));
    image::ImageEncoder::write_image(encoder, img.as_raw(), 64, 64, image::ExtendedColorType::L8)
        .unwrap();

    let asset = Asset::new(
        "test.png",
        AssetMediaType::Png,
        png_bytes.len() as u64,
        Some(64),
        Some(64),
    );
    (png_bytes, asset)
}

/// Set up a ServiceContext with a project containing a raster image object.
/// Returns (ctx, object_id).
fn setup_raster_project() -> (ServiceContext, ObjectId) {
    let ctx = ServiceContext::new();
    let mut project = Project::new("ImageTest");
    // Rasters must live on image layers per the layer-content
    // invariant, so we create an explicit Image layer here rather
    // than using the default (Line) layer.
    let layer = beambench_core::Layer::new("Image", beambench_core::OperationType::Image);
    let layer_id = layer.id;
    project.layers.push(layer);

    let (png_bytes, asset) = make_test_image();
    let asset_key = asset.id.to_string();
    project.add_asset(asset, png_bytes);

    let obj = ProjectObject::new(
        "TestImg",
        layer_id,
        Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(40.0, 40.0)),
        ObjectData::RasterImage {
            asset_key,
            original_width_px: 64,
            original_height_px: 64,
            adjustments: None,
            masks: Vec::new(),
        },
    );
    let obj_id = obj.id;
    project.add_object(obj);
    *ctx.project.lock().unwrap() = Some(project);
    (ctx, obj_id)
}

fn encode_gray_png(img: &image::GrayImage, filename: &str) -> (Vec<u8>, Asset) {
    let (width, height) = img.dimensions();
    let mut png_bytes = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(std::io::Cursor::new(&mut png_bytes));
    image::ImageEncoder::write_image(
        encoder,
        img.as_raw(),
        width,
        height,
        image::ExtendedColorType::L8,
    )
    .unwrap();
    let asset = Asset::new(
        filename,
        AssetMediaType::Png,
        png_bytes.len() as u64,
        Some(width),
        Some(height),
    );
    (png_bytes, asset)
}

fn setup_two_square_raster_project() -> (ServiceContext, ObjectId) {
    let ctx = ServiceContext::new();
    let mut project = Project::new("BoundaryTraceTest");
    let layer = beambench_core::Layer::new("Image", beambench_core::OperationType::Image);
    let layer_id = layer.id;
    project.layers.push(layer);

    let mut img = image::GrayImage::from_pixel(64, 64, image::Luma([255]));
    for y in 12..24 {
        for x in 8..20 {
            img.put_pixel(x, y, image::Luma([0]));
        }
        for x in 44..56 {
            img.put_pixel(x, y, image::Luma([0]));
        }
    }
    let (png_bytes, asset) = encode_gray_png(&img, "two-squares.png");
    let asset_key = asset.id.to_string();
    project.add_asset(asset, png_bytes);

    let obj = ProjectObject::new(
        "TwoSquares",
        layer_id,
        Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(64.0, 64.0)),
        ObjectData::RasterImage {
            asset_key,
            original_width_px: 64,
            original_height_px: 64,
            adjustments: None,
            masks: Vec::new(),
        },
    );
    let obj_id = obj.id;
    project.add_object(obj);
    *ctx.project.lock().unwrap() = Some(project);
    (ctx, obj_id)
}

fn svg_path_numbers(paths: &[String]) -> Vec<f64> {
    paths
        .iter()
        .flat_map(|path| {
            path.split(|ch: char| {
                !(ch.is_ascii_digit() || ch == '.' || ch == '-' || ch == 'e' || ch == 'E')
            })
            .filter_map(|part| {
                if part.is_empty() || part == "-" {
                    None
                } else {
                    part.parse::<f64>().ok()
                }
            })
            .collect::<Vec<_>>()
        })
        .collect()
}

#[test]
fn trace_preview_returns_paths_without_modifying_project() {
    let (ctx, obj_id) = setup_raster_project();

    let result = trace_image_preview(
        &ctx,
        TraceImageInput {
            object_id: obj_id,
            threshold: 128,
            cutoff: 0,
            turdsize: 2,
            alphamax: 1.0,
            opttolerance: 0.2,
            trace_alpha: false,
            sketch_trace: false,
            delete_source: false,
            preview_request_id: None,
            boundary: None,
        },
    )
    .unwrap();

    // Verify paths are non-empty and dimensions are correct
    assert!(
        !result.paths.is_empty(),
        "Trace should produce at least one path"
    );
    assert_eq!(result.source_width, 64);
    assert_eq!(result.source_height, 64);

    // Verify project object count is unchanged (only the original raster object)
    let guard = ctx.project.lock().unwrap();
    let project = guard.as_ref().unwrap();
    assert_eq!(project.objects.len(), 1);
    assert_eq!(project.objects[0].id, obj_id);

    // Verify undo state has no undo (preview should not push a snapshot)
    drop(guard);
    let undo = ctx.undo_state().unwrap();
    assert!(!undo.can_undo, "Preview should not push undo snapshot");
}

#[test]
fn trace_preview_boundary_traces_crop_and_offsets_paths_to_source_pixels() {
    let (ctx, obj_id) = setup_two_square_raster_project();

    let result = trace_image_preview(
        &ctx,
        TraceImageInput {
            object_id: obj_id,
            threshold: 128,
            cutoff: 0,
            turdsize: 2,
            alphamax: 1.0,
            opttolerance: 0.2,
            trace_alpha: false,
            sketch_trace: false,
            delete_source: false,
            preview_request_id: None,
            boundary: Some(TraceBoundaryPx {
                x: 40.0,
                y: 8.0,
                width: 20.0,
                height: 24.0,
            }),
        },
    )
    .unwrap();

    assert_eq!(result.source_width, 64);
    assert_eq!(result.source_height, 64);
    assert!(!result.paths.is_empty());

    let numbers = svg_path_numbers(&result.paths);
    let xs = numbers.iter().step_by(2).copied().collect::<Vec<_>>();
    assert!(
        xs.iter().all(|x| *x >= 39.0),
        "cropped preview paths should be translated back into full-image coordinates: {xs:?}"
    );
}

#[test]
fn trace_different_thresholds_produce_different_results() {
    let (ctx, obj_id) = setup_raster_project();

    let result_low = trace_image_preview(
        &ctx,
        TraceImageInput {
            object_id: obj_id,
            threshold: 64,
            cutoff: 0,
            turdsize: 2,
            alphamax: 1.0,
            opttolerance: 0.2,
            trace_alpha: false,
            sketch_trace: false,
            delete_source: false,
            preview_request_id: None,
            boundary: None,
        },
    )
    .unwrap();

    let result_high = trace_image_preview(
        &ctx,
        TraceImageInput {
            object_id: obj_id,
            threshold: 200,
            cutoff: 0,
            turdsize: 2,
            alphamax: 1.0,
            opttolerance: 0.2,
            trace_alpha: false,
            sketch_trace: false,
            delete_source: false,
            preview_request_id: None,
            boundary: None,
        },
    )
    .unwrap();

    // Different thresholds should produce different trace outputs.
    // They may differ in path count, or in path content. At minimum,
    // the serialized paths should not be identical.
    let low_paths_joined: String = result_low.paths.join("|");
    let high_paths_joined: String = result_high.paths.join("|");
    assert_ne!(
        low_paths_joined, high_paths_joined,
        "Different thresholds should produce different trace results"
    );
}

#[test]
fn image_adjust_roundtrip() {
    let (ctx, obj_id) = setup_raster_project();

    // Read the current object data to get the asset_key
    let asset_key = {
        let guard = ctx.project.lock().unwrap();
        let project = guard.as_ref().unwrap();
        let obj = project.find_object(obj_id).unwrap();
        match &obj.data {
            ObjectData::RasterImage { asset_key, .. } => asset_key.clone(),
            _ => panic!("Expected RasterImage"),
        }
    };

    // Update object data with adjustments
    let new_data = ObjectData::RasterImage {
        asset_key,
        original_width_px: 64,
        original_height_px: 64,
        adjustments: Some(RasterAdjustments {
            brightness: 0.5,
            ..RasterAdjustments::default()
        }),
        masks: Vec::new(),
    };
    let updated = update_object_data(&ctx, obj_id, new_data).unwrap();

    // Verify adjustments are stored
    match &updated.data {
        ObjectData::RasterImage { adjustments, .. } => {
            let adj = adjustments.as_ref().expect("adjustments should be Some");
            assert!(
                (adj.brightness - 0.5).abs() < f64::EPSILON,
                "Brightness should be 0.5"
            );
        }
        _ => panic!("Expected RasterImage"),
    }
}

#[test]
fn trace_preview_with_no_project_returns_error() {
    let ctx = ServiceContext::new();
    // No project loaded — should return an error

    let result = trace_image_preview(
        &ctx,
        TraceImageInput {
            object_id: ObjectId::new(),
            threshold: 128,
            cutoff: 0,
            turdsize: 2,
            alphamax: 1.0,
            opttolerance: 0.2,
            trace_alpha: false,
            sketch_trace: false,
            delete_source: false,
            preview_request_id: None,
            boundary: None,
        },
    );

    assert!(result.is_err(), "Should error when no project is open");
}

#[test]
fn stale_trace_preview_request_returns_stale_revision_error() {
    let (ctx, obj_id) = setup_raster_project();
    ctx.latest_trace_preview_request_id
        .store(2, Ordering::Release);

    let result = trace_image_preview(
        &ctx,
        TraceImageInput {
            object_id: obj_id,
            threshold: 128,
            cutoff: 0,
            turdsize: 2,
            alphamax: 1.0,
            opttolerance: 0.2,
            trace_alpha: false,
            sketch_trace: false,
            delete_source: false,
            preview_request_id: Some(1),
            boundary: None,
        },
    );

    let err = result.expect_err("stale preview should abort");
    assert_eq!(err.code, crate::error::ServiceErrorCode::StaleRevision);
}

#[test]
fn stale_trace_preview_request_cancels_during_trace_callback_checks() {
    let (ctx, obj_id) = setup_raster_project();
    let request_id = 7;
    ctx.latest_trace_preview_request_id
        .store(request_id, Ordering::Release);
    let cancel_check_count = Arc::new(AtomicUsize::new(0));
    let cancel_check_count_for_hook = Arc::clone(&cancel_check_count);

    set_trace_preview_cancel_check_hook(Some(Arc::new(move |ctx, request_id| {
        if request_id != Some(7) {
            return;
        }
        let seen = cancel_check_count_for_hook.fetch_add(1, Ordering::SeqCst) + 1;
        if seen == 2 {
            ctx.latest_trace_preview_request_id
                .store(8, Ordering::Release);
        }
    })));

    let result = trace_image_preview(
        &ctx,
        TraceImageInput {
            object_id: obj_id,
            threshold: 128,
            cutoff: 0,
            turdsize: 2,
            alphamax: 1.0,
            opttolerance: 0.2,
            trace_alpha: false,
            sketch_trace: false,
            delete_source: false,
            preview_request_id: Some(request_id),
            boundary: None,
        },
    );

    set_trace_preview_cancel_check_hook(None);

    let err = result.expect_err("stale preview should abort during trace callback");
    assert_eq!(err.code, crate::error::ServiceErrorCode::StaleRevision);
    assert!(
        cancel_check_count.load(Ordering::SeqCst) >= 2,
        "trace callback should have been polled at least twice"
    );
}

#[test]
fn trace_raster_image_adds_vectors_and_pushes_undo() {
    let (ctx, obj_id) = setup_raster_project();

    // Count objects before trace
    let before_count = {
        let guard = ctx.project.lock().unwrap();
        guard.as_ref().unwrap().objects.len()
    };
    assert_eq!(before_count, 1, "Should start with 1 raster object");

    let result = trace_raster_image(
        &ctx,
        TraceImageInput {
            object_id: obj_id,
            threshold: 128,
            cutoff: 0,
            turdsize: 2,
            alphamax: 1.0,
            opttolerance: 0.2,
            trace_alpha: false,
            sketch_trace: false,
            delete_source: false,
            preview_request_id: None,
            boundary: None,
        },
    )
    .unwrap();

    // 1. Returned objects should all be VectorPath type
    assert!(
        !result.is_empty(),
        "Trace should produce at least one vector object"
    );
    for obj in &result {
        assert!(
            matches!(&obj.data, ObjectData::VectorPath { .. }),
            "Traced object should be VectorPath, got {:?}",
            std::mem::discriminant(&obj.data)
        );
    }

    // 2. Project should now have more objects than before
    let guard = ctx.project.lock().unwrap();
    let project = guard.as_ref().unwrap();
    assert!(
        project.objects.len() > before_count,
        "Project should have more objects after trace"
    );

    // 3. Original raster object should still exist
    assert!(
        project.find_object(obj_id).is_some(),
        "Original raster object should still be present"
    );
    drop(guard);

    // 4. Undo should be available (snapshot was pushed)
    let undo = ctx.undo_state().unwrap();
    assert!(undo.can_undo, "Should be able to undo after trace");
}

#[test]
fn trace_raster_image_boundary_maps_crop_to_correct_world_bounds() {
    let (ctx, obj_id) = setup_two_square_raster_project();

    let result = trace_raster_image(
        &ctx,
        TraceImageInput {
            object_id: obj_id,
            threshold: 128,
            cutoff: 0,
            turdsize: 2,
            alphamax: 1.0,
            opttolerance: 0.2,
            trace_alpha: false,
            sketch_trace: false,
            delete_source: false,
            preview_request_id: None,
            boundary: Some(TraceBoundaryPx {
                x: 40.0,
                y: 8.0,
                width: 20.0,
                height: 24.0,
            }),
        },
    )
    .unwrap();

    assert!(!result.is_empty());
    assert!(
        result.iter().all(|obj| obj.bounds.min.x >= 39.0),
        "world-space traced bounds should remain in the selected right-side crop: {:?}",
        result.iter().map(|obj| obj.bounds).collect::<Vec<_>>()
    );
}

#[test]
fn trace_raster_image_rejects_out_of_image_boundary_without_mutating() {
    let (ctx, obj_id) = setup_two_square_raster_project();
    let before_count = ctx.project.lock().unwrap().as_ref().unwrap().objects.len();

    let err = trace_raster_image(
        &ctx,
        TraceImageInput {
            object_id: obj_id,
            threshold: 128,
            cutoff: 0,
            turdsize: 2,
            alphamax: 1.0,
            opttolerance: 0.2,
            trace_alpha: false,
            sketch_trace: false,
            delete_source: true,
            preview_request_id: None,
            boundary: Some(TraceBoundaryPx {
                x: 100.0,
                y: 100.0,
                width: 10.0,
                height: 10.0,
            }),
        },
    )
    .expect_err("out-of-image boundary should be rejected");

    assert_eq!(err.code, crate::error::ServiceErrorCode::InvalidInput);
    assert!(err.message.contains("Trace boundary"));
    let project = ctx.project.lock().unwrap();
    assert_eq!(project.as_ref().unwrap().objects.len(), before_count);
    assert!(!ctx.undo_state().unwrap().can_undo);
}

#[test]
fn trace_raster_image_invalidates_plan_cache() {
    let (ctx, obj_id) = setup_raster_project();

    // Pre-populate the plan cache so we can verify it gets cleared
    *ctx.plan_cache.lock().unwrap() = Some(ExecutionPlan {
        id: Uuid::new_v4(),
        project_id: Uuid::new_v4(),
        revision_hash: "dummy".to_string(),
        created_at: Utc::now(),
        bounds: Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
        total_distance_mm: 0.0,
        estimated_duration_secs: 0.0,
        segments: vec![],
        layer_order: vec![],
        warnings: vec![],
        failed_entries: vec![],
    });
    assert!(
        ctx.plan_cache.lock().unwrap().is_some(),
        "Cache should be populated before trace"
    );

    trace_raster_image(
        &ctx,
        TraceImageInput {
            object_id: obj_id,
            threshold: 128,
            cutoff: 0,
            turdsize: 2,
            alphamax: 1.0,
            opttolerance: 0.2,
            trace_alpha: false,
            sketch_trace: false,
            delete_source: false,
            preview_request_id: None,
            boundary: None,
        },
    )
    .unwrap();

    assert!(
        ctx.plan_cache.lock().unwrap().is_none(),
        "Plan cache should be invalidated after trace_raster_image"
    );
}

/// Create a test RGBA PNG: 40x40 with an opaque 20x20 center square,
/// transparent everywhere else.
fn make_rgba_test_image() -> (Vec<u8>, Asset) {
    let mut img = image::RgbaImage::new(40, 40);
    for y in 0..40 {
        for x in 0..40 {
            let alpha = if x >= 10 && x < 30 && y >= 10 && y < 30 {
                255
            } else {
                0
            };
            img.put_pixel(x, y, image::Rgba([128, 128, 128, alpha]));
        }
    }
    let mut png_bytes = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(std::io::Cursor::new(&mut png_bytes));
    image::ImageEncoder::write_image(
        encoder,
        img.as_raw(),
        40,
        40,
        image::ExtendedColorType::Rgba8,
    )
    .unwrap();
    let asset = Asset::new(
        "rgba_test.png",
        AssetMediaType::Png,
        png_bytes.len() as u64,
        Some(40),
        Some(40),
    );
    (png_bytes, asset)
}

fn setup_rgba_project() -> (ServiceContext, ObjectId) {
    let ctx = ServiceContext::new();
    let mut project = Project::new("AlphaTest");
    let layer = beambench_core::Layer::new("Image", beambench_core::OperationType::Image);
    let layer_id = layer.id;
    project.layers.push(layer);

    let (png_bytes, asset) = make_rgba_test_image();
    let asset_key = asset.id.to_string();
    project.add_asset(asset, png_bytes);

    let obj = ProjectObject::new(
        "RgbaImg",
        layer_id,
        Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(40.0, 40.0)),
        ObjectData::RasterImage {
            asset_key,
            original_width_px: 40,
            original_height_px: 40,
            adjustments: None,
            masks: Vec::new(),
        },
    );
    let obj_id = obj.id;
    project.add_object(obj);
    *ctx.project.lock().unwrap() = Some(project);
    (ctx, obj_id)
}

#[test]
fn trace_alpha_traces_opaque_region_only() {
    let (ctx, obj_id) = setup_rgba_project();

    let result = trace_raster_image(
        &ctx,
        TraceImageInput {
            object_id: obj_id,
            threshold: 128,
            cutoff: 0,
            turdsize: 1,
            alphamax: 1.0,
            opttolerance: 0.2,
            trace_alpha: true,
            sketch_trace: false,
            delete_source: false,
            preview_request_id: None,
            boundary: None,
        },
    )
    .unwrap();

    // The opaque 20x20 center (pixels 10..30 in a 40px image) should produce
    // exactly 1 traced path. With trace_alpha, the alpha channel (255 in center,
    // 0 outside) is what gets traced — not the uniform gray pixel values.
    assert_eq!(
        result.len(),
        1,
        "should produce exactly 1 traced object for the opaque square, got {}",
        result.len()
    );

    // Verify the traced geometry is centered on the opaque square, not the
    // full image frame. The opaque region is pixels 10..30 in a 40x40 image
    // with 1:1 world scale → bounds should be roughly within x=8..32, y=8..32.
    let traced_obj = &result[0];
    let b = &traced_obj.bounds;
    assert!(
        b.min.x >= 5.0 && b.min.y >= 5.0 && b.max.x <= 35.0 && b.max.y <= 35.0,
        "traced bounds should be centered on opaque square, got ({:.1},{:.1})→({:.1},{:.1})",
        b.min.x,
        b.min.y,
        b.max.x,
        b.max.y
    );
    // And the bounds should NOT cover the full image (0→40)
    assert!(
        b.min.x > 2.0 || b.min.y > 2.0,
        "traced bounds should not start at image origin — alpha trace should skip transparent edges"
    );
}

#[test]
fn trace_alpha_preview_returns_paths_for_opaque_region() {
    let (ctx, obj_id) = setup_rgba_project();

    let result = trace_image_preview(
        &ctx,
        TraceImageInput {
            object_id: obj_id,
            threshold: 128,
            cutoff: 0,
            turdsize: 1,
            alphamax: 1.0,
            opttolerance: 0.2,
            trace_alpha: true,
            sketch_trace: false,
            delete_source: false,
            preview_request_id: None,
            boundary: None,
        },
    )
    .unwrap();

    assert!(
        !result.paths.is_empty(),
        "alpha preview should return traced paths"
    );
    assert_eq!(result.source_width, 40);
    assert_eq!(result.source_height, 40);

    // Preview paths are in image-pixel coordinates. The opaque square spans
    // pixels 10..30 — the SVG d-string should contain coordinates in that
    // range, NOT near 0 or 40 (which would indicate a full-frame trace).
    let d = &result.paths[0];
    // Extract all numeric coordinates from the SVG d-string
    let coords: Vec<f64> = d
        .split(|c: char| !c.is_ascii_digit() && c != '.' && c != '-')
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse::<f64>().ok())
        .collect();
    assert!(!coords.is_empty(), "path should contain coordinates");
    // At least some coordinates should be in the 8..32 range (opaque square region)
    let in_center = coords.iter().any(|&c| c > 8.0 && c < 32.0);
    assert!(
        in_center,
        "traced path coordinates should be in the opaque center region, got {:?}",
        &coords[..coords.len().min(10)]
    );
}

#[test]
fn trace_delete_source_removes_raster_keeps_vectors_in_one_undo() {
    use super::project::undo_project;

    let (ctx, obj_id) = setup_raster_project();
    let (source_layer_id, source_layer_name) = {
        let guard = ctx.project.lock().unwrap();
        let p = guard.as_ref().unwrap();
        let obj = p.find_object(obj_id).unwrap();
        let layer = p.find_layer(obj.layer_id).unwrap();
        (layer.id, layer.name.clone())
    };

    // Verify source object exists before trace
    {
        let guard = ctx.project.lock().unwrap();
        let p = guard.as_ref().unwrap();
        assert!(
            p.find_object(obj_id).is_some(),
            "source raster should exist before trace"
        );
    }

    let traced = trace_raster_image(
        &ctx,
        TraceImageInput {
            object_id: obj_id,
            threshold: 128,
            cutoff: 0,
            turdsize: 2,
            alphamax: 1.0,
            opttolerance: 0.2,
            trace_alpha: false,
            sketch_trace: false,
            delete_source: true,
            preview_request_id: None,
            boundary: None,
        },
    )
    .unwrap();

    assert!(!traced.is_empty(), "should produce traced vector objects");
    let traced_ids: Vec<_> = traced.iter().map(|o| o.id).collect();

    // Source raster should be gone, traced vectors should exist
    {
        let guard = ctx.project.lock().unwrap();
        let p = guard.as_ref().unwrap();
        assert!(
            p.find_object(obj_id).is_none(),
            "source raster should be removed after delete_source=true"
        );
        assert!(
            p.find_layer(source_layer_id).is_none(),
            "source image layer should be pruned when it becomes empty"
        );
        for &tid in &traced_ids {
            let traced = p
                .find_object(tid)
                .expect("traced vector should exist in project");
            let traced_layer = p
                .find_layer(traced.layer_id)
                .expect("traced layer should exist");
            assert_eq!(
                traced_layer.name, source_layer_name,
                "new traced layer should inherit the original image layer name"
            );
        }
    }

    // Execute undo — should restore source AND remove traced vectors in one step
    let undo_state = ctx.undo_state().unwrap();
    assert!(undo_state.can_undo, "undo should be available");

    let restored = undo_project(&ctx).unwrap();

    // Source raster should be back
    assert!(
        restored.find_object(obj_id).is_some(),
        "source raster should be restored after undo"
    );
    assert!(
        restored.find_layer(source_layer_id).is_some(),
        "source image layer should be restored after undo"
    );
    // Traced vectors should be gone
    for &tid in &traced_ids {
        assert!(
            restored.find_object(tid).is_none(),
            "traced vector should be removed after undo"
        );
    }
}

#[test]
fn trace_zero_result_does_not_mutate_project_or_delete_source() {
    let (ctx, obj_id) = setup_raster_project();

    // Use a threshold that won't match any pixels in the gradient test
    // image (gradient goes 0→255 left-to-right, so cutoff=254 threshold=255
    // only traces pixels at brightness 254-255, which with turdsize=9999
    // will be filtered out).
    let result = trace_raster_image(
        &ctx,
        TraceImageInput {
            object_id: obj_id,
            threshold: 255,
            cutoff: 254,
            turdsize: 9999, // filter everything
            alphamax: 1.0,
            opttolerance: 0.2,
            trace_alpha: false,
            sketch_trace: false,
            delete_source: true, // this should NOT fire when result is empty
            preview_request_id: None,
            boundary: None,
        },
    )
    .unwrap();

    assert!(
        result.is_empty(),
        "should produce no traced objects with extreme turdsize"
    );

    // Source raster should still exist — delete_source must NOT fire on empty result
    {
        let guard = ctx.project.lock().unwrap();
        let p = guard.as_ref().unwrap();
        assert!(
            p.find_object(obj_id).is_some(),
            "source raster must survive when trace produces zero paths"
        );
    }

    // No undo snapshot should have been pushed
    let undo = ctx.undo_state().unwrap();
    assert!(
        !undo.can_undo,
        "no undo step should exist after zero-result trace"
    );
}

#[test]
fn adjust_preview_cache_hit_on_identical_second_call() {
    use super::imports::{AdjustImagePreviewInput, adjust_image_preview};

    let (ctx, obj_id) = setup_raster_project();

    let input = AdjustImagePreviewInput {
        object_id: obj_id,
        brightness: 0.0,
        contrast: 0.0,
        gamma: 1.0,
        invert: false,
        threshold: 128,
        saturation: 1.0,
        sharpen: 0.0,
        edge_enhance: false,
        enhance_radius: 0.0,
        enhance_amount: 0.0,
        enhance_denoise: 0.0,
        mode: "floyd_steinberg".to_string(),
        dpi: 254,
        negative: false,
        pass_through: false,
        halftone_cells_per_inch: 10,
        halftone_angle_deg: 0.0,
        newsprint_angle_deg: 45.0,
        newsprint_frequency: 10.0,
    };

    // First call — processes from scratch
    let result1 = adjust_image_preview(&ctx, input.clone()).unwrap();
    assert_eq!(
        ctx.preview_cache.hit_count(),
        0,
        "first call should be a miss"
    );

    // Second identical call — should hit the preview full-result cache
    let result2 = adjust_image_preview(&ctx, input).unwrap();
    assert_eq!(
        ctx.preview_cache.hit_count(),
        1,
        "second identical call should hit preview cache"
    );

    // Results must be identical
    assert_eq!(result1.width, result2.width);
    assert_eq!(result1.height, result2.height);
    assert_eq!(result1.png_base64, result2.png_base64);
}

#[test]
fn plan_rebuild_reuses_cached_rasters() {
    use super::planning::generate_plan;

    let (ctx, _obj_id) = setup_raster_project();

    // First plan build — cache miss
    let _plan1 = generate_plan(&ctx).unwrap();
    let misses_after_first = ctx.raster_cache.miss_count();
    assert!(
        misses_after_first > 0,
        "first build should have cache misses"
    );
    let hits_after_first = ctx.raster_cache.hit_count();

    // Invalidate plan cache (simulates a vector edit) but raster cache stays
    super::planning::invalidate_plan_cache(&ctx).unwrap();

    // Second plan build with identical raster params — should hit cache
    let _plan2 = generate_plan(&ctx).unwrap();
    let hits_after_second = ctx.raster_cache.hit_count();
    assert!(
        hits_after_second > hits_after_first,
        "second plan build should reuse cached rasters: hits went from {} to {}",
        hits_after_first,
        hits_after_second
    );
}

#[test]
fn staged_cache_reuses_decoded_image_across_brightness_changes() {
    use super::imports::{AdjustImagePreviewInput, adjust_image_preview};

    let (ctx, obj_id) = setup_raster_project();

    let base_input = AdjustImagePreviewInput {
        object_id: obj_id,
        brightness: 0.0,
        contrast: 0.0,
        gamma: 1.0,
        invert: false,
        threshold: 128,
        saturation: 1.0,
        sharpen: 0.0,
        edge_enhance: false,
        enhance_radius: 0.0,
        enhance_amount: 0.0,
        enhance_denoise: 0.0,
        mode: "floyd_steinberg".to_string(),
        dpi: 254,
        negative: false,
        pass_through: false,
        halftone_cells_per_inch: 10,
        halftone_angle_deg: 0.0,
        newsprint_angle_deg: 45.0,
        newsprint_frequency: 10.0,
    };

    // First call — decode+scale from scratch
    let _r1 = adjust_image_preview(&ctx, base_input.clone()).unwrap();
    let scaled_hits_after_first = ctx.scaled_image_cache.hit_count();

    // Second call with different brightness — should reuse decoded/scaled image
    let mut input2 = base_input.clone();
    input2.brightness = 0.5;
    let _r2 = adjust_image_preview(&ctx, input2).unwrap();
    let scaled_hits_after_second = ctx.scaled_image_cache.hit_count();

    assert!(
        scaled_hits_after_second > scaled_hits_after_first,
        "brightness change should reuse scaled image cache: hits {} → {}",
        scaled_hits_after_first,
        scaled_hits_after_second
    );
}

#[test]
fn staged_cache_invalidates_on_saturation_change() {
    use super::imports::{AdjustImagePreviewInput, adjust_image_preview};

    let (ctx, obj_id) = setup_raster_project();

    let base_input = AdjustImagePreviewInput {
        object_id: obj_id,
        brightness: 0.0,
        contrast: 0.0,
        gamma: 1.0,
        invert: false,
        threshold: 128,
        saturation: 1.0,
        sharpen: 0.0,
        edge_enhance: false,
        enhance_radius: 0.0,
        enhance_amount: 0.0,
        enhance_denoise: 0.0,
        mode: "floyd_steinberg".to_string(),
        dpi: 254,
        negative: false,
        pass_through: false,
        halftone_cells_per_inch: 10,
        halftone_angle_deg: 0.0,
        newsprint_angle_deg: 45.0,
        newsprint_frequency: 10.0,
    };

    // First call — decode+scale from scratch
    let _r1 = adjust_image_preview(&ctx, base_input.clone()).unwrap();
    let scaled_misses_after_first = ctx.scaled_image_cache.miss_count();

    // Second call with different saturation — should NOT reuse decoded image
    // (saturation is part of the decode/scale stage key)
    let mut input2 = base_input;
    input2.saturation = 0.5;
    let _r2 = adjust_image_preview(&ctx, input2).unwrap();
    let scaled_misses_after_second = ctx.scaled_image_cache.miss_count();

    assert!(
        scaled_misses_after_second > scaled_misses_after_first,
        "saturation change should miss scaled image cache: misses {} → {}",
        scaled_misses_after_first,
        scaled_misses_after_second
    );
}

#[test]
fn preview_does_not_pollute_planner_cache() {
    use super::imports::{AdjustImagePreviewInput, adjust_image_preview};

    let (ctx, obj_id) = setup_raster_project();

    let input = AdjustImagePreviewInput {
        object_id: obj_id,
        brightness: 0.3,
        contrast: 0.0,
        gamma: 1.0,
        invert: false,
        threshold: 128,
        saturation: 1.0,
        sharpen: 0.0,
        edge_enhance: false,
        enhance_radius: 0.0,
        enhance_amount: 0.0,
        enhance_denoise: 0.0,
        mode: "floyd_steinberg".to_string(),
        dpi: 254,
        negative: false,
        pass_through: false,
        halftone_cells_per_inch: 10,
        halftone_angle_deg: 0.0,
        newsprint_angle_deg: 45.0,
        newsprint_frequency: 10.0,
    };

    // Multiple preview calls with different settings
    let _r1 = adjust_image_preview(&ctx, input.clone()).unwrap();
    let mut input2 = input.clone();
    input2.brightness = 0.5;
    let _r2 = adjust_image_preview(&ctx, input2).unwrap();
    let mut input3 = input;
    input3.contrast = 0.3;
    let _r3 = adjust_image_preview(&ctx, input3).unwrap();

    // Planner raster_cache should have zero entries — preview uses its own cache
    assert_eq!(
        ctx.raster_cache.len(),
        0,
        "preview should not insert into shared planner raster_cache"
    );
    // Preview cache should have entries
    assert!(
        ctx.preview_cache.len() > 0,
        "preview results should be stored in the separate preview_cache"
    );
}

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use beambench_common::{Bounds, Point2D};
use beambench_core::{AppSettings, MachineProfile, ObjectData, Project, ProjectObject, ShapeKind};
use beambench_service::ServiceContext;
use http_body_util::BodyExt;
use serde_json::{Value, json};
use std::sync::Arc;
use tower::ServiceExt;

fn fixture() -> (Arc<ServiceContext>, String) {
    let profile = MachineProfile {
        bed_width_mm: 400.0,
        bed_height_mm: 400.0,
        ..Default::default()
    };
    let settings = AppSettings {
        active_profile_id: Some(profile.id),
        machine_profiles: vec![profile.clone()],
        ..Default::default()
    };
    let ctx = Arc::new(ServiceContext::with_settings(settings));
    let mut project = Project::new("Agent workflows");
    project.machine_profile_id = Some(profile.id);
    let layer = project.ensure_default_layer();
    let object = ProjectObject::new(
        "Rectangle",
        layer,
        Bounds::new(Point2D::new(10.0, 10.0), Point2D::new(30.0, 30.0)),
        ObjectData::Shape {
            kind: ShapeKind::Rectangle,
            width: 20.0,
            height: 20.0,
            corner_radius: 0.0,
        },
    );
    let id = object.id.to_string();
    project.add_object(object);
    *ctx.project.lock().unwrap() = Some(project);
    (ctx, id)
}

async fn call(
    ctx: &Arc<ServiceContext>,
    method: &str,
    path: &str,
    body: Value,
) -> (StatusCode, Value) {
    let response = super::build_router(ctx.clone())
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let value = serde_json::from_slice(&bytes).unwrap_or_else(|_| {
        panic!(
            "Non-JSON response {status}: {}",
            String::from_utf8_lossy(&bytes)
        )
    });
    (status, value)
}

async fn native(ctx: &Arc<ServiceContext>, group: &str, command: &str, mut body: Value) -> Value {
    body["command"] = command.into();
    let (status, value) = call(ctx, "POST", &format!("/api/v1/workflows/{group}"), body).await;
    assert_eq!(status, StatusCode::OK, "{command}: {value}");
    value
}

#[tokio::test]
async fn native_configuration_is_visible_undoable_and_rejects_unknown_fields() {
    let (ctx, _) = fixture();
    let mut events = ctx.events.subscribe();
    native(&ctx, "project", "set_material_height", json!({"value":3.0})).await;
    assert_eq!(
        ctx.project
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .material_height_mm,
        Some(3.0)
    );
    let mut saw_refresh = false;
    while let Ok(event) = events.try_recv() {
        saw_refresh |= event.contains("project.workflow.applied");
    }
    assert!(saw_refresh);
    native(&ctx, "project", "undo_project", json!({})).await;
    assert_eq!(
        ctx.project
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .material_height_mm,
        None
    );
    let before = ctx.project.lock().unwrap().clone();
    let (status, _) = call(
        &ctx,
        "POST",
        "/api/v1/workflows/project",
        json!({"command":"set_material_height","value":4,"typo":true}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(*ctx.project.lock().unwrap(), before);
    native(
        &ctx,
        "project",
        "set_material_height",
        json!({"value":null}),
    )
    .await;
    assert!(!ctx.undo_state().unwrap().can_undo);
}

#[tokio::test]
async fn read_workflows_do_not_discard_preview_or_add_history() {
    let (ctx, id) = fixture();
    native(&ctx, "vector", "convert_to_path", json!({"object_id":id})).await;
    let before = ctx.project.lock().unwrap().clone();
    let mut events = ctx.events.subscribe();
    native(&ctx, "vector", "get_editable_path", json!({"object_id":id})).await;
    assert_eq!(*ctx.project.lock().unwrap(), before);
    assert!(events.try_recv().is_err());
}

#[tokio::test]
async fn native_vector_topology_uses_desktop_node_operations() {
    let (ctx, id) = fixture();
    native(&ctx, "vector", "convert_to_path", json!({"object_id":id})).await;
    let before = ctx.project.lock().unwrap().clone();
    native(
        &ctx,
        "vector",
        "convert_segment_to_curve",
        json!({"object_id":id,"subpath_idx":0,"command_idx":1}),
    )
    .await;
    let result = native(&ctx, "vector", "get_editable_path", json!({"object_id":id})).await;
    assert!(
        result.to_string().contains("cubic") || result.to_string().contains("Cubic"),
        "{result}"
    );
    native(&ctx, "project", "undo_project", json!({})).await;
    assert_eq!(
        ctx.project.lock().unwrap().as_ref().unwrap().objects,
        before.unwrap().objects
    );
}

#[tokio::test]
async fn library_selection_round_trip_inserts_editable_art_and_undo_removes_it() {
    let _persist = crate::test_support::PersistTestGuard::new();
    let dir = tempfile::tempdir().unwrap();
    let (ctx, id) = fixture();
    let library = native(
        &ctx,
        "art_library",
        "create_art_library",
        json!({"path":dir.path().join("art.bbart"),"name":"CLI art"}),
    )
    .await;
    let library_id = library["library_id"]
        .as_str()
        .unwrap_or_else(|| panic!("{library}"));
    let item = native(&ctx,"art_library","add_selection_to_art_library",json!({"library_id":library_id,"object_ids":[id],"name":"Rectangle","category":"Shapes","tags":["test"]})).await;
    native(
        &ctx,
        "art_library",
        "insert_art_library_item_to_project",
        json!({"library_id":library_id,"item_id":item["id"],"drop_x":50,"drop_y":50}),
    )
    .await;
    assert_eq!(
        ctx.project.lock().unwrap().as_ref().unwrap().objects.len(),
        2
    );
    native(&ctx, "project", "undo_project", json!({})).await;
    assert_eq!(
        ctx.project.lock().unwrap().as_ref().unwrap().objects.len(),
        1
    );
    native(
        &ctx,
        "art_library",
        "unload_art_library",
        json!({"library_id":library_id}),
    )
    .await;
    native(
        &ctx,
        "art_library",
        "load_art_library",
        json!({"path":dir.path().join("art.bbart")}),
    )
    .await;
}

#[tokio::test]
async fn quality_preview_export_and_canvas_use_the_native_pipeline() {
    let (ctx, _) = fixture();
    let dir = tempfile::tempdir().unwrap();
    let before = ctx.project.lock().unwrap().clone();
    let request = json!({"kind":"material","enable_text":false,"enable_border":false,
        "x_axis":{"param":"speed","min":100,"max":200,"count":2},
        "y_axis":{"param":"power","min":10,"max":20,"count":2}});
    let preview = native(
        &ctx,
        "quality_test",
        "quality_test_preview",
        json!({"request":request}),
    )
    .await;
    assert!(preview["preview"].is_object());
    native(
        &ctx,
        "quality_test",
        "quality_test_export_gcode",
        json!({"request":request,"path":dir.path().join("test.gcode")}),
    )
    .await;
    assert!(
        std::fs::read_to_string(dir.path().join("test.gcode"))
            .unwrap()
            .contains("M5")
    );
    assert_eq!(*ctx.project.lock().unwrap(), before);
    assert!(!ctx.undo_state().unwrap().can_undo);
    let added = native(
        &ctx,
        "quality_test",
        "quality_test_create_material_on_canvas",
        json!({"request":request}),
    )
    .await;
    assert!(!added["createdObjectIds"].as_array().unwrap().is_empty());
    native(&ctx, "project", "undo_project", json!({})).await;
    assert_eq!(
        ctx.project.lock().unwrap().as_ref().unwrap().objects,
        before.unwrap().objects
    );
}

#[tokio::test]
async fn quality_hardware_commands_require_confirmations_before_any_device_access() {
    let (ctx, _) = fixture();
    for (command, expected) in [
        ("quality_test_frame", vec!["confirm_motion"]),
        (
            "quality_test_start",
            vec!["confirm_motion", "confirm_laser_on"],
        ),
    ] {
        let (status, result) = call(
            &ctx,
            "POST",
            "/api/v1/workflows/quality_test",
            json!({"command":command,"request":{"kind":"material"}}),
        )
        .await;
        assert_eq!(status, StatusCode::PRECONDITION_REQUIRED);
        assert_eq!(result["missing"], json!(expected));
        assert!(ctx.job.lock().unwrap().is_none());
    }
}

#[tokio::test]
async fn profile_patch_changes_z_and_rotary_without_resetting_other_fields() {
    let _persist = crate::test_support::PersistTestGuard::new();
    let (ctx, _) = fixture();
    let profile = ctx.settings.lock().unwrap().machine_profiles[0].clone();
    let path = format!("/api/v1/profiles/{}", profile.id);
    let (status, result) = call(&ctx,"PATCH",&path,json!({"supports_z_moves":true,"z_move_feed_mm_min":420,"rotary_enabled":true,"rotary_mm_per_rotation":360,"rotary_object_diameter_mm":80})).await;
    assert_eq!(status, StatusCode::OK, "{result}");
    let updated = ctx.settings.lock().unwrap().machine_profiles[0].clone();
    assert!(updated.supports_z_moves && updated.rotary_enabled);
    assert_eq!(updated.z_move_feed_mm_min, 420.0);
    assert_eq!(updated.bed_width_mm, profile.bed_width_mm);
    let (status, _) = call(&ctx, "PATCH", &path, json!({"z_move_feed_mm_min":-1})).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(ctx.settings.lock().unwrap().machine_profiles[0], updated);
}

#[tokio::test]
async fn variable_text_csv_preview_batch_and_undo_preserve_the_source() {
    let (ctx, _) = fixture();
    let result = beambench_service::ops::design::run_transaction(&ctx, serde_json::from_value(json!({
        "schema_version":1,"operations":[{"op":"create_text","text":"SN-{Serial:1,1,3}","x":10,"y":40,"ref":"label"}],"options":{}
    })).unwrap(), beambench_service::ops::design::TransactionMode::Apply);
    assert!(result.applied, "{result:?}");
    let id = ctx
        .project
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .objects
        .iter()
        .find(|o| matches!(o.data, ObjectData::Text { .. }))
        .unwrap()
        .id
        .to_string();
    let before = ctx.project.lock().unwrap().clone();
    let mut config =
        beambench_service::ops::workflows::schema()["examples"]["variable_text_config"].clone();
    config["source"]["totalCopies"] = 3.into();
    let preview = native(
        &ctx,
        "variable_text",
        "generate_batch_preview",
        json!({"object_id":id,"config":config}),
    )
    .await;
    assert_eq!(preview, json!(["SN-001", "SN-002", "SN-003"]));
    let batch = native(
        &ctx,
        "variable_text",
        "generate_variable_text_batch",
        json!({"object_id":id,"config":config,"offset_step":5}),
    )
    .await;
    assert_eq!(batch["copies"].as_array().unwrap().len(), 3);
    native(&ctx, "project", "undo_project", json!({})).await;
    assert_eq!(
        ctx.project.lock().unwrap().as_ref().unwrap().objects,
        before.unwrap().objects
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("names.csv");
    std::fs::write(&path, "name,code\n\"Smith, Jo\",A1\nPat,A2\n").unwrap();
    let csv = native(&ctx, "variable_text", "load_csv_file", json!({"path":path})).await;
    assert_eq!(csv["headers"], json!(["name", "code"]));
    assert_eq!(csv["rows"][0], json!(["Smith, Jo", "A1"]));
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // Deliberately hold the lock while exercising the busy response.
async fn native_edit_rejects_concurrent_design_transaction() {
    let (ctx, _) = fixture();
    let before = ctx.project.lock().unwrap().clone();
    let _transaction = ctx.design_transaction_lock.lock().unwrap();
    let (status, result) = call(
        &ctx,
        "POST",
        "/api/v1/workflows/project",
        json!({"command":"set_material_height","value":3}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{result}");
    assert_eq!(*ctx.project.lock().unwrap(), before);
}

#[tokio::test]
async fn image_conversion_masks_crop_and_undo_use_native_geometry() {
    let (ctx, id) = fixture();
    let before = ctx.project.lock().unwrap().clone();
    let (status, _) = call(
        &ctx,
        "POST",
        "/api/v1/workflows/vector",
        json!({"command":"convert_to_bitmap","object_id":id,"dpi":1e100}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(*ctx.project.lock().unwrap(), before);
    assert!(!ctx.undo_state().unwrap().can_undo);
    let converted = native(
        &ctx,
        "vector",
        "convert_to_bitmap",
        json!({"object_id":id,"dpi":100}),
    )
    .await;
    assert_eq!(converted["data"]["type"], "raster_image");
    assert!(ctx.project.lock().unwrap().as_ref().unwrap().dirty);
    let mask_id = {
        let mut guard = ctx.project.lock().unwrap();
        let project = guard.as_mut().unwrap();
        let mask = ProjectObject::new(
            "mask",
            project.ensure_default_layer(),
            Bounds::new(Point2D::new(10.0, 10.0), Point2D::new(20.0, 30.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 20.0,
                corner_radius: 0.0,
            },
        );
        let id = mask.id;
        project.add_object(mask);
        id
    };
    let masked = native(
        &ctx,
        "vector",
        "assign_image_mask",
        json!({"image_object_id":id,"mask_object_ids":[mask_id],"polarity":"keep_inside"}),
    )
    .await;
    assert_eq!(masked["data"]["masks"].as_array().unwrap().len(), 1);
    let before_crop = ctx.project.lock().unwrap().clone();
    let cropped = native(
        &ctx,
        "vector",
        "crop_image",
        json!({"image_object_id":id,"mask_object_id":mask_id}),
    )
    .await;
    assert!(
        cropped["bounds"]["max"]["x"].as_f64().unwrap()
            - cropped["bounds"]["min"]["x"].as_f64().unwrap()
            < 20.0
    );
    assert!(
        ctx.project
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .find_object(mask_id)
            .is_none()
    );
    native(&ctx, "project", "undo_project", json!({})).await;
    assert_eq!(
        ctx.project.lock().unwrap().as_ref().unwrap().objects,
        before_crop.unwrap().objects
    );
}

#[tokio::test]
async fn nesting_route_places_parts_and_undo_restores_them() {
    let (ctx, container) = fixture();
    let part = {
        let mut guard = ctx.project.lock().unwrap();
        let project = guard.as_mut().unwrap();
        let object = ProjectObject::new(
            "part",
            project.ensure_default_layer(),
            Bounds::new(Point2D::new(40.0, 40.0), Point2D::new(45.0, 45.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 5.0,
                height: 5.0,
                corner_radius: 0.0,
            },
        );
        let id = object.id;
        project.add_object(object);
        id
    };
    let before = ctx.project.lock().unwrap().clone();
    let options = beambench_service::ops::nesting::NestOptions {
        time_limit_ms: 100,
        allow_rotation: false,
        ..Default::default()
    };
    let (status, result) = call(
        &ctx,
        "POST",
        "/api/v1/workflows/nest",
        json!({"selected_ids":[container,part],"options":options}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert_eq!(result["placedObjectIds"], json!([part]));
    let placed = ctx
        .project
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .find_object(part)
        .unwrap()
        .bounds;
    assert!(
        placed.min.x >= 10.0 - 1e-6
            && placed.max.x <= 30.0 + 1e-6
            && placed.min.y >= 10.0 - 1e-6
            && placed.max.y <= 30.0 + 1e-6
    );
    native(&ctx, "project", "undo_project", json!({})).await;
    assert_eq!(
        ctx.project.lock().unwrap().as_ref().unwrap().objects,
        before.unwrap().objects
    );
}

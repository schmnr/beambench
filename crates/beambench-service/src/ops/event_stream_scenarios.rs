// Run with: cargo test -p beambench-service event_stream_scenarios
//       or: cargo nextest run -p beambench-service -E 'test(event_stream_scenarios)'

use super::app::{UpdateAppSettingsInput, update_app_settings};
use super::project::{
    AddLayerInput, AddObjectInput, UpdateLayerInput, add_layer, add_object, create_project,
    update_layer,
};
use crate::ServiceContext;
use beambench_common::{Bounds, Point2D};
use beambench_core::{ObjectData, OperationType, ShapeKind};

#[test]
fn project_creation_emits_event() {
    let ctx = ServiceContext::with_settings(Default::default());
    let mut rx = ctx.events.subscribe();

    create_project(&ctx, "TestProject").unwrap();

    let msg = rx.try_recv().unwrap();
    let value: serde_json::Value = serde_json::from_str(&msg).unwrap();
    assert_eq!(value["type"], "project.created");
    assert!(value["id"].is_u64());
    assert!(value["timestamp"].is_string());
    assert!(value["payload"].is_object());
}

#[test]
fn add_layer_emits_event() {
    let ctx = ServiceContext::with_settings(Default::default());
    create_project(&ctx, "LayerTest").unwrap();
    let mut rx = ctx.events.subscribe();

    add_layer(
        &ctx,
        AddLayerInput {
            name: "Engrave".to_string(),
            operation: OperationType::Image,
        },
    )
    .unwrap();

    let msg = rx.try_recv().unwrap();
    let value: serde_json::Value = serde_json::from_str(&msg).unwrap();
    assert_eq!(value["type"], "project.layer.added");
}

#[test]
fn add_object_emits_event() {
    let ctx = ServiceContext::with_settings(Default::default());
    create_project(&ctx, "ObjTest").unwrap();
    let layer = add_layer(
        &ctx,
        AddLayerInput {
            name: "Line".to_string(),
            operation: OperationType::Line,
        },
    )
    .unwrap();
    let layer_id = layer.id;
    let mut rx = ctx.events.subscribe();

    add_object(
        &ctx,
        AddObjectInput {
            name: "Circle".to_string(),
            layer_id,
            object_data: ObjectData::Shape {
                kind: ShapeKind::Ellipse,
                width: 20.0,
                height: 20.0,
                corner_radius: 0.0,
            },
            bounds: Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(20.0, 20.0)),
        },
    )
    .unwrap();

    let msg = rx.try_recv().unwrap();
    let value: serde_json::Value = serde_json::from_str(&msg).unwrap();
    assert_eq!(value["type"], "project.object.added");
}

#[test]
fn update_layer_emits_event() {
    let ctx = ServiceContext::with_settings(Default::default());
    create_project(&ctx, "UpdateTest").unwrap();
    let layer = add_layer(
        &ctx,
        AddLayerInput {
            name: "Line".to_string(),
            operation: OperationType::Line,
        },
    )
    .unwrap();
    let layer_id = layer.id;
    let mut rx = ctx.events.subscribe();

    update_layer(
        &ctx,
        layer_id,
        UpdateLayerInput {
            name: Some("Updated".to_string()),
            ..Default::default()
        },
    )
    .unwrap();

    let msg = rx.try_recv().unwrap();
    let value: serde_json::Value = serde_json::from_str(&msg).unwrap();
    assert_eq!(value["type"], "project.layer.updated");
}

#[test]
fn sequential_operations_emit_ordered_events() {
    let ctx = ServiceContext::with_settings(Default::default());
    let mut rx = ctx.events.subscribe();

    create_project(&ctx, "SeqTest").unwrap();
    let layer = add_layer(
        &ctx,
        AddLayerInput {
            name: "Line".to_string(),
            operation: OperationType::Line,
        },
    )
    .unwrap();
    let layer_id = layer.id;

    add_layer(
        &ctx,
        AddLayerInput {
            name: "Extra".to_string(),
            operation: OperationType::Cut,
        },
    )
    .unwrap();

    add_object(
        &ctx,
        AddObjectInput {
            name: "Rect".to_string(),
            layer_id,
            object_data: ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
            bounds: Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
        },
    )
    .unwrap();

    update_layer(
        &ctx,
        layer_id,
        UpdateLayerInput {
            visible: Some(false),
            ..Default::default()
        },
    )
    .unwrap();

    // Drain all available events
    let mut event_ids: Vec<u64> = Vec::new();
    loop {
        match rx.try_recv() {
            Ok(msg) => {
                let value: serde_json::Value = serde_json::from_str(&msg).unwrap();
                let id = value["id"].as_u64().expect("event id should be u64");
                event_ids.push(id);
            }
            Err(_) => break,
        }
    }

    // We should have at least 5 events: project.created, layer.added (setup), layer.added, object.added, layer.updated
    assert!(
        event_ids.len() >= 5,
        "Expected at least 5 events, got {}",
        event_ids.len()
    );

    // Event ids must be monotonically increasing
    for window in event_ids.windows(2) {
        assert!(
            window[1] > window[0],
            "Event ids should be monotonically increasing: {} > {} failed",
            window[1],
            window[0],
        );
    }
}

#[test]
fn settings_update_emits_event() {
    let _guard = crate::test_support::PersistTestGuard::new();
    let ctx = ServiceContext::with_settings(Default::default());
    let mut rx = ctx.events.subscribe();

    update_app_settings(
        &ctx,
        UpdateAppSettingsInput {
            autosave_enabled: Some(false),
            ..Default::default()
        },
    )
    .unwrap();

    let msg = rx.try_recv().unwrap();
    let value: serde_json::Value = serde_json::from_str(&msg).unwrap();
    assert_eq!(value["type"], "app.settings.updated");
}

//! Cross-surface parity tests: verify API responses match direct service state.

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::http::StatusCode;
    use beambench_common::{Bounds, Point2D};
    use beambench_core::Project;
    use beambench_core::object::{ObjectData, ShapeKind};
    use beambench_service::ServiceContext;
    use tower::ServiceExt;

    async fn collect_body(response: axum::http::Response<axum::body::Body>) -> Vec<u8> {
        use http_body_util::BodyExt;
        let body = response.into_body();
        let collected = body.collect().await.unwrap();
        collected.to_bytes().to_vec()
    }

    #[tokio::test]
    async fn create_project_api_matches_service_state() {
        let ctx = Arc::new(ServiceContext::new());
        let app = crate::routes::build_router(ctx.clone());

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/projects")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(r#"{"name": "Parity Test"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        // Verify service state directly
        let project = ctx.project.lock().unwrap();
        let p = project.as_ref().expect("project should exist after create");
        assert_eq!(p.metadata.project_name, "Parity Test");
    }

    #[tokio::test]
    async fn add_object_api_matches_direct_read() {
        let ctx = Arc::new(ServiceContext::new());
        let mut project = Project::new("Add Object Parity");
        let layer_id = project.ensure_default_layer();
        *ctx.project.lock().unwrap() = Some(project);

        let app = crate::routes::build_router(ctx.clone());

        let body = serde_json::json!({
            "name": "TestRect",
            "layer_id": layer_id.to_string(),
            "object_data": {
                "type": "shape",
                "kind": "rectangle",
                "width": 15.0,
                "height": 15.0,
                "corner_radius": 0.0
            },
            "bounds": { "min": { "x": 0.0, "y": 0.0 }, "max": { "x": 15.0, "y": 15.0 } }
        });

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/projects/objects")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        // API should accept the object
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "Expected success status, got: {}",
            response.status()
        );

        // Verify via direct service state
        let guard = ctx.project.lock().unwrap();
        let p = guard.as_ref().unwrap();
        assert_eq!(p.objects.len(), 1, "Should have exactly one object");
        assert_eq!(p.objects[0].name, "TestRect");
    }

    #[tokio::test]
    async fn undo_api_reverts_change() {
        let ctx = Arc::new(ServiceContext::new());
        let mut project = Project::new("Undo Parity");
        let layer_id = project.ensure_default_layer();
        *ctx.project.lock().unwrap() = Some(project);

        // Add an object via service to have something to undo
        use beambench_service::ops::project::{AddObjectInput, add_object};
        let _obj = add_object(
            &ctx,
            AddObjectInput {
                name: "WillUndo".into(),
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

        // Verify object exists
        assert_eq!(
            ctx.project.lock().unwrap().as_ref().unwrap().objects.len(),
            1
        );

        // Undo via API
        let app = crate::routes::build_router(ctx.clone());
        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/projects/undo")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        // Verify undo removed the object
        let guard = ctx.project.lock().unwrap();
        let p = guard.as_ref().unwrap();
        assert_eq!(p.objects.len(), 0, "Undo should have removed the object");
    }

    #[tokio::test]
    async fn update_layer_api_matches_service_state() {
        // Layer-level PATCH now only carries shell fields (name / enabled /
        // visible / color_tag). Per-entry settings (speed / power / mode …)
        // go through the cut-entry PATCH route at
        // `/projects/layers/{id}/entries/{entry_id}`, so this scenario
        // drives both endpoints to confirm API-vs-service parity end-to-end.
        let ctx = Arc::new(ServiceContext::new());
        let mut project = Project::new("Layer Parity");
        let layer_id = project.ensure_default_layer();
        let entry_id = project.find_layer(layer_id).unwrap().primary_entry().id;
        *ctx.project.lock().unwrap() = Some(project);

        let app = crate::routes::build_router(ctx.clone());

        // 1. Patch the layer shell.
        let layer_body = serde_json::json!({
            "name": "Renamed Layer",
            "color_tag": "#abcdef",
        });
        let layer_response = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("PATCH")
                    .uri(&format!("/api/v1/projects/layers/{}", layer_id))
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(layer_body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(layer_response.status(), StatusCode::OK);

        // 2. Patch the primary cut entry.
        let entry_body = serde_json::json!({
            "speed_mm_min": 750.0,
            "power_percent": 45.0,
        });
        let entry_response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("PATCH")
                    .uri(&format!(
                        "/api/v1/projects/layers/{}/entries/{}",
                        layer_id, entry_id
                    ))
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(entry_body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(entry_response.status(), StatusCode::OK);

        // Verify via direct service state.
        let guard = ctx.project.lock().unwrap();
        let p = guard.as_ref().unwrap();
        let layer = p.layers.iter().find(|l| l.id == layer_id).unwrap();
        assert_eq!(layer.name, "Renamed Layer");
        assert_eq!(layer.color_tag.0, "#C0C0C0");
        assert!((layer.primary_entry().speed_mm_min - 750.0).abs() < 0.01);
        assert!((layer.primary_entry().power_percent - 45.0).abs() < 0.01);
    }

    #[tokio::test]
    async fn export_svg_api_returns_content() {
        let ctx = Arc::new(ServiceContext::new());
        let mut project = Project::new("SVG Parity");
        let layer_id = project.ensure_default_layer();
        project.add_object(beambench_core::object::ProjectObject::new(
            "Rect",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(20.0, 20.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 20.0,
                height: 20.0,
                corner_radius: 0.0,
            },
        ));
        *ctx.project.lock().unwrap() = Some(project);

        let app = crate::routes::build_router(ctx.clone());

        // When no `path` is provided, the SVG export returns inline content
        let body = serde_json::json!({ "selection_only": false });

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/export/svg")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let resp_body = collect_body(response).await;
        let json: serde_json::Value = serde_json::from_slice(&resp_body).unwrap();
        // Should contain SVG content (inline mode) or a path (file mode)
        assert!(
            json.get("content").is_some() || json.get("path").is_some(),
            "Response should contain content or path"
        );
    }
}

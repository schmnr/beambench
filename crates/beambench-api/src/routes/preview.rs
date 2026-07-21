use std::sync::Arc;

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use beambench_service::ServiceContext;
use beambench_service::ops::planning::{self, SessionJobOptions};
use serde::Deserialize;

use crate::response::{ApiError, map_service_error};

pub fn router() -> Router<Arc<ServiceContext>> {
    Router::new()
        .route("/generate", post(generate_preview))
        .route("/stats", get(get_plan_stats))
        .route("/export/gcode", post(export_gcode))
}

async fn generate_preview(
    State(ctx): State<Arc<ServiceContext>>,
    body: Option<Json<SessionJobOptions>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let options = body.map(|Json(value)| value).unwrap_or_default();
    let preview =
        planning::generate_preview_with_options(&ctx, &options).map_err(map_service_error)?;
    Ok(Json(serde_json::json!({ "preview": preview })))
}

async fn get_plan_stats(
    State(ctx): State<Arc<ServiceContext>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let stats = planning::get_plan_stats(&ctx).map_err(map_service_error)?;
    Ok(Json(serde_json::to_value(stats).unwrap_or_default()))
}

#[derive(Deserialize)]
struct ExportGcodeBody {
    output_path: String,
    #[serde(default)]
    cut_selected_graphics: bool,
    #[serde(default)]
    use_selection_origin: bool,
    #[serde(default)]
    selected_object_ids: Vec<String>,
}

async fn export_gcode(
    State(ctx): State<Arc<ServiceContext>>,
    Json(body): Json<ExportGcodeBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let output_path = crate::response::validate_output_path(&body.output_path)?;
    let options = SessionJobOptions {
        cut_selected_graphics: body.cut_selected_graphics,
        use_selection_origin: body.use_selection_origin,
        selected_object_ids: body.selected_object_ids,
    };
    let path = planning::export_gcode_to_path_with_options(&ctx, &output_path, &options)
        .map_err(map_service_error)?;
    let bytes = std::fs::metadata(&output_path)
        .map(|m| m.len() as usize)
        .unwrap_or_default();
    Ok(Json(serde_json::json!({
        "exported": true,
        "path": path,
        "bytes": bytes,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;
    use tower::ServiceExt;

    async fn collect_body(response: axum::http::Response<axum::body::Body>) -> Vec<u8> {
        use http_body_util::BodyExt;
        let body = response.into_body();
        let collected = body.collect().await.unwrap();
        collected.to_bytes().to_vec()
    }

    #[tokio::test]
    async fn generate_preview_no_project_returns_404() {
        let ctx = Arc::new(ServiceContext::new());
        let app = crate::routes::build_router(ctx);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/preview/generate")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn export_gcode_without_project_returns_not_found() {
        let ctx = Arc::new(ServiceContext::new());
        let app = crate::routes::build_router(ctx);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/preview/export/gcode")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        r#"{"output_path": "/tmp/test.gcode"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn generate_preview_with_project() {
        use beambench_common::{Bounds, Point2D};
        use beambench_core::Project;
        use beambench_core::object::{ObjectData, ShapeKind};

        let ctx = Arc::new(ServiceContext::new());
        let mut project = Project::new("Preview Test");
        let layer_id = project.ensure_default_layer();
        project.add_object(beambench_core::object::ProjectObject::new(
            "Rect",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        ));
        ctx.project.lock().unwrap().replace(project);

        let app = crate::routes::build_router(ctx);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/preview/generate")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = collect_body(response).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json.get("preview").is_some());
    }

    #[tokio::test]
    async fn export_gcode_with_project_creates_file() {
        use beambench_common::{Bounds, Point2D};
        use beambench_core::Project;
        use beambench_core::object::{ObjectData, ShapeKind};

        let ctx = Arc::new(ServiceContext::new());
        let mut project = Project::new("GCode Export Test");
        let layer_id = project.ensure_default_layer();
        project.add_object(beambench_core::object::ProjectObject::new(
            "Rect",
            layer_id,
            Bounds::new(Point2D::new(5.0, 5.0), Point2D::new(25.0, 25.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 20.0,
                height: 20.0,
                corner_radius: 0.0,
            },
        ));
        ctx.project.lock().unwrap().replace(project);

        let tmp = tempfile::tempdir().unwrap();
        let gcode_path = tmp.path().join("test.gcode");

        let app = crate::routes::build_router(ctx);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/preview/export/gcode")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        serde_json::json!({ "output_path": gcode_path.to_str().unwrap() })
                            .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = collect_body(response).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["exported"], true);
        assert!(gcode_path.exists(), "G-code file should have been created");
    }
}

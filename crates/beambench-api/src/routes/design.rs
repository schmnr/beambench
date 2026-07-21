use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use beambench_common::Id;
use beambench_core::{ObjectId, PrintMode};
use beambench_service::ServiceContext;
use beambench_service::ops::design::{self, DesignPlan, DesignTransactionResult, TransactionMode};
use serde::Deserialize;
use uuid::Uuid;

use crate::response::{ApiError, map_service_error};

pub fn router() -> Router<Arc<ServiceContext>> {
    Router::new()
        .route("/describe", get(describe))
        .route("/schema", get(schema))
        .route("/render", post(render))
        .route("/transaction/plan", post(plan_transaction))
        .route("/transaction/apply", post(apply_transaction))
}

async fn describe(State(ctx): State<Arc<ServiceContext>>) -> Json<serde_json::Value> {
    Json(design::describe(&ctx))
}

async fn schema() -> Json<serde_json::Value> {
    Json(design::schema())
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum RenderFormat {
    Svg,
    Png,
}

#[derive(Deserialize)]
struct RenderBody {
    format: RenderFormat,
    path: String,
    #[serde(default)]
    selection_only: bool,
    #[serde(default)]
    selected_ids: Vec<String>,
    #[serde(default)]
    color: bool,
    #[serde(default = "default_pixels_per_mm")]
    pixels_per_mm: f64,
}

fn default_pixels_per_mm() -> f64 {
    4.0
}

fn parse_selected_ids(raw: &[String]) -> Result<Vec<ObjectId>, ApiError> {
    raw.iter()
        .map(|s| {
            let uuid = Uuid::parse_str(s).map_err(|e| {
                map_service_error(beambench_service::ServiceError::invalid_input(format!(
                    "Invalid object ID: {e}"
                )))
            })?;
            Ok(Id::from_uuid(uuid))
        })
        .collect()
}

async fn render(
    State(ctx): State<Arc<ServiceContext>>,
    Json(body): Json<RenderBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let output_path = crate::response::validate_output_path(&body.path)?;
    let selected_ids = parse_selected_ids(&body.selected_ids)?;
    let mode = if body.color {
        PrintMode::Color
    } else {
        PrintMode::Black
    };
    let bytes = {
        let project_guard = ctx.project.lock().map_err(|e| {
            map_service_error(beambench_service::ServiceError::internal(format!(
                "Failed to lock project: {e}"
            )))
        })?;
        let project = project_guard.as_ref().ok_or_else(|| {
            map_service_error(beambench_service::ServiceError::not_found(
                "No project open",
            ))
        })?;
        match body.format {
            RenderFormat::Svg => {
                let document = beambench_core::render_print_document_with_selection(
                    project,
                    mode,
                    body.selection_only,
                    &selected_ids,
                )
                .map_err(|e| {
                    map_service_error(beambench_service::ServiceError::internal(format!(
                        "Failed to render visual SVG: {e}"
                    )))
                })?;
                std::fs::write(&output_path, document.svg.as_bytes()).map_err(|e| {
                    map_service_error(beambench_service::ServiceError::persistence(format!(
                        "Failed to write render: {e}"
                    )))
                })?;
                document.svg.len()
            }
            RenderFormat::Png => {
                let png = beambench_core::render_print_png(
                    project,
                    mode,
                    body.selection_only,
                    &selected_ids,
                    body.pixels_per_mm,
                )
                .map_err(|e| {
                    map_service_error(beambench_service::ServiceError::internal(format!(
                        "Failed to render PNG: {e}"
                    )))
                })?;
                std::fs::write(&output_path, &png).map_err(|e| {
                    map_service_error(beambench_service::ServiceError::persistence(format!(
                        "Failed to write render: {e}"
                    )))
                })?;
                png.len()
            }
        }
    };

    Ok(Json(serde_json::json!({
        "schema_version": design::DESIGN_SCHEMA_VERSION,
        "format": match body.format {
            RenderFormat::Svg => "svg",
            RenderFormat::Png => "png",
        },
        "path": body.path,
        "bytes": bytes,
    })))
}

async fn plan_transaction(
    State(ctx): State<Arc<ServiceContext>>,
    Json(plan): Json<DesignPlan>,
) -> (StatusCode, Json<DesignTransactionResult>) {
    transaction_response(design::run_transaction(&ctx, plan, TransactionMode::Plan))
}

async fn apply_transaction(
    State(ctx): State<Arc<ServiceContext>>,
    Json(plan): Json<DesignPlan>,
) -> (StatusCode, Json<DesignTransactionResult>) {
    transaction_response(design::run_transaction(&ctx, plan, TransactionMode::Apply))
}

fn transaction_response(
    result: DesignTransactionResult,
) -> (StatusCode, Json<DesignTransactionResult>) {
    let status = match result.error.as_ref().map(|error| error.code) {
        Some("BUSY") => StatusCode::CONFLICT,
        Some(_) => StatusCode::BAD_REQUEST,
        None => StatusCode::OK,
    };
    (status, Json(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    use beambench_core::Project;
    use serde_json::json;
    use tower::ServiceExt;

    async fn collect_body(response: axum::http::Response<axum::body::Body>) -> Vec<u8> {
        use http_body_util::BodyExt;
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec()
    }

    fn ctx_with_project() -> Arc<ServiceContext> {
        let ctx = Arc::new(ServiceContext::new());
        *ctx.project.lock().unwrap() = Some(Project::new("Design API"));
        ctx
    }

    fn rectangle_plan() -> serde_json::Value {
        json!({
            "operations": [{
                "op": "create_rectangle",
                "ref": "box",
                "x": 10.0,
                "y": 10.0,
                "width": 20.0,
                "height": 15.0
            }],
            "options": {
                "validate_bounds": true,
                "allow_out_of_bounds": false
            }
        })
    }

    #[tokio::test]
    async fn describe_and_schema_routes_return_schema_version() {
        let app = crate::routes::build_router(ctx_with_project());
        let response = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/v1/design/describe")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = collect_body(response).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["schema_version"], 1);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/v1/design/schema")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = collect_body(response).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["schema_version"], 1);
        assert!(json["operations"].as_array().unwrap().len() > 10);
    }

    #[tokio::test]
    async fn render_route_writes_visual_png() {
        let app = crate::routes::build_router(ctx_with_project());
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("canvas.png");
        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/design/render")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        json!({
                            "format": "png",
                            "path": path.to_string_lossy(),
                            "pixels_per_mm": 2.0
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = collect_body(response).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["schema_version"], 1);
        assert_eq!(json["format"], "png");
        assert!(path.exists());
        assert!(std::fs::metadata(path).unwrap().len() > 0);
    }

    #[tokio::test]
    async fn render_route_writes_visual_svg() {
        let app = crate::routes::build_router(ctx_with_project());
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("canvas.svg");
        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/design/render")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        json!({
                            "format": "svg",
                            "path": path.to_string_lossy()
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let svg = std::fs::read_to_string(path).unwrap();
        assert!(svg.contains("<svg"));
    }

    #[tokio::test]
    async fn plan_and_apply_routes_use_same_transaction_contract() {
        let ctx = ctx_with_project();
        let app = crate::routes::build_router(ctx.clone());
        let plan_body = rectangle_plan().to_string();
        let response = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/design/transaction/plan")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(plan_body.clone()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = collect_body(response).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["schema_version"], 1);
        assert_eq!(json["applied"], false);
        assert_eq!(
            ctx.project.lock().unwrap().as_ref().unwrap().objects.len(),
            0
        );

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/design/transaction/apply")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(plan_body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = collect_body(response).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["applied"], true);
        assert_eq!(
            ctx.project.lock().unwrap().as_ref().unwrap().objects.len(),
            1
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn busy_transaction_returns_409() {
        let ctx = ctx_with_project();
        let _guard = ctx.design_transaction_lock.lock().unwrap();
        let app = crate::routes::build_router(ctx.clone());
        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/design/transaction/apply")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(rectangle_plan().to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);
        let body = collect_body(response).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"]["code"], "BUSY");
    }
}

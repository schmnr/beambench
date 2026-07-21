use std::sync::Arc;

use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use base64::Engine;
use beambench_common::Id;
use beambench_core::ObjectId;
use beambench_service::ServiceContext;
use beambench_service::ops::export;
use serde::Deserialize;
use uuid::Uuid;

use crate::response::{ApiError, map_service_error};

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

pub fn router() -> Router<Arc<ServiceContext>> {
    Router::new()
        .route("/svg", post(export_svg))
        .route("/dxf", post(export_dxf))
        .route("/pdf", post(export_pdf))
        .route("/eps", post(export_eps))
        .route("/ai", post(export_ai))
}

#[derive(Deserialize)]
struct ExportSvgBody {
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    selection_only: bool,
    #[serde(default)]
    selected_ids: Vec<String>,
}

#[derive(Deserialize)]
struct ExportDxfBody {
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    selection_only: bool,
    #[serde(default)]
    selected_ids: Vec<String>,
}

#[derive(Deserialize)]
struct ExportPdfBody {
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    selection_only: bool,
    #[serde(default)]
    selected_ids: Vec<String>,
}

/// Export the current project or selection as SVG.
async fn export_svg(
    State(ctx): State<Arc<ServiceContext>>,
    Json(body): Json<ExportSvgBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let selected_ids = parse_selected_ids(&body.selected_ids)?;
    let output = export::export_document(
        &ctx,
        export::ExportDocumentInput {
            path: body.path,
            selection_only: body.selection_only,
            selected_ids,
            format: export::ExportFormat::Svg,
        },
    )
    .map_err(map_service_error)?;

    match output.content {
        export::ExportDocumentContent::Svg(content) => Ok(Json(if let Some(path) = output.path {
            serde_json::json!({
                "path": path,
                "format": output.format.as_str(),
                "bytes": output.bytes,
            })
        } else {
            serde_json::json!({
                "format": output.format.as_str(),
                "content": content,
            })
        })),
        _ => unreachable!(),
    }
}

/// Export the current project or selection as DXF.
async fn export_dxf(
    State(ctx): State<Arc<ServiceContext>>,
    Json(body): Json<ExportDxfBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let selected_ids = parse_selected_ids(&body.selected_ids)?;
    let output = export::export_document(
        &ctx,
        export::ExportDocumentInput {
            path: body.path,
            selection_only: body.selection_only,
            selected_ids,
            format: export::ExportFormat::Dxf,
        },
    )
    .map_err(map_service_error)?;

    match output.content {
        export::ExportDocumentContent::Dxf(content) => Ok(Json(if let Some(path) = output.path {
            serde_json::json!({
                "path": path,
                "format": output.format.as_str(),
                "bytes": output.bytes,
            })
        } else {
            serde_json::json!({
                "format": output.format.as_str(),
                "content": content,
            })
        })),
        _ => unreachable!(),
    }
}

/// Export the current project or selection as PDF.
async fn export_pdf(
    State(ctx): State<Arc<ServiceContext>>,
    Json(body): Json<ExportPdfBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let selected_ids = parse_selected_ids(&body.selected_ids)?;
    let output = export::export_document(
        &ctx,
        export::ExportDocumentInput {
            path: body.path,
            selection_only: body.selection_only,
            selected_ids,
            format: export::ExportFormat::Pdf,
        },
    )
    .map_err(map_service_error)?;

    match output.content {
        export::ExportDocumentContent::Pdf(content) => Ok(Json(if let Some(path) = output.path {
            serde_json::json!({
                "path": path,
                "format": output.format.as_str(),
                "bytes": output.bytes,
            })
        } else {
            serde_json::json!({
                "format": output.format.as_str(),
                "content_base64": base64::engine::general_purpose::STANDARD.encode(content),
            })
        })),
        _ => unreachable!(),
    }
}

#[derive(Deserialize)]
struct ExportEpsBody {
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    selection_only: bool,
    #[serde(default)]
    selected_ids: Vec<String>,
}

/// Export the current project or selection as EPS.
async fn export_eps(
    State(ctx): State<Arc<ServiceContext>>,
    Json(body): Json<ExportEpsBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let selected_ids = parse_selected_ids(&body.selected_ids)?;
    let output = export::export_document(
        &ctx,
        export::ExportDocumentInput {
            path: body.path,
            selection_only: body.selection_only,
            selected_ids,
            format: export::ExportFormat::Eps,
        },
    )
    .map_err(map_service_error)?;

    match output.content {
        export::ExportDocumentContent::Eps(content) => Ok(Json(if let Some(path) = output.path {
            serde_json::json!({
                "path": path,
                "format": output.format.as_str(),
                "bytes": output.bytes,
            })
        } else {
            serde_json::json!({
                "format": output.format.as_str(),
                "content": content,
            })
        })),
        _ => unreachable!(),
    }
}

#[derive(Deserialize)]
struct ExportAiBody {
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    selection_only: bool,
    #[serde(default)]
    selected_ids: Vec<String>,
}

/// Export the current project or selection as AI.
async fn export_ai(
    State(ctx): State<Arc<ServiceContext>>,
    Json(body): Json<ExportAiBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let selected_ids = parse_selected_ids(&body.selected_ids)?;
    let output = export::export_document(
        &ctx,
        export::ExportDocumentInput {
            path: body.path,
            selection_only: body.selection_only,
            selected_ids,
            format: export::ExportFormat::Ai,
        },
    )
    .map_err(map_service_error)?;

    match output.content {
        export::ExportDocumentContent::Ai(content) => Ok(Json(if let Some(path) = output.path {
            serde_json::json!({
                "path": path,
                "format": output.format.as_str(),
                "bytes": output.bytes,
            })
        } else {
            serde_json::json!({
                "format": output.format.as_str(),
                "content": content,
            })
        })),
        _ => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;
    use base64::Engine;
    use beambench_common::{Bounds, Point2D};
    use beambench_core::{ObjectData, Project, ProjectObject, ShapeKind};
    use tempfile::tempdir;
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

    fn seeded_context() -> Arc<ServiceContext> {
        let (ctx, _) = seeded_context_with_id();
        ctx
    }

    fn seeded_context_with_id() -> (Arc<ServiceContext>, String) {
        let ctx = Arc::new(ServiceContext::with_settings(Default::default()));
        let mut project = Project::new("Export");
        let layer_id = project.ensure_default_layer();
        let obj = ProjectObject::new(
            "Rect",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(25.0, 10.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 25.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        );
        let obj_id = obj.id.to_string();
        project.add_object(obj);
        *ctx.project.lock().unwrap() = Some(project);
        (ctx, obj_id)
    }

    #[tokio::test]
    async fn export_svg_returns_inline_content() {
        let ctx = seeded_context();
        let router = crate::routes::build_router(ctx.clone());

        let response = router
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/export/svg")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(r#"{"selection_only": false}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn export_dxf_writes_requested_path() {
        let ctx = seeded_context();
        let router = crate::routes::build_router(ctx.clone());
        let dir = tempdir().unwrap();
        let out_path = dir.path().join("export.dxf");

        let response = router
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/export/dxf")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(format!(
                        r#"{{"path":"{}","selection_only": false}}"#,
                        out_path.to_string_lossy()
                    )))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert!(out_path.exists());
    }

    #[tokio::test]
    async fn export_pdf_returns_inline_base64_content() {
        let ctx = seeded_context();
        let router = crate::routes::build_router(ctx);

        let response = router
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/export/pdf")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(r#"{"selection_only": false}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = collect_body(response).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let encoded = json["content_base64"].as_str().unwrap_or_default();
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .unwrap();
        assert!(!decoded.is_empty());
        assert!(decoded.starts_with(b"%PDF"));
    }

    #[tokio::test]
    async fn export_eps_returns_inline_content() {
        let ctx = seeded_context();
        let router = crate::routes::build_router(ctx.clone());

        let response = router
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/export/eps")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(r#"{"selection_only": false}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = collect_body(response).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let content = json["content"].as_str().unwrap_or_default();
        assert!(content.contains("%!PS-Adobe-3.0 EPSF-3.0"));
    }

    #[tokio::test]
    async fn export_ai_returns_inline_content() {
        let ctx = seeded_context();
        let router = crate::routes::build_router(ctx.clone());

        let response = router
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/export/ai")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(r#"{"selection_only": false}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = collect_body(response).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let content = json["content"].as_str().unwrap_or_default();
        assert!(content.contains("%%Creator: Adobe Illustrator"));
    }

    #[tokio::test]
    async fn export_eps_with_selected_ids_returns_content() {
        let (ctx, obj_id) = seeded_context_with_id();
        let router = crate::routes::build_router(ctx);

        let response = router
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/export/eps")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(format!(
                        r#"{{"selection_only": true, "selected_ids": ["{obj_id}"]}}"#
                    )))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = collect_body(response).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let content = json["content"].as_str().unwrap_or_default();
        assert!(content.contains("%!PS-Adobe-3.0 EPSF-3.0"));
        assert!(content.contains("moveto"));
    }

    #[tokio::test]
    async fn export_ai_with_selected_ids_returns_content() {
        let (ctx, obj_id) = seeded_context_with_id();
        let router = crate::routes::build_router(ctx);

        let response = router
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/export/ai")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(format!(
                        r#"{{"selection_only": true, "selected_ids": ["{obj_id}"]}}"#
                    )))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = collect_body(response).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let content = json["content"].as_str().unwrap_or_default();
        assert!(content.contains("%%Creator: Adobe Illustrator"));
        assert!(content.contains("moveto"));
    }

    #[tokio::test]
    async fn export_eps_with_invalid_selected_id_returns_error() {
        let ctx = seeded_context();
        let router = crate::routes::build_router(ctx);

        let response = router
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/export/eps")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        r#"{"selection_only": true, "selected_ids": ["not-a-uuid"]}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_ne!(response.status(), StatusCode::OK);
    }
}

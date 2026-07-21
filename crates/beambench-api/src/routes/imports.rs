use std::sync::Arc;

use axum::routing::post;
use axum::{Json, Router};
use beambench_common::Id;
use beambench_core::LayerId;
use beambench_service::ServiceContext;
use beambench_service::ops::imports;
use serde::Deserialize;
use uuid::Uuid;

use crate::response::ApiError;

pub fn router() -> Router<Arc<ServiceContext>> {
    Router::new()
        .route("/svg", post(import_svg))
        .route("/image", post(import_image))
        .route("/files", post(import_files))
        .route("/dxf", post(import_dxf))
        .route("/pdf", post(import_pdf))
        .route("/ai", post(import_ai))
}

#[derive(Debug, Deserialize)]
struct ImportSingleBody {
    file_path: String,
    layer_id: String,
}

#[derive(Debug, Deserialize)]
struct ImportFilesBody {
    file_paths: Vec<String>,
    layer_id: String,
}

fn parse_layer_id(raw: &str) -> Result<LayerId, ApiError> {
    let uuid = Uuid::parse_str(raw).map_err(|e| {
        (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": format!("Invalid layer ID: {e}")})),
        )
    })?;
    Ok(Id::from_uuid(uuid))
}

async fn import_svg(
    axum::extract::State(ctx): axum::extract::State<Arc<ServiceContext>>,
    Json(body): Json<ImportSingleBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let objects = imports::import_svg_from_path(
        &ctx,
        imports::ImportSvgInput {
            file_path: body.file_path,
            layer_id: parse_layer_id(&body.layer_id)?,
        },
    )
    .map_err(|e| {
        use crate::response::map_service_error;
        map_service_error(e)
    })?;
    Ok(Json(serde_json::json!({
        "objects": objects,
        "count": objects.len(),
    })))
}

async fn import_image(
    axum::extract::State(ctx): axum::extract::State<Arc<ServiceContext>>,
    Json(body): Json<ImportSingleBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let object = imports::import_image_from_path(
        &ctx,
        imports::ImportImageInput {
            file_path: body.file_path,
            layer_id: parse_layer_id(&body.layer_id)?,
        },
    )
    .map_err(|e| {
        use crate::response::map_service_error;
        map_service_error(e)
    })?;
    Ok(Json(serde_json::to_value(object).unwrap_or_default()))
}

async fn import_files(
    axum::extract::State(ctx): axum::extract::State<Arc<ServiceContext>>,
    Json(body): Json<ImportFilesBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let objects = imports::import_files_from_paths(
        &ctx,
        imports::ImportFilesInput {
            file_paths: body.file_paths,
            layer_id: parse_layer_id(&body.layer_id)?,
        },
    )
    .map_err(|e| {
        use crate::response::map_service_error;
        map_service_error(e)
    })?;
    Ok(Json(serde_json::json!({
        "objects": objects,
        "count": objects.len(),
    })))
}

async fn import_dxf(
    axum::extract::State(ctx): axum::extract::State<Arc<ServiceContext>>,
    Json(body): Json<ImportSingleBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let objects = imports::import_vector_file_from_path(
        &ctx,
        imports::ImportVectorFileInput {
            file_path: body.file_path,
            layer_id: parse_layer_id(&body.layer_id)?,
            format: imports::VectorImportFormat::Dxf,
        },
    )
    .map_err(crate::response::map_service_error)?;
    Ok(Json(serde_json::json!({
        "objects": objects,
        "count": objects.len(),
    })))
}

async fn import_pdf(
    axum::extract::State(ctx): axum::extract::State<Arc<ServiceContext>>,
    Json(body): Json<ImportSingleBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let objects = imports::import_vector_file_from_path(
        &ctx,
        imports::ImportVectorFileInput {
            file_path: body.file_path,
            layer_id: parse_layer_id(&body.layer_id)?,
            format: imports::VectorImportFormat::Pdf,
        },
    )
    .map_err(crate::response::map_service_error)?;
    Ok(Json(serde_json::json!({
        "objects": objects,
        "count": objects.len(),
    })))
}

async fn import_ai(
    axum::extract::State(ctx): axum::extract::State<Arc<ServiceContext>>,
    Json(body): Json<ImportSingleBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let objects = imports::import_vector_file_from_path(
        &ctx,
        imports::ImportVectorFileInput {
            file_path: body.file_path,
            layer_id: parse_layer_id(&body.layer_id)?,
            format: imports::VectorImportFormat::Ai,
        },
    )
    .map_err(crate::response::map_service_error)?;
    Ok(Json(serde_json::json!({
        "objects": objects,
        "count": objects.len(),
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;
    use beambench_common::{Bounds, Point2D};
    use beambench_core::{ObjectData, ProjectObject, ShapeKind, export_pdf};
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

    fn sample_svg() -> &'static [u8] {
        br#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"><rect x="0" y="0" width="20" height="10"/></svg>"#
    }

    fn sample_pdf() -> Vec<u8> {
        let mut project = beambench_core::Project::new("Import PDF");
        let layer_id = project.ensure_default_layer();
        project.add_object(ProjectObject::new(
            "Rect",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(25.0, 10.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 25.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        ));
        export_pdf(&project, false, &[])
    }

    #[tokio::test]
    async fn import_svg_route_validates_layer_and_path() {
        let ctx = Arc::new(ServiceContext::new());
        *ctx.project.lock().unwrap() = Some(beambench_core::Project::new("Import"));
        let app = crate::routes::build_router(ctx.clone());

        let response = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/projects/import/svg")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(format!(
                        r#"{{"file_path":"missing.svg","layer_id":"{}"}}"#,
                        Uuid::new_v4()
                    )))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

        let dir = tempdir().unwrap();
        let svg_path = dir.path().join("demo.svg");
        std::fs::write(&svg_path, sample_svg()).unwrap();
        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/projects/import/svg")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(format!(
                        r#"{{"file_path":"{}","layer_id":"{}"}}"#,
                        svg_path.to_string_lossy(),
                        Uuid::new_v4()
                    )))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn import_svg_route_imports_objects() {
        let ctx = Arc::new(ServiceContext::new());
        let mut project = beambench_core::Project::new("Import");
        let layer_id = project.ensure_default_layer();
        *ctx.project.lock().unwrap() = Some(project);
        let app = crate::routes::build_router(ctx);

        let dir = tempdir().unwrap();
        let svg_path = dir.path().join("demo.svg");
        std::fs::write(&svg_path, sample_svg()).unwrap();

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/projects/import/svg")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(format!(
                        r#"{{"file_path":"{}","layer_id":"{}"}}"#,
                        svg_path.to_string_lossy(),
                        layer_id
                    )))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = collect_body(response).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["count"], 1);
    }

    #[tokio::test]
    async fn import_dxf_route_imports_objects() {
        let ctx = Arc::new(ServiceContext::new());
        let mut project = beambench_core::Project::new("Import");
        let layer_id = project.ensure_default_layer();
        *ctx.project.lock().unwrap() = Some(project);
        let app = crate::routes::build_router(ctx);

        let dir = tempdir().unwrap();
        let dxf_path = dir.path().join("demo.dxf");
        std::fs::write(
            &dxf_path,
            "0\nSECTION\n2\nENTITIES\n0\nLINE\n8\n0\n10\n0\n20\n0\n11\n10\n21\n10\n0\nENDSEC\n0\nEOF\n",
        )
        .unwrap();

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/projects/import/dxf")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(format!(
                        r#"{{"file_path":"{}","layer_id":"{}"}}"#,
                        dxf_path.to_string_lossy(),
                        layer_id
                    )))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn import_pdf_route_imports_objects() {
        let ctx = Arc::new(ServiceContext::new());
        let mut project = beambench_core::Project::new("Import");
        let layer_id = project.ensure_default_layer();
        *ctx.project.lock().unwrap() = Some(project);
        let app = crate::routes::build_router(ctx);

        let dir = tempdir().unwrap();
        let pdf_path = dir.path().join("demo.pdf");
        std::fs::write(&pdf_path, sample_pdf()).unwrap();

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/projects/import/pdf")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(format!(
                        r#"{{"file_path":"{}","layer_id":"{}"}}"#,
                        pdf_path.to_string_lossy(),
                        layer_id
                    )))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = collect_body(response).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["count"].as_u64().unwrap_or_default() >= 1);
    }

    #[tokio::test]
    async fn import_ai_route_imports_objects() {
        let ctx = Arc::new(ServiceContext::new());
        let mut project = beambench_core::Project::new("Import");
        let layer_id = project.ensure_default_layer();
        *ctx.project.lock().unwrap() = Some(project);
        let app = crate::routes::build_router(ctx);

        let dir = tempdir().unwrap();
        let ai_path = dir.path().join("demo.ai");
        std::fs::write(&ai_path, sample_pdf()).unwrap();

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/projects/import/ai")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(format!(
                        r#"{{"file_path":"{}","layer_id":"{}"}}"#,
                        ai_path.to_string_lossy(),
                        layer_id
                    )))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = collect_body(response).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["count"].as_u64().unwrap_or_default() >= 1);
    }
}

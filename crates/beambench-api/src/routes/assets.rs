use std::sync::Arc;

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use beambench_core::AssetMediaType;
use beambench_service::ServiceContext;
use beambench_service::ops::assets;
use serde::Deserialize;

use crate::response::{ApiError, map_service_error};

pub fn router() -> Router<Arc<ServiceContext>> {
    Router::new()
        .route("/", get(list_assets))
        .route("/{id}", get(get_asset))
        .route("/import", post(import_asset))
}

/// List all assets in the current project (metadata only, no binary data).
async fn list_assets(
    State(ctx): State<Arc<ServiceContext>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let asset_list = assets::list_assets(&ctx).map_err(map_service_error)?;
    Ok(Json(serde_json::json!({
        "assets": asset_list,
        "count": asset_list.len(),
    })))
}

/// Get a single asset's binary data by ID.
async fn get_asset(
    State(ctx): State<Arc<ServiceContext>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let uuid = uuid::Uuid::parse_str(&id).map_err(|e| {
        map_service_error(beambench_service::ServiceError::invalid_input(format!(
            "Invalid asset ID: {e}"
        )))
    })?;
    let asset_id: beambench_core::AssetId = beambench_common::Id::from_uuid(uuid);
    let (asset, data) = assets::get_asset(&ctx, asset_id).map_err(map_service_error)?;

    let content_type = match asset.media_type {
        AssetMediaType::Png => "image/png",
        AssetMediaType::Jpeg => "image/jpeg",
        AssetMediaType::Bmp => "image/bmp",
        AssetMediaType::Gif => "image/gif",
        AssetMediaType::Tiff => "image/tiff",
        AssetMediaType::Webp => "image/webp",
        AssetMediaType::Tga => "image/x-tga",
        AssetMediaType::Svg => "image/svg+xml",
        AssetMediaType::Dxf => "image/vnd.dxf",
        AssetMediaType::Ai => "application/postscript",
        AssetMediaType::Pdf => "application/pdf",
        AssetMediaType::Eps => "application/postscript",
    };

    // SVG assets can contain scripts. Navigation to a same-origin asset must
    // never grant it access to project or console GET endpoints.
    Ok((
        [
            (axum::http::header::CONTENT_TYPE, content_type),
            (
                axum::http::header::CONTENT_SECURITY_POLICY,
                "sandbox; default-src 'none'; img-src data:; style-src 'unsafe-inline'",
            ),
            (axum::http::header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        data,
    ))
}

#[derive(Deserialize)]
struct ImportAssetBody {
    path: String,
}

/// Import an asset from a file path on disk.
async fn import_asset(
    State(ctx): State<Arc<ServiceContext>>,
    Json(body): Json<ImportAssetBody>,
) -> Result<(axum::http::StatusCode, Json<serde_json::Value>), ApiError> {
    let (asset, byte_size) = assets::import_asset_from_path(&ctx, std::path::Path::new(&body.path))
        .map_err(map_service_error)?;
    Ok((
        axum::http::StatusCode::CREATED,
        Json(serde_json::json!({
            "asset_id": asset.id.to_string(),
            "filename": asset.original_filename,
            "media_type": asset.media_type.extension(),
            "byte_size": byte_size,
        })),
    ))
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
    async fn list_assets_no_project_returns_not_found() {
        let ctx = Arc::new(ServiceContext::new());
        let app = crate::routes::build_router(ctx);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/v1/assets")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn list_assets_with_empty_project() {
        let ctx = Arc::new(ServiceContext::new());
        {
            let mut guard = ctx.project.lock().unwrap();
            *guard = Some(beambench_core::Project::new("test"));
        }
        let app = crate::routes::build_router(ctx);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/v1/assets")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = collect_body(response).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["count"], 0);
        assert!(json["assets"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn get_asset_no_project_returns_not_found() {
        let ctx = Arc::new(ServiceContext::new());
        let app = crate::routes::build_router(ctx);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/v1/assets/550e8400-e29b-41d4-a716-446655440000")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn import_asset_no_project_returns_not_found() {
        let ctx = Arc::new(ServiceContext::new());
        let app = crate::routes::build_router(ctx);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/assets/import")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(r#"{"path":"/tmp/nonexistent.png"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}

#[cfg(test)]
mod active_asset_tests {
    use super::*;
    use tower::ServiceExt;

    #[tokio::test]
    async fn svg_assets_are_sandboxed_without_changing_bytes() {
        let ctx = Arc::new(ServiceContext::new());
        let bytes = br#"<svg xmlns="http://www.w3.org/2000/svg"><script>fetch('/api/v1/projects')</script></svg>"#.to_vec();
        let asset = beambench_core::Asset::new(
            "active.svg",
            AssetMediaType::Svg,
            bytes.len() as u64,
            None,
            None,
        );
        let mut project = beambench_core::Project::new("test");
        project.add_asset(asset.clone(), bytes.clone());
        *ctx.project.lock().unwrap() = Some(project);
        let response = crate::routes::build_router(ctx)
            .oneshot(
                axum::http::Request::builder()
                    .uri(format!("/api/v1/assets/{}", asset.id))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        assert_eq!(
            response.headers()[axum::http::header::CONTENT_TYPE],
            "image/svg+xml"
        );
        assert!(
            response.headers()[axum::http::header::CONTENT_SECURITY_POLICY]
                .to_str()
                .unwrap()
                .starts_with("sandbox; default-src 'none'")
        );
        assert_eq!(
            response.headers()[axum::http::header::X_CONTENT_TYPE_OPTIONS],
            "nosniff"
        );
        use http_body_util::BodyExt;
        assert_eq!(
            response.into_body().collect().await.unwrap().to_bytes(),
            bytes
        );
    }
}

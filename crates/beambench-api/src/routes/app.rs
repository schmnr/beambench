use std::sync::Arc;

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use beambench_service::ServiceContext;
use beambench_service::ops::app::{self, UpdateAppSettingsInput};

use crate::response::{ApiError, map_service_error};

pub fn router() -> Router<Arc<ServiceContext>> {
    Router::new()
        .route("/status", get(get_status))
        .route("/settings", get(get_settings).post(update_settings))
}

async fn get_status(
    State(ctx): State<Arc<ServiceContext>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let status = app::get_app_status(&ctx).map_err(map_service_error)?;
    let connected = ctx.session.lock().map(|s| s.is_some()).unwrap_or(false);
    let has_project = ctx.project.lock().map(|p| p.is_some()).unwrap_or(false);
    Ok(Json(serde_json::json!({
        "app_version": status.version,
        "state": status.state,
        "connected": connected,
        "has_project": has_project,
    })))
}

async fn get_settings(
    State(ctx): State<Arc<ServiceContext>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let settings = app::get_app_settings(&ctx).map_err(map_service_error)?;
    Ok(Json(serde_json::to_value(settings).unwrap_or_default()))
}

async fn update_settings(
    State(ctx): State<Arc<ServiceContext>>,
    Json(input): Json<UpdateAppSettingsInput>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let updated = app::update_app_settings(&ctx, input).map_err(map_service_error)?;
    Ok(Json(serde_json::to_value(updated).unwrap_or_default()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::PersistTestGuard;
    use axum::http::StatusCode;

    #[tokio::test]
    async fn get_status_returns_version() {
        let ctx = Arc::new(ServiceContext::new());
        let app = crate::routes::build_router(ctx);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/v1/app/status")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = collect_body(response).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json.get("app_version").is_some());
        assert_eq!(json["connected"], false);
        assert_eq!(json["has_project"], false);
    }

    #[tokio::test]
    async fn get_settings_returns_defaults() {
        let ctx = Arc::new(ServiceContext::with_settings(
            beambench_core::AppSettings::default(),
        ));
        let app = crate::routes::build_router(ctx);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/v1/app/settings")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = collect_body(response).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["autosave_enabled"], true);
        assert_eq!(json["api_port"], 5900);
        assert_eq!(json["ui_theme"], "dark");
    }

    #[tokio::test]
    async fn update_settings_merges_fields() {
        let _guard = PersistTestGuard::new();
        let ctx = Arc::new(ServiceContext::with_settings(
            beambench_core::AppSettings::default(),
        ));
        let app = crate::routes::build_router(ctx);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/app/settings")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        r#"{"api_port": 8080, "autosave_enabled": false}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = collect_body(response).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["api_port"], 8080);
        assert_eq!(json["autosave_enabled"], false);
        // Unchanged fields should keep defaults
        assert_eq!(json["api_localhost_only"], true);
    }

    #[tokio::test]
    async fn update_settings_writes_snap_threshold_px() {
        let _guard = PersistTestGuard::new();
        let ctx = Arc::new(ServiceContext::with_settings(
            beambench_core::AppSettings::default(),
        ));
        let app = crate::routes::build_router(ctx);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/app/settings")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(r#"{"snap_threshold_px": 12.0}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = collect_body(response).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["snap_threshold_px"], 12.0);
    }

    #[tokio::test]
    async fn update_settings_accepts_display_schema_fields() {
        let _guard = PersistTestGuard::new();
        let ctx = Arc::new(ServiceContext::with_settings(
            beambench_core::AppSettings::default(),
        ));
        let app = crate::routes::build_router(ctx);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/app/settings")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        r#"{
                            "ui_theme": "system",
                            "dark_mode": true,
                            "antialiasing": true,
                            "filled_rendering": true,
                            "reduce_motion": true,
                            "show_palette_labels": true,
                            "cursor_size": "large",
                            "toolbar_icon_size": "small",
                            "click_tolerance_px": 9.5,
                            "scroll_zoom": false,
                            "debug_log_enabled": true
                        }"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = collect_body(response).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["ui_theme"], "system");
        assert_eq!(json["dark_mode"], true);
        assert_eq!(json["antialiasing"], true);
        assert_eq!(json["filled_rendering"], true);
        assert_eq!(json["reduce_motion"], true);
        assert_eq!(json["show_palette_labels"], true);
        assert_eq!(json["cursor_size"], "large");
        assert_eq!(json["toolbar_icon_size"], "small");
        assert_eq!(json["click_tolerance_px"], 9.5);
        assert_eq!(json["scroll_zoom"], false);
        assert_eq!(json["debug_log_enabled"], true);
    }

    #[tokio::test]
    async fn update_settings_rejects_invalid_ui_theme() {
        let _guard = PersistTestGuard::new();
        let ctx = Arc::new(ServiceContext::with_settings(
            beambench_core::AppSettings::default(),
        ));
        let app = crate::routes::build_router(ctx);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/app/settings")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(r#"{"ui_theme":"sepia"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    // Helper: oneshot is from tower::ServiceExt, re-exported by axum
    use tower::ServiceExt;

    async fn collect_body(response: axum::http::Response<axum::body::Body>) -> Vec<u8> {
        use http_body_util::BodyExt;
        let body = response.into_body();
        let collected = body.collect().await.unwrap();
        collected.to_bytes().to_vec()
    }
}

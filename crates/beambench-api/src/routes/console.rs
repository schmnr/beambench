use std::sync::Arc;

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use beambench_service::{ServiceContext, agent};
use serde::Deserialize;

use crate::response::{ApiError, confirmation_required};

pub fn router() -> Router<Arc<ServiceContext>> {
    Router::new().route("/", get(get_console_log).post(send_gcode_line_handler))
}

#[derive(Deserialize)]
struct GetConsoleQuery {
    #[serde(default = "default_limit")]
    limit: usize,
}

fn default_limit() -> usize {
    100
}

#[derive(Deserialize)]
struct SendGcodeBody {
    line: String,
    #[serde(default)]
    confirm_raw_gcode: bool,
    #[serde(default)]
    confirm_laser_on: bool,
}

/// Get console log entries (sent/received G-code).
async fn get_console_log(
    State(ctx): State<Arc<ServiceContext>>,
    axum::extract::Query(query): axum::extract::Query<GetConsoleQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let entries = ctx.get_console_log(query.limit).map_err(|e| {
        (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": e})),
        )
    })?;
    Ok(Json(serde_json::json!({
        "entries": entries,
        "count": entries.len(),
    })))
}

/// Send a G-code line to the active machine session.
async fn send_gcode_line_handler(
    State(ctx): State<Arc<ServiceContext>>,
    Json(body): Json<SendGcodeBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if !body.confirm_raw_gcode {
        return Err(confirmation_required(&["confirm_raw_gcode"]));
    }
    if agent::raw_gcode_requires_laser_confirmation(&body.line) && !body.confirm_laser_on {
        return Err(confirmation_required(&["confirm_laser_on"]));
    }
    ctx.send_gcode_line(&body.line).map_err(|e| {
        (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": e})),
        )
    })?;
    Ok(Json(serde_json::json!({
        "success": true,
        "line": body.line,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;
    use tower::ServiceExt;

    #[tokio::test]
    async fn get_console_log_returns_empty_when_no_entries() {
        let ctx = Arc::new(ServiceContext::with_settings(Default::default()));
        let router = crate::routes::build_router(ctx.clone());

        let response = router
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/v1/console")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        use http_body_util::BodyExt;
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["count"], 0);
        assert!(json["entries"].is_array());
    }

    #[tokio::test]
    async fn send_gcode_line_fails_without_session() {
        let ctx = Arc::new(ServiceContext::with_settings(Default::default()));
        let router = crate::routes::build_router(ctx.clone());

        let response = router
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/console")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        r#"{"line": "G0 X10", "confirm_raw_gcode": true}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn send_gcode_line_requires_raw_confirmation_first() {
        let ctx = Arc::new(ServiceContext::with_settings(Default::default()));
        let router = crate::routes::build_router(ctx.clone());

        let response = router
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/console")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(r#"{"line": "M3 S1"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::PRECONDITION_REQUIRED);
        use http_body_util::BodyExt;
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error_code"], "CONFIRMATION_REQUIRED");
        assert_eq!(json["missing"][0], "confirm_raw_gcode");
    }
}

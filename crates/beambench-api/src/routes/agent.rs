use std::sync::Arc;

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use beambench_service::{ServiceContext, agent};

use crate::response::{ApiError, map_service_error};

pub fn router() -> Router<Arc<ServiceContext>> {
    Router::new()
        .route("/capabilities", get(get_capabilities))
        .route("/state", get(get_state))
        .route("/guide", get(get_guide))
}

async fn get_capabilities() -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(agent::capabilities_response()))
}

async fn get_state(
    State(ctx): State<Arc<ServiceContext>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let state = agent::agent_state(&ctx).map_err(map_service_error)?;
    Ok(Json(state))
}

async fn get_guide() -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(agent::guide_response()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{Method, StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    async fn read_json(response: axum::http::Response<axum::body::Body>) -> serde_json::Value {
        let body = response.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice(&body).unwrap()
    }

    #[tokio::test]
    async fn agent_capabilities_returns_registry_shape() {
        let ctx = Arc::new(ServiceContext::new());
        let app = crate::routes::build_router(ctx);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/v1/agent/capabilities")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let json = read_json(response).await;
        assert_eq!(json["schema_version"], 1);
        assert!(json["app_version"].as_str().is_some());
        assert!(json["generated_at"].as_str().is_some());
        assert!(json["capabilities"].as_array().unwrap().len() > 10);
        assert!(
            json["warning_codes"]
                .as_array()
                .unwrap()
                .iter()
                .any(|code| code == "SELECTION_STALE")
        );
    }

    #[tokio::test]
    async fn agent_state_returns_warning_array_without_project() {
        let ctx = Arc::new(ServiceContext::new());
        let app = crate::routes::build_router(ctx);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/v1/agent/state")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let json = read_json(response).await;
        assert_eq!(json["schema_version"], 1);
        assert!(
            json["warnings"]
                .as_array()
                .unwrap()
                .iter()
                .any(|warning| { warning["code"] == "NO_ACTIVE_PROJECT" })
        );
    }

    #[tokio::test]
    async fn agent_guide_returns_bootstrap_and_safety_summary() {
        let ctx = Arc::new(ServiceContext::new());
        let app = crate::routes::build_router(ctx);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/v1/agent/guide")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let json = read_json(response).await;
        assert_eq!(json["schema_version"], 1);
        assert_eq!(json["safety_summary"]["motion_flag"], "--confirm-motion");
        assert_eq!(json["polling"]["state_ms"], 1000);
        assert!(json["bootstrap"].as_array().unwrap().len() >= 3);
    }

    #[tokio::test]
    async fn supported_capability_api_refs_resolve_against_router() {
        let ctx = Arc::new(ServiceContext::new());
        let app = crate::routes::build_router(ctx);

        for cap in agent::capabilities() {
            let Some(api) = cap.api else {
                continue;
            };
            let method: Method = api
                .method
                .parse()
                .unwrap_or_else(|_| panic!("invalid method for {}", cap.id));
            let response = app
                .clone()
                .oneshot(
                    axum::http::Request::builder()
                        .method(method)
                        .uri(api.path)
                        .header("content-type", "application/json")
                        .body(axum::body::Body::from("{}"))
                        .unwrap(),
                )
                .await
                .unwrap();
            let status = response.status();
            let body = response.into_body().collect().await.unwrap().to_bytes();

            assert_ne!(
                status,
                StatusCode::METHOD_NOT_ALLOWED,
                "capability {} points at route with wrong method {} {}",
                cap.id,
                api.method,
                api.path
            );
            if status == StatusCode::NOT_FOUND {
                let body_json = serde_json::from_slice::<serde_json::Value>(&body);
                assert!(
                    body_json
                        .as_ref()
                        .is_ok_and(|value| value.get("error").is_some()),
                    "capability {} points at missing route {} {}",
                    cap.id,
                    api.method,
                    api.path
                );
            }
        }
    }
}

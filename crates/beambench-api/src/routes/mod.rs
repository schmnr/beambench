pub mod agent;
pub mod app;
pub mod assets;
pub mod camera;
pub mod console;
pub mod design;
pub mod events;
pub mod export;
pub mod imports;
pub mod jobs;
pub mod machine;
pub mod macros;
pub mod materials;
pub mod preview;
pub mod profiles;
pub mod projects;
pub mod vector;

#[cfg(test)]
mod parity_scenarios;

use std::sync::Arc;

use axum::Router;
use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::{self, Next};
use axum::response::Response;
use beambench_service::ServiceContext;

/// Reject requests that carry a browser `Origin` header. The API can move the
/// laser and start jobs; non-browser clients (CLI, scripts) never send Origin,
/// while a cross-site `fetch` from any web page always does — so this blocks
/// drive-by requests from websites without affecting legitimate API consumers.
async fn reject_browser_origins(request: Request, next: Next) -> Result<Response, StatusCode> {
    if request.headers().contains_key(axum::http::header::ORIGIN) {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(next.run(request).await)
}

/// Build the full API router with all endpoint groups.
pub fn build_router(ctx: Arc<ServiceContext>) -> Router {
    Router::new()
        .nest("/api/v1/app", app::router())
        .nest("/api/v1/agent", agent::router())
        .nest("/api/v1/camera", camera::router())
        .nest("/api/v1/projects", projects::router())
        .nest("/api/v1/projects/import", imports::router())
        .nest("/api/v1/machine", machine::router())
        .nest("/api/v1/jobs", jobs::router())
        .nest("/api/v1/preview", preview::router())
        .nest("/api/v1/vector", vector::router())
        .nest("/api/v1/design", design::router())
        .nest("/api/v1/events", events::router())
        .nest("/api/v1/assets", assets::router())
        .nest("/api/v1/profiles", profiles::router())
        .nest("/api/v1/console", console::router())
        .nest("/api/v1/materials", materials::router())
        .nest("/api/v1/macros", macros::router())
        .nest("/api/v1/export", export::router())
        .layer(middleware::from_fn(reject_browser_origins))
        .with_state(ctx)
}

#[cfg(test)]
mod origin_guard_tests {
    use std::sync::Arc;

    use axum::http::StatusCode;
    use beambench_service::ServiceContext;
    use tower::ServiceExt;

    #[tokio::test]
    async fn requests_with_browser_origin_are_rejected() {
        let ctx = Arc::new(ServiceContext::new());
        let app = super::build_router(ctx);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/v1/app/status")
                    .header("Origin", "https://evil.example")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn requests_without_origin_pass_through() {
        let ctx = Arc::new(ServiceContext::new());
        let app = super::build_router(ctx);

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
    }
}

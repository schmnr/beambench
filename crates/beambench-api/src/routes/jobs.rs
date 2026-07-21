use std::sync::Arc;

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use beambench_common::machine::JobProgress;
use beambench_service::ServiceContext;
use beambench_service::ops::machine;
use beambench_service::ops::planning::SessionJobOptions;
use serde::Deserialize;

use crate::response::{ApiError, confirmation_required, map_service_error};

pub fn router() -> Router<Arc<ServiceContext>> {
    Router::new()
        .route("/preflight", post(run_preflight))
        .route("/start", post(start_job))
        .route("/pause", post(pause_job))
        .route("/resume", post(resume_job))
        .route("/cancel", post(cancel_job))
        .route("/frame", post(frame_job))
        .route("/progress", get(get_progress))
}

#[derive(Debug, Default, Deserialize)]
struct FrameJobBody {
    frame_mode: Option<String>,
    selected_object_ids: Option<Vec<String>>,
    laser_on_override: Option<bool>,
    feed_rate: Option<f64>,
    #[serde(default)]
    confirm_motion: bool,
    #[serde(default)]
    confirm_laser_on: bool,
}

#[derive(Debug, Default, Deserialize)]
struct StartJobBody {
    #[serde(flatten)]
    options: SessionJobOptions,
    #[serde(default)]
    confirm_motion: bool,
    #[serde(default)]
    confirm_laser_on: bool,
}

async fn run_preflight(
    State(ctx): State<Arc<ServiceContext>>,
    body: Option<Json<SessionJobOptions>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let options = body.map(|Json(value)| value).unwrap_or_default();
    let report =
        machine::run_preflight_check_with_options(&ctx, &options).map_err(map_service_error)?;
    Ok(Json(serde_json::to_value(report).unwrap_or_default()))
}

async fn start_job(
    State(ctx): State<Arc<ServiceContext>>,
    body: Option<Json<StartJobBody>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let options = body.map(|Json(value)| value).unwrap_or_default();
    if !options.confirm_motion {
        return Err(confirmation_required(&["confirm_motion"]));
    }
    if !options.confirm_laser_on {
        return Err(confirmation_required(&["confirm_laser_on"]));
    }
    let job_ctx = ctx.clone();
    let job_options = options.options;
    let progress = tokio::task::spawn_blocking(move || {
        machine::start_job_with_options(&job_ctx, &job_options)
    })
    .await
    .map_err(|error| {
        map_service_error(beambench_service::ServiceError::internal(format!(
            "Job start task failed: {error}"
        )))
    })?
    .map_err(map_service_error)?;
    machine::spawn_job_tick_loop(ctx.clone());
    Ok(Json(serde_json::json!({
        "started": true,
        "progress": progress,
    })))
}

async fn pause_job(
    State(ctx): State<Arc<ServiceContext>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    machine::pause_job(&ctx).map_err(map_service_error)?;
    Ok(Json(serde_json::json!({ "paused": true })))
}

async fn resume_job(
    State(ctx): State<Arc<ServiceContext>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    machine::resume_job(&ctx).map_err(map_service_error)?;
    Ok(Json(serde_json::json!({ "resumed": true })))
}

async fn cancel_job(
    State(ctx): State<Arc<ServiceContext>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    machine::cancel_job(&ctx).map_err(map_service_error)?;
    Ok(Json(serde_json::json!({ "cancelled": true })))
}

async fn frame_job(
    State(ctx): State<Arc<ServiceContext>>,
    body: Option<Json<FrameJobBody>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let body = body.map(|Json(body)| body).unwrap_or_default();
    if !body.confirm_motion {
        return Err(confirmation_required(&["confirm_motion"]));
    }
    if body.laser_on_override.unwrap_or(false) && !body.confirm_laser_on {
        return Err(confirmation_required(&["confirm_laser_on"]));
    }
    let frame_mode = body.frame_mode.unwrap_or_else(|| "rectangular".to_string());
    let selected_object_ids = body.selected_object_ids.unwrap_or_default();
    let frame_ctx = ctx.clone();
    let laser_on_override = body.laser_on_override.unwrap_or(false);
    let feed_rate = body.feed_rate;
    let progress = tokio::task::spawn_blocking(move || {
        machine::frame_job(
            &frame_ctx,
            &frame_mode,
            &selected_object_ids,
            laser_on_override,
            feed_rate,
        )
    })
    .await
    .map_err(|error| {
        map_service_error(beambench_service::ServiceError::internal(format!(
            "Frame start task failed: {error}"
        )))
    })?
    .map_err(map_service_error)?;
    machine::spawn_job_tick_loop(ctx.clone());
    Ok(Json(serde_json::json!({
        "framing": true,
        "progress": progress,
    })))
}

async fn get_progress(
    State(ctx): State<Arc<ServiceContext>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Read-only: streaming is driven by the background tick loop. A GET must
    // not advance the job (a stalled poller would stall G-code delivery, and
    // a fast poller would race the tick loop).
    let progress: Option<JobProgress> = {
        let job_lock = ctx.job.lock().map_err(|e| {
            map_service_error(beambench_service::ServiceError::internal(format!(
                "lock: {e}"
            )))
        })?;
        job_lock.as_ref().map(|job| job.progress())
    };
    let progress = match progress {
        Some(progress) => Some(progress),
        None => ctx
            .last_job_progress
            .lock()
            .map_err(|e| {
                map_service_error(beambench_service::ServiceError::internal(format!(
                    "lock: {e}"
                )))
            })?
            .clone(),
    };
    match progress {
        Some(progress) => Ok(Json(serde_json::to_value(progress).unwrap_or_default())),
        None => Ok(Json(
            serde_json::to_value(JobProgress::default()).unwrap_or_default(),
        )),
    }
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
    async fn get_progress_no_job() {
        let ctx = Arc::new(ServiceContext::new());
        let app = crate::routes::build_router(ctx);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/v1/jobs/progress")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = collect_body(response).await;
        let progress: JobProgress = serde_json::from_slice(&body).unwrap();
        assert_eq!(progress, JobProgress::default());
    }

    #[tokio::test]
    async fn preflight_without_project_returns_error() {
        let ctx = Arc::new(ServiceContext::new());
        let app = crate::routes::build_router(ctx);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/jobs/preflight")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // No project loaded → should return error status
        assert_ne!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn start_job_without_project_returns_error() {
        let ctx = Arc::new(ServiceContext::new());
        let app = crate::routes::build_router(ctx);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/jobs/start")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        r#"{"confirm_motion":true,"confirm_laser_on":true}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_ne!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn start_job_requires_motion_confirmation() {
        let ctx = Arc::new(ServiceContext::new());
        let app = crate::routes::build_router(ctx);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/jobs/start")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::PRECONDITION_REQUIRED);
        let body = collect_body(response).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error_code"], "CONFIRMATION_REQUIRED");
        assert_eq!(json["missing"][0], "confirm_motion");
    }

    #[tokio::test]
    async fn preflight_with_project_but_no_session_returns_error() {
        use beambench_common::{Bounds, Point2D};
        use beambench_core::Project;
        use beambench_core::object::{ObjectData, ShapeKind};

        let ctx = Arc::new(ServiceContext::new());
        let mut project = Project::new("Preflight Test");
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
                    .uri("/api/v1/jobs/preflight")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // With a project loaded but no machine session, preflight returns an error
        // (PRECONDITION_FAILED for "Not connected")
        assert_ne!(response.status(), StatusCode::OK);
        let body = collect_body(response).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        // The error response should contain an error object
        assert!(json.get("error").is_some());
    }

    #[tokio::test]
    async fn frame_job_uses_selected_object_ids_from_request_body() {
        use beambench_common::{Bounds, Point2D};
        use beambench_core::Project;
        use beambench_core::object::{ObjectData, ShapeKind};

        let ctx = Arc::new(ServiceContext::new());
        let mut project = Project::new("Frame Test");
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
                    .uri("/api/v1/jobs/frame")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        r#"{"frame_mode":"rubber_band","selected_object_ids":["missing-object"],"confirm_motion":true}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::PRECONDITION_FAILED);

        let body = collect_body(response).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"]["message"], "No objects to frame");
    }

    #[test]
    fn frame_job_body_deserializes_laser_on_override() {
        let body: FrameJobBody = serde_json::from_str(
            r#"{"frame_mode":"rectangular","selected_object_ids":["obj-1"],"laser_on_override":true}"#,
        )
        .unwrap();
        assert_eq!(body.frame_mode.as_deref(), Some("rectangular"));
        assert_eq!(
            body.selected_object_ids.as_deref(),
            Some(&["obj-1".to_string()][..])
        );
        assert_eq!(body.laser_on_override, Some(true));
    }
}

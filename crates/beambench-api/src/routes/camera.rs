use std::sync::Arc;

use axum::extract::{Query, State};
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use beambench_common::{
    AlignmentPointSet, CalibrationPointSet, CameraAlignment, CameraCalibration,
    CameraOverlayRenderView,
};
use beambench_service::ServiceContext;
use beambench_service::ops::camera;
use serde::Deserialize;

use crate::response::{ApiError, map_service_error};

pub fn router() -> Router<Arc<ServiceContext>> {
    Router::new()
        .route("/devices", get(list_devices))
        .route("/select", post(select_camera))
        .route("/overlay", get(get_overlay_state))
        .route("/state", get(get_camera_state))
        .route("/capture", post(capture_frame))
        .route("/overlay/display", patch(update_overlay_display))
        .route("/overlay/fit-to-bed", post(fit_overlay_to_bed))
        .route("/overlay/transform", post(update_overlay_transform))
        .route("/overlay/discard", post(discard_overlay_draft))
        .route("/overlay/save-alignment", post(save_overlay_alignment))
        .route("/overlay/render", post(render_overlay))
        .route("/calibration", get(get_calibration))
        .route("/calibration/solve", post(solve_calibration))
        .route("/calibration/save", post(save_calibration))
        .route("/calibration/reset", post(reset_calibration))
        .route("/alignment", get(get_alignment))
        .route("/alignment/solve", post(solve_alignment))
        .route("/alignment/save", post(save_alignment))
        .route("/alignment/reset", post(reset_alignment))
}

#[derive(Debug, Deserialize)]
struct CameraQuery {
    #[serde(default)]
    camera_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SelectCameraBody {
    #[serde(default)]
    camera_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CaptureBody {
    #[serde(default)]
    camera_id: Option<String>,
    #[serde(default)]
    output_path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OverlayDisplayBody {
    #[serde(default)]
    overlay_visible: Option<bool>,
    #[serde(default)]
    overlay_opacity: Option<f64>,
    #[serde(default)]
    overlay_adjust_mode: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct OverlayTransformPatchBody {
    #[serde(default)]
    x: Option<f64>,
    #[serde(default)]
    y: Option<f64>,
    #[serde(default)]
    scale: Option<f64>,
    #[serde(default)]
    rotation_deg: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct OverlayNudgeBody {
    dx: f64,
    dy: f64,
}

#[derive(Debug, Deserialize)]
struct OverlayTransformBody {
    #[serde(default)]
    set: Option<OverlayTransformPatchBody>,
    #[serde(default)]
    nudge: Option<OverlayNudgeBody>,
    #[serde(default)]
    scale_factor: Option<f64>,
    #[serde(default)]
    rotate_deg: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct OverlayRenderBody {
    output_path: String,
    #[serde(default)]
    view: CameraOverlayRenderView,
    #[serde(default)]
    keep: bool,
}

#[derive(Debug, Deserialize)]
struct SolveCalibrationBody {
    camera_id: String,
    points: CalibrationPointSet,
}

#[derive(Debug, Deserialize)]
struct SaveCalibrationBody {
    camera_id: String,
    calibration: CameraCalibration,
}

#[derive(Debug, Deserialize)]
struct SolveAlignmentBody {
    #[serde(default)]
    camera_id: Option<String>,
    points: AlignmentPointSet,
}

#[derive(Debug, Deserialize)]
struct SaveAlignmentBody {
    #[serde(default)]
    camera_id: Option<String>,
    alignment: CameraAlignment,
}

async fn list_devices(
    State(ctx): State<Arc<ServiceContext>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let devices = camera::list_devices(&ctx).map_err(map_service_error)?;
    Ok(Json(serde_json::json!({
        "devices": devices,
        "count": devices.len(),
    })))
}

async fn select_camera(
    State(ctx): State<Arc<ServiceContext>>,
    Json(body): Json<SelectCameraBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let selected_camera_id =
        camera::select_camera(&ctx, body.camera_id).map_err(map_service_error)?;
    Ok(Json(serde_json::json!({
        "selected_camera_id": selected_camera_id,
    })))
}

async fn get_overlay_state(
    State(ctx): State<Arc<ServiceContext>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let overlay = camera::get_overlay_state(&ctx).map_err(map_service_error)?;
    Ok(Json(serde_json::to_value(overlay).unwrap_or_default()))
}

async fn get_camera_state(
    State(ctx): State<Arc<ServiceContext>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let state = camera::get_camera_state(&ctx).map_err(map_service_error)?;
    Ok(Json(serde_json::to_value(state).unwrap_or_default()))
}

async fn capture_frame(
    State(ctx): State<Arc<ServiceContext>>,
    Json(body): Json<CaptureBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let state = camera::capture_camera_agent(
        ctx,
        camera::CameraCaptureAgentInput {
            camera_id: body.camera_id,
            output_path: body.output_path,
        },
    )
    .await
    .map_err(map_service_error)?;
    Ok(Json(serde_json::to_value(state).unwrap_or_default()))
}

async fn update_overlay_display(
    State(ctx): State<Arc<ServiceContext>>,
    Json(body): Json<OverlayDisplayBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let state = camera::update_overlay_display(
        &ctx,
        camera::CameraOverlayDisplayUpdate {
            overlay_visible: body.overlay_visible,
            overlay_opacity: body.overlay_opacity,
            overlay_adjust_mode: body.overlay_adjust_mode,
        },
    )
    .map_err(map_service_error)?;
    Ok(Json(serde_json::to_value(state).unwrap_or_default()))
}

async fn fit_overlay_to_bed(
    State(ctx): State<Arc<ServiceContext>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let state = camera::fit_overlay_to_bed(&ctx).map_err(map_service_error)?;
    Ok(Json(serde_json::to_value(state).unwrap_or_default()))
}

async fn update_overlay_transform(
    State(ctx): State<Arc<ServiceContext>>,
    Json(body): Json<OverlayTransformBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let state = camera::update_overlay_transform(
        &ctx,
        camera::CameraOverlayTransformCommand {
            set: body.set.map(|set| camera::CameraOverlayTransformPatch {
                x: set.x,
                y: set.y,
                scale: set.scale,
                rotation_deg: set.rotation_deg,
            }),
            nudge: body.nudge.map(|nudge| camera::CameraOverlayNudge {
                dx: nudge.dx,
                dy: nudge.dy,
            }),
            scale_factor: body.scale_factor,
            rotate_deg: body.rotate_deg,
        },
    )
    .map_err(map_service_error)?;
    Ok(Json(serde_json::to_value(state).unwrap_or_default()))
}

async fn discard_overlay_draft(
    State(ctx): State<Arc<ServiceContext>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let state = camera::discard_overlay_draft(&ctx).map_err(map_service_error)?;
    Ok(Json(serde_json::to_value(state).unwrap_or_default()))
}

async fn save_overlay_alignment(
    State(ctx): State<Arc<ServiceContext>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let state = camera::save_overlay_draft_alignment(&ctx).map_err(map_service_error)?;
    Ok(Json(serde_json::to_value(state).unwrap_or_default()))
}

async fn render_overlay(
    State(ctx): State<Arc<ServiceContext>>,
    Json(body): Json<OverlayRenderBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let result = camera::request_overlay_render(
        ctx,
        camera::CameraOverlayRenderRequest {
            output_path: body.output_path,
            view: body.view,
            keep: body.keep,
        },
    )
    .await
    .map_err(map_service_error)?;
    Ok(Json(serde_json::to_value(result).unwrap_or_default()))
}

async fn get_calibration(
    State(ctx): State<Arc<ServiceContext>>,
    Query(query): Query<CameraQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let calibration =
        camera::get_camera_calibration(&ctx, query.camera_id).map_err(map_service_error)?;
    Ok(Json(serde_json::to_value(calibration).unwrap_or_default()))
}

async fn solve_calibration(
    State(ctx): State<Arc<ServiceContext>>,
    Json(body): Json<SolveCalibrationBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let result = camera::solve_camera_calibration(
        &ctx,
        camera::CameraCalibrationInput {
            camera_id: body.camera_id,
            points: body.points,
        },
    )
    .map_err(map_service_error)?;
    Ok(Json(serde_json::to_value(result).unwrap_or_default()))
}

async fn save_calibration(
    State(ctx): State<Arc<ServiceContext>>,
    Json(body): Json<SaveCalibrationBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let calibration = camera::save_camera_calibration(
        &ctx,
        camera::SaveCameraCalibrationInput {
            camera_id: body.camera_id,
            calibration: body.calibration,
        },
    )
    .map_err(map_service_error)?;
    Ok(Json(serde_json::to_value(calibration).unwrap_or_default()))
}

async fn reset_calibration(
    State(ctx): State<Arc<ServiceContext>>,
    Json(body): Json<CameraQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    camera::reset_camera_calibration(&ctx, body.camera_id).map_err(map_service_error)?;
    Ok(Json(serde_json::json!({ "reset": true })))
}

async fn get_alignment(
    State(ctx): State<Arc<ServiceContext>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let alignment = camera::get_camera_alignment(&ctx).map_err(map_service_error)?;
    Ok(Json(serde_json::to_value(alignment).unwrap_or_default()))
}

async fn solve_alignment(
    State(ctx): State<Arc<ServiceContext>>,
    Json(body): Json<SolveAlignmentBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let alignment = camera::solve_camera_alignment(
        &ctx,
        camera::CameraAlignmentInput {
            camera_id: body.camera_id,
            points: body.points,
        },
    )
    .map_err(map_service_error)?;
    Ok(Json(serde_json::to_value(alignment).unwrap_or_default()))
}

async fn save_alignment(
    State(ctx): State<Arc<ServiceContext>>,
    Json(body): Json<SaveAlignmentBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let alignment = camera::save_camera_alignment(&ctx, body.camera_id, body.alignment)
        .map_err(map_service_error)?;
    Ok(Json(serde_json::to_value(alignment).unwrap_or_default()))
}

async fn reset_alignment(
    State(ctx): State<Arc<ServiceContext>>,
    Json(body): Json<CameraQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    camera::reset_camera_alignment(&ctx, body.camera_id).map_err(map_service_error)?;
    Ok(Json(serde_json::json!({ "reset": true })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;
    use beambench_common::CameraDeviceInfo;
    use beambench_core::{AppSettings, MachineProfile};
    use tower::ServiceExt;

    async fn collect_body(response: axum::http::Response<axum::body::Body>) -> Vec<u8> {
        use http_body_util::BodyExt;
        let body = response.into_body();
        let collected = body.collect().await.unwrap();
        collected.to_bytes().to_vec()
    }

    fn ctx_with_profile() -> Arc<ServiceContext> {
        let profile = MachineProfile {
            name: "Camera".to_string(),
            ..Default::default()
        };
        let mut settings = AppSettings::default();
        settings.active_profile_id = Some(profile.id);
        settings.machine_profiles.push(profile);
        let ctx = Arc::new(ServiceContext::with_settings(settings));
        *ctx.camera_devices_override.lock().unwrap() = Some(vec![CameraDeviceInfo {
            camera_id: "cam-a".to_string(),
            display_name: "Camera A".to_string(),
            backend_kind: beambench_common::CameraBackendKind::MockSnapshot,
            available: true,
            width_px: 640,
            height_px: 480,
            status_text: "Ready".to_string(),
        }]);
        ctx
    }

    #[tokio::test]
    async fn list_devices_returns_camera_catalog() {
        let app = crate::routes::build_router(ctx_with_profile());
        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/v1/camera/devices")
                    .body(axum::body::Body::empty())
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
    async fn capture_returns_agent_camera_state_with_artifact_path() {
        let app = crate::routes::build_router(ctx_with_profile());
        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/camera/capture")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(r#"{"camera_id":"cam-a"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = collect_body(response).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["schema_version"], 1);
        assert_eq!(json["selected_camera_id"], "cam-a");
        assert_eq!(json["latest_capture"]["path"], json["frame"]["file_path"]);
        assert_ne!(json["frame"]["handle_id"], json["frame"]["file_path"]);
        assert_eq!(json["display"]["status"], "preview_fitted_to_bed");
    }

    #[tokio::test]
    async fn solve_and_save_calibration_roundtrip() {
        let app = crate::routes::build_router(ctx_with_profile());
        let solve_response = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/camera/calibration/solve")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        r#"{"camera_id":"cam-a","points":{"image_width_px":100,"image_height_px":100,"points":[{"image_x":0.0,"image_y":0.0,"reference_x":10.0,"reference_y":20.0},{"image_x":10.0,"image_y":0.0,"reference_x":20.0,"reference_y":20.0},{"image_x":0.0,"image_y":10.0,"reference_x":10.0,"reference_y":30.0}]}}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(solve_response.status(), StatusCode::OK);
        let solved_body = collect_body(solve_response).await;
        let solved: serde_json::Value = serde_json::from_slice(&solved_body).unwrap();

        let save_response = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/camera/calibration/save")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        serde_json::json!({
                            "camera_id": "cam-a",
                            "calibration": solved["calibration"].clone(),
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(save_response.status(), StatusCode::OK);

        let get_response = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/v1/camera/calibration?camera_id=cam-a")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(get_response.status(), StatusCode::OK);
        let body = collect_body(get_response).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["transform"].is_object());
    }
}

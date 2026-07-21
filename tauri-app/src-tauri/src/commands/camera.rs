use std::sync::Arc;

use beambench_common::{
    AlignmentPointSet, CalibrationPointSet, CalibrationSolveResult, CameraAgentState,
    CameraAlignment, CameraCalibration, CameraDeviceInfo, CameraFrameHandle, CameraOverlayState,
    SimilarityTransform,
};
use beambench_service::ServiceContext;
use beambench_service::ops::camera;
use tauri::{State, ipc::InvokeBody};

#[tauri::command]
pub fn list_camera_devices(
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<Vec<CameraDeviceInfo>, String> {
    camera::list_devices(&svc).map_err(Into::into)
}

#[tauri::command]
pub fn select_camera_device(
    camera_id: Option<String>,
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<Option<String>, String> {
    camera::select_camera(&svc, camera_id).map_err(Into::into)
}

#[tauri::command]
pub fn get_camera_calibration(
    camera_id: Option<String>,
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<Option<CameraCalibration>, String> {
    camera::get_camera_calibration(&svc, camera_id).map_err(Into::into)
}

#[tauri::command]
pub fn solve_camera_calibration(
    camera_id: String,
    points: CalibrationPointSet,
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<CalibrationSolveResult, String> {
    camera::solve_camera_calibration(&svc, camera::CameraCalibrationInput { camera_id, points })
        .map_err(Into::into)
}

#[tauri::command]
pub fn save_camera_calibration(
    camera_id: String,
    calibration: CameraCalibration,
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<CameraCalibration, String> {
    camera::save_camera_calibration(
        &svc,
        camera::SaveCameraCalibrationInput {
            camera_id,
            calibration,
        },
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn reset_camera_calibration(
    camera_id: Option<String>,
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<(), String> {
    camera::reset_camera_calibration(&svc, camera_id).map_err(Into::into)
}

#[tauri::command]
pub fn capture_camera_frame(
    camera_id: Option<String>,
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<CameraFrameHandle, String> {
    camera::capture_camera_frame(&svc, camera_id).map_err(String::from)
}

#[tauri::command]
pub fn register_camera_agent_bridge(svc: State<'_, Arc<ServiceContext>>) -> Result<(), String> {
    camera::set_camera_agent_bridge_connected(&svc, true);
    Ok(())
}

#[tauri::command]
pub fn unregister_camera_agent_bridge(svc: State<'_, Arc<ServiceContext>>) -> Result<(), String> {
    camera::set_camera_agent_bridge_connected(&svc, false);
    Ok(())
}

fn request_header(request: &tauri::ipc::Request<'_>, name: &str) -> Result<String, String> {
    request
        .headers()
        .get(name)
        .ok_or_else(|| format!("Missing camera frame header '{name}'"))?
        .to_str()
        .map(str::to_string)
        .map_err(|_| format!("Invalid camera frame header '{name}'"))
}

fn request_header_u32(request: &tauri::ipc::Request<'_>, name: &str) -> Result<u32, String> {
    request_header(request, name)?
        .parse()
        .map_err(|_| format!("Invalid camera frame header '{name}'"))
}

#[tauri::command]
pub fn save_camera_frame_bytes(
    request: tauri::ipc::Request<'_>,
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<CameraFrameHandle, String> {
    let camera_id = request_header(&request, "camera-id")?;
    let media_type = request_header(&request, "media-type")?;
    let width_px = request_header_u32(&request, "width-px")?;
    let height_px = request_header_u32(&request, "height-px")?;
    let image_data = match request.body() {
        InvokeBody::Raw(data) => data.to_vec(),
        InvokeBody::Json(serde_json::Value::Array(data)) => {
            let mut bytes = Vec::with_capacity(data.len());
            for value in data {
                let Some(byte) = value.as_u64().filter(|byte| *byte <= u8::MAX as u64) else {
                    return Err("Expected camera frame bytes".to_string());
                };
                bytes.push(byte as u8);
            }
            bytes
        }
        _ => return Err("Expected raw camera frame bytes".to_string()),
    };
    camera::save_camera_frame_bytes(
        &svc,
        camera::SaveCameraFrameBytesInput {
            camera_id,
            image_data,
            width_px,
            height_px,
            media_type,
        },
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn get_camera_alignment(
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<Option<CameraAlignment>, String> {
    camera::get_camera_alignment(&svc).map_err(Into::into)
}

#[tauri::command]
pub fn solve_camera_alignment(
    camera_id: Option<String>,
    points: AlignmentPointSet,
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<CameraAlignment, String> {
    camera::solve_camera_alignment(&svc, camera::CameraAlignmentInput { camera_id, points })
        .map_err(Into::into)
}

#[tauri::command]
pub fn update_camera_alignment(
    alignment: CameraAlignment,
    camera_id: Option<String>,
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<CameraAlignment, String> {
    camera::save_camera_alignment(&svc, camera_id, alignment).map_err(Into::into)
}

#[tauri::command]
pub fn reset_camera_alignment(
    camera_id: Option<String>,
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<(), String> {
    camera::reset_camera_alignment(&svc, camera_id).map_err(Into::into)
}

#[tauri::command]
pub fn get_camera_overlay_state(
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<CameraOverlayState, String> {
    camera::get_overlay_state(&svc).map_err(Into::into)
}

#[tauri::command]
pub fn get_camera_agent_state(
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<CameraAgentState, String> {
    camera::get_camera_state(&svc).map_err(Into::into)
}

#[tauri::command]
pub fn update_camera_overlay_display(
    overlay_visible: Option<bool>,
    overlay_opacity: Option<f64>,
    overlay_adjust_mode: Option<bool>,
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<CameraAgentState, String> {
    camera::update_overlay_display(
        &svc,
        camera::CameraOverlayDisplayUpdate {
            overlay_visible,
            overlay_opacity,
            overlay_adjust_mode,
        },
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn fit_camera_overlay_to_bed(
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<CameraAgentState, String> {
    camera::fit_overlay_to_bed(&svc).map_err(Into::into)
}

#[tauri::command]
pub fn discard_camera_overlay_draft(
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<CameraAgentState, String> {
    camera::discard_overlay_draft(&svc).map_err(Into::into)
}

#[tauri::command]
pub fn save_camera_overlay_alignment(
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<CameraAgentState, String> {
    camera::save_overlay_draft_alignment(&svc).map_err(Into::into)
}

#[tauri::command]
pub fn commit_camera_overlay_transform(
    transform: SimilarityTransform,
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<CameraAgentState, String> {
    camera::update_overlay_transform(
        &svc,
        camera::CameraOverlayTransformCommand {
            set: Some(camera::CameraOverlayTransformPatch {
                x: Some(transform.translation_x),
                y: Some(transform.translation_y),
                scale: Some(transform.scale),
                rotation_deg: Some(transform.rotation_deg),
            }),
            nudge: None,
            scale_factor: None,
            rotate_deg: None,
        },
    )
    .map_err(Into::into)
}

#[tauri::command]
pub fn complete_camera_capture_request(
    request_id: String,
    frame: Option<CameraFrameHandle>,
    error: Option<String>,
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<(), String> {
    camera::complete_camera_capture_request(&svc, request_id, frame, error).map_err(Into::into)
}

#[tauri::command]
pub fn complete_camera_overlay_render_request(
    request_id: String,
    path: Option<String>,
    error: Option<String>,
    svc: State<'_, Arc<ServiceContext>>,
) -> Result<(), String> {
    camera::complete_camera_overlay_render_request(&svc, request_id, path, error)
        .map_err(Into::into)
}

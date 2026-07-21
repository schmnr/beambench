use beambench_camera as camera_backend;
use beambench_common::{
    AlignmentPointSet, CalibrationPointSet, CalibrationSolveResult, CameraAgentState,
    CameraAlignment, CameraAlignmentSource, CameraArtifactInfo, CameraCalibration,
    CameraDeviceInfo, CameraFrameHandle, CameraOverlayDisplayState, CameraOverlayRenderOptions,
    CameraOverlayRenderResult, CameraOverlayRenderView, CameraOverlayRuntimeState,
    CameraOverlayState, CameraOverlayStatus, SimilarityTransform,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::atomic::Ordering;
use std::time::Duration;
use uuid::Uuid;

use crate::context::{
    PendingCameraCaptureRequest, PendingCameraOverlayRenderRequest, ServiceContext,
};
use crate::error::{ServiceError, ServiceResult};
use crate::ops::profiles::{self, ProfileLookup};
use crate::persist::persist_settings_to_disk;

#[derive(Debug, Clone)]
pub struct CameraCalibrationInput {
    pub camera_id: String,
    pub points: CalibrationPointSet,
}

#[derive(Debug, Clone)]
pub struct SaveCameraCalibrationInput {
    pub camera_id: String,
    pub calibration: beambench_common::CameraCalibration,
}

#[derive(Debug, Clone)]
pub struct CameraAlignmentInput {
    pub camera_id: Option<String>,
    pub points: AlignmentPointSet,
}

#[derive(Debug, Clone)]
pub struct SaveCameraFrameBytesInput {
    pub camera_id: String,
    pub image_data: Vec<u8>,
    pub width_px: u32,
    pub height_px: u32,
    pub media_type: String,
}

pub const CAMERA_AGENT_SCHEMA_VERSION: u32 = 1;
const OVERLAY_MIN_SCALE: f64 = 0.01;
const OVERLAY_MAX_SCALE: f64 = 20.0;
const MAX_CAMERA_FRAME_BYTES: usize = 64 * 1024 * 1024;
const MAX_CAMERA_FRAME_DIMENSION: u32 = 16_384;
const MAX_CAMERA_FRAME_PIXELS: u64 = 40_000_000;
const MAX_CAMERA_DECODE_BYTES: u64 = 256 * 1024 * 1024;
const APP_ASSIST_TIMEOUT: Duration = Duration::from_secs(15);
#[cfg(test)]
static APP_ASSIST_TIMEOUT_OVERRIDE_MS: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

fn app_assist_timeout() -> Duration {
    #[cfg(test)]
    {
        let ms = APP_ASSIST_TIMEOUT_OVERRIDE_MS.load(Ordering::Relaxed);
        if ms > 0 {
            return Duration::from_millis(ms);
        }
    }
    APP_ASSIST_TIMEOUT
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CameraCaptureAgentInput {
    pub camera_id: Option<String>,
    pub output_path: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CameraOverlayDisplayUpdate {
    pub overlay_visible: Option<bool>,
    pub overlay_opacity: Option<f64>,
    pub overlay_adjust_mode: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CameraOverlayTransformPatch {
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub scale: Option<f64>,
    pub rotation_deg: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CameraOverlayTransformCommand {
    #[serde(default)]
    pub set: Option<CameraOverlayTransformPatch>,
    #[serde(default)]
    pub nudge: Option<CameraOverlayNudge>,
    #[serde(default)]
    pub scale_factor: Option<f64>,
    #[serde(default)]
    pub rotate_deg: Option<f64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CameraOverlayNudge {
    pub dx: f64,
    pub dy: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraOverlayRenderRequest {
    pub output_path: String,
    pub view: CameraOverlayRenderView,
    pub keep: bool,
}

fn lock_err(name: &str, e: impl std::fmt::Display) -> ServiceError {
    ServiceError::internal(format!("Failed to lock {name}: {e}"))
}

fn current_devices(ctx: &ServiceContext) -> ServiceResult<Vec<CameraDeviceInfo>> {
    let override_guard = ctx
        .camera_devices_override
        .lock()
        .map_err(|e| lock_err("camera_devices_override", e))?;
    Ok(override_guard
        .clone()
        .unwrap_or_else(camera_backend::list_cameras))
}

fn ensure_device(ctx: &ServiceContext, camera_id: &str) -> ServiceResult<CameraDeviceInfo> {
    current_devices(ctx)?
        .into_iter()
        .find(|camera| camera.camera_id == camera_id)
        .ok_or_else(|| ServiceError::not_found(format!("Camera '{camera_id}' not found")))
}

fn active_profile(ctx: &ServiceContext) -> ServiceResult<beambench_core::MachineProfile> {
    let active_profile_id = profiles::get_active_profile_id(ctx)?
        .ok_or_else(|| ServiceError::invalid_state("No active machine profile"))?;
    profiles::get_profile(ctx, ProfileLookup::Id(active_profile_id))
}

fn update_active_profile<F>(
    ctx: &ServiceContext,
    mutator: F,
) -> ServiceResult<beambench_core::MachineProfile>
where
    F: FnOnce(&mut beambench_core::MachineProfile) -> ServiceResult<()>,
{
    let mut profile = active_profile(ctx)?;
    mutator(&mut profile)?;

    {
        let mut settings = ctx.settings.lock().map_err(|e| lock_err("settings", e))?;
        let stored = settings
            .machine_profiles
            .iter_mut()
            .find(|candidate| candidate.id == profile.id)
            .ok_or_else(|| ServiceError::not_found("Active profile not found"))?;
        *stored = profile.clone();
    }
    persist_settings_to_disk(ctx);
    Ok(profile)
}

fn set_selected_camera_id(
    ctx: &ServiceContext,
    camera_id: String,
) -> ServiceResult<beambench_core::MachineProfile> {
    let _ = ensure_device(ctx, &camera_id)?;
    let previous_profile = active_profile(ctx)?;
    let selection_changed =
        previous_profile.selected_camera_id.as_deref() != Some(camera_id.as_str());
    let profile = update_active_profile(ctx, |profile| {
        profile.selected_camera_id = Some(camera_id.clone());
        if selection_changed {
            profile.camera_calibration = None;
            profile.camera_alignment = None;
        }
        Ok(())
    })?;
    if selection_changed {
        clear_frame_for_profile(ctx, profile.id)?;
        let mut runtime = ctx
            .camera_overlay_runtime
            .lock()
            .map_err(|e| lock_err("camera_overlay_runtime", e))?;
        runtime.remove(&profile.id);
    }
    Ok(profile)
}

fn current_profile_and_camera_id(
    ctx: &ServiceContext,
    requested_camera_id: Option<String>,
) -> ServiceResult<(beambench_core::MachineProfile, String)> {
    match requested_camera_id {
        Some(camera_id) => {
            let profile = set_selected_camera_id(ctx, camera_id.clone())?;
            Ok((profile, camera_id))
        }
        None => {
            let profile = active_profile(ctx)?;
            let camera_id = profile.selected_camera_id.clone().ok_or_else(|| {
                ServiceError::invalid_state("No camera selected on the active profile")
            })?;
            let _ = ensure_device(ctx, &camera_id)?;
            Ok((profile, camera_id))
        }
    }
}

fn current_frame_for_profile(
    ctx: &ServiceContext,
    profile_id: beambench_core::MachineProfileId,
) -> ServiceResult<Option<CameraFrameHandle>> {
    let frames = ctx
        .camera_frames
        .lock()
        .map_err(|e| lock_err("camera_frames", e))?;
    Ok(frames.get(&profile_id).cloned())
}

fn camera_dimensions_for_profile(
    ctx: &ServiceContext,
    profile: &beambench_core::MachineProfile,
    camera_id: &str,
) -> ServiceResult<Option<(u32, u32)>> {
    if let Some(frame) = current_frame_for_profile(ctx, profile.id)? {
        return Ok(Some((frame.width_px, frame.height_px)));
    }
    let device = ensure_device(ctx, camera_id)?;
    if device.width_px > 0 && device.height_px > 0 {
        Ok(Some((device.width_px, device.height_px)))
    } else {
        Ok(None)
    }
}

/// delete a frame's backing PNG file, swallowing any I/O error
/// (file already gone, never materialized, permission issue) — we don't
/// want cleanup-on-best-effort to surface errors to callers.
fn delete_frame_file(frame: &CameraFrameHandle) {
    if frame.file_path.is_empty() {
        return;
    }
    let _ = std::fs::remove_file(&frame.file_path);
}

fn clear_frame_for_profile(
    ctx: &ServiceContext,
    profile_id: beambench_core::MachineProfileId,
) -> ServiceResult<()> {
    let mut frames = ctx
        .camera_frames
        .lock()
        .map_err(|e| lock_err("camera_frames", e))?;
    if let Some(prev) = frames.remove(&profile_id) {
        delete_frame_file(&prev);
    }
    Ok(())
}

fn frame_format(media_type: &str) -> ServiceResult<(&'static str, image::ImageFormat)> {
    match media_type {
        "image/png" => Ok(("png", image::ImageFormat::Png)),
        "image/jpeg" => Ok(("jpg", image::ImageFormat::Jpeg)),
        other => Err(ServiceError::invalid_input(format!(
            "Unsupported camera frame media type '{other}'"
        ))),
    }
}

fn validate_camera_frame(
    input: &SaveCameraFrameBytesInput,
    expected_format: image::ImageFormat,
) -> ServiceResult<(u32, u32)> {
    if input.image_data.is_empty() {
        return Err(ServiceError::invalid_input("Camera frame is empty"));
    }
    if input.image_data.len() > MAX_CAMERA_FRAME_BYTES {
        return Err(ServiceError::invalid_input(format!(
            "Camera frame exceeds the {} MiB limit",
            MAX_CAMERA_FRAME_BYTES / (1024 * 1024)
        )));
    }

    let actual_format = image::guess_format(&input.image_data).map_err(|_| {
        ServiceError::invalid_input("Camera frame is not a valid PNG or JPEG image")
    })?;
    if actual_format != expected_format {
        return Err(ServiceError::invalid_input(
            "Camera frame bytes do not match the declared media type",
        ));
    }

    let (width_px, height_px) =
        image::ImageReader::with_format(std::io::Cursor::new(&input.image_data), actual_format)
            .into_dimensions()
            .map_err(|_| ServiceError::invalid_input("Camera frame could not be decoded"))?;
    let pixel_count = u64::from(width_px) * u64::from(height_px);
    if width_px == 0
        || height_px == 0
        || width_px > MAX_CAMERA_FRAME_DIMENSION
        || height_px > MAX_CAMERA_FRAME_DIMENSION
        || pixel_count > MAX_CAMERA_FRAME_PIXELS
    {
        return Err(ServiceError::invalid_input(
            "Camera frame dimensions exceed supported limits",
        ));
    }
    if width_px != input.width_px || height_px != input.height_px {
        return Err(ServiceError::invalid_input(format!(
            "Camera frame dimensions are {width_px}x{height_px}, not the declared {}x{}",
            input.width_px, input.height_px
        )));
    }

    let mut decoder =
        image::ImageReader::with_format(std::io::Cursor::new(&input.image_data), actual_format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_CAMERA_FRAME_DIMENSION);
    limits.max_image_height = Some(MAX_CAMERA_FRAME_DIMENSION);
    limits.max_alloc = Some(MAX_CAMERA_DECODE_BYTES);
    decoder.limits(limits);
    decoder
        .decode()
        .map_err(|_| ServiceError::invalid_input("Camera frame could not be decoded"))?;
    Ok((width_px, height_px))
}

fn validate_overlay_transform(transform: &SimilarityTransform, label: &str) -> ServiceResult<()> {
    if !transform.scale.is_finite()
        || !transform.rotation_deg.is_finite()
        || !transform.translation_x.is_finite()
        || !transform.translation_y.is_finite()
    {
        return Err(ServiceError::invalid_input(format!(
            "{label} transform must contain finite values"
        )));
    }
    if transform.scale <= 0.0 {
        return Err(ServiceError::invalid_input(format!(
            "{label} scale must be greater than zero"
        )));
    }
    Ok(())
}

fn validate_camera_calibration(calibration: &CameraCalibration) -> ServiceResult<()> {
    validate_overlay_transform(&calibration.transform, "Camera calibration")?;
    let pixel_count =
        u64::from(calibration.image_width_px) * u64::from(calibration.image_height_px);
    if calibration.image_width_px == 0
        || calibration.image_height_px == 0
        || calibration.image_width_px > MAX_CAMERA_FRAME_DIMENSION
        || calibration.image_height_px > MAX_CAMERA_FRAME_DIMENSION
        || pixel_count > MAX_CAMERA_FRAME_PIXELS
    {
        return Err(ServiceError::invalid_input(
            "Camera calibration dimensions exceed supported limits",
        ));
    }
    if !calibration.rmse_px.is_finite() || calibration.rmse_px < 0.0 {
        return Err(ServiceError::invalid_input(
            "Camera calibration RMSE must be a non-negative finite value",
        ));
    }
    if !calibration.quality_score.is_finite() || !(0.0..=1.0).contains(&calibration.quality_score) {
        return Err(ServiceError::invalid_input(
            "Camera calibration quality must be between 0 and 1",
        ));
    }
    Ok(())
}

fn validate_camera_alignment(alignment: &CameraAlignment) -> ServiceResult<()> {
    validate_overlay_transform(&alignment.transform, "Camera alignment")?;
    match (alignment.image_width_px, alignment.image_height_px) {
        (Some(width_px), Some(height_px))
            if width_px > 0
                && height_px > 0
                && width_px <= MAX_CAMERA_FRAME_DIMENSION
                && height_px <= MAX_CAMERA_FRAME_DIMENSION
                && u64::from(width_px) * u64::from(height_px) <= MAX_CAMERA_FRAME_PIXELS => {}
        (None, None) => {}
        _ => {
            return Err(ServiceError::invalid_input(
                "Camera alignment frame dimensions are incomplete or unsupported",
            ));
        }
    }
    if !alignment.rmse_mm.is_finite() || alignment.rmse_mm < 0.0 {
        return Err(ServiceError::invalid_input(
            "Camera alignment RMSE must be a non-negative finite value",
        ));
    }
    if !alignment.quality_score.is_finite() || !(0.0..=1.0).contains(&alignment.quality_score) {
        return Err(ServiceError::invalid_input(
            "Camera alignment quality must be between 0 and 1",
        ));
    }
    Ok(())
}

pub fn camera_frame_dir() -> std::path::PathBuf {
    std::env::temp_dir().join("beam-bench-camera")
}

fn is_camera_frame_path(path: &std::path::Path) -> bool {
    path.is_file()
        && path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| matches!(ext.to_ascii_lowercase().as_str(), "png" | "jpg" | "jpeg"))
            .unwrap_or(false)
}

pub fn cleanup_stale_frame_files() -> usize {
    let dir = camera_frame_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return 0;
    };

    let mut deleted = 0usize;
    for entry in entries.flatten() {
        let path = entry.path();
        if !is_camera_frame_path(&path) {
            continue;
        }
        match std::fs::remove_file(&path) {
            Ok(()) => deleted += 1,
            Err(error) => {
                tracing::warn!(path = %path.display(), error = %error, "Failed to delete stale camera frame");
            }
        }
    }
    deleted
}

fn frame_dir() -> std::path::PathBuf {
    camera_frame_dir()
}

fn store_frame_for_profile(
    ctx: &ServiceContext,
    profile_id: beambench_core::MachineProfileId,
    frame: CameraFrameHandle,
) -> ServiceResult<CameraFrameHandle> {
    let mut frames = ctx
        .camera_frames
        .lock()
        .map_err(|e| lock_err("camera_frames", e))?;
    // delete the previous frame's PNG file before swapping so
    // temp files don't accumulate across captures.
    if let Some(prev) = frames.insert(profile_id, frame.clone()) {
        delete_frame_file(&prev);
    }
    Ok(frame)
}

fn active_workspace_dims(
    ctx: &ServiceContext,
    profile: &beambench_core::MachineProfile,
) -> (f64, f64) {
    if let Ok(project) = ctx.project.lock()
        && let Some(project) = project.as_ref()
    {
        return (
            project.workspace.bed_width_mm.max(0.0),
            project.workspace.bed_height_mm.max(0.0),
        );
    }
    (
        profile.bed_width_mm.max(0.0),
        profile.bed_height_mm.max(0.0),
    )
}

pub fn fit_camera_overlay_to_bed(
    width_px: u32,
    height_px: u32,
    bed_width_mm: f64,
    bed_height_mm: f64,
) -> SimilarityTransform {
    if width_px == 0 || height_px == 0 || bed_width_mm <= 0.0 || bed_height_mm <= 0.0 {
        return SimilarityTransform::default();
    }
    let width = width_px as f64;
    let height = height_px as f64;
    let scale = (bed_width_mm / width).min(bed_height_mm / height);
    SimilarityTransform {
        scale,
        rotation_deg: 0.0,
        translation_x: (bed_width_mm - width * scale) / 2.0,
        translation_y: (bed_height_mm - height * scale) / 2.0,
    }
}

fn transform_center(width_px: u32, height_px: u32, transform: &SimilarityTransform) -> (f64, f64) {
    map_camera_pixel_to_workspace(width_px as f64 / 2.0, height_px as f64 / 2.0, transform)
}

fn map_camera_pixel_to_workspace(x: f64, y: f64, transform: &SimilarityTransform) -> (f64, f64) {
    let rotation_rad = transform.rotation_deg.to_radians();
    let cos = rotation_rad.cos();
    let sin = rotation_rad.sin();
    (
        transform.scale * (cos * x - sin * y) + transform.translation_x,
        transform.scale * (sin * x + cos * y) + transform.translation_y,
    )
}

fn transform_keeping_center(
    width_px: u32,
    height_px: u32,
    start: &SimilarityTransform,
    scale: f64,
    rotation_deg: f64,
) -> SimilarityTransform {
    let (center_x, center_y) = transform_center(width_px, height_px, start);
    let center_px_x = width_px as f64 / 2.0;
    let center_px_y = height_px as f64 / 2.0;
    let rotation_rad = rotation_deg.to_radians();
    let cos = rotation_rad.cos();
    let sin = rotation_rad.sin();
    let rotated_x = scale * (cos * center_px_x - sin * center_px_y);
    let rotated_y = scale * (sin * center_px_x + cos * center_px_y);
    SimilarityTransform {
        scale,
        rotation_deg,
        translation_x: center_x - rotated_x,
        translation_y: center_y - rotated_y,
    }
}

fn runtime_for_profile(
    ctx: &ServiceContext,
    profile_id: beambench_core::MachineProfileId,
) -> ServiceResult<CameraOverlayRuntimeState> {
    let runtime = ctx
        .camera_overlay_runtime
        .lock()
        .map_err(|e| lock_err("camera_overlay_runtime", e))?;
    Ok(runtime.get(&profile_id).cloned().unwrap_or_default())
}

fn set_runtime_for_profile(
    ctx: &ServiceContext,
    profile_id: beambench_core::MachineProfileId,
    runtime: CameraOverlayRuntimeState,
) -> ServiceResult<CameraOverlayRuntimeState> {
    {
        let mut guard = ctx
            .camera_overlay_runtime
            .lock()
            .map_err(|e| lock_err("camera_overlay_runtime", e))?;
        guard.insert(profile_id, runtime.clone());
    }
    emit_overlay_runtime_updated(ctx)?;
    Ok(runtime)
}

fn mutate_runtime_for_profile<F>(
    ctx: &ServiceContext,
    profile_id: beambench_core::MachineProfileId,
    mutator: F,
) -> ServiceResult<CameraOverlayRuntimeState>
where
    F: FnOnce(&mut CameraOverlayRuntimeState),
{
    let mut runtime = runtime_for_profile(ctx, profile_id)?;
    mutator(&mut runtime);
    set_runtime_for_profile(ctx, profile_id, runtime)
}

fn effective_transform(
    overlay: &CameraOverlayState,
    runtime: &CameraOverlayRuntimeState,
) -> Option<SimilarityTransform> {
    if runtime.draft_dirty || runtime.overlay_adjust_mode {
        if let Some(transform) = runtime.draft_overlay_transform.clone() {
            return Some(transform);
        }
    }
    overlay
        .alignment
        .as_ref()
        .map(|alignment| alignment.transform.clone())
        .or_else(|| {
            overlay
                .calibration
                .as_ref()
                .map(|calibration| calibration.transform.clone())
        })
        .or_else(|| runtime.draft_overlay_transform.clone())
}

fn saved_profile_transform(
    profile: &beambench_core::MachineProfile,
) -> Option<SimilarityTransform> {
    profile
        .camera_alignment
        .as_ref()
        .map(|alignment| alignment.transform.clone())
        .or_else(|| {
            profile
                .camera_calibration
                .as_ref()
                .map(|calibration| calibration.transform.clone())
        })
}

fn display_status(
    overlay: &CameraOverlayState,
    runtime: &CameraOverlayRuntimeState,
) -> CameraOverlayStatus {
    if overlay.frame.is_none() {
        CameraOverlayStatus::NoFrame
    } else if runtime.draft_dirty {
        CameraOverlayStatus::UnsavedChanges
    } else if runtime.overlay_adjust_mode {
        CameraOverlayStatus::AdjustingOverlay
    } else if overlay.alignment.is_some() || overlay.calibration.is_some() {
        CameraOverlayStatus::SavedAlignment
    } else if runtime.draft_overlay_transform.is_some() {
        CameraOverlayStatus::PreviewFittedToBed
    } else {
        CameraOverlayStatus::NoAlignment
    }
}

fn display_state(
    overlay: &CameraOverlayState,
    runtime: &CameraOverlayRuntimeState,
) -> CameraOverlayDisplayState {
    let effective = effective_transform(overlay, runtime);
    CameraOverlayDisplayState {
        overlay_visible: runtime.overlay_visible,
        overlay_opacity: runtime.overlay_opacity,
        draft_overlay_transform: runtime.draft_overlay_transform.clone(),
        draft_base_transform: runtime.draft_base_transform.clone(),
        draft_dirty: runtime.draft_dirty,
        overlay_adjust_mode: runtime.overlay_adjust_mode,
        status: display_status(overlay, runtime),
        effective_transform: effective,
    }
}

fn emit_overlay_runtime_updated(ctx: &ServiceContext) -> ServiceResult<()> {
    let state = get_camera_state(ctx)?;
    ctx.emit_event(
        "camera.overlay.runtime.updated",
        serde_json::to_value(state).unwrap_or_default(),
    );
    Ok(())
}

fn apply_capture_overlay_defaults(
    ctx: &ServiceContext,
    profile: &beambench_core::MachineProfile,
    frame: &CameraFrameHandle,
) -> ServiceResult<()> {
    let calibration_is_stale = profile
        .camera_calibration
        .as_ref()
        .is_some_and(|calibration| {
            calibration.image_width_px != frame.width_px
                || calibration.image_height_px != frame.height_px
        });
    let alignment_is_stale = profile.camera_alignment.as_ref().is_some_and(|alignment| {
        match (alignment.image_width_px, alignment.image_height_px) {
            (Some(width_px), Some(height_px)) => {
                width_px != frame.width_px || height_px != frame.height_px
            }
            (None, None) => false,
            _ => true,
        }
    });
    let alignment_needs_binding = profile.camera_alignment.as_ref().is_some_and(|alignment| {
        alignment.image_width_px.is_none() && alignment.image_height_px.is_none()
    });
    let profile = if calibration_is_stale {
        let updated = update_active_profile(ctx, |profile| {
            profile.camera_calibration = None;
            profile.camera_alignment = None;
            Ok(())
        })?;
        ctx.emit_event(
            "camera.calibration.invalidated",
            json!({
                "profile_id": updated.id,
                "camera_id": updated.selected_camera_id.clone(),
                "reason": "frame_dimensions_changed",
                "frame_width_px": frame.width_px,
                "frame_height_px": frame.height_px,
            }),
        );
        updated
    } else if alignment_is_stale {
        let updated = update_active_profile(ctx, |profile| {
            profile.camera_alignment = None;
            Ok(())
        })?;
        ctx.emit_event(
            "camera.alignment.invalidated",
            json!({
                "profile_id": updated.id,
                "camera_id": updated.selected_camera_id.clone(),
                "reason": "frame_dimensions_changed",
                "frame_width_px": frame.width_px,
                "frame_height_px": frame.height_px,
            }),
        );
        updated
    } else if alignment_needs_binding {
        update_active_profile(ctx, |profile| {
            if let Some(alignment) = profile.camera_alignment.as_mut() {
                alignment.image_width_px = Some(frame.width_px);
                alignment.image_height_px = Some(frame.height_px);
            }
            Ok(())
        })?
    } else {
        profile.clone()
    };
    let mut runtime = runtime_for_profile(ctx, profile.id)?;
    runtime.overlay_visible = true;
    runtime.overlay_adjust_mode = false;
    runtime.draft_dirty = false;
    if profile.camera_alignment.is_some() || profile.camera_calibration.is_some() {
        runtime.draft_overlay_transform = None;
        runtime.draft_base_transform = None;
    } else {
        let (bed_width, bed_height) = active_workspace_dims(ctx, &profile);
        let fitted =
            fit_camera_overlay_to_bed(frame.width_px, frame.height_px, bed_width, bed_height);
        runtime.draft_overlay_transform = Some(fitted.clone());
        runtime.draft_base_transform = Some(fitted);
    }
    set_runtime_for_profile(ctx, profile.id, runtime)?;
    Ok(())
}

fn remember_capture_artifact(
    ctx: &ServiceContext,
    artifact: CameraArtifactInfo,
) -> ServiceResult<()> {
    let mut guard = ctx
        .camera_latest_capture_artifact
        .lock()
        .map_err(|e| lock_err("camera_latest_capture_artifact", e))?;
    *guard = Some(artifact);
    Ok(())
}

fn remember_render_artifact(
    ctx: &ServiceContext,
    artifact: CameraArtifactInfo,
) -> ServiceResult<()> {
    let mut guard = ctx
        .camera_latest_render_artifact
        .lock()
        .map_err(|e| lock_err("camera_latest_render_artifact", e))?;
    *guard = Some(artifact);
    Ok(())
}

pub fn list_devices(ctx: &ServiceContext) -> ServiceResult<Vec<CameraDeviceInfo>> {
    let devices = current_devices(ctx)?;
    ctx.emit_event(
        "camera.devices.updated",
        json!({
            "count": devices.len(),
            "devices": devices,
        }),
    );
    Ok(devices)
}

pub fn select_camera(
    ctx: &ServiceContext,
    camera_id: Option<String>,
) -> ServiceResult<Option<String>> {
    let selected = match camera_id {
        Some(camera_id) => {
            let profile = set_selected_camera_id(ctx, camera_id.clone())?;
            ctx.emit_event(
                "camera.devices.updated",
                json!({
                    "selected_camera_id": profile.selected_camera_id,
                }),
            );
            profile.selected_camera_id
        }
        None => {
            let previous_profile = active_profile(ctx)?;
            let selection_changed = previous_profile.selected_camera_id.is_some();
            let profile = update_active_profile(ctx, |profile| {
                profile.selected_camera_id = None;
                profile.camera_calibration = None;
                profile.camera_alignment = None;
                Ok(())
            })?;
            if selection_changed {
                clear_frame_for_profile(ctx, profile.id)?;
                let mut runtime = ctx
                    .camera_overlay_runtime
                    .lock()
                    .map_err(|e| lock_err("camera_overlay_runtime", e))?;
                runtime.remove(&profile.id);
            }
            ctx.emit_event(
                "camera.devices.updated",
                json!({
                    "selected_camera_id": profile.selected_camera_id,
                }),
            );
            profile.selected_camera_id
        }
    };
    Ok(selected)
}

pub fn get_camera_calibration(
    ctx: &ServiceContext,
    camera_id: Option<String>,
) -> ServiceResult<Option<beambench_common::CameraCalibration>> {
    let profile = match active_profile(ctx) {
        Ok(p) => p,
        Err(_) => return Ok(None),
    };
    let Some(selected_camera_id) = profile.selected_camera_id.clone() else {
        return Ok(None);
    };
    if let Some(camera_id) = camera_id
        && camera_id != selected_camera_id
    {
        return Ok(None);
    }
    Ok(profile.camera_calibration)
}

pub fn solve_camera_calibration(
    ctx: &ServiceContext,
    input: CameraCalibrationInput,
) -> ServiceResult<CalibrationSolveResult> {
    let _ = ensure_device(ctx, &input.camera_id)?;
    let result =
        camera_backend::solve_calibration(&input.points).map_err(ServiceError::invalid_input)?;
    validate_camera_calibration(&result.calibration)?;
    let profile = set_selected_camera_id(ctx, input.camera_id.clone())?;
    ctx.emit_event(
        "camera.calibration.solved",
        json!({
            "camera_id": input.camera_id,
            "profile_id": profile.id,
            "quality_score": result.calibration.quality_score,
            "point_count": result.point_count,
        }),
    );
    Ok(result)
}

pub fn save_camera_calibration(
    ctx: &ServiceContext,
    input: SaveCameraCalibrationInput,
) -> ServiceResult<beambench_common::CameraCalibration> {
    let _ = ensure_device(ctx, &input.camera_id)?;
    validate_camera_calibration(&input.calibration)?;
    let previous_profile = active_profile(ctx)?;
    let selection_changed =
        previous_profile.selected_camera_id.as_deref() != Some(input.camera_id.as_str());
    if !selection_changed
        && let Some(frame) = current_frame_for_profile(ctx, previous_profile.id)?
        && (frame.width_px != input.calibration.image_width_px
            || frame.height_px != input.calibration.image_height_px)
    {
        return Err(ServiceError::invalid_input(format!(
            "Camera calibration dimensions {}x{} do not match the current frame {}x{}",
            input.calibration.image_width_px,
            input.calibration.image_height_px,
            frame.width_px,
            frame.height_px
        )));
    }
    let profile = update_active_profile(ctx, |profile| {
        profile.selected_camera_id = Some(input.camera_id.clone());
        if selection_changed {
            profile.camera_alignment = None;
        }
        profile.camera_calibration = Some(input.calibration.clone());
        Ok(())
    })?;
    if selection_changed {
        clear_frame_for_profile(ctx, profile.id)?;
    }
    mutate_runtime_for_profile(ctx, profile.id, |runtime| {
        runtime.overlay_visible = true;
        runtime.draft_overlay_transform = None;
        runtime.draft_base_transform = None;
        runtime.draft_dirty = false;
        runtime.overlay_adjust_mode = false;
    })?;
    ctx.emit_event(
        "camera.calibration.saved",
        json!({
            "camera_id": input.camera_id,
            "profile_id": profile.id,
            "quality_score": input.calibration.quality_score,
        }),
    );
    Ok(input.calibration)
}

pub fn reset_camera_calibration(
    ctx: &ServiceContext,
    camera_id: Option<String>,
) -> ServiceResult<()> {
    let (_, camera_id) = current_profile_and_camera_id(ctx, camera_id)?;
    let profile = update_active_profile(ctx, |profile| {
        profile.camera_calibration = None;
        Ok(())
    })?;
    let frame = current_frame_for_profile(ctx, profile.id)?;
    mutate_runtime_for_profile(ctx, profile.id, |runtime| {
        runtime.draft_dirty = false;
        runtime.overlay_adjust_mode = false;
        if profile.camera_alignment.is_none() {
            if let Some(frame) = frame.as_ref() {
                let (bed_width, bed_height) = active_workspace_dims(ctx, &profile);
                let fitted = fit_camera_overlay_to_bed(
                    frame.width_px,
                    frame.height_px,
                    bed_width,
                    bed_height,
                );
                runtime.draft_overlay_transform = Some(fitted.clone());
                runtime.draft_base_transform = Some(fitted);
            } else {
                runtime.draft_overlay_transform = None;
                runtime.draft_base_transform = None;
            }
        }
    })?;
    ctx.emit_event(
        "camera.calibration.reset",
        json!({
            "camera_id": camera_id,
            "profile_id": profile.id,
        }),
    );
    Ok(())
}

pub fn get_camera_alignment(ctx: &ServiceContext) -> ServiceResult<Option<CameraAlignment>> {
    let profile = match active_profile(ctx) {
        Ok(p) => p,
        Err(_) => return Ok(None),
    };
    Ok(profile.camera_alignment)
}

pub fn solve_camera_alignment(
    ctx: &ServiceContext,
    input: CameraAlignmentInput,
) -> ServiceResult<CameraAlignment> {
    let (profile, camera_id) = current_profile_and_camera_id(ctx, input.camera_id)?;
    let mut alignment =
        camera_backend::solve_alignment(&input.points).map_err(ServiceError::invalid_input)?;
    if let Some((width_px, height_px)) = camera_dimensions_for_profile(ctx, &profile, &camera_id)? {
        alignment.image_width_px = Some(width_px);
        alignment.image_height_px = Some(height_px);
    }
    validate_camera_alignment(&alignment)?;
    ctx.emit_event(
        "camera.alignment.solved",
        json!({
            "camera_id": camera_id,
            "profile_id": profile.id,
            "quality_score": alignment.quality_score,
            "point_count": input.points.points.len(),
        }),
    );
    Ok(alignment)
}

pub fn save_camera_alignment(
    ctx: &ServiceContext,
    camera_id: Option<String>,
    mut alignment: CameraAlignment,
) -> ServiceResult<CameraAlignment> {
    validate_camera_alignment(&alignment)?;
    let (profile, camera_id) = current_profile_and_camera_id(ctx, camera_id)?;
    if let Some((width_px, height_px)) = camera_dimensions_for_profile(ctx, &profile, &camera_id)? {
        if let (Some(saved_width), Some(saved_height)) =
            (alignment.image_width_px, alignment.image_height_px)
            && (saved_width != width_px || saved_height != height_px)
        {
            return Err(ServiceError::invalid_input(format!(
                "Camera alignment dimensions {saved_width}x{saved_height} do not match the current frame {width_px}x{height_px}"
            )));
        }
        alignment.image_width_px = Some(width_px);
        alignment.image_height_px = Some(height_px);
    }
    validate_camera_alignment(&alignment)?;
    update_active_profile(ctx, |profile| {
        profile.camera_alignment = Some(alignment.clone());
        Ok(())
    })?;
    mutate_runtime_for_profile(ctx, profile.id, |runtime| {
        runtime.overlay_visible = true;
        runtime.draft_overlay_transform = None;
        runtime.draft_base_transform = None;
        runtime.draft_dirty = false;
        runtime.overlay_adjust_mode = false;
    })?;
    ctx.emit_event(
        "camera.alignment.saved",
        json!({
            "camera_id": camera_id,
            "profile_id": profile.id,
            "quality_score": alignment.quality_score,
        }),
    );
    Ok(alignment)
}

pub fn reset_camera_alignment(
    ctx: &ServiceContext,
    camera_id: Option<String>,
) -> ServiceResult<()> {
    let (profile, camera_id) = current_profile_and_camera_id(ctx, camera_id)?;
    update_active_profile(ctx, |profile| {
        profile.camera_alignment = None;
        Ok(())
    })?;
    let frame = current_frame_for_profile(ctx, profile.id)?;
    mutate_runtime_for_profile(ctx, profile.id, |runtime| {
        runtime.draft_dirty = false;
        runtime.overlay_adjust_mode = false;
        if let Some(frame) = frame.as_ref() {
            let (bed_width, bed_height) = active_workspace_dims(ctx, &profile);
            let fitted =
                fit_camera_overlay_to_bed(frame.width_px, frame.height_px, bed_width, bed_height);
            runtime.draft_overlay_transform = Some(fitted.clone());
            runtime.draft_base_transform = Some(fitted);
        } else {
            runtime.draft_overlay_transform = None;
            runtime.draft_base_transform = None;
        }
    })?;
    ctx.emit_event(
        "camera.alignment.reset",
        json!({
            "camera_id": camera_id,
            "profile_id": profile.id,
        }),
    );
    Ok(())
}

pub fn capture_camera_frame(
    ctx: &ServiceContext,
    camera_id: Option<String>,
) -> ServiceResult<CameraFrameHandle> {
    let (profile, camera_id) = current_profile_and_camera_id(ctx, camera_id)?;
    let device = ensure_device(ctx, &camera_id)?;
    let frame = camera_backend::capture_frame_for_device(&device).map_err(ServiceError::machine)?;
    store_frame_for_profile(ctx, profile.id, frame.clone())?;
    apply_capture_overlay_defaults(ctx, &profile, &frame)?;
    ctx.emit_event(
        "camera.frame.captured",
        json!({
            "camera_id": camera_id,
            "profile_id": profile.id,
            "frame": frame,
        }),
    );
    Ok(frame)
}

pub fn save_camera_frame_bytes(
    ctx: &ServiceContext,
    input: SaveCameraFrameBytesInput,
) -> ServiceResult<CameraFrameHandle> {
    let (profile, camera_id) = current_profile_and_camera_id(ctx, Some(input.camera_id.clone()))?;
    let _ = ensure_device(ctx, &camera_id)?;
    let (extension, expected_format) = frame_format(&input.media_type)?;
    let (width_px, height_px) = validate_camera_frame(&input, expected_format)?;

    let dir = frame_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|e| ServiceError::machine(format!("Failed to create frame directory: {e}")))?;
    let handle_id = Uuid::new_v4().to_string();
    let file_path = dir.join(format!("{handle_id}.{extension}"));
    std::fs::write(&file_path, input.image_data)
        .map_err(|e| ServiceError::machine(format!("Failed to save camera frame: {e}")))?;

    let frame = CameraFrameHandle {
        handle_id,
        file_path: file_path.to_string_lossy().to_string(),
        width_px,
        height_px,
        media_type: input.media_type,
        captured_at: Utc::now().to_rfc3339(),
    };
    let frame = store_frame_for_profile(ctx, profile.id, frame)?;
    apply_capture_overlay_defaults(ctx, &profile, &frame)?;
    ctx.emit_event(
        "camera.frame.captured",
        json!({
            "camera_id": camera_id,
            "profile_id": profile.id,
            "frame": frame,
        }),
    );
    Ok(frame)
}

pub fn get_overlay_state(ctx: &ServiceContext) -> ServiceResult<CameraOverlayState> {
    let profile = match active_profile(ctx) {
        Ok(p) => p,
        Err(_) => {
            return Ok(CameraOverlayState {
                selected_camera_id: None,
                frame: None,
                calibration: None,
                alignment: None,
                overlay_ready: false,
            });
        }
    };
    let frame = if profile.selected_camera_id.is_some() {
        current_frame_for_profile(ctx, profile.id)?
    } else {
        None
    };
    Ok(CameraOverlayState {
        selected_camera_id: profile.selected_camera_id.clone(),
        frame: frame.clone(),
        calibration: profile.camera_calibration.clone(),
        alignment: profile.camera_alignment.clone(),
        overlay_ready: profile.selected_camera_id.is_some()
            && frame.is_some()
            && (profile.camera_alignment.is_some() || profile.camera_calibration.is_some()),
    })
}

pub fn get_camera_state(ctx: &ServiceContext) -> ServiceResult<CameraAgentState> {
    let overlay = get_overlay_state(ctx)?;
    let profile = active_profile(ctx).ok();
    let runtime = if let Some(profile) = profile.as_ref() {
        runtime_for_profile(ctx, profile.id)?
    } else {
        CameraOverlayRuntimeState::default()
    };
    let latest_capture = ctx
        .camera_latest_capture_artifact
        .lock()
        .map_err(|e| lock_err("camera_latest_capture_artifact", e))?
        .clone();
    let latest_render = ctx
        .camera_latest_render_artifact
        .lock()
        .map_err(|e| lock_err("camera_latest_render_artifact", e))?
        .clone();
    Ok(CameraAgentState {
        schema_version: CAMERA_AGENT_SCHEMA_VERSION,
        selected_camera_id: overlay.selected_camera_id.clone(),
        frame: overlay.frame.clone(),
        calibration: overlay.calibration.clone(),
        alignment: overlay.alignment.clone(),
        overlay_ready: overlay.overlay_ready,
        frontend_bridge_connected: ctx.camera_agent_bridge_connected.load(Ordering::Relaxed),
        display: display_state(&overlay, &runtime),
        latest_capture,
        latest_render,
    })
}

pub fn update_overlay_display(
    ctx: &ServiceContext,
    input: CameraOverlayDisplayUpdate,
) -> ServiceResult<CameraAgentState> {
    let profile = active_profile(ctx)?;
    mutate_runtime_for_profile(ctx, profile.id, |runtime| {
        if let Some(visible) = input.overlay_visible {
            runtime.overlay_visible = visible;
        }
        if let Some(opacity) = input.overlay_opacity {
            runtime.overlay_opacity = opacity.clamp(0.0, 1.0);
        }
        if let Some(adjust_mode) = input.overlay_adjust_mode {
            runtime.overlay_adjust_mode = adjust_mode;
            if adjust_mode {
                runtime.overlay_visible = true;
            }
            if !adjust_mode && !runtime.draft_dirty && saved_profile_transform(&profile).is_some() {
                runtime.draft_overlay_transform = None;
                runtime.draft_base_transform = None;
            }
        }
    })?;
    get_camera_state(ctx)
}

pub fn fit_overlay_to_bed(ctx: &ServiceContext) -> ServiceResult<CameraAgentState> {
    let profile = active_profile(ctx)?;
    let frame = current_frame_for_profile(ctx, profile.id)?
        .ok_or_else(|| ServiceError::invalid_state("No camera frame to fit"))?;
    let (bed_width, bed_height) = active_workspace_dims(ctx, &profile);
    let fitted = fit_camera_overlay_to_bed(frame.width_px, frame.height_px, bed_width, bed_height);
    let saved_transform = saved_profile_transform(&profile);
    mutate_runtime_for_profile(ctx, profile.id, |runtime| {
        runtime.overlay_visible = true;
        runtime.overlay_adjust_mode = true;
        runtime.draft_overlay_transform = Some(fitted.clone());
        runtime.draft_base_transform = saved_transform.clone().or(Some(fitted));
        runtime.draft_dirty = saved_transform.is_some();
    })?;
    get_camera_state(ctx)
}

pub fn discard_overlay_draft(ctx: &ServiceContext) -> ServiceResult<CameraAgentState> {
    let profile = active_profile(ctx)?;
    mutate_runtime_for_profile(ctx, profile.id, |runtime| {
        runtime.overlay_adjust_mode = false;
        runtime.draft_dirty = false;
        if saved_profile_transform(&profile).is_some() {
            runtime.draft_overlay_transform = None;
            runtime.draft_base_transform = None;
        } else {
            runtime.draft_overlay_transform = runtime.draft_base_transform.clone();
        }
    })?;
    get_camera_state(ctx)
}

pub fn save_overlay_draft_alignment(ctx: &ServiceContext) -> ServiceResult<CameraAgentState> {
    let profile = active_profile(ctx)?;
    let runtime = runtime_for_profile(ctx, profile.id)?;
    let transform = runtime
        .draft_overlay_transform
        .clone()
        .ok_or_else(|| ServiceError::invalid_state("No draft camera overlay transform to save"))?;
    let camera_id = profile
        .selected_camera_id
        .clone()
        .ok_or_else(|| ServiceError::invalid_state("No camera selected on the active profile"))?;
    let alignment = CameraAlignment {
        transform,
        image_width_px: None,
        image_height_px: None,
        rmse_mm: 0.0,
        quality_score: 1.0,
        solved_at: Utc::now().to_rfc3339(),
        source: CameraAlignmentSource::ManualAdjust,
    };
    save_camera_alignment(ctx, Some(camera_id), alignment)?;
    get_camera_state(ctx)
}

fn current_overlay_transform_for_edit(
    ctx: &ServiceContext,
    profile: &beambench_core::MachineProfile,
) -> ServiceResult<SimilarityTransform> {
    let overlay = get_overlay_state(ctx)?;
    let runtime = runtime_for_profile(ctx, profile.id)?;
    effective_transform(&overlay, &runtime)
        .ok_or_else(|| ServiceError::invalid_state("No camera overlay transform to edit"))
}

pub fn update_overlay_transform(
    ctx: &ServiceContext,
    input: CameraOverlayTransformCommand,
) -> ServiceResult<CameraAgentState> {
    let profile = active_profile(ctx)?;
    let frame = current_frame_for_profile(ctx, profile.id)?
        .ok_or_else(|| ServiceError::invalid_state("No camera frame to transform"))?;
    let mut transform = current_overlay_transform_for_edit(ctx, &profile)?;
    if let Some(set) = input.set {
        if let Some(x) = set.x {
            transform.translation_x = x;
        }
        if let Some(y) = set.y {
            transform.translation_y = y;
        }
        if let Some(scale) = set.scale {
            transform.scale = scale.clamp(OVERLAY_MIN_SCALE, OVERLAY_MAX_SCALE);
        }
        if let Some(rotation_deg) = set.rotation_deg {
            transform.rotation_deg = rotation_deg;
        }
    }
    if let Some(nudge) = input.nudge {
        transform.translation_x += nudge.dx;
        transform.translation_y += nudge.dy;
    }
    if let Some(factor) = input.scale_factor {
        let next_scale = (transform.scale * factor).clamp(OVERLAY_MIN_SCALE, OVERLAY_MAX_SCALE);
        transform = transform_keeping_center(
            frame.width_px,
            frame.height_px,
            &transform,
            next_scale,
            transform.rotation_deg,
        );
    }
    if let Some(deg) = input.rotate_deg {
        transform = transform_keeping_center(
            frame.width_px,
            frame.height_px,
            &transform,
            transform.scale,
            transform.rotation_deg + deg,
        );
    }

    mutate_runtime_for_profile(ctx, profile.id, |runtime| {
        runtime.overlay_visible = true;
        runtime.draft_overlay_transform = Some(transform);
        runtime.draft_base_transform = runtime
            .draft_base_transform
            .clone()
            .or_else(|| saved_profile_transform(&profile));
        runtime.draft_dirty = true;
    })?;
    get_camera_state(ctx)
}

fn artifact_for_frame(
    frame: &CameraFrameHandle,
    output_path: Option<String>,
) -> ServiceResult<CameraArtifactInfo> {
    let temporary = output_path.is_none();
    let path = if let Some(output_path) = output_path {
        let output = std::path::PathBuf::from(output_path);
        if let Some(parent) = output.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(&frame.file_path, &output).map_err(|e| {
            ServiceError::persistence(format!("Failed to copy camera capture artifact: {e}"))
        })?;
        output.to_string_lossy().to_string()
    } else {
        frame.file_path.clone()
    };
    let bytes = std::fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
    Ok(CameraArtifactInfo {
        path,
        temporary,
        bytes,
    })
}

pub async fn capture_camera_agent(
    ctx: std::sync::Arc<ServiceContext>,
    input: CameraCaptureAgentInput,
) -> ServiceResult<CameraAgentState> {
    let (profile, camera_id) = current_profile_and_camera_id(&ctx, input.camera_id.clone())?;
    let device = ensure_device(&ctx, &camera_id)?;
    let frame = if device.backend_kind == beambench_common::CameraBackendKind::Native {
        request_app_assisted_capture(ctx.clone(), camera_id.clone()).await?
    } else {
        capture_camera_frame(&ctx, Some(camera_id.clone()))?
    };
    let profile = active_profile(&ctx).unwrap_or(profile);
    apply_capture_overlay_defaults(&ctx, &profile, &frame)?;
    let artifact = artifact_for_frame(&frame, input.output_path)?;
    remember_capture_artifact(&ctx, artifact)?;
    get_camera_state(&ctx)
}

async fn request_app_assisted_capture(
    ctx: std::sync::Arc<ServiceContext>,
    camera_id: String,
) -> ServiceResult<CameraFrameHandle> {
    if !ctx.camera_agent_bridge_connected.load(Ordering::Relaxed) {
        return Err(ServiceError::invalid_state(
            "Beam Bench app must be running and open for browser camera capture",
        ));
    }
    let request_id = Uuid::new_v4().to_string();
    let (tx, rx) = tokio::sync::oneshot::channel();
    {
        let mut pending = ctx
            .pending_camera_capture_request
            .lock()
            .map_err(|e| lock_err("pending_camera_capture_request", e))?;
        if pending.is_some() {
            return Err(ServiceError::busy("Camera capture is already in progress"));
        }
        *pending = Some(PendingCameraCaptureRequest {
            request_id: request_id.clone(),
            tx,
        });
    }
    ctx.emit_event(
        "camera.capture.requested",
        json!({
            "request_id": request_id,
            "camera_id": camera_id,
        }),
    );
    match tokio::time::timeout(app_assist_timeout(), rx).await {
        Ok(Ok(result)) => result,
        Ok(Err(_)) => Err(ServiceError::internal(
            "Camera capture request was cancelled",
        )),
        Err(_) => {
            clear_pending_capture_request(&ctx, &request_id);
            Err(ServiceError::invalid_state(
                "Timed out waiting for Beam Bench app camera capture",
            ))
        }
    }
}

fn clear_pending_capture_request(ctx: &ServiceContext, request_id: &str) {
    if let Ok(mut pending) = ctx.pending_camera_capture_request.lock()
        && pending
            .as_ref()
            .is_some_and(|request| request.request_id == request_id)
    {
        *pending = None;
    }
}

pub fn complete_camera_capture_request(
    ctx: &ServiceContext,
    request_id: String,
    frame: Option<CameraFrameHandle>,
    error: Option<String>,
) -> ServiceResult<()> {
    let pending = {
        let mut guard = ctx
            .pending_camera_capture_request
            .lock()
            .map_err(|e| lock_err("pending_camera_capture_request", e))?;
        let Some(pending) = guard.take() else {
            return Err(ServiceError::not_found("No pending camera capture request"));
        };
        if pending.request_id != request_id {
            *guard = Some(pending);
            return Err(ServiceError::not_found(
                "Camera capture request id does not match",
            ));
        }
        pending
    };
    let result = if let Some(error) = error {
        Err(ServiceError::machine(error))
    } else {
        frame.ok_or_else(|| ServiceError::invalid_input("Missing captured camera frame"))
    };
    let _ = pending.tx.send(result);
    Ok(())
}

pub fn set_camera_agent_bridge_connected(ctx: &ServiceContext, connected: bool) {
    ctx.camera_agent_bridge_connected
        .store(connected, Ordering::Relaxed);
    ctx.emit_event(
        "camera.agent_bridge.updated",
        json!({ "connected": connected }),
    );
}

pub async fn request_overlay_render(
    ctx: std::sync::Arc<ServiceContext>,
    input: CameraOverlayRenderRequest,
) -> ServiceResult<CameraOverlayRenderResult> {
    if !ctx.camera_agent_bridge_connected.load(Ordering::Relaxed) {
        return Err(ServiceError::invalid_state(
            "Beam Bench app must be running and open for camera overlay render",
        ));
    }
    let request_id = Uuid::new_v4().to_string();
    let (tx, rx) = tokio::sync::oneshot::channel();
    let temporary = !input.keep;
    {
        let mut pending = ctx
            .pending_camera_overlay_render_request
            .lock()
            .map_err(|e| lock_err("pending_camera_overlay_render_request", e))?;
        if pending.is_some() {
            return Err(ServiceError::busy(
                "Camera overlay render is already in progress",
            ));
        }
        *pending = Some(PendingCameraOverlayRenderRequest {
            request_id: request_id.clone(),
            view: input.view,
            temporary,
            tx,
        });
    }
    let options = CameraOverlayRenderOptions {
        output_path: input.output_path.clone(),
        view: input.view,
        format: "png".to_string(),
    };
    ctx.emit_event(
        "camera.overlay.render.requested",
        json!({
            "request_id": request_id,
            "options": options,
        }),
    );
    let result = match tokio::time::timeout(app_assist_timeout(), rx).await {
        Ok(Ok(result)) => result,
        Ok(Err(_)) => Err(ServiceError::internal(
            "Camera overlay render request was cancelled",
        )),
        Err(_) => {
            clear_pending_render_request(&ctx, &request_id);
            Err(ServiceError::invalid_state(
                "Timed out waiting for Beam Bench app camera overlay render",
            ))
        }
    }?;
    remember_render_artifact(
        &ctx,
        CameraArtifactInfo {
            path: result.path.clone(),
            temporary: result.temporary,
            bytes: result.bytes,
        },
    )?;
    Ok(result)
}

fn clear_pending_render_request(ctx: &ServiceContext, request_id: &str) {
    if let Ok(mut pending) = ctx.pending_camera_overlay_render_request.lock()
        && pending
            .as_ref()
            .is_some_and(|request| request.request_id == request_id)
    {
        *pending = None;
    }
}

pub fn complete_camera_overlay_render_request(
    ctx: &ServiceContext,
    request_id: String,
    path: Option<String>,
    error: Option<String>,
) -> ServiceResult<()> {
    let pending = {
        let mut guard = ctx
            .pending_camera_overlay_render_request
            .lock()
            .map_err(|e| lock_err("pending_camera_overlay_render_request", e))?;
        let Some(pending) = guard.take() else {
            return Err(ServiceError::not_found(
                "No pending camera overlay render request",
            ));
        };
        if pending.request_id != request_id {
            *guard = Some(pending);
            return Err(ServiceError::not_found(
                "Camera overlay render request id does not match",
            ));
        }
        pending
    };
    let result = if let Some(error) = error {
        Err(ServiceError::machine(error))
    } else {
        let path = path.ok_or_else(|| ServiceError::invalid_input("Missing render output path"))?;
        let bytes = std::fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
        Ok(CameraOverlayRenderResult {
            schema_version: CAMERA_AGENT_SCHEMA_VERSION,
            path,
            view: pending.view,
            format: "png".to_string(),
            temporary: pending.temporary,
            bytes,
        })
    };
    let _ = pending.tx.send(result);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ServiceErrorCode;
    use beambench_common::{
        AlignmentPoint, CalibrationPoint, CameraAlignment, CameraAlignmentSource,
        CameraCalibration, SimilarityTransform,
    };
    use beambench_core::{AppSettings, MachineProfile};

    fn ctx_with_active_profile() -> ServiceContext {
        let profile = MachineProfile {
            name: "Camera Test".to_string(),
            ..Default::default()
        };
        let profile_id = profile.id;
        let mut settings = AppSettings::default();
        settings.active_profile_id = Some(profile_id);
        settings.machine_profiles.push(profile);
        let ctx = ServiceContext::with_settings(settings);
        *ctx.camera_devices_override.lock().unwrap() = Some(vec![
            CameraDeviceInfo {
                camera_id: "cam-a".to_string(),
                display_name: "Camera A".to_string(),
                backend_kind: beambench_common::CameraBackendKind::MockSnapshot,
                available: true,
                width_px: 640,
                height_px: 480,
                status_text: "Ready".to_string(),
            },
            CameraDeviceInfo {
                camera_id: "cam-b".to_string(),
                display_name: "Camera B".to_string(),
                backend_kind: beambench_common::CameraBackendKind::MockSnapshot,
                available: true,
                width_px: 800,
                height_px: 600,
                status_text: "Ready".to_string(),
            },
        ]);
        ctx
    }

    fn test_alignment() -> CameraAlignment {
        CameraAlignment {
            transform: SimilarityTransform {
                scale: 0.2,
                rotation_deg: 0.0,
                translation_x: 10.0,
                translation_y: 20.0,
            },
            image_width_px: Some(640),
            image_height_px: Some(480),
            rmse_mm: 0.1,
            quality_score: 0.97,
            solved_at: Utc::now().to_rfc3339(),
            source: CameraAlignmentSource::SolvedPoints,
        }
    }

    fn test_calibration() -> CameraCalibration {
        CameraCalibration {
            image_width_px: 640,
            image_height_px: 480,
            transform: SimilarityTransform {
                scale: 0.2,
                rotation_deg: 0.0,
                translation_x: 5.0,
                translation_y: 10.0,
            },
            rmse_px: 0.1,
            quality_score: 0.97,
            solved_at: Utc::now().to_rfc3339(),
        }
    }

    fn test_png(width_px: u32, height_px: u32) -> Vec<u8> {
        let image = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            width_px,
            height_px,
            image::Rgba([0, 0, 0, 255]),
        ));
        let mut bytes = std::io::Cursor::new(Vec::new());
        image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
        bytes.into_inner()
    }

    struct AppAssistTimeoutGuard;

    impl Drop for AppAssistTimeoutGuard {
        fn drop(&mut self) {
            APP_ASSIST_TIMEOUT_OVERRIDE_MS.store(0, Ordering::Relaxed);
        }
    }

    fn use_app_assist_timeout_for_test(timeout: Duration) -> AppAssistTimeoutGuard {
        APP_ASSIST_TIMEOUT_OVERRIDE_MS.store(timeout.as_millis() as u64, Ordering::Relaxed);
        AppAssistTimeoutGuard
    }

    #[test]
    fn list_devices_returns_override() {
        let ctx = ctx_with_active_profile();
        let devices = list_devices(&ctx).unwrap();
        assert_eq!(devices.len(), 2);
    }

    #[test]
    fn capture_and_overlay_state_roundtrip() {
        let ctx = ctx_with_active_profile();
        select_camera(&ctx, Some("cam-a".to_string())).unwrap();
        let frame = capture_camera_frame(&ctx, None).unwrap();
        let overlay = get_overlay_state(&ctx).unwrap();
        assert_eq!(overlay.selected_camera_id.as_deref(), Some("cam-a"));
        assert_eq!(overlay.frame.unwrap().handle_id, frame.handle_id);
    }

    #[test]
    fn save_camera_frame_bytes_stores_overlay_state() {
        let ctx = ctx_with_active_profile();
        let frame = save_camera_frame_bytes(
            &ctx,
            SaveCameraFrameBytesInput {
                camera_id: "cam-a".to_string(),
                image_data: test_png(1, 1),
                width_px: 1,
                height_px: 1,
                media_type: "image/png".to_string(),
            },
        )
        .unwrap();

        let overlay = get_overlay_state(&ctx).unwrap();
        assert_eq!(overlay.selected_camera_id.as_deref(), Some("cam-a"));
        assert_eq!(overlay.frame.unwrap().handle_id, frame.handle_id);
    }

    #[test]
    fn save_camera_frame_rejects_unsupported_media_type() {
        let ctx = ctx_with_active_profile();
        let err = save_camera_frame_bytes(
            &ctx,
            SaveCameraFrameBytesInput {
                camera_id: "cam-a".to_string(),
                image_data: vec![0],
                width_px: 1,
                height_px: 1,
                media_type: "image/gif".to_string(),
            },
        )
        .unwrap_err();

        assert_eq!(err.code, ServiceErrorCode::InvalidInput);
        assert_eq!(
            err.message,
            "Unsupported camera frame media type 'image/gif'"
        );
    }

    #[test]
    fn save_camera_frame_rejects_invalid_or_mismatched_image_data() {
        let ctx = ctx_with_active_profile();
        let invalid = save_camera_frame_bytes(
            &ctx,
            SaveCameraFrameBytesInput {
                camera_id: "cam-a".to_string(),
                image_data: vec![137, 80, 78, 71],
                width_px: 1,
                height_px: 1,
                media_type: "image/png".to_string(),
            },
        )
        .unwrap_err();
        assert_eq!(invalid.code, ServiceErrorCode::InvalidInput);

        let mut truncated_png = test_png(2, 2);
        truncated_png.truncate(33);
        let truncated = save_camera_frame_bytes(
            &ctx,
            SaveCameraFrameBytesInput {
                camera_id: "cam-a".to_string(),
                image_data: truncated_png,
                width_px: 2,
                height_px: 2,
                media_type: "image/png".to_string(),
            },
        )
        .unwrap_err();
        assert_eq!(truncated.code, ServiceErrorCode::InvalidInput);
        assert_eq!(truncated.message, "Camera frame could not be decoded");

        let mismatched = save_camera_frame_bytes(
            &ctx,
            SaveCameraFrameBytesInput {
                camera_id: "cam-a".to_string(),
                image_data: test_png(2, 3),
                width_px: 3,
                height_px: 2,
                media_type: "image/png".to_string(),
            },
        )
        .unwrap_err();
        assert_eq!(mismatched.code, ServiceErrorCode::InvalidInput);
        assert!(mismatched.message.contains("2x3"));
    }

    #[test]
    fn overlay_ready_requires_selected_frame_and_alignment() {
        let ctx = ctx_with_active_profile();
        select_camera(&ctx, Some("cam-a".to_string())).unwrap();
        assert!(!get_overlay_state(&ctx).unwrap().overlay_ready);

        capture_camera_frame(&ctx, None).unwrap();
        assert!(!get_overlay_state(&ctx).unwrap().overlay_ready);

        save_camera_alignment(&ctx, Some("cam-a".to_string()), test_alignment()).unwrap();
        assert!(get_overlay_state(&ctx).unwrap().overlay_ready);
    }

    #[test]
    fn saved_calibration_is_an_effective_overlay_transform() {
        let ctx = ctx_with_active_profile();
        select_camera(&ctx, Some("cam-a".to_string())).unwrap();
        capture_camera_frame(&ctx, None).unwrap();
        save_camera_calibration(
            &ctx,
            SaveCameraCalibrationInput {
                camera_id: "cam-a".to_string(),
                calibration: test_calibration(),
            },
        )
        .unwrap();

        let state = get_camera_state(&ctx).unwrap();
        assert!(state.overlay_ready);
        assert_eq!(
            state.display.effective_transform,
            Some(test_calibration().transform)
        );
    }

    #[test]
    fn fitted_draft_discards_back_to_saved_camera_mapping() {
        let ctx = ctx_with_active_profile();
        select_camera(&ctx, Some("cam-a".to_string())).unwrap();
        capture_camera_frame(&ctx, None).unwrap();
        let calibration = test_calibration();
        save_camera_calibration(
            &ctx,
            SaveCameraCalibrationInput {
                camera_id: "cam-a".to_string(),
                calibration: calibration.clone(),
            },
        )
        .unwrap();

        let fitted = fit_overlay_to_bed(&ctx).unwrap();
        assert!(fitted.display.draft_dirty);
        assert_eq!(
            fitted.display.draft_base_transform,
            Some(calibration.transform.clone())
        );

        let discarded = discard_overlay_draft(&ctx).unwrap();
        assert!(!discarded.display.draft_dirty);
        assert!(discarded.display.draft_overlay_transform.is_none());
        assert_eq!(
            discarded.display.effective_transform,
            Some(calibration.transform)
        );
    }

    #[test]
    fn direct_save_rejects_invalid_camera_transforms() {
        let ctx = ctx_with_active_profile();
        let mut calibration = test_calibration();
        calibration.transform.scale = 0.0;
        let calibration_error = save_camera_calibration(
            &ctx,
            SaveCameraCalibrationInput {
                camera_id: "cam-a".to_string(),
                calibration,
            },
        )
        .unwrap_err();
        assert_eq!(calibration_error.code, ServiceErrorCode::InvalidInput);

        let mut alignment = test_alignment();
        alignment.quality_score = 2.0;
        let alignment_error =
            save_camera_alignment(&ctx, Some("cam-a".to_string()), alignment).unwrap_err();
        assert_eq!(alignment_error.code, ServiceErrorCode::InvalidInput);
    }

    #[test]
    fn capture_at_new_resolution_invalidates_saved_camera_mapping() {
        let ctx = ctx_with_active_profile();
        select_camera(&ctx, Some("cam-a".to_string())).unwrap();
        capture_camera_frame(&ctx, None).unwrap();
        save_camera_calibration(
            &ctx,
            SaveCameraCalibrationInput {
                camera_id: "cam-a".to_string(),
                calibration: test_calibration(),
            },
        )
        .unwrap();
        save_camera_alignment(&ctx, Some("cam-a".to_string()), test_alignment()).unwrap();

        save_camera_frame_bytes(
            &ctx,
            SaveCameraFrameBytesInput {
                camera_id: "cam-a".to_string(),
                image_data: test_png(2, 2),
                width_px: 2,
                height_px: 2,
                media_type: "image/png".to_string(),
            },
        )
        .unwrap();

        let state = get_camera_state(&ctx).unwrap();
        assert!(state.calibration.is_none());
        assert!(state.alignment.is_none());
        assert!(!state.overlay_ready);
        assert_eq!(
            state.display.status,
            CameraOverlayStatus::PreviewFittedToBed
        );
    }

    #[test]
    fn capture_at_new_resolution_invalidates_alignment_without_calibration() {
        let ctx = ctx_with_active_profile();
        select_camera(&ctx, Some("cam-a".to_string())).unwrap();
        save_camera_alignment(&ctx, Some("cam-a".to_string()), test_alignment()).unwrap();

        save_camera_frame_bytes(
            &ctx,
            SaveCameraFrameBytesInput {
                camera_id: "cam-a".to_string(),
                image_data: test_png(2, 2),
                width_px: 2,
                height_px: 2,
                media_type: "image/png".to_string(),
            },
        )
        .unwrap();

        let state = get_camera_state(&ctx).unwrap();
        assert!(state.alignment.is_none());
        assert!(!state.overlay_ready);
        assert_eq!(
            state.display.status,
            CameraOverlayStatus::PreviewFittedToBed
        );
    }

    #[test]
    fn overlay_ready_false_for_partial_overlay_state_matrix() {
        let frame_without_alignment = ctx_with_active_profile();
        select_camera(&frame_without_alignment, Some("cam-a".to_string())).unwrap();
        capture_camera_frame(&frame_without_alignment, None).unwrap();
        assert!(
            !get_overlay_state(&frame_without_alignment)
                .unwrap()
                .overlay_ready
        );

        let frame_with_alignment_without_calibration = ctx_with_active_profile();
        select_camera(
            &frame_with_alignment_without_calibration,
            Some("cam-a".to_string()),
        )
        .unwrap();
        capture_camera_frame(&frame_with_alignment_without_calibration, None).unwrap();
        save_camera_alignment(
            &frame_with_alignment_without_calibration,
            Some("cam-a".to_string()),
            test_alignment(),
        )
        .unwrap();
        assert!(
            get_overlay_state(&frame_with_alignment_without_calibration)
                .unwrap()
                .overlay_ready
        );

        let alignment_without_frame = ctx_with_active_profile();
        select_camera(&alignment_without_frame, Some("cam-a".to_string())).unwrap();
        save_camera_alignment(
            &alignment_without_frame,
            Some("cam-a".to_string()),
            test_alignment(),
        )
        .unwrap();
        assert!(
            !get_overlay_state(&alignment_without_frame)
                .unwrap()
                .overlay_ready
        );
    }

    #[test]
    fn camera_state_creates_fit_preview_after_capture_without_alignment() {
        let ctx = ctx_with_active_profile();
        select_camera(&ctx, Some("cam-a".to_string())).unwrap();
        let frame = capture_camera_frame(&ctx, None).unwrap();

        let state = get_camera_state(&ctx).unwrap();

        assert_eq!(state.schema_version, CAMERA_AGENT_SCHEMA_VERSION);
        assert_eq!(
            state.display.status,
            CameraOverlayStatus::PreviewFittedToBed
        );
        assert!(state.display.overlay_visible);
        let draft = state.display.draft_overlay_transform.unwrap();
        let expected_scale = (200.0 / frame.width_px as f64).min(200.0 / frame.height_px as f64);
        assert!((draft.scale - expected_scale).abs() < 1e-9);
        assert_eq!(state.display.effective_transform, Some(draft));
    }

    #[test]
    fn camera_state_reuses_saved_alignment_after_capture() {
        let ctx = ctx_with_active_profile();
        select_camera(&ctx, Some("cam-a".to_string())).unwrap();
        let alignment = test_alignment();
        save_camera_alignment(&ctx, Some("cam-a".to_string()), alignment.clone()).unwrap();

        capture_camera_frame(&ctx, None).unwrap();
        let state = get_camera_state(&ctx).unwrap();

        assert_eq!(state.display.status, CameraOverlayStatus::SavedAlignment);
        assert!(state.display.draft_overlay_transform.is_none());
        assert_eq!(state.display.effective_transform, Some(alignment.transform));
    }

    #[test]
    fn overlay_transform_patch_preserves_omitted_fields_and_clamps_scale() {
        let ctx = ctx_with_active_profile();
        select_camera(&ctx, Some("cam-a".to_string())).unwrap();
        capture_camera_frame(&ctx, None).unwrap();

        let state = update_overlay_transform(
            &ctx,
            CameraOverlayTransformCommand {
                set: Some(CameraOverlayTransformPatch {
                    x: None,
                    y: Some(12.0),
                    scale: Some(100.0),
                    rotation_deg: None,
                }),
                ..Default::default()
            },
        )
        .unwrap();
        let transform = state.display.draft_overlay_transform.unwrap();

        assert_eq!(transform.translation_y, 12.0);
        assert_eq!(transform.scale, OVERLAY_MAX_SCALE);
        assert_eq!(state.display.status, CameraOverlayStatus::UnsavedChanges);
    }

    #[test]
    fn dirty_draft_transform_becomes_effective_over_saved_alignment() {
        let ctx = ctx_with_active_profile();
        select_camera(&ctx, Some("cam-a".to_string())).unwrap();
        let alignment = test_alignment();
        save_camera_alignment(&ctx, Some("cam-a".to_string()), alignment.clone()).unwrap();
        capture_camera_frame(&ctx, None).unwrap();

        let state = update_overlay_transform(
            &ctx,
            CameraOverlayTransformCommand {
                nudge: Some(CameraOverlayNudge { dx: 15.0, dy: 0.0 }),
                ..Default::default()
            },
        )
        .unwrap();
        let draft = state.display.draft_overlay_transform.clone().unwrap();

        assert_eq!(state.display.status, CameraOverlayStatus::UnsavedChanges);
        assert_ne!(draft, alignment.transform);
        assert_eq!(state.display.effective_transform, Some(draft));
    }

    #[tokio::test]
    async fn app_assisted_capture_requires_frontend_bridge() {
        let ctx = std::sync::Arc::new(ctx_with_active_profile());
        select_camera(&ctx, Some("cam-a".to_string())).unwrap();
        let err = request_app_assisted_capture(ctx, "cam-a".to_string())
            .await
            .unwrap_err();

        assert_eq!(err.code, ServiceErrorCode::InvalidState);
        assert!(err.message.contains("Beam Bench app must be running"));
    }

    #[tokio::test]
    async fn app_assisted_capture_returns_busy_when_request_is_in_flight() {
        let ctx = std::sync::Arc::new(ctx_with_active_profile());
        set_camera_agent_bridge_connected(&ctx, true);
        let (tx, _rx) = tokio::sync::oneshot::channel();
        *ctx.pending_camera_capture_request.lock().unwrap() = Some(PendingCameraCaptureRequest {
            request_id: "existing-capture".to_string(),
            tx,
        });

        let err = request_app_assisted_capture(ctx, "cam-a".to_string())
            .await
            .unwrap_err();

        assert_eq!(err.code, ServiceErrorCode::Busy);
        assert!(err.message.contains("already in progress"));
    }

    #[tokio::test]
    async fn app_assisted_render_returns_busy_when_request_is_in_flight() {
        let ctx = std::sync::Arc::new(ctx_with_active_profile());
        set_camera_agent_bridge_connected(&ctx, true);
        let (tx, _rx) = tokio::sync::oneshot::channel();
        *ctx.pending_camera_overlay_render_request.lock().unwrap() =
            Some(PendingCameraOverlayRenderRequest {
                request_id: "existing-render".to_string(),
                view: CameraOverlayRenderView::Fit,
                temporary: true,
                tx,
            });

        let err = request_overlay_render(
            ctx,
            CameraOverlayRenderRequest {
                output_path: std::env::temp_dir()
                    .join("beam-bench-render-busy-test.png")
                    .to_string_lossy()
                    .to_string(),
                view: CameraOverlayRenderView::Fit,
                keep: false,
            },
        )
        .await
        .unwrap_err();

        assert_eq!(err.code, ServiceErrorCode::Busy);
        assert!(err.message.contains("already in progress"));
    }

    #[tokio::test]
    async fn app_assisted_capture_timeout_clears_pending_request() {
        let _timeout = use_app_assist_timeout_for_test(Duration::from_millis(1));
        let ctx = std::sync::Arc::new(ctx_with_active_profile());
        set_camera_agent_bridge_connected(&ctx, true);

        let err = request_app_assisted_capture(ctx.clone(), "cam-a".to_string())
            .await
            .unwrap_err();

        assert_eq!(err.code, ServiceErrorCode::InvalidState);
        assert!(err.message.contains("Timed out waiting"));
        assert!(ctx.pending_camera_capture_request.lock().unwrap().is_none());
    }

    #[test]
    fn switching_camera_clears_previous_frame() {
        let ctx = ctx_with_active_profile();
        select_camera(&ctx, Some("cam-a".to_string())).unwrap();
        capture_camera_frame(&ctx, None).unwrap();
        save_camera_calibration(
            &ctx,
            SaveCameraCalibrationInput {
                camera_id: "cam-a".to_string(),
                calibration: test_calibration(),
            },
        )
        .unwrap();
        save_camera_alignment(&ctx, Some("cam-a".to_string()), test_alignment()).unwrap();

        select_camera(&ctx, Some("cam-b".to_string())).unwrap();

        let overlay = get_overlay_state(&ctx).unwrap();
        assert_eq!(overlay.selected_camera_id.as_deref(), Some("cam-b"));
        assert!(overlay.frame.is_none());
        assert!(overlay.calibration.is_none());
        assert!(overlay.alignment.is_none());
        assert!(!overlay.overlay_ready);
    }

    #[test]
    fn clearing_camera_selection_clears_previous_frame() {
        let ctx = ctx_with_active_profile();
        select_camera(&ctx, Some("cam-a".to_string())).unwrap();
        capture_camera_frame(&ctx, None).unwrap();

        select_camera(&ctx, None).unwrap();

        let overlay = get_overlay_state(&ctx).unwrap();
        assert!(overlay.selected_camera_id.is_none());
        assert!(overlay.frame.is_none());
        assert!(!overlay.overlay_ready);
    }

    #[test]
    fn calibration_roundtrip_persists_on_profile() {
        let ctx = ctx_with_active_profile();
        let solved = solve_camera_calibration(
            &ctx,
            CameraCalibrationInput {
                camera_id: "cam-a".to_string(),
                points: CalibrationPointSet {
                    image_width_px: 100,
                    image_height_px: 100,
                    points: vec![
                        CalibrationPoint {
                            image_x: 0.0,
                            image_y: 0.0,
                            reference_x: 10.0,
                            reference_y: 20.0,
                        },
                        CalibrationPoint {
                            image_x: 10.0,
                            image_y: 0.0,
                            reference_x: 20.0,
                            reference_y: 20.0,
                        },
                        CalibrationPoint {
                            image_x: 0.0,
                            image_y: 10.0,
                            reference_x: 10.0,
                            reference_y: 30.0,
                        },
                    ],
                },
            },
        )
        .unwrap();
        save_camera_calibration(
            &ctx,
            SaveCameraCalibrationInput {
                camera_id: "cam-a".to_string(),
                calibration: solved.calibration.clone(),
            },
        )
        .unwrap();
        let loaded = get_camera_calibration(&ctx, Some("cam-a".to_string())).unwrap();
        assert!(loaded.is_some());
    }

    #[test]
    fn alignment_roundtrip_and_reset() {
        let ctx = ctx_with_active_profile();
        let alignment = solve_camera_alignment(
            &ctx,
            CameraAlignmentInput {
                camera_id: Some("cam-b".to_string()),
                points: AlignmentPointSet {
                    points: vec![
                        AlignmentPoint {
                            camera_x: 0.0,
                            camera_y: 0.0,
                            workspace_x_mm: 5.0,
                            workspace_y_mm: 8.0,
                        },
                        AlignmentPoint {
                            camera_x: 10.0,
                            camera_y: 0.0,
                            workspace_x_mm: 15.0,
                            workspace_y_mm: 8.0,
                        },
                        AlignmentPoint {
                            camera_x: 0.0,
                            camera_y: 10.0,
                            workspace_x_mm: 5.0,
                            workspace_y_mm: 18.0,
                        },
                    ],
                },
            },
        )
        .unwrap();
        save_camera_alignment(&ctx, Some("cam-b".to_string()), alignment).unwrap();
        assert!(get_camera_alignment(&ctx).unwrap().is_some());
        reset_camera_alignment(&ctx, Some("cam-b".to_string())).unwrap();
        assert!(get_camera_alignment(&ctx).unwrap().is_none());
    }
}

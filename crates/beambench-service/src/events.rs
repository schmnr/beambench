use std::path::Path;

use beambench_common::CameraOverlayState;
use beambench_common::machine::{
    ControllerFamily, ControllerModel, DeviceCapabilities, DiscoveryScanState, JobProgress,
    MachineStatus, SessionState, TransportKind,
};
use beambench_core::{MachineProfile, Project};
use chrono::Utc;
use serde::Serialize;
use serde_json::{Value, json};

use crate::context::ServiceContext;
use crate::error::{ServiceError, ServiceResult};

#[derive(Debug, Clone, Serialize)]
pub struct ServiceEventEnvelope {
    pub id: u64,
    #[serde(rename = "type")]
    pub event_type: String,
    pub timestamp: String,
    pub payload: Value,
}

impl ServiceEventEnvelope {
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("event envelope should serialize")
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeSnapshot {
    pub app: Value,
    pub project: Option<Value>,
    pub machine: Value,
    pub job: Option<Value>,
    pub discovery: DiscoveryScanState,
    pub camera: Option<Value>,
    pub active_profile_id: Option<String>,
}

fn lock_err(name: &str, e: impl std::fmt::Display) -> ServiceError {
    ServiceError::internal(format!("Failed to lock {name}: {e}"))
}

pub fn timestamp() -> String {
    Utc::now().to_rfc3339()
}

pub fn project_summary(project: &Project, path: Option<&Path>) -> Value {
    json!({
        "id": project.metadata.project_id,
        "name": project.metadata.project_name,
        "dirty": project.dirty,
        "path": path.map(|p| p.to_string_lossy().to_string()),
        "layer_count": project.layers.len(),
        "object_count": project.objects.len(),
        "asset_count": project.assets.len(),
    })
}

pub fn layer_summary(layer: &beambench_core::Layer) -> Value {
    json!({
        "id": layer.id,
        "name": layer.name,
        "enabled": layer.enabled,
        "operation": layer.primary_entry().operation,
        "entries_count": layer.entries.len(),
    })
}

pub fn object_summary(object: &beambench_core::ProjectObject) -> Value {
    let object_type = match &object.data {
        beambench_core::ObjectData::RasterImage { .. } => "raster_image",
        beambench_core::ObjectData::VectorPath { .. } => "vector_path",
        beambench_core::ObjectData::Shape { .. } => "shape",
        beambench_core::ObjectData::Star { .. } => "star",
        beambench_core::ObjectData::Text { .. } => "text",
        beambench_core::ObjectData::Polygon { .. } => "polygon",
        beambench_core::ObjectData::Barcode { .. } => "barcode",
        beambench_core::ObjectData::Group { .. } => "group",
        beambench_core::ObjectData::VirtualClone { .. } => "virtual_clone",
    };
    json!({
        "id": object.id,
        "name": object.name,
        "layer_id": object.layer_id,
        "visible": object.visible,
        "locked": object.locked,
        "bounds": object.bounds,
        "object_type": object_type,
    })
}

pub fn profile_summary(profile: &MachineProfile) -> Value {
    json!({
        "id": profile.id,
        "name": profile.name,
        "preset_id": profile.preset_id,
        "preset_version": profile.preset_version,
        "bed_width_mm": profile.bed_width_mm,
        "bed_height_mm": profile.bed_height_mm,
        "firmware_type": profile.firmware_type,
        "selected_camera_id": profile.selected_camera_id,
        "has_camera_calibration": profile.camera_calibration.is_some(),
        "has_camera_alignment": profile.camera_alignment.is_some(),
    })
}

pub fn camera_summary(state: &CameraOverlayState) -> Value {
    json!({
        "selected_camera_id": state.selected_camera_id,
        "frame": state.frame,
        "calibration": state.calibration,
        "alignment": state.alignment,
        "overlay_ready": state.overlay_ready,
    })
}

pub fn machine_summary(
    controller_family: Option<ControllerFamily>,
    controller_model: Option<ControllerModel>,
    transport_kind: Option<TransportKind>,
    capabilities: Option<DeviceCapabilities>,
    session_state: SessionState,
    machine_status: Option<MachineStatus>,
) -> Value {
    json!({
        "controller_family": controller_family,
        "controller_model": controller_model,
        "transport_kind": transport_kind,
        "capabilities": capabilities,
        "session_state": session_state,
        "machine_status": machine_status,
    })
}

pub fn job_summary(progress: &JobProgress) -> Value {
    json!({
        "state": progress.state,
        "total_lines": progress.total_lines,
        "queued_lines": progress.queued_lines,
        "sent_lines": progress.sent_lines,
        "acknowledged_lines": progress.acknowledged_lines,
        "elapsed_secs": progress.elapsed_secs,
        "estimated_remaining_secs": progress.estimated_remaining_secs,
        "buffer_fill_bytes": progress.buffer_fill_bytes,
        "error_message": progress.error_message.clone(),
    })
}

pub fn runtime_snapshot(ctx: &ServiceContext) -> ServiceResult<RuntimeSnapshot> {
    let app = json!({
        "version": env!("CARGO_PKG_VERSION"),
        "state": "ready",
    });

    let project = {
        let project_guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
        let path_guard = ctx
            .project_path
            .lock()
            .map_err(|e| lock_err("project_path", e))?;
        project_guard
            .as_ref()
            .map(|project| project_summary(project, path_guard.as_deref()))
    };

    let (
        controller_family,
        controller_model,
        transport_kind,
        capabilities,
        session_state,
        machine_status,
    ) = {
        let session_guard = ctx.session.lock().map_err(|e| lock_err("session", e))?;
        match session_guard.as_ref() {
            Some(session) => (
                Some(session.controller_family()),
                Some(session.controller_model()),
                Some(session.transport_kind()),
                Some(session.capabilities()),
                session.session_state(),
                Some(session.machine_status()),
            ),
            None => (None, None, None, None, SessionState::Disconnected, None),
        }
    };

    let job = {
        let job_guard = ctx.job.lock().map_err(|e| lock_err("job", e))?;
        job_guard.as_ref().map(|job| job_summary(&job.progress()))
    };

    let discovery = {
        let guard = ctx
            .discovery_state
            .lock()
            .map_err(|e| lock_err("discovery_state", e))?;
        guard.clone()
    };

    let active_profile_id = {
        let settings_guard = ctx.settings.lock().map_err(|e| lock_err("settings", e))?;
        settings_guard.active_profile_id.map(|id| id.to_string())
    };

    let camera = crate::ops::camera::get_overlay_state(ctx)
        .ok()
        .map(|state| camera_summary(&state));

    Ok(RuntimeSnapshot {
        app,
        project,
        machine: machine_summary(
            controller_family,
            controller_model,
            transport_kind,
            capabilities,
            session_state,
            machine_status,
        ),
        job,
        discovery,
        camera,
        active_profile_id,
    })
}

pub fn system_snapshot_payload(ctx: &ServiceContext) -> ServiceResult<Value> {
    serde_json::to_value(runtime_snapshot(ctx)?)
        .map_err(|e| ServiceError::internal(format!("Failed to serialize runtime snapshot: {e}")))
}

pub fn system_heartbeat_payload() -> Value {
    json!({})
}

pub fn system_warning_payload(message: impl Into<String>) -> Value {
    json!({
        "message": message.into(),
    })
}

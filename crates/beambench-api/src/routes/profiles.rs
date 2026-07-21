use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use beambench_common::{CameraAlignment, CameraCalibration, Id};
use beambench_core::{
    MachineProfileId, RuidaTableAxis, ScanningOffsetEntry, TransferMode, WorkspaceOrigin,
};
use beambench_service::ServiceContext;
use beambench_service::ops::discovery;
use beambench_service::ops::profiles::{self, ProfileLookup, SaveProfileInput};
use serde::Deserialize;

use crate::response::{ApiError, map_service_error};

pub fn router() -> Router<Arc<ServiceContext>> {
    Router::new()
        .route("/", get(list_profiles).post(create_profile))
        .route("/presets", get(list_presets))
        .route("/suggestion", get(profile_suggestion))
        .route("/bootstrap", post(bootstrap_profile))
        .route("/active", post(set_active))
        .route("/{id}/preset-diff/{preset_id}", get(preset_diff))
        .route("/{id}/preset", post(apply_preset))
        .route(
            "/{id}",
            get(get_profile)
                .patch(update_profile)
                .delete(delete_profile),
        )
}

#[derive(Debug, Deserialize)]
struct SaveProfileRequest {
    name: Option<String>,
    #[serde(default)]
    preset_id: Option<Option<String>>,
    #[serde(default)]
    preset_version: Option<Option<u32>>,
    bed_width_mm: Option<f64>,
    bed_height_mm: Option<f64>,
    max_speed_mm_min: Option<f64>,
    max_power_percent: Option<f64>,
    #[serde(default)]
    homing_enabled: Option<bool>,
    default_baud_rate: Option<u32>,
    firmware_type: Option<String>,
    #[serde(default)]
    notes: Option<String>,
    origin: Option<WorkspaceOrigin>,
    laser_offset_x: Option<f64>,
    laser_offset_y: Option<f64>,
    #[serde(default)]
    enable_laser_offset: Option<bool>,
    #[serde(default)]
    swap_xy: Option<bool>,
    #[serde(default)]
    selected_camera_id: Option<Option<String>>,
    #[serde(default)]
    camera_calibration: Option<Option<CameraCalibration>>,
    #[serde(default)]
    camera_alignment: Option<Option<CameraAlignment>>,
    #[serde(default)]
    job_checklist: Option<bool>,
    #[serde(default)]
    frame_continuously: Option<bool>,
    #[serde(default)]
    laser_on_when_framing: Option<bool>,
    tab_pulse_width_ms: Option<f64>,
    #[serde(default)]
    cnc_machine: Option<bool>,
    #[serde(default)]
    use_constant_power: Option<bool>,
    #[serde(default)]
    emit_s_every_g1: Option<bool>,
    s_value_max: Option<u32>,
    #[serde(default)]
    use_g0_for_overscan: Option<bool>,
    air_assist_on_gcode: Option<String>,
    air_assist_off_gcode: Option<String>,
    air_assist_on_delay_ms: Option<u32>,
    job_header_gcode: Option<String>,
    job_footer_gcode: Option<String>,
    transfer_mode: Option<TransferMode>,
    #[serde(default)]
    preferred_default_origin: Option<Option<WorkspaceOrigin>>,
    #[serde(default)]
    scanning_offsets: Option<Vec<ScanningOffsetEntry>>,
    #[serde(default)]
    enable_scanning_offset: Option<bool>,
    dot_width_mm: Option<f64>,
    #[serde(default)]
    enable_dot_width: Option<bool>,
    ruida_table_axis: Option<RuidaTableAxis>,
    #[serde(default)]
    enable_laser_fire_button: Option<bool>,
    default_fire_power_percent: Option<f64>,
}

#[derive(Deserialize)]
struct ActiveProfileRequest {
    profile_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BootstrapProfileRequest {
    candidate_id: String,
    #[serde(default)]
    profile_name: Option<String>,
    #[serde(default)]
    activate: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct ApplyPresetRequest {
    preset_id: String,
    #[serde(default)]
    confirm_diff: bool,
}

fn default_bed_width() -> f64 {
    200.0
}
fn default_bed_height() -> f64 {
    200.0
}
fn default_max_speed() -> f64 {
    3000.0
}
fn default_max_power() -> f64 {
    100.0
}
fn default_baud_rate() -> u32 {
    115200
}
fn default_firmware_type() -> String {
    "grbl".to_string()
}

fn parse_profile_id(id: &str) -> Result<MachineProfileId, ApiError> {
    let uuid = uuid::Uuid::parse_str(id).map_err(|e| {
        map_service_error(beambench_service::ServiceError::invalid_input(format!(
            "Invalid profile ID: {e}"
        )))
    })?;
    Ok(Id::from_uuid(uuid))
}

fn create_save_input(
    body: SaveProfileRequest,
    profile_id: Option<MachineProfileId>,
) -> Result<SaveProfileInput, ApiError> {
    let defaults = beambench_core::MachineProfile::default();
    let name = body.name.ok_or_else(|| {
        map_service_error(beambench_service::ServiceError::invalid_input(
            "Profile name is required",
        ))
    })?;
    Ok(SaveProfileInput {
        profile_id,
        name,
        preset_id: body.preset_id.unwrap_or(defaults.preset_id),
        preset_version: body.preset_version.unwrap_or(defaults.preset_version),
        bed_width_mm: body.bed_width_mm.unwrap_or(default_bed_width()),
        bed_height_mm: body.bed_height_mm.unwrap_or(default_bed_height()),
        max_speed_mm_min: body.max_speed_mm_min.unwrap_or(default_max_speed()),
        max_power_percent: body.max_power_percent.unwrap_or(default_max_power()),
        homing_enabled: body.homing_enabled.unwrap_or(defaults.homing_enabled),
        default_baud_rate: body.default_baud_rate.unwrap_or(default_baud_rate()),
        firmware_type: body.firmware_type.unwrap_or_else(default_firmware_type),
        notes: body.notes.unwrap_or_default(),
        selected_camera_id: body.selected_camera_id.unwrap_or(None),
        camera_calibration: body.camera_calibration.unwrap_or(None),
        camera_alignment: body.camera_alignment.unwrap_or(None),
        origin: body.origin.unwrap_or(defaults.origin),
        laser_offset_x: body.laser_offset_x.unwrap_or(defaults.laser_offset_x),
        laser_offset_y: body.laser_offset_y.unwrap_or(defaults.laser_offset_y),
        enable_laser_offset: body
            .enable_laser_offset
            .unwrap_or(defaults.enable_laser_offset),
        swap_xy: body.swap_xy.unwrap_or(defaults.swap_xy),
        job_checklist: body.job_checklist.unwrap_or(defaults.job_checklist),
        frame_continuously: body
            .frame_continuously
            .unwrap_or(defaults.frame_continuously),
        laser_on_when_framing: body
            .laser_on_when_framing
            .unwrap_or(defaults.laser_on_when_framing),
        tab_pulse_width_ms: body
            .tab_pulse_width_ms
            .unwrap_or(defaults.tab_pulse_width_ms),
        cnc_machine: body.cnc_machine.unwrap_or(defaults.cnc_machine),
        use_constant_power: body
            .use_constant_power
            .unwrap_or(defaults.use_constant_power),
        emit_s_every_g1: body.emit_s_every_g1.unwrap_or(defaults.emit_s_every_g1),
        s_value_max: body.s_value_max.unwrap_or(defaults.s_value_max),
        use_g0_for_overscan: body
            .use_g0_for_overscan
            .unwrap_or(defaults.use_g0_for_overscan),
        air_assist_on_gcode: body
            .air_assist_on_gcode
            .unwrap_or(defaults.air_assist_on_gcode),
        air_assist_off_gcode: body
            .air_assist_off_gcode
            .unwrap_or(defaults.air_assist_off_gcode),
        air_assist_on_delay_ms: body
            .air_assist_on_delay_ms
            .unwrap_or(defaults.air_assist_on_delay_ms),
        job_header_gcode: body.job_header_gcode.unwrap_or(defaults.job_header_gcode),
        job_footer_gcode: body.job_footer_gcode.unwrap_or(defaults.job_footer_gcode),
        transfer_mode: body.transfer_mode.unwrap_or(defaults.transfer_mode),
        preferred_default_origin: body
            .preferred_default_origin
            .unwrap_or(defaults.preferred_default_origin),
        scanning_offsets: body
            .scanning_offsets
            .unwrap_or_else(|| defaults.scanning_offsets.clone()),
        enable_scanning_offset: body
            .enable_scanning_offset
            .unwrap_or(defaults.enable_scanning_offset),
        dot_width_mm: body.dot_width_mm.unwrap_or(defaults.dot_width_mm),
        enable_dot_width: body.enable_dot_width.unwrap_or(defaults.enable_dot_width),
        supports_z_moves: defaults.supports_z_moves,
        z_move_feed_mm_min: defaults.z_move_feed_mm_min,
        ruida_table_axis: body.ruida_table_axis.unwrap_or(defaults.ruida_table_axis),
        enable_laser_fire_button: body
            .enable_laser_fire_button
            .unwrap_or(defaults.enable_laser_fire_button),
        default_fire_power_percent: body
            .default_fire_power_percent
            .unwrap_or(defaults.default_fire_power_percent),
        quality_test_settings: defaults.quality_test_settings.clone(),
    })
}

fn merge_save_input(
    existing: beambench_core::MachineProfile,
    body: SaveProfileRequest,
) -> SaveProfileInput {
    SaveProfileInput {
        profile_id: Some(existing.id),
        name: body.name.unwrap_or(existing.name),
        preset_id: body.preset_id.unwrap_or(existing.preset_id),
        preset_version: body.preset_version.unwrap_or(existing.preset_version),
        bed_width_mm: body.bed_width_mm.unwrap_or(existing.bed_width_mm),
        bed_height_mm: body.bed_height_mm.unwrap_or(existing.bed_height_mm),
        max_speed_mm_min: body.max_speed_mm_min.unwrap_or(existing.max_speed_mm_min),
        max_power_percent: body.max_power_percent.unwrap_or(existing.max_power_percent),
        homing_enabled: body.homing_enabled.unwrap_or(existing.homing_enabled),
        default_baud_rate: body.default_baud_rate.unwrap_or(existing.default_baud_rate),
        firmware_type: body.firmware_type.unwrap_or(existing.firmware_type),
        notes: body.notes.unwrap_or(existing.notes),
        selected_camera_id: body
            .selected_camera_id
            .unwrap_or(existing.selected_camera_id),
        camera_calibration: body
            .camera_calibration
            .unwrap_or(existing.camera_calibration),
        camera_alignment: body.camera_alignment.unwrap_or(existing.camera_alignment),
        origin: body.origin.unwrap_or(existing.origin),
        laser_offset_x: body.laser_offset_x.unwrap_or(existing.laser_offset_x),
        laser_offset_y: body.laser_offset_y.unwrap_or(existing.laser_offset_y),
        enable_laser_offset: body
            .enable_laser_offset
            .unwrap_or(existing.enable_laser_offset),
        swap_xy: body.swap_xy.unwrap_or(existing.swap_xy),
        job_checklist: body.job_checklist.unwrap_or(existing.job_checklist),
        frame_continuously: body
            .frame_continuously
            .unwrap_or(existing.frame_continuously),
        laser_on_when_framing: body
            .laser_on_when_framing
            .unwrap_or(existing.laser_on_when_framing),
        tab_pulse_width_ms: body
            .tab_pulse_width_ms
            .unwrap_or(existing.tab_pulse_width_ms),
        cnc_machine: body.cnc_machine.unwrap_or(existing.cnc_machine),
        use_constant_power: body
            .use_constant_power
            .unwrap_or(existing.use_constant_power),
        emit_s_every_g1: body.emit_s_every_g1.unwrap_or(existing.emit_s_every_g1),
        s_value_max: body.s_value_max.unwrap_or(existing.s_value_max),
        use_g0_for_overscan: body
            .use_g0_for_overscan
            .unwrap_or(existing.use_g0_for_overscan),
        air_assist_on_gcode: body
            .air_assist_on_gcode
            .unwrap_or(existing.air_assist_on_gcode),
        air_assist_off_gcode: body
            .air_assist_off_gcode
            .unwrap_or(existing.air_assist_off_gcode),
        air_assist_on_delay_ms: body
            .air_assist_on_delay_ms
            .unwrap_or(existing.air_assist_on_delay_ms),
        job_header_gcode: body.job_header_gcode.unwrap_or(existing.job_header_gcode),
        job_footer_gcode: body.job_footer_gcode.unwrap_or(existing.job_footer_gcode),
        transfer_mode: body.transfer_mode.unwrap_or(existing.transfer_mode),
        preferred_default_origin: body
            .preferred_default_origin
            .unwrap_or(existing.preferred_default_origin),
        scanning_offsets: body.scanning_offsets.unwrap_or(existing.scanning_offsets),
        enable_scanning_offset: body
            .enable_scanning_offset
            .unwrap_or(existing.enable_scanning_offset),
        dot_width_mm: body.dot_width_mm.unwrap_or(existing.dot_width_mm),
        enable_dot_width: body.enable_dot_width.unwrap_or(existing.enable_dot_width),
        supports_z_moves: existing.supports_z_moves,
        z_move_feed_mm_min: existing.z_move_feed_mm_min,
        ruida_table_axis: body.ruida_table_axis.unwrap_or(existing.ruida_table_axis),
        enable_laser_fire_button: body
            .enable_laser_fire_button
            .unwrap_or(existing.enable_laser_fire_button),
        default_fire_power_percent: body
            .default_fire_power_percent
            .unwrap_or(existing.default_fire_power_percent),
        quality_test_settings: existing.quality_test_settings,
    }
}

async fn list_presets() -> Result<Json<serde_json::Value>, ApiError> {
    let presets = profiles::profile_presets();
    Ok(Json(serde_json::json!({
        "presets": presets,
        "count": presets.len(),
    })))
}

async fn profile_suggestion(
    State(ctx): State<Arc<ServiceContext>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let suggestion = profiles::suggest_profile_preset(&ctx).map_err(map_service_error)?;
    Ok(Json(serde_json::to_value(suggestion).unwrap_or_default()))
}

async fn preset_diff(
    State(ctx): State<Arc<ServiceContext>>,
    Path((id, preset_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let profile_id = parse_profile_id(&id)?;
    let diff =
        profiles::profile_preset_diff(&ctx, profile_id, &preset_id).map_err(map_service_error)?;
    Ok(Json(serde_json::json!({ "diff": diff })))
}

async fn apply_preset(
    State(ctx): State<Arc<ServiceContext>>,
    Path(id): Path<String>,
    Json(body): Json<ApplyPresetRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let profile_id = parse_profile_id(&id)?;
    let diff = profiles::profile_preset_diff(&ctx, profile_id, &body.preset_id)
        .map_err(map_service_error)?;
    if !body.confirm_diff {
        return Err((
            StatusCode::PRECONDITION_REQUIRED,
            Json(serde_json::json!({
                "error_code": "CONFIRMATION_REQUIRED",
                "missing": ["confirm_diff"],
                "message": "Applying this preset changes profile fields and requires explicit confirmation.",
                "diff": diff,
            })),
        ));
    }
    let result = profiles::apply_profile_preset(&ctx, profile_id, &body.preset_id)
        .map_err(map_service_error)?;
    Ok(Json(serde_json::to_value(result).unwrap_or_default()))
}

async fn list_profiles(
    State(ctx): State<Arc<ServiceContext>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let profile_list = profiles::list_profiles(&ctx).map_err(map_service_error)?;
    let active_profile_id = profiles::get_active_profile_id(&ctx)
        .map_err(map_service_error)?
        .map(|id| id.to_string());
    Ok(Json(serde_json::json!({
        "profiles": profile_list,
        "count": profile_list.len(),
        "active_profile_id": active_profile_id,
    })))
}

async fn get_profile(
    State(ctx): State<Arc<ServiceContext>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let profile = profiles::get_profile(&ctx, ProfileLookup::Id(parse_profile_id(&id)?))
        .map_err(map_service_error)?;
    Ok(Json(serde_json::to_value(profile).unwrap_or_default()))
}

async fn create_profile(
    State(ctx): State<Arc<ServiceContext>>,
    Json(body): Json<SaveProfileRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let saved =
        profiles::save_profile(&ctx, create_save_input(body, None)?).map_err(map_service_error)?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::to_value(saved).unwrap_or_default()),
    ))
}

async fn update_profile(
    State(ctx): State<Arc<ServiceContext>>,
    Path(id): Path<String>,
    Json(body): Json<SaveProfileRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let profile_id = parse_profile_id(&id)?;
    let existing =
        profiles::get_profile(&ctx, ProfileLookup::Id(profile_id)).map_err(map_service_error)?;
    let saved = profiles::save_profile(&ctx, merge_save_input(existing, body))
        .map_err(map_service_error)?;
    Ok(Json(serde_json::to_value(saved).unwrap_or_default()))
}

async fn set_active(
    State(ctx): State<Arc<ServiceContext>>,
    Json(body): Json<ActiveProfileRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let requested_id = body.profile_id.clone();
    let profile_id = match body.profile_id {
        Some(id) => Some(parse_profile_id(&id)?),
        None => None,
    };
    profiles::set_active_profile(&ctx, profile_id).map_err(map_service_error)?;
    Ok(Json(
        serde_json::json!({ "active_profile_id": requested_id }),
    ))
}

async fn bootstrap_profile(
    State(ctx): State<Arc<ServiceContext>>,
    Json(body): Json<BootstrapProfileRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let profile = discovery::bootstrap_profile(
        &ctx,
        discovery::BootstrapProfileInput {
            candidate_id: body.candidate_id,
            profile_name: body.profile_name,
            activate: body.activate.unwrap_or(true),
        },
    )
    .map_err(map_service_error)?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "profile": profile,
            "activated": body.activate.unwrap_or(true),
        })),
    ))
}

async fn delete_profile(
    State(ctx): State<Arc<ServiceContext>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    profiles::delete_profile(&ctx, parse_profile_id(&id)?).map_err(map_service_error)?;
    Ok(Json(serde_json::json!({ "deleted": true })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;
    use beambench_grbl::GrblSession;
    use beambench_serial::MockSerialTransport;
    use tower::ServiceExt;

    async fn collect_body(response: axum::http::Response<axum::body::Body>) -> Vec<u8> {
        use http_body_util::BodyExt;
        let body = response.into_body();
        let collected = body.collect().await.unwrap();
        collected.to_bytes().to_vec()
    }

    fn install_grbl_session_with_info(ctx: &ServiceContext, lines: &[&str]) {
        let mut transport = MockSerialTransport::new("mock");
        for line in lines {
            transport.enqueue_response(line);
        }
        let mut session = GrblSession::new(Box::new(transport));
        session.connect().unwrap();
        session.poll().unwrap();
        *ctx.session.lock().unwrap() = Some(
            beambench_service::runtime::MachineSessionHandle::Grbl(session.into()),
        );
    }

    #[tokio::test]
    async fn list_profiles_empty() {
        let ctx = Arc::new(ServiceContext::with_settings(
            beambench_core::AppSettings::default(),
        ));
        let app = crate::routes::build_router(ctx);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/v1/profiles")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = collect_body(response).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["count"], 0);
        assert!(json["profiles"].as_array().unwrap().is_empty());
        assert!(json["active_profile_id"].is_null());
    }

    #[tokio::test]
    async fn create_get_update_activate_delete_profile_roundtrip() {
        let ctx = Arc::new(ServiceContext::with_settings(
            beambench_core::AppSettings::default(),
        ));
        let app = crate::routes::build_router(ctx.clone());

        let response = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/profiles")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        r#"{"name":"My Laser","bed_width_mm":400.0}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);

        let body = collect_body(response).await;
        let created: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let profile_id = created["id"].as_str().unwrap().to_string();

        let get_response = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .uri(format!("/api/v1/profiles/{profile_id}"))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(get_response.status(), StatusCode::OK);

        let update_response = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("PATCH")
                    .uri(format!("/api/v1/profiles/{profile_id}"))
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        r#"{"name":"Updated Laser","default_baud_rate":230400}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(update_response.status(), StatusCode::OK);

        let update_body = collect_body(update_response).await;
        let updated: serde_json::Value = serde_json::from_slice(&update_body).unwrap();
        assert_eq!(updated["id"], profile_id);
        assert_eq!(updated["name"], "Updated Laser");
        assert_eq!(updated["default_baud_rate"], 230400);
        assert_eq!(updated["bed_width_mm"], 400.0);

        let active_response = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/profiles/active")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(format!(
                        r#"{{"profile_id":"{profile_id}"}}"#
                    )))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(active_response.status(), StatusCode::OK);

        let list_response = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/v1/profiles")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let list_body = collect_body(list_response).await;
        let listed: serde_json::Value = serde_json::from_slice(&list_body).unwrap();
        assert_eq!(listed["active_profile_id"], profile_id);

        let delete_response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("DELETE")
                    .uri(format!("/api/v1/profiles/{profile_id}"))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(delete_response.status(), StatusCode::OK);

        assert!(ctx.settings.lock().unwrap().active_profile_id.is_none());
    }

    #[tokio::test]
    async fn create_and_update_profile_roundtrip_phase5_fields() {
        let ctx = Arc::new(ServiceContext::with_settings(
            beambench_core::AppSettings::default(),
        ));
        let app = crate::routes::build_router(ctx);

        let response = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/profiles")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        r#"{
                          "name":"Phase5",
                          "use_constant_power":true,
                          "emit_s_every_g1":true,
                          "use_g0_for_overscan":true,
                          "enable_scanning_offset":true,
                          "scanning_offsets":[{"speed_mm_min":1000.0,"offset_mm":0.1}],
                          "dot_width_mm":0.15,
                          "enable_dot_width":true
                        }"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);

        let body = collect_body(response).await;
        let created: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let profile_id = created["id"].as_str().unwrap().to_string();
        assert_eq!(created["use_constant_power"], true);
        assert_eq!(created["emit_s_every_g1"], true);
        assert_eq!(created["use_g0_for_overscan"], true);
        assert_eq!(created["enable_scanning_offset"], true);
        assert_eq!(created["scanning_offsets"][0]["speed_mm_min"], 1000.0);
        assert_eq!(created["dot_width_mm"], 0.15);
        assert_eq!(created["enable_dot_width"], true);

        let update_response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("PATCH")
                    .uri(format!("/api/v1/profiles/{profile_id}"))
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        r#"{
                          "use_constant_power":false,
                          "emit_s_every_g1":false,
                          "scanning_offsets":[
                            {"speed_mm_min":2000.0,"offset_mm":0.2},
                            {"speed_mm_min":500.0,"offset_mm":0.05}
                          ],
                          "dot_width_mm":0.2
                        }"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(update_response.status(), StatusCode::OK);

        let update_body = collect_body(update_response).await;
        let updated: serde_json::Value = serde_json::from_slice(&update_body).unwrap();
        assert_eq!(updated["use_constant_power"], false);
        assert_eq!(updated["emit_s_every_g1"], false);
        assert_eq!(updated["use_g0_for_overscan"], true);
        assert_eq!(updated["scanning_offsets"][0]["speed_mm_min"], 500.0);
        assert_eq!(updated["scanning_offsets"][1]["speed_mm_min"], 2000.0);
        assert_eq!(updated["dot_width_mm"], 0.2);
        assert_eq!(updated["enable_dot_width"], true);
    }

    #[tokio::test]
    async fn profile_presets_diff_and_apply_require_confirmation() {
        let ctx = Arc::new(ServiceContext::with_settings(
            beambench_core::AppSettings::default(),
        ));
        let app = crate::routes::build_router(ctx.clone());

        let create_response = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/profiles")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(r#"{"name":"Preset Test"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(create_response.status(), StatusCode::CREATED);
        let created: serde_json::Value =
            serde_json::from_slice(&collect_body(create_response).await).unwrap();
        let profile_id = created["id"].as_str().unwrap();

        let presets_response = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/v1/profiles/presets")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(presets_response.status(), StatusCode::OK);

        let diff_response = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .uri(format!(
                        "/api/v1/profiles/{profile_id}/preset-diff/sculpfun_s30_pro_max_20w"
                    ))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(diff_response.status(), StatusCode::OK);
        let diff: serde_json::Value =
            serde_json::from_slice(&collect_body(diff_response).await).unwrap();
        assert!(
            diff["diff"]
                .as_array()
                .unwrap()
                .iter()
                .any(|entry| { entry["field"] == "air_assist_on_gcode" && entry["new"] == "M8" })
        );

        let rejected = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri(format!("/api/v1/profiles/{profile_id}/preset"))
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        r#"{"preset_id":"sculpfun_s30_pro_max_20w"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(rejected.status(), StatusCode::PRECONDITION_REQUIRED);
        let rejected_body: serde_json::Value =
            serde_json::from_slice(&collect_body(rejected).await).unwrap();
        assert_eq!(rejected_body["error_code"], "CONFIRMATION_REQUIRED");
        assert_eq!(rejected_body["missing"][0], "confirm_diff");
        assert!(rejected_body["diff"].as_array().unwrap().len() > 0);

        let applied = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri(format!("/api/v1/profiles/{profile_id}/preset"))
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        r#"{"preset_id":"sculpfun_s30_pro_max_20w","confirm_diff":true}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(applied.status(), StatusCode::OK);
        let applied_body: serde_json::Value =
            serde_json::from_slice(&collect_body(applied).await).unwrap();
        assert_eq!(applied_body["applied"], true);
        assert_eq!(applied_body["profile"]["air_assist_on_gcode"], "M8");
    }

    #[tokio::test]
    async fn profile_suggestion_route_reports_reason_shapes() {
        let ctx = Arc::new(ServiceContext::with_settings(
            beambench_core::AppSettings::default(),
        ));
        let app = crate::routes::build_router(ctx.clone());

        let disconnected = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/v1/profiles/suggestion")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(disconnected.status(), StatusCode::OK);
        let disconnected_body: serde_json::Value =
            serde_json::from_slice(&collect_body(disconnected).await).unwrap();
        assert!(disconnected_body["suggestion"].is_null());
        assert_eq!(disconnected_body["reason"], "not_connected");

        install_grbl_session_with_info(
            &ctx,
            &[
                "Grbl 1.1h ['$' for help]",
                "[VER:Sculpfun S30 Pro Max 20W:]",
            ],
        );

        let matched = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/v1/profiles/suggestion")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(matched.status(), StatusCode::OK);
        let matched_body: serde_json::Value =
            serde_json::from_slice(&collect_body(matched).await).unwrap();
        assert_eq!(matched_body["suggestion"], "sculpfun_s30_pro_max_20w");
        assert_eq!(matched_body["reason"], "matched");
    }

    #[tokio::test]
    async fn set_active_missing_profile_returns_not_found() {
        let ctx = Arc::new(ServiceContext::with_settings(
            beambench_core::AppSettings::default(),
        ));
        let app = crate::routes::build_router(ctx);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/profiles/active")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(format!(
                        r#"{{"profile_id":"{}"}}"#,
                        uuid::Uuid::new_v4()
                    )))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}

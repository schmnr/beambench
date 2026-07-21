use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use beambench_common::controller_choice::{
    ControllerConnectionResult, ControllerDriverId, ControllerMismatchDecision, ControllerSelection,
};
use beambench_common::machine::{DiscoveryTcpTarget, DiscoveryUsbTarget, MachineConnectionTarget};
use beambench_service::ops::discovery::{self, StartDiscoveryInput};
use beambench_service::ops::machine::{self, ConnectMachineInput, JogMachineInput};
use beambench_service::{ServiceContext, agent};
use serde::Deserialize;

use crate::response::{ApiError, confirmation_required, map_service_error};

const CONTROLLER_CONNECTION_CHALLENGE_EXPIRY: Duration = Duration::from_secs(121);

pub fn router() -> Router<Arc<ServiceContext>> {
    Router::new()
        .route("/status", get(get_status))
        .route("/session", get(get_session))
        .route("/discover", get(get_discovery).post(start_discovery))
        .route("/discover/cancel", post(cancel_discovery))
        .route("/connect", post(connect))
        .route(
            "/connect/controller/serial",
            post(connect_controller_serial),
        )
        .route(
            "/connect/controller/network",
            post(connect_controller_network),
        )
        .route(
            "/connect/controller/continue",
            post(continue_controller_connection),
        )
        .route("/connect/usb", post(connect_usb))
        .route("/usb/lihuiyu", get(list_lihuiyu_usb_devices))
        .route("/disconnect", post(disconnect))
        .route("/home", post(home))
        .route("/unlock", post(unlock))
        .route("/jog", post(jog))
        .route("/frame", post(frame))
        .route("/test-air", post(test_air))
        .route("/emergency-stop", post(emergency_stop))
        .route("/set-origin", post(set_origin))
        .route("/reset-origin", post(reset_origin))
        .route("/ports", get(list_ports))
        .route("/overrides/feed", post(set_feed_override))
        .route("/overrides/spindle", post(set_spindle_override))
        .route("/overrides/reset", post(reset_overrides))
        .route("/console", get(get_console_log))
        .route("/send", post(send_gcode))
}

#[derive(Deserialize)]
struct ConnectBody {
    #[serde(default)]
    port: Option<String>,
    #[serde(default)]
    baud_rate: Option<u32>,
    #[serde(default)]
    candidate_id: Option<String>,
}

#[derive(Deserialize)]
struct UsbConnectBody {
    bus_id: String,
    device_address: u8,
    #[serde(default)]
    port_numbers: Vec<u8>,
}

#[derive(Deserialize)]
struct SerialControllerConnectBody {
    port_name: String,
    #[serde(default)]
    baud_rate: Option<u32>,
    #[serde(default)]
    selection: ControllerSelection,
}

#[derive(Deserialize)]
struct NetworkControllerConnectBody {
    host: String,
    #[serde(default)]
    port: Option<u16>,
    #[serde(default)]
    selection: ControllerSelection,
}

#[derive(Deserialize)]
struct ContinueControllerConnectionBody {
    attempt_id: String,
    selection: ControllerSelection,
    #[serde(default)]
    decision: Option<ControllerMismatchDecision>,
}

#[derive(Debug, Deserialize, Default)]
struct DiscoverBody {
    #[serde(default)]
    tcp_targets: Vec<DiscoveryTcpTarget>,
    #[serde(default)]
    usb_targets: Vec<DiscoveryUsbTarget>,
}

#[derive(Deserialize)]
struct JogBody {
    x_mm: f64,
    y_mm: f64,
    #[serde(default)]
    z_mm: Option<f64>,
    feed_rate: f64,
    #[serde(default)]
    continuous: bool,
    #[serde(default)]
    confirm_motion: bool,
}

#[derive(Deserialize, Default)]
struct ConfirmationBody {
    #[serde(default)]
    confirm_motion: bool,
}

#[derive(Deserialize, Default)]
struct FrameBody {
    #[serde(default)]
    confirm_motion: bool,
    #[serde(default)]
    confirm_laser_on: bool,
    #[serde(default)]
    laser_on_override: bool,
    #[serde(default)]
    feed_rate: Option<f64>,
}

#[derive(Deserialize, Default)]
struct TestAirBody {
    #[serde(default = "default_test_air_duration_ms")]
    duration_ms: u64,
    #[serde(default)]
    confirm_air_assist: bool,
}

fn default_test_air_duration_ms() -> u64 {
    1000
}

#[derive(Deserialize)]
struct OverrideBody {
    action: String,
}

async fn get_status(
    State(ctx): State<Arc<ServiceContext>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let runtime = machine::runtime_state(&ctx).map_err(map_service_error)?;
    Ok(Json(serde_json::json!({
        "connected": runtime.session_state != beambench_common::machine::SessionState::Disconnected,
        "controller_family": runtime.controller_family,
        "controller_model": runtime.controller_model,
        "controller_driver": runtime.controller_driver,
        "controller_selection": runtime.controller_selection,
        "detected_controller_identity": runtime.detected_controller_identity,
        "experimental_mode": runtime.experimental_mode,
        "product_tier": runtime.product_tier,
        "evidence_state": runtime.evidence_state,
        "transport_kind": runtime.transport_kind,
        "capabilities": runtime.capabilities,
        "controller_info": runtime.controller_info,
        "session_state": runtime.session_state,
        "machine_status": runtime.machine_status,
        "machine_coordinates_valid": runtime.machine_coordinates_valid,
        "job_active": runtime.job_active,
        "job_progress": runtime.job_progress,
    })))
}

async fn get_discovery(
    State(ctx): State<Arc<ServiceContext>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let state = discovery::get_discovery_state(&ctx).map_err(map_service_error)?;
    Ok(Json(serde_json::to_value(state).unwrap_or_default()))
}

async fn start_discovery(
    State(ctx): State<Arc<ServiceContext>>,
    Json(body): Json<DiscoverBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let state = discovery::start_discovery(
        &ctx,
        StartDiscoveryInput {
            tcp_targets: body.tcp_targets,
            usb_targets: body.usb_targets,
        },
    )
    .map_err(map_service_error)?;
    Ok(Json(serde_json::to_value(state).unwrap_or_default()))
}

async fn cancel_discovery(
    State(ctx): State<Arc<ServiceContext>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let state = discovery::cancel_discovery(&ctx).map_err(map_service_error)?;
    Ok(Json(serde_json::to_value(state).unwrap_or_default()))
}

async fn get_session(
    State(ctx): State<Arc<ServiceContext>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let runtime = machine::runtime_state(&ctx).map_err(map_service_error)?;
    Ok(Json(serde_json::to_value(runtime).unwrap_or_default()))
}

async fn connect(
    State(ctx): State<Arc<ServiceContext>>,
    Json(body): Json<ConnectBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let target = match (body.candidate_id, body.port, body.baud_rate) {
        (Some(candidate_id), _, _) => MachineConnectionTarget::DiscoveryCandidate { candidate_id },
        (None, Some(port), baud_rate) => MachineConnectionTarget::GrblSerial {
            port_name: port,
            baud_rate: baud_rate.unwrap_or(115200),
        },
        _ => {
            return Err(map_service_error(
                beambench_service::ServiceError::invalid_input(
                    "Expected either candidate_id or port/baud_rate",
                ),
            ));
        }
    };
    let state = machine::connect_machine(&ctx, ConnectMachineInput { target })
        .map_err(map_service_error)?;

    let runtime = machine::runtime_state(&ctx).map_err(map_service_error)?;

    Ok(Json(serde_json::json!({
        "connected": true,
        "session_state": state,
        "controller_family": runtime.controller_family,
        "controller_model": runtime.controller_model,
        "transport_kind": runtime.transport_kind,
        "capabilities": runtime.capabilities,
    })))
}

fn schedule_controller_connection_expiry(
    ctx: Arc<ServiceContext>,
    result: &ControllerConnectionResult,
) {
    let ControllerConnectionResult::Challenge { attempt_id, .. } = result else {
        return;
    };
    let attempt_id = attempt_id.clone();
    tokio::spawn(async move {
        tokio::time::sleep(CONTROLLER_CONNECTION_CHALLENGE_EXPIRY).await;
        if let Err(error) = machine::expire_controller_connection(&ctx, &attempt_id) {
            tracing::warn!(%error, "Failed to expire API controller connection challenge");
        }
    });
}

fn controller_connection_json(
    result: ControllerConnectionResult,
) -> Result<Json<serde_json::Value>, ApiError> {
    serde_json::to_value(result).map(Json).map_err(|error| {
        map_service_error(beambench_service::ServiceError::internal(format!(
            "Serializing controller connection result failed: {error}"
        )))
    })
}

async fn connect_controller_serial(
    State(ctx): State<Arc<ServiceContext>>,
    Json(body): Json<SerialControllerConnectBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let connection_ctx = ctx.clone();
    let result = tokio::task::spawn_blocking(move || {
        machine::begin_controller_connection(
            &connection_ctx,
            machine::BeginControllerConnectionInput {
                port_name: body.port_name,
                baud_rate: body.baud_rate.unwrap_or(115200),
                selection: body.selection,
            },
        )
    })
    .await
    .map_err(|error| {
        map_service_error(beambench_service::ServiceError::internal(format!(
            "Serial controller connection task failed: {error}"
        )))
    })?
    .map_err(map_service_error)?;
    schedule_controller_connection_expiry(ctx, &result);
    controller_connection_json(result)
}

async fn connect_controller_network(
    State(ctx): State<Arc<ServiceContext>>,
    Json(body): Json<NetworkControllerConnectBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let port = body.port.unwrap_or_else(|| {
        if matches!(
            &body.selection,
            ControllerSelection::KnownDriver {
                driver: ControllerDriverId::Ruida
            }
        ) {
            50200
        } else {
            23
        }
    });
    let connection_ctx = ctx.clone();
    let result = tokio::task::spawn_blocking(move || {
        machine::begin_network_controller_connection(
            &connection_ctx,
            machine::BeginNetworkControllerConnectionInput {
                host: body.host,
                port,
                selection: body.selection,
            },
        )
    })
    .await
    .map_err(|error| {
        map_service_error(beambench_service::ServiceError::internal(format!(
            "Network controller connection task failed: {error}"
        )))
    })?
    .map_err(map_service_error)?;
    schedule_controller_connection_expiry(ctx, &result);
    controller_connection_json(result)
}

async fn continue_controller_connection(
    State(ctx): State<Arc<ServiceContext>>,
    Json(body): Json<ContinueControllerConnectionBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let connection_ctx = ctx.clone();
    let result = tokio::task::spawn_blocking(move || {
        machine::continue_controller_connection(
            &connection_ctx,
            machine::ContinueControllerConnectionInput {
                attempt_id: body.attempt_id,
                selection: body.selection,
                decision: body.decision,
            },
        )
    })
    .await
    .map_err(|error| {
        map_service_error(beambench_service::ServiceError::internal(format!(
            "Controller connection continuation task failed: {error}"
        )))
    })?
    .map_err(map_service_error)?;
    schedule_controller_connection_expiry(ctx, &result);
    controller_connection_json(result)
}

async fn list_lihuiyu_usb_devices() -> Result<Json<serde_json::Value>, ApiError> {
    let devices = tokio::task::spawn_blocking(machine::list_lihuiyu_usb_devices)
        .await
        .map_err(|error| {
            map_service_error(beambench_service::ServiceError::internal(format!(
                "USB controller enumeration task failed: {error}"
            )))
        })?
        .map_err(map_service_error)?;
    Ok(Json(serde_json::to_value(devices).unwrap_or_default()))
}

async fn connect_usb(
    State(ctx): State<Arc<ServiceContext>>,
    Json(body): Json<UsbConnectBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let result = tokio::task::spawn_blocking(move || {
        machine::begin_usb_controller_connection(
            &ctx,
            machine::BeginUsbControllerConnectionInput {
                bus_id: body.bus_id,
                device_address: body.device_address,
                port_numbers: body.port_numbers,
                selection: ControllerSelection::KnownDriver {
                    driver: ControllerDriverId::Lihuiyu,
                },
            },
        )
    })
    .await
    .map_err(|error| {
        map_service_error(beambench_service::ServiceError::internal(format!(
            "USB controller connection task failed: {error}"
        )))
    })?
    .map_err(map_service_error)?;
    controller_connection_json(result)
}

async fn disconnect(
    State(ctx): State<Arc<ServiceContext>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    machine::disconnect_machine(&ctx).map_err(map_service_error)?;
    Ok(Json(serde_json::json!({ "disconnected": true })))
}

async fn home(
    State(ctx): State<Arc<ServiceContext>>,
    body: Option<Json<ConfirmationBody>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let body = body.map(|Json(body)| body).unwrap_or_default();
    if !body.confirm_motion {
        return Err(confirmation_required(&["confirm_motion"]));
    }
    machine::home(&ctx).map_err(map_service_error)?;
    Ok(Json(serde_json::json!({ "homing": true })))
}

async fn unlock(
    State(ctx): State<Arc<ServiceContext>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    machine::unlock(&ctx).map_err(map_service_error)?;
    Ok(Json(serde_json::json!({ "unlocked": true })))
}

async fn jog(
    State(ctx): State<Arc<ServiceContext>>,
    Json(body): Json<JogBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if !body.confirm_motion {
        return Err(confirmation_required(&["confirm_motion"]));
    }
    machine::jog(
        &ctx,
        JogMachineInput {
            x_mm: body.x_mm,
            y_mm: body.y_mm,
            z_mm: body.z_mm,
            feed_rate: body.feed_rate,
            continuous: body.continuous,
        },
    )
    .map_err(map_service_error)?;
    Ok(Json(serde_json::json!({ "jogging": true })))
}

async fn frame(
    State(ctx): State<Arc<ServiceContext>>,
    body: Option<Json<FrameBody>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let body = body.map(|Json(body)| body).unwrap_or_default();
    if !body.confirm_motion {
        return Err(confirmation_required(&["confirm_motion"]));
    }
    if body.laser_on_override && !body.confirm_laser_on {
        return Err(confirmation_required(&["confirm_laser_on"]));
    }
    let progress = machine::frame_job(
        &ctx,
        "rectangular",
        &[],
        body.laser_on_override,
        body.feed_rate,
    )
    .map_err(map_service_error)?;
    Ok(Json(serde_json::json!({
        "framing": true,
        "progress": progress,
    })))
}

async fn test_air(
    State(ctx): State<Arc<ServiceContext>>,
    body: Option<Json<TestAirBody>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let body = body.map(|Json(body)| body).unwrap_or_default();
    if !body.confirm_air_assist {
        return Err(confirmation_required(&["confirm_air_assist"]));
    }
    let result = machine::test_air_assist(&ctx, body.duration_ms).map_err(map_service_error)?;
    Ok(Json(serde_json::to_value(result).unwrap_or_default()))
}

async fn emergency_stop(
    State(ctx): State<Arc<ServiceContext>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    machine::emergency_stop(&ctx).map_err(map_service_error)?;
    Ok(Json(serde_json::json!({ "emergency_stop": true })))
}

async fn set_origin(
    State(ctx): State<Arc<ServiceContext>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pos = machine::set_work_origin(&ctx).map_err(map_service_error)?;
    Ok(Json(
        serde_json::json!({ "origin_set": true, "position": pos }),
    ))
}

async fn reset_origin(
    State(ctx): State<Arc<ServiceContext>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    machine::reset_work_origin(&ctx).map_err(map_service_error)?;
    Ok(Json(serde_json::json!({ "origin_reset": true })))
}

async fn list_ports() -> Result<Json<serde_json::Value>, ApiError> {
    let ports = machine::list_serial_ports_op().map_err(map_service_error)?;
    Ok(Json(serde_json::json!({
        "ports": ports,
        "count": ports.len(),
    })))
}

async fn set_feed_override(
    State(ctx): State<Arc<ServiceContext>>,
    Json(body): Json<OverrideBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    machine::set_feed_override(&ctx, &body.action).map_err(map_service_error)?;
    Ok(Json(serde_json::json!({ "feed_override": body.action })))
}

async fn set_spindle_override(
    State(ctx): State<Arc<ServiceContext>>,
    Json(body): Json<OverrideBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    machine::set_spindle_override(&ctx, &body.action).map_err(map_service_error)?;
    Ok(Json(serde_json::json!({ "spindle_override": body.action })))
}

async fn reset_overrides(
    State(ctx): State<Arc<ServiceContext>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    machine::reset_all_overrides(&ctx).map_err(map_service_error)?;
    Ok(Json(serde_json::json!({ "overrides_reset": true })))
}

#[derive(Deserialize)]
struct ConsoleQuery {
    #[serde(default = "default_console_limit")]
    limit: usize,
}

fn default_console_limit() -> usize {
    200
}

async fn get_console_log(
    State(ctx): State<Arc<ServiceContext>>,
    axum::extract::Query(query): axum::extract::Query<ConsoleQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let entries = ctx
        .get_console_log(query.limit)
        .map_err(|e| map_service_error(beambench_service::ServiceError::machine(e)))?;
    Ok(Json(
        serde_json::json!({ "entries": entries, "count": entries.len() }),
    ))
}

#[derive(Deserialize)]
struct SendGcodeBody {
    line: String,
    #[serde(default)]
    confirm_raw_gcode: bool,
    #[serde(default)]
    confirm_laser_on: bool,
}

async fn send_gcode(
    State(ctx): State<Arc<ServiceContext>>,
    Json(body): Json<SendGcodeBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if !body.confirm_raw_gcode {
        return Err(confirmation_required(&["confirm_raw_gcode"]));
    }
    if agent::raw_gcode_requires_laser_confirmation(&body.line) && !body.confirm_laser_on {
        return Err(confirmation_required(&["confirm_laser_on"]));
    }
    ctx.send_gcode_line(&body.line)
        .map_err(|e| map_service_error(beambench_service::ServiceError::machine(e)))?;
    Ok(Json(serde_json::json!({ "sent": true, "line": body.line })))
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
    async fn machine_status_disconnected() {
        let ctx = Arc::new(ServiceContext::new());
        let app = crate::routes::build_router(ctx);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/v1/machine/status")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = collect_body(response).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["connected"], false);
        assert_eq!(json["session_state"], "disconnected");
        assert_eq!(json["machine_coordinates_valid"], false);
    }

    #[tokio::test]
    async fn home_requires_motion_confirmation() {
        let ctx = Arc::new(ServiceContext::new());
        let app = crate::routes::build_router(ctx);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/machine/home")
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
    async fn lihuiyu_usb_connect_route_validates_before_opening_hardware() {
        let ctx = Arc::new(ServiceContext::new());
        let app = crate::routes::build_router(ctx);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/machine/connect/usb")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        r#"{"bus_id":"","device_address":1,"port_numbers":[]}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = collect_body(response).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"]["code"], "invalid_input");
        assert!(
            json["error"]["message"]
                .as_str()
                .unwrap()
                .contains("USB bus ID")
        );
    }

    #[tokio::test]
    async fn explicit_serial_controller_route_validates_before_opening_hardware() {
        let ctx = Arc::new(ServiceContext::new());
        let app = crate::routes::build_router(ctx);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/machine/connect/controller/serial")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        r#"{"port_name":"","selection":{"mode":"known_driver","driver":"marlin"}}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = collect_body(response).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"]["code"], "invalid_input");
        assert!(
            json["error"]["message"]
                .as_str()
                .unwrap()
                .contains("Serial port")
        );
    }

    #[tokio::test]
    async fn explicit_network_controller_route_rejects_serial_driver_before_transport() {
        let ctx = Arc::new(ServiceContext::new());
        let app = crate::routes::build_router(ctx);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/machine/connect/controller/network")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        r#"{"host":"127.0.0.1","selection":{"mode":"known_driver","driver":"smoothieware"}}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = collect_body(response).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"]["code"], "invalid_input");
        assert!(
            json["error"]["message"]
                .as_str()
                .unwrap()
                .contains("Network")
        );
    }

    #[tokio::test]
    async fn controller_continuation_route_rejects_unknown_attempt() {
        let ctx = Arc::new(ServiceContext::new());
        let app = crate::routes::build_router(ctx);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/machine/connect/controller/continue")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        r#"{"attempt_id":"missing-attempt","selection":{"mode":"known_driver","driver":"grbl"}}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::PRECONDITION_FAILED);
        let body = collect_body(response).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"]["code"], "invalid_state");
    }

    #[tokio::test]
    async fn raw_gcode_requires_raw_and_laser_confirmation() {
        let ctx = Arc::new(ServiceContext::new());
        let app = crate::routes::build_router(ctx);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/machine/send")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        r#"{"line":"G1 X1 S100","confirm_raw_gcode":true}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::PRECONDITION_REQUIRED);
        let body = collect_body(response).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error_code"], "CONFIRMATION_REQUIRED");
        assert_eq!(json["missing"][0], "confirm_laser_on");
    }

    #[tokio::test]
    async fn test_air_requires_air_confirmation() {
        let ctx = Arc::new(ServiceContext::new());
        let app = crate::routes::build_router(ctx);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/machine/test-air")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::PRECONDITION_REQUIRED);
        let body = collect_body(response).await;
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error_code"], "CONFIRMATION_REQUIRED");
        assert_eq!(json["missing"][0], "confirm_air_assist");
    }
}

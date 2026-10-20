//! D1/D1 Pro native network protocol facts, independently implemented from
//! the public sources pinned in docs/machines/xtool-d1-network-protocol.md.
//! These helpers do not open a connection or enable machine controls.

use serde::Deserialize;
use serde_json::Value;
use thiserror::Error;

pub const D1_HTTP_PORT: u16 = 8080;
pub const D1_STATUS_PORT: u16 = 8081;
pub const D1_JOB_UPLOAD_PATH: &str = "/cnc/data?filetype=1";
pub const D1_FRAME_UPLOAD_PATH: &str = "/cnc/data?filetype=0";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum D1Model {
    D1,
    D1Pro,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum D1WorkingState {
    Idle,
    ApiJob,
    ButtonJob,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum D1StatusEvent {
    Idle,
    RunningOnline,
    AwaitingButton,
    RunningOffline,
    Framing,
    FrameReady,
    Paused,
    Stopped,
    Error,
    FlameAlarm,
    TiltAlarm,
    MovementAlarm,
    LimitAlarm,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct D1Progress {
    pub percent: f64,
    pub elapsed_ms: u64,
    pub line: u64,
}

#[derive(Debug, Error)]
pub enum D1ProtocolError {
    #[error("xTool D1 returned HTTP {0}")]
    HttpStatus(u16),
    #[error("xTool D1 response is invalid: {0}")]
    InvalidResponse(&'static str),
    #[error("the controller did not identify as an original xTool D1 or D1 Pro")]
    UnsupportedModel,
    #[error("xTool D1 rejected the request")]
    Rejected,
    #[error("xTool D1 reported an unknown working state")]
    UnknownWorkingState,
    #[error("xTool D1 reported an unknown status event")]
    UnknownStatusEvent,
    #[error("xTool D1 upload boundary is invalid or occurs in the payload")]
    InvalidBoundary,
}

fn json_body(status: u16, body: &[u8]) -> Result<Value, D1ProtocolError> {
    if !(200..300).contains(&status) {
        return Err(D1ProtocolError::HttpStatus(status));
    }
    serde_json::from_slice(body)
        .map_err(|_| D1ProtocolError::InvalidResponse("expected a JSON object"))
}

fn checked_object(status: u16, body: &[u8]) -> Result<Value, D1ProtocolError> {
    let value = json_body(status, body)?;
    if value.get("result").and_then(Value::as_str) != Some("ok") {
        return Err(D1ProtocolError::Rejected);
    }
    Ok(value)
}

pub fn require_d1_ack(status: u16, body: &[u8]) -> Result<(), D1ProtocolError> {
    checked_object(status, body).map(|_| ())
}

/// Only the machine-type endpoint identifies a model. A user-assigned name,
/// firmware substring or nested JSON field is not positive identity evidence.
pub fn parse_d1_model(status: u16, body: &[u8]) -> Result<D1Model, D1ProtocolError> {
    let value = checked_object(status, body)?;
    match value.get("type").and_then(Value::as_str) {
        Some("xTool D1") => Ok(D1Model::D1),
        Some("xTool D1Pro" | "xTool D1 Pro") => Ok(D1Model::D1Pro),
        _ => Err(D1ProtocolError::UnsupportedModel),
    }
}

/// This coarse state cannot distinguish pause, framing or readiness. Never
/// infer a completed owned job from HTTP state zero alone.
pub fn parse_d1_working_state(status: u16, body: &[u8]) -> Result<D1WorkingState, D1ProtocolError> {
    let value = checked_object(status, body)?;
    match value.get("working") {
        Some(Value::String(value)) => match value.as_str() {
            "0" => Ok(D1WorkingState::Idle),
            "1" => Ok(D1WorkingState::ApiJob),
            "2" => Ok(D1WorkingState::ButtonJob),
            _ => Err(D1ProtocolError::UnknownWorkingState),
        },
        Some(Value::Number(value)) => match value.as_u64() {
            Some(0) => Ok(D1WorkingState::Idle),
            Some(1) => Ok(D1WorkingState::ApiJob),
            Some(2) => Ok(D1WorkingState::ButtonJob),
            _ => Err(D1ProtocolError::UnknownWorkingState),
        },
        _ => Err(D1ProtocolError::UnknownWorkingState),
    }
}

/// Text frames may have a trailing newline. Do not remove the last character
/// unconditionally, or an un-terminated event loses part of its state name.
pub fn parse_d1_status_event(text: &str) -> Result<D1StatusEvent, D1ProtocolError> {
    match text.trim() {
        "ok:IDLE" => Ok(D1StatusEvent::Idle),
        "ok:WORKING_ONLINE" => Ok(D1StatusEvent::RunningOnline),
        "ok:WORKING_ONLINE_READY" => Ok(D1StatusEvent::AwaitingButton),
        "ok:WORKING_OFFLINE" => Ok(D1StatusEvent::RunningOffline),
        "ok:WORKING_FRAMING" => Ok(D1StatusEvent::Framing),
        "ok:WORKING_FRAME_READY" => Ok(D1StatusEvent::FrameReady),
        "ok:PAUSING" => Ok(D1StatusEvent::Paused),
        "WORK_STOPPED" => Ok(D1StatusEvent::Stopped),
        "ok:ERROR" => Ok(D1StatusEvent::Error),
        "err:flameCheck" => Ok(D1StatusEvent::FlameAlarm),
        "err:tiltCheck" => Ok(D1StatusEvent::TiltAlarm),
        "err:movingCheck" => Ok(D1StatusEvent::MovementAlarm),
        "err:limitCheck" => Ok(D1StatusEvent::LimitAlarm),
        _ => Err(D1ProtocolError::UnknownStatusEvent),
    }
}

/// The progress endpoint retains the previous job's values while idle.
pub fn parse_d1_progress(status: u16, body: &[u8]) -> Result<D1Progress, D1ProtocolError> {
    #[derive(Deserialize)]
    struct Reply {
        progress: f64,
        working: u64,
        line: u64,
    }
    let value = json_body(status, body)?;
    let reply: Reply = serde_json::from_value(value)
        .map_err(|_| D1ProtocolError::InvalidResponse("invalid progress fields"))?;
    if !reply.progress.is_finite() || !(0.0..=100.0).contains(&reply.progress) {
        return Err(D1ProtocolError::InvalidResponse(
            "progress outside 0 to 100",
        ));
    }
    Ok(D1Progress {
        percent: reply.progress,
        elapsed_ms: reply.working,
        line: reply.line,
    })
}

/// Encode the single plain G-code file used by the D-series upload endpoint.
/// The caller supplies a fresh boundary; reject collisions rather than send
/// a payload that the device could truncate into another multipart section.
pub fn encode_d1_upload(gcode: &str, boundary: &str) -> Result<Vec<u8>, D1ProtocolError> {
    if boundary.is_empty()
        || boundary.len() > 70
        || !boundary
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        || gcode.contains(boundary)
    {
        return Err(D1ProtocolError::InvalidBoundary);
    }
    Ok(format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"tmp.gcode\"\r\nContent-Type: text/plain\r\n\r\n{gcode}\r\n--{boundary}--\r\n"
    )
    .into_bytes())
}

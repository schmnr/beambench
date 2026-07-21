use axum::Json;
use axum::http::StatusCode;
use beambench_service::{ServiceError, ServiceErrorCode};

pub type ApiError = (StatusCode, Json<serde_json::Value>);

pub fn map_service_error(err: ServiceError) -> ApiError {
    let status = match err.code {
        ServiceErrorCode::NotFound => StatusCode::NOT_FOUND,
        ServiceErrorCode::InvalidInput => StatusCode::BAD_REQUEST,
        ServiceErrorCode::InvalidState => StatusCode::PRECONDITION_FAILED,
        ServiceErrorCode::Busy => StatusCode::CONFLICT,
        ServiceErrorCode::Conflict => StatusCode::CONFLICT,
        ServiceErrorCode::StaleRevision => StatusCode::PRECONDITION_FAILED,
        ServiceErrorCode::MachineIo => StatusCode::BAD_GATEWAY,
        ServiceErrorCode::Persistence => StatusCode::INTERNAL_SERVER_ERROR,
        ServiceErrorCode::Internal => StatusCode::INTERNAL_SERVER_ERROR,
    };
    let body = serde_json::json!({
        "error": {
            "code": err.code,
            "message": err.message,
            "details": err.details,
        }
    });
    (status, Json(body))
}

/// Validate a caller-supplied output path for endpoints that write files.
/// Requires an absolute path with no `..` components so a request can't
/// traverse out of whatever location the caller named.
pub fn validate_output_path(raw: &str) -> Result<std::path::PathBuf, ApiError> {
    let path = std::path::PathBuf::from(raw);
    if !path.is_absolute() {
        return Err(map_service_error(ServiceError::invalid_input(
            "Output path must be absolute",
        )));
    }
    if path
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(map_service_error(ServiceError::invalid_input(
            "Output path must not contain '..'",
        )));
    }
    Ok(path)
}

pub fn confirmation_required(missing: &[&str]) -> ApiError {
    let message = match missing {
        ["confirm_motion"] => {
            "This command can move the machine and requires explicit confirmation."
        }
        ["confirm_laser_on"] => {
            "This command can turn the laser on and requires explicit confirmation."
        }
        ["confirm_raw_gcode"] => {
            "This command sends raw G-code and requires explicit confirmation."
        }
        ["confirm_air_assist"] => {
            "This command can toggle air assist and requires explicit confirmation."
        }
        _ => "This command requires explicit confirmation.",
    };
    (
        StatusCode::PRECONDITION_REQUIRED,
        Json(serde_json::json!({
            "error_code": "CONFIRMATION_REQUIRED",
            "missing": missing,
            "message": message,
        })),
    )
}

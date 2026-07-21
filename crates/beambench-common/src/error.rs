use serde::Serialize;
use thiserror::Error;

/// Top-level application error enum.
/// Each variant includes a structured code for frontend consumption.
#[derive(Debug, Error, Serialize)]
pub enum AppError {
    #[error("not found: {message}")]
    NotFound { code: String, message: String },

    #[error("invalid input: {message}")]
    InvalidInput { code: String, message: String },

    #[error("machine error: {message}")]
    MachineError { code: String, message: String },

    #[error("internal error: {message}")]
    Internal { code: String, message: String },
}

/// Structured error response sent to the frontend.
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub code: String,
    pub message: String,
    pub details: Option<String>,
}

impl From<AppError> for ErrorResponse {
    fn from(err: AppError) -> Self {
        match &err {
            AppError::NotFound { code, message } => ErrorResponse {
                code: code.clone(),
                message: message.clone(),
                details: None,
            },
            AppError::InvalidInput { code, message } => ErrorResponse {
                code: code.clone(),
                message: message.clone(),
                details: None,
            },
            AppError::MachineError { code, message } => ErrorResponse {
                code: code.clone(),
                message: message.clone(),
                details: None,
            },
            AppError::Internal { code, message } => ErrorResponse {
                code: code.clone(),
                message: message.clone(),
                details: None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_converts_to_response() {
        let err = AppError::NotFound {
            code: "PROJECT_NOT_FOUND".into(),
            message: "No project is open".into(),
        };
        let resp = ErrorResponse::from(err);
        assert_eq!(resp.code, "PROJECT_NOT_FOUND");
    }

    #[test]
    fn error_serializes_to_json() {
        let resp = ErrorResponse {
            code: "INVALID_INPUT".into(),
            message: "Speed must be positive".into(),
            details: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("INVALID_INPUT"));
    }
}

use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceErrorCode {
    NotFound,
    InvalidInput,
    InvalidState,
    Busy,
    Conflict,
    StaleRevision,
    MachineIo,
    Persistence,
    Internal,
}

/// Unified structured error for service-layer operations.
#[derive(Debug, Clone, Error, Serialize)]
#[error("{message}")]
pub struct ServiceError {
    pub code: ServiceErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl ServiceError {
    pub fn new(code: ServiceErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: None,
        }
    }

    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(ServiceErrorCode::NotFound, message)
    }

    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self::new(ServiceErrorCode::InvalidInput, message)
    }

    pub fn invalid_state(message: impl Into<String>) -> Self {
        Self::new(ServiceErrorCode::InvalidState, message)
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(ServiceErrorCode::Conflict, message)
    }

    pub fn busy(message: impl Into<String>) -> Self {
        Self::new(ServiceErrorCode::Busy, message)
    }

    pub fn stale_revision(message: impl Into<String>) -> Self {
        Self::new(ServiceErrorCode::StaleRevision, message)
    }

    pub fn machine(message: impl Into<String>) -> Self {
        Self::new(ServiceErrorCode::MachineIo, message)
    }

    pub fn persistence(message: impl Into<String>) -> Self {
        Self::new(ServiceErrorCode::Persistence, message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(ServiceErrorCode::Internal, message)
    }
}

impl From<std::io::Error> for ServiceError {
    fn from(value: std::io::Error) -> Self {
        ServiceError::persistence(format!("IO error: {value}"))
    }
}

impl From<ServiceError> for String {
    fn from(e: ServiceError) -> String {
        e.to_string()
    }
}

pub type ServiceResult<T> = Result<T, ServiceError>;

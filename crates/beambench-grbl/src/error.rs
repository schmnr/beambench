//! GRBL adapter error types.

use beambench_serial::SerialError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GrblError {
    #[error("transport error: {0}")]
    Transport(#[from] SerialError),

    #[error("invalid response: {0}")]
    InvalidResponse(String),

    #[error("alarm {code}: {message}")]
    Alarm { code: u8, message: String },

    #[error("GRBL error {code}: {message}")]
    GrblError { code: u8, message: String },

    #[error("timeout waiting for response")]
    Timeout,

    #[error("a fresh GRBL status report is required before validating this transport")]
    FreshStatusRequired,

    #[error("invalid state transition from {from} to {to}")]
    InvalidTransition { from: String, to: String },

    #[error("cannot {action} while session is {state}")]
    InvalidState { action: &'static str, state: String },

    #[error("not connected")]
    NotConnected,

    #[error("unsupported firmware: {0}")]
    UnsupportedFirmware(String),

    #[error("G-code generation error: {0}")]
    GcodeError(String),
}

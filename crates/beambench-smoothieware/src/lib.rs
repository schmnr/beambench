//! Smoothieware controller adapter.
//!
//! This crate owns bounded firmware identity detection and will own the
//! controller's motion-power dialect, acknowledged execution, completion, and
//! recovery contracts. Smoothieware remains separate from GRBL even when its
//! firmware reports GRBL compatibility mode.

#![forbid(unsafe_code)]

mod adapter;
mod gcode;
mod identity;
mod laser;
mod probe;
mod session;

pub use adapter::{SmoothiewareAdapterDescriptor, SmoothiewareSerialAdapter};
pub use gcode::{SmoothiewareGcodeConfig, SmoothiewareGcodeError, generate_smoothieware_gcode};
pub use identity::{
    MAX_SMOOTHIEWARE_IDENTITY_LINE_BYTES, SmoothiewareIdentity, SmoothiewareIdentityDetector,
    SmoothiewareIdentityEvidence, SmoothiewareIdentityStatus,
};
pub use laser::{
    SMOOTHIEWARE_CANCEL_COMMAND, SMOOTHIEWARE_FINISH_MOVES_COMMAND, SmoothiewareLaserCommandError,
    SmoothiewareLaserCommands, SmoothiewarePowerMode, SmoothiewarePowerScale,
};
pub use probe::{
    SMOOTHIEWARE_IDENTITY_COMMAND, SmoothiewareIdentityProbeOutcome,
    SmoothiewareIdentityProbeResult, SmoothiewareIdentityProbeSequence,
};
pub use session::{
    SMOOTHIEWARE_LASER_ENABLE_QUERY, SMOOTHIEWARE_MAXIMUM_S_QUERY,
    SMOOTHIEWARE_PROPORTIONAL_POWER_QUERY, SmoothiewareJobOutcome, SmoothiewareSerialSession,
    SmoothiewareSerialSessionConfig, SmoothiewareSessionError, SmoothiewareSessionState,
};

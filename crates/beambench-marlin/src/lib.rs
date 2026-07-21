//! Standard Marlin and Snapmaker 2.0 controller adapters.
//!
//! This crate owns bounded `M115` identity detection, dialect-specific laser
//! G-code generation, acknowledged serial execution, completion barriers, and
//! reconnect-required cancellation/recovery contracts.

#![forbid(unsafe_code)]

mod adapter;
mod gcode;
mod identity;
mod laser;
mod probe;
mod session;
mod snapmaker;

pub use adapter::{MarlinAdapterDescriptor, MarlinSerialAdapter, SnapmakerSerialAdapter};
pub use gcode::{MarlinGcodeConfig, MarlinGcodeError, generate_marlin_gcode};
pub use identity::{
    MAX_MARLIN_IDENTITY_LINE_BYTES, MarlinDialect, MarlinIdentity, MarlinIdentityDetector,
    MarlinIdentityEvidence, MarlinIdentityStatus,
};
pub use laser::{
    MARLIN_FINISH_MOVES_COMMAND, MARLIN_LASER_OFF_COMMAND, MarlinLaserCommandError,
    MarlinLaserCommands, MarlinLaserMode, MarlinPowerScale,
};
pub use probe::{
    MARLIN_IDENTITY_COMMAND, MarlinIdentityProbeOutcome, MarlinIdentityProbeResult,
    MarlinIdentityProbeSequence,
};
pub use session::{
    MARLIN_CANCEL_COMMAND, MarlinJobOutcome, MarlinSerialSession, MarlinSerialSessionConfig,
    MarlinSessionError, MarlinSessionState,
};
pub use snapmaker::{
    SnapmakerGcodeConfig, SnapmakerGcodeError, SnapmakerLaserMode, generate_snapmaker_gcode,
};

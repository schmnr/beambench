//! GRBL protocol adapter for Beam Bench.
//! Parses and generates GRBL commands, manages session state.

pub mod adapter;
pub mod commands;
pub mod error;
pub mod gcode;
pub mod identity;
mod identity_probe;
pub mod parser;
pub mod session;
pub mod settings;
pub mod state;

pub use adapter::{
    FluidNcNetworkAdapter, FluidNcSerialAdapter, GrblFamilyAdapter, GrblFamilyAdapterDescriptor,
    GrblFamilyAdapterError, GrblFamilySerialAdapter, GrblHalNetworkAdapter, GrblHalSerialAdapter,
};
pub use beambench_common::{
    GrblFamilyDialect, GrblFamilyIdentity, GrblFamilyIdentityEvidence, GrblFamilyIdentityStatus,
};
pub use error::GrblError;
pub use gcode::{GcodeConfig, generate_gcode, interpolate_scanning_offset};
pub use identity::{GrblFamilyIdentityDetector, MAX_IDENTITY_LINE_BYTES};
pub use identity_probe::{
    DEFAULT_IDENTITY_PROBE_COMMAND_TIMEOUT, DEFAULT_IDENTITY_PROBE_POLL_INTERVAL,
    GrblFamilyIdentityProbeConfig, GrblFamilyIdentityProbeOutcome, GrblFamilyIdentityProbeResult,
};
pub use parser::{GrblResponse, parse_response};
pub use session::GrblSession;
pub use settings::{GrblSettingId, GrblSettings, parse_setting_id, parse_setting_line};
pub use state::SessionStateMachine;

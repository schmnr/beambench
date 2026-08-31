//! xTool network-appliance protocols supported by Beam Bench.
//!
//! The original xTool M1 uses an HTTP service over Wi-Fi or its USB network
//! interface. It does not expose the serial GRBL stream used by most diode
//! controllers.

mod compiler;
mod protocol;
mod runtime;

pub use compiler::{
    M1CompileConfig, M1CompileError, M1CompiledJob, compile_m1_job, package_m1_gcode,
};
pub use protocol::{
    M1HttpIo, M1HttpResponse, M1Identity, M1IdentityConfidence, M1ProtocolError, M1Status,
    parse_m1_status, probe_m1_identity,
};
pub use runtime::{M1Runtime, M1RuntimeError, M1RuntimePhase, M1RuntimeSnapshot};

pub const M1_DEFAULT_PORT: u16 = 8080;
pub const M1_WORKSPACE_WIDTH_MM: f64 = 385.0;
pub const M1_WORKSPACE_HEIGHT_MM: f64 = 300.0;
pub const M1_POWER_MAXIMUM: u32 = 1_000;

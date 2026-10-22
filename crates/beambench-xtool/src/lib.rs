//! xTool network-appliance protocols supported by Beam Bench.
//!
//! The original xTool M1 uses an HTTP service over Wi-Fi or its USB network
//! interface. It does not expose the serial GRBL stream used by most diode
//! controllers.

mod compiler;
mod d1_compiler;
mod d1_protocol;
mod d1_runtime;
mod d1_serial;
mod protocol;
mod runtime;

pub use compiler::{
    M1CompileConfig, M1CompileError, M1CompiledJob, compile_m1_job, package_m1_gcode,
};
pub use d1_compiler::{D1_MAX_JOB_BYTES, D1CompileConfig, D1CompiledJob, compile_d1_job};
pub use d1_protocol::{
    D1_FRAME_UPLOAD_PATH, D1_HTTP_PORT, D1_JOB_UPLOAD_PATH, D1_STATUS_PORT, D1Model, D1Progress,
    D1ProtocolError, D1StatusEvent, D1WorkingState, encode_d1_upload, parse_d1_model,
    parse_d1_progress, parse_d1_status_event, parse_d1_working_state, require_d1_ack,
};
pub use d1_runtime::{D1Identity, D1NetworkIo, D1Runtime, D1RuntimeError, D1RuntimePhase};
pub use d1_serial::{D1_SERIAL_BAUD, D1SerialIo, D1SerialRuntime, D1SerialSnapshot};
pub use protocol::{M1HttpIo as XToolHttpIo, M1HttpResponse as XToolHttpResponse};
pub use protocol::{
    M1HttpIo, M1HttpResponse, M1Identity, M1IdentityConfidence, M1ProtocolError, M1Status,
    parse_m1_status, probe_m1_identity,
};
pub use runtime::{M1Runtime, M1RuntimeError, M1RuntimePhase, M1RuntimeSnapshot};

pub const M1_DEFAULT_PORT: u16 = 8080;
pub const M1_WORKSPACE_WIDTH_MM: f64 = 385.0;
pub const M1_WORKSPACE_HEIGHT_MM: f64 = 300.0;
pub const M1_POWER_MAXIMUM: u32 = 1_000;

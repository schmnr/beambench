//! Job streaming engine for Beam Bench.
//! Feeds G-code to the machine at the correct rate with flow control.

pub mod engine;
pub mod error;
pub mod job;
pub mod preflight;
pub mod progress;

pub use engine::StreamingEngine;
pub use error::StreamerError;
pub use job::JobController;
pub use preflight::{
    check_profile_mismatch, check_raster_motion_bounds, check_tool_layers, run_preflight,
};
pub use progress::ProgressTracker;

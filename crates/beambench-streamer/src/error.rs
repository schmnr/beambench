//! Streaming engine error types.

use beambench_grbl::GrblError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StreamerError {
    #[error("GRBL error: {0}")]
    Grbl(#[from] GrblError),

    #[error("a job is already active")]
    JobActive,

    #[error("no active job")]
    NoActiveJob,

    #[error("acknowledgement timeout")]
    AckTimeout,

    #[error("no progress timeout")]
    NoProgressTimeout,

    #[error("buffer desync")]
    BufferDesync,

    #[error("job failed: {0}")]
    JobFailed(String),

    #[error("job cancelled")]
    JobCancelled,

    #[error("disconnected during job")]
    Disconnected,

    #[error("alarm during job: code {0}")]
    AlarmDuringJob(u8),
}

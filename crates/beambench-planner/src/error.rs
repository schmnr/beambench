//! Error types for planner operations.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum PlannerError {
    #[error("Cannot build plan from empty project")]
    EmptyPlan,
    #[error("Geometry exceeds workspace bounds: {0}")]
    BoundsExceeded(String),
    #[error("Invalid planner settings: {0}")]
    InvalidSettings(String),
    #[error("Plan generation cancelled")]
    Cancelled,
    #[error("Raster processing failed: {0}")]
    RasterError(#[from] beambench_raster::RasterError),
    #[error("Vector normalization failed for object: {0}")]
    NormalizationFailed(String),
}

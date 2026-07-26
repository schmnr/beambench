//! Error types for planner operations.

use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoundsAxis {
    X,
    Y,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoundsBoundary {
    Min,
    Max,
}

/// Structured details for geometry that falls outside the machine workspace.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BoundsViolation {
    pub axis: BoundsAxis,
    pub boundary: BoundsBoundary,
    pub amount_mm: f64,
    pub coordinate_mm: f64,
    pub limit_mm: f64,
    pub segment_index: usize,
    pub geometry_label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_object_id: Option<String>,
}

impl BoundsViolation {
    pub fn new(
        axis: BoundsAxis,
        boundary: BoundsBoundary,
        coordinate_mm: f64,
        limit_mm: f64,
        segment_index: usize,
        geometry_label: impl Into<String>,
        source_object_id: Option<&str>,
    ) -> Self {
        Self {
            axis,
            boundary,
            amount_mm: (coordinate_mm - limit_mm).abs(),
            coordinate_mm,
            limit_mm,
            segment_index,
            geometry_label: geometry_label.into(),
            source_object_id: source_object_id.map(str::to_owned),
        }
    }
}

impl fmt::Display for BoundsViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let axis = match self.axis {
            BoundsAxis::X => "x",
            BoundsAxis::Y => "y",
        };
        match self.boundary {
            BoundsBoundary::Min => write!(
                f,
                "Segment {} {} {}={} is below bed origin {}",
                self.segment_index, self.geometry_label, axis, self.coordinate_mm, self.limit_mm
            ),
            BoundsBoundary::Max => {
                let dimension = match self.axis {
                    BoundsAxis::X => "width",
                    BoundsAxis::Y => "height",
                };
                write!(
                    f,
                    "Segment {} {} {}={} exceeds bed {} {}",
                    self.segment_index,
                    self.geometry_label,
                    axis,
                    self.coordinate_mm,
                    dimension,
                    self.limit_mm
                )
            }
        }
    }
}

#[derive(Debug, Error)]
pub enum PlannerError {
    #[error("Cannot build plan from empty project")]
    EmptyPlan,
    #[error("Geometry exceeds workspace bounds: {0}")]
    BoundsExceeded(BoundsViolation),
    #[error("Invalid planner settings: {0}")]
    InvalidSettings(String),
    #[error("Plan generation cancelled")]
    Cancelled,
    #[error("Raster processing failed: {0}")]
    RasterError(#[from] beambench_raster::RasterError),
    #[error("Vector normalization failed for object: {0}")]
    NormalizationFailed(String),
}

//! Input to the planner's optimization pipeline.
//!
//! Composes the project-persisted [`ProjectOptimization`] settings with
//! runtime-only state ([`OptimizationRuntime`]) and planner calibration.
//! The split is deliberate: [`ProjectOptimization`] travels with the
//! project file on disk, while [`OptimizationRuntime`] holds values
//! sourced from the live machine session (e.g. `current_position`) that
//! must never enter a persisted file.
//!
//! Phase 4a: Introduces the type and the [`build_plan_with_input`]
//! entry point. The workhorse [`build_plan_with_settings`] is retained
//! as a shim that builds a [`PlannerInput`] from a pre-M1
//! [`OptimizationSettings`] value so external callers continue to work
//! unchanged during the rollout.
//!
//! [`build_plan_with_input`]: crate::builder::build_plan_with_input
//! [`build_plan_with_settings`]: crate::builder::build_plan_with_settings
//! [`OptimizationSettings`]: crate::plan::OptimizationSettings

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use beambench_common::Bounds;
use beambench_core::{ProjectOptimization, QualityTestOrdering};

use crate::plan::PlannerCalibration;

/// Runtime-only optimization state that lives alongside [`ProjectOptimization`]
/// but never persists to disk.
///
/// Today's only field is the live machine position used by
/// `StartFromMode::CurrentPosition`. Pre-M1 this sat on
/// `beambench_planner::OptimizationSettings.current_position` and was
/// stripped before persistence; Phase 4a gives it a dedicated type so the
/// persisted and ephemeral surfaces are structurally distinct.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OptimizationRuntime {
    /// Live machine position captured at plan time.
    ///
    /// `None` means unknown (treat as `(0, 0)` at the planner boundary,
    /// matching pre-M1 behavior).
    pub current_position: Option<(f64, f64)>,
}

/// Full input to the optimization pipeline.
///
/// Always construct via [`PlannerInput::new`] so future fields don't
/// silently inherit `Default` values when callers add them.
#[derive(Clone, Default)]
pub struct PlannerCancellation {
    latest_request_id: Option<Arc<AtomicU64>>,
    request_id: u64,
}

impl PlannerCancellation {
    pub fn new(latest_request_id: Arc<AtomicU64>, request_id: u64) -> Self {
        Self {
            latest_request_id: Some(latest_request_id),
            request_id,
        }
    }

    pub fn should_cancel(&self) -> bool {
        self.latest_request_id
            .as_ref()
            .is_some_and(|latest| latest.load(Ordering::Acquire) != self.request_id)
    }
}

impl fmt::Debug for PlannerCancellation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PlannerCancellation")
            .field("request_id", &self.request_id)
            .field("enabled", &self.latest_request_id.is_some())
            .finish()
    }
}

impl PartialEq for PlannerCancellation {
    fn eq(&self, other: &Self) -> bool {
        self.request_id == other.request_id
            && self.latest_request_id.is_some() == other.latest_request_id.is_some()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlannerInput {
    pub optimization: ProjectOptimization,
    pub runtime: OptimizationRuntime,
    pub calibration: PlannerCalibration,
    pub cancellation: PlannerCancellation,
    /// Ordering policy. `RowMajor` bypasses every reorder/dedupe/start-point pass so
    /// quality-test transient jobs emit segments in the order they were appended.
    pub ordering: QualityTestOrdering,
    /// Optional bounds used for the job-origin anchor instead of whole-job
    /// content bounds. Session job options use this for "Use Selection Origin"
    /// without writing anything into the project file.
    pub job_origin_bounds_override: Option<Bounds>,
}

impl PlannerInput {
    pub fn new(
        optimization: ProjectOptimization,
        runtime: OptimizationRuntime,
        calibration: PlannerCalibration,
    ) -> Self {
        Self {
            optimization,
            runtime,
            calibration,
            cancellation: PlannerCancellation::default(),
            ordering: QualityTestOrdering::Normal,
            job_origin_bounds_override: None,
        }
    }

    /// Build an input with defaults everywhere. Useful in tests and in
    /// the zero-arg `build_plan(&Project)` path when the project's own
    /// [`ProjectOptimization`] has not yet been threaded through.
    pub fn with_defaults() -> Self {
        Self {
            optimization: ProjectOptimization::default(),
            runtime: OptimizationRuntime::default(),
            calibration: PlannerCalibration::default(),
            cancellation: PlannerCancellation::default(),
            ordering: QualityTestOrdering::Normal,
            job_origin_bounds_override: None,
        }
    }

    pub fn with_cancellation(mut self, cancellation: PlannerCancellation) -> Self {
        self.cancellation = cancellation;
        self
    }

    /// Mark this input as a quality-test transient job — bypasses optimization passes.
    pub fn with_row_major_ordering(mut self) -> Self {
        self.ordering = QualityTestOrdering::RowMajor;
        self
    }

    pub fn with_job_origin_bounds(mut self, bounds: Bounds) -> Self {
        self.job_origin_bounds_override = Some(bounds);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_defaults_matches_component_defaults() {
        let input = PlannerInput::with_defaults();
        assert_eq!(input.optimization, ProjectOptimization::default());
        assert_eq!(input.runtime, OptimizationRuntime::default());
        assert_eq!(input.runtime.current_position, None);
    }

    #[test]
    fn new_preserves_fields() {
        let opt = ProjectOptimization {
            inner_first: true,
            ..Default::default()
        };
        let runtime = OptimizationRuntime {
            current_position: Some((5.0, 7.0)),
        };
        let cal = PlannerCalibration {
            dot_width_mm: 0.1,
            enable_dot_width: true,
        };
        let input = PlannerInput::new(opt.clone(), runtime.clone(), cal.clone());
        assert_eq!(input.optimization, opt);
        assert_eq!(input.runtime, runtime);
        assert_eq!(input.calibration.dot_width_mm, 0.1);
    }
}

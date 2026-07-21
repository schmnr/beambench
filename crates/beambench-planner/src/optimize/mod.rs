//! Optimization pipeline for the planner.
//!
//! Consumes a [`PlannerInput`] (a [`ProjectOptimization`] composed with
//! ephemeral runtime state) and produces the ordered, travel-aware
//! [`ExecutionPlan`]. Today this module is the structural home of two
//! pre-M1 helpers — polyline ordering and travel-segment handling —
//! moved out of the planner root.
//!
//! Phase 4b/4c scope: adds the per-flag passes — [`dedupe`],
//! [`inner_first`], [`direction`], [`start_point`] — as siblings of
//! [`order`] and [`travel`]. Each pass is a pure function over
//! `Vec<T: Orderable>` (or `Vec<PlanSegment>` for cross-layer passes),
//! gated on a flag from the caller's [`ProjectOptimization`], and
//! tested in isolation in its own `#[cfg(test)]` module.
//!
//! Subsequent phases will add structural sort-key passes for
//! group/layer/priority ordering key off-cases.
//!
//! [`ProjectOptimization`]: beambench_core::ProjectOptimization
//! [`ExecutionPlan`]: crate::plan::ExecutionPlan

pub mod dedupe;
pub mod direction;
pub mod inner_first;
pub mod input;
pub mod order;
pub mod sort_keys;
pub mod start_point;
pub mod travel;

pub use input::{OptimizationRuntime, PlannerCancellation, PlannerInput};

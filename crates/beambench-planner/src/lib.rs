//! Toolpath planning for Beam Bench.
//! Generates optimized laser paths from design objects.

pub mod builder;
pub mod error;
pub mod extensions;
pub mod fill_raster;
pub mod image_params;
pub mod optimize;
pub mod plan;
pub mod ramp;
pub mod scanline;
pub mod stats;
pub mod validate;

pub use beambench_core::layer::OperationType;
pub use builder::{
    build_frame_plan, build_hull_frame_plan, build_plan, build_plan_with_input,
    build_plan_with_input_and_cache, calculate_plan_bounds, offset_fill_boolean_tolerances_mm,
};
pub use error::PlannerError;
pub use extensions::{
    air_assist_commands, anchor_segments_to_target, apply_kerf_offset, apply_tabs,
    apply_workspace_origin_transform, flip_bounds_y, job_anchor_point, translate_segments,
    z_offset_command,
};
pub use image_params::build_image_raster_params;
pub use optimize::{OptimizationRuntime, PlannerCancellation, PlannerInput};
pub use plan::{
    DirectionMode, ExecutionPlan, PlanEntryFailure, PlanEntryFailureReason, PlanSegment, PlanStats,
    PlanWarning, PlannerCalibration, PowerMode, ScanAxis, ScanDirection, ScanRun, Scanline,
};

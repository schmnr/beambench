//! Preview generation for Beam Bench.
//! Distills ExecutionPlans into lightweight PreviewData for canvas rendering.

pub mod distill;
pub mod types;

pub use distill::distill_preview;
pub use types::{
    PreviewData, PreviewFrame, PreviewLayer, PreviewStats, RasterPreview, RasterPreviewBitmap,
    RasterRunExtent, TravelMove, VectorPreview,
};

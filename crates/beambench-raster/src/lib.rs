//! Raster processing pipeline for Beam Bench.
//! Converts images to laser-engravable data.

pub mod adjust;
pub mod cache;
pub mod decode;
pub mod dither;
pub mod error;
pub mod fill;
pub mod filters;
pub mod pipeline;
pub mod rotate;
pub mod transform;
pub mod transpose;
pub mod types;

pub use error::RasterError;
pub use fill::flood_fill_scanlines;
pub use pipeline::{process_raster, process_raster_with_cache};
pub use rotate::{RotatedRaster, rotate_raster};
pub use transpose::transpose_raster;
pub use types::{ProcessedRaster, RasterPixelFormat, RasterProcessingParams};

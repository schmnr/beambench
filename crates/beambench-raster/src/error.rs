//! Error types for raster processing.

use thiserror::Error;

/// Errors that can occur during raster processing.
#[derive(Debug, Error)]
pub enum RasterError {
    /// Failed to decode image from source bytes.
    #[error("Failed to decode image: {0}")]
    DecodeError(String),

    /// Image has zero width or height.
    #[error("Image is empty (zero width or height)")]
    EmptyImage,

    /// Invalid dimensions provided for processing.
    #[error("Invalid dimensions: {0}")]
    InvalidDimensions(String),
}

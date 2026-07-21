//! Core types for raster processing.

use beambench_common::{RasterAdjustments, RasterMode};
use serde::{Deserialize, Serialize};

/// Pixel format for processed raster data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RasterPixelFormat {
    /// 1 bit per pixel, packed into bytes (MSB first).
    Binary,
    /// 8 bits per pixel grayscale.
    Grayscale8,
}

/// Processed raster data ready for laser engraving.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcessedRaster {
    /// Width of the raster in pixels.
    pub width_px: u32,
    /// Height of the raster in pixels.
    pub height_px: u32,
    /// Line interval in millimeters (spacing between scan lines, Y direction).
    pub line_interval_mm: f64,
    /// Pixel spacing in millimeters (X direction).
    /// For normal modes this equals line_interval_mm.
    /// For pass-through mode this may differ (native pixel spacing).
    #[serde(default = "default_x_pixel_mm")]
    pub x_pixel_mm: f64,
    /// Pixel format of the data.
    pub format: RasterPixelFormat,
    /// Raw pixel data.
    /// For Binary format: packed 1-bit-per-pixel, MSB first, rows padded to byte boundaries.
    /// For Grayscale8 format: 1 byte per pixel.
    pub data: Vec<u8>,
}

fn default_x_pixel_mm() -> f64 {
    0.0 // Sentinel; callers should set this properly. 0.0 signals "use line_interval_mm".
}

impl ProcessedRaster {
    /// Effective X pixel spacing: returns x_pixel_mm if positive, otherwise line_interval_mm.
    pub fn effective_x_pixel_mm(&self) -> f64 {
        if self.x_pixel_mm > 0.0 {
            self.x_pixel_mm
        } else {
            self.line_interval_mm
        }
    }
}

/// Parameters for raster processing pipeline.
#[derive(Debug, Clone)]
pub struct RasterProcessingParams {
    /// Source image bytes (PNG, JPEG, etc.).
    pub source_bytes: Vec<u8>,
    /// Target dimensions in millimeters (width, height).
    pub bounds_mm: (f64, f64),
    /// Resolution in dots per inch.
    pub dpi: u32,
    /// Processing mode (grayscale, threshold, dithering).
    pub mode: RasterMode,
    /// Image adjustments to apply.
    pub adjustments: RasterAdjustments,
    /// Pass-through mode: skip rescale/adjustments, preserve native pixels.
    #[allow(dead_code)]
    pub pass_through: bool,
    /// Halftone: cells per inch for dot grid size.
    pub halftone_cells_per_inch: u32,
    /// Halftone: rotation angle in degrees.
    pub halftone_angle_deg: f64,
    /// Newsprint: rotation angle in degrees.
    pub newsprint_angle_deg: f64,
    /// Newsprint: dot frequency (dots per inch of screen).
    pub newsprint_frequency: f64,
    /// Layer-level invert (negative image). Applied after per-object
    /// adjustments and before dithering.
    pub invert: bool,
}

impl RasterProcessingParams {
    /// Compute a content-addressed cache key from all fields that affect
    /// the `process_raster` output. Every field is explicitly included.
    pub fn cache_key(&self) -> String {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        // Source image identity
        h.update(&self.source_bytes);
        // Target geometry
        h.update(self.bounds_mm.0.to_le_bytes());
        h.update(self.bounds_mm.1.to_le_bytes());
        h.update(self.dpi.to_le_bytes());
        // Mode — explicit stable byte tag
        h.update([match self.mode {
            beambench_common::RasterMode::Grayscale => 0u8,
            beambench_common::RasterMode::Threshold => 1,
            beambench_common::RasterMode::FloydSteinberg => 2,
            beambench_common::RasterMode::OrderedDither => 3,
            beambench_common::RasterMode::Stucki => 4,
            beambench_common::RasterMode::Jarvis => 5,
            beambench_common::RasterMode::Sierra => 6,
            beambench_common::RasterMode::Atkinson => 7,
            beambench_common::RasterMode::Halftone => 8,
            beambench_common::RasterMode::Newsprint => 9,
            beambench_common::RasterMode::Sketch => 10,
        }]);
        // Per-object adjustments — every field explicitly
        h.update(self.adjustments.brightness.to_le_bytes());
        h.update(self.adjustments.contrast.to_le_bytes());
        h.update(self.adjustments.gamma.to_le_bytes());
        h.update([self.adjustments.invert as u8]);
        h.update([self.adjustments.threshold]);
        h.update(self.adjustments.saturation.to_le_bytes());
        h.update(self.adjustments.sharpen.to_le_bytes());
        h.update([self.adjustments.edge_enhance as u8]);
        h.update(self.adjustments.enhance_radius.to_le_bytes());
        h.update(self.adjustments.enhance_amount.to_le_bytes());
        h.update(self.adjustments.enhance_denoise.to_le_bytes());
        // Flags
        h.update([self.pass_through as u8]);
        h.update([self.invert as u8]);
        // Halftone/newsprint params
        h.update(self.halftone_cells_per_inch.to_le_bytes());
        h.update(self.halftone_angle_deg.to_le_bytes());
        h.update(self.newsprint_angle_deg.to_le_bytes());
        h.update(self.newsprint_frequency.to_le_bytes());
        format!("{:x}", h.finalize())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_params() -> RasterProcessingParams {
        RasterProcessingParams {
            source_bytes: vec![0u8; 64],
            bounds_mm: (100.0, 100.0),
            dpi: 254,
            mode: RasterMode::FloydSteinberg,
            adjustments: RasterAdjustments::default(),
            pass_through: false,
            halftone_cells_per_inch: 10,
            halftone_angle_deg: 0.0,
            newsprint_angle_deg: 45.0,
            newsprint_frequency: 10.0,
            invert: false,
        }
    }

    #[test]
    fn cache_key_same_params_same_key() {
        let params1 = default_params();
        let params2 = params1.clone();
        assert_eq!(params1.cache_key(), params2.cache_key());
    }

    #[test]
    fn cache_key_different_brightness_different_key() {
        let params1 = default_params();
        let mut params2 = params1.clone();
        params2.adjustments.brightness = 0.5;
        assert_ne!(params1.cache_key(), params2.cache_key());
    }

    #[test]
    fn cache_key_different_mode_different_key() {
        let params1 = default_params();
        let mut params2 = params1.clone();
        params2.mode = RasterMode::Threshold;
        assert_ne!(params1.cache_key(), params2.cache_key());
    }

    #[test]
    fn cache_key_different_source_different_key() {
        let params1 = default_params();
        let mut params2 = params1.clone();
        params2.source_bytes = vec![99; 100];
        assert_ne!(params1.cache_key(), params2.cache_key());
    }

    #[test]
    fn raster_pixel_format_binary() {
        let format = RasterPixelFormat::Binary;
        assert_eq!(format, RasterPixelFormat::Binary);
    }

    #[test]
    fn raster_pixel_format_grayscale() {
        let format = RasterPixelFormat::Grayscale8;
        assert_eq!(format, RasterPixelFormat::Grayscale8);
    }

    #[test]
    fn processed_raster_creation() {
        let raster = ProcessedRaster {
            width_px: 100,
            height_px: 50,
            line_interval_mm: 0.1,
            x_pixel_mm: 0.1,
            format: RasterPixelFormat::Binary,
            data: vec![0xFF; 625], // (100 + 7) / 8 * 50 = 13 * 50 = 650, but simplified
        };
        assert_eq!(raster.width_px, 100);
        assert_eq!(raster.height_px, 50);
        assert_eq!(raster.effective_x_pixel_mm(), 0.1);
    }

    #[test]
    fn effective_x_pixel_mm_fallback() {
        let raster = ProcessedRaster {
            width_px: 10,
            height_px: 10,
            line_interval_mm: 0.2,
            x_pixel_mm: 0.0, // sentinel
            format: RasterPixelFormat::Grayscale8,
            data: vec![128; 100],
        };
        // Falls back to line_interval_mm
        assert_eq!(raster.effective_x_pixel_mm(), 0.2);
    }

    #[test]
    fn effective_x_pixel_mm_explicit() {
        let raster = ProcessedRaster {
            width_px: 10,
            height_px: 10,
            line_interval_mm: 0.2,
            x_pixel_mm: 0.15,
            format: RasterPixelFormat::Grayscale8,
            data: vec![128; 100],
        };
        assert_eq!(raster.effective_x_pixel_mm(), 0.15);
    }

    #[test]
    fn raster_processing_params_creation() {
        let params = RasterProcessingParams {
            source_bytes: vec![0x89, 0x50, 0x4E, 0x47], // PNG header
            bounds_mm: (25.4, 25.4),
            dpi: 300,
            mode: RasterMode::FloydSteinberg,
            adjustments: RasterAdjustments::default(),
            pass_through: false,
            halftone_cells_per_inch: 10,
            halftone_angle_deg: 0.0,
            newsprint_angle_deg: 45.0,
            newsprint_frequency: 10.0,
            invert: false,
        };
        assert_eq!(params.bounds_mm, (25.4, 25.4));
        assert_eq!(params.dpi, 300);
    }

    #[test]
    fn raster_pixel_format_serializes_to_snake_case() {
        let format = RasterPixelFormat::Grayscale8;
        let json = serde_json::to_string(&format).unwrap();
        assert_eq!(json, "\"grayscale8\"");
    }
}

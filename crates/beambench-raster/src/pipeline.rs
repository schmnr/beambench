//! Main raster processing pipeline.

use crate::{
    adjust::apply_adjustments,
    decode::decode_image,
    dither::{
        apply_threshold, atkinson, floyd_steinberg, halftone_dither, jarvis, newsprint_dither,
        ordered_dither, sierra, sketch_dither, stucki,
    },
    error::RasterError,
    transform::scale_to_target,
    types::{ProcessedRaster, RasterPixelFormat, RasterProcessingParams},
};
use beambench_common::RasterMode;

/// Process raster image from source bytes to laser-engravable data.
///
/// Pipeline steps:
/// 1. Validate input parameters
/// 2. Decode image from source bytes
/// 3. Calculate target dimensions from bounds and DPI
/// 4. Scale image to target dimensions
/// 5. Apply image adjustments (brightness, contrast, gamma, invert)
/// 6. Convert to output format based on mode
///
/// # Errors
///
/// Returns `RasterError::InvalidDimensions` if bounds or DPI are invalid.
/// Returns `RasterError::DecodeError` if image cannot be decoded.
/// Returns `RasterError::EmptyImage` if decoded image is empty.
/// Process raster with optional staged image cache for slider responsiveness.
/// When `scaled_cache` is provided, decoded+scaled images are cached so that
/// adjustment-only changes (brightness, contrast, gamma) skip the expensive
/// decode+scale stages.
pub fn process_raster_with_cache(
    params: RasterProcessingParams,
    scaled_cache: Option<&crate::cache::ScaledImageCache>,
) -> Result<ProcessedRaster, RasterError> {
    process_raster_inner(params, scaled_cache)
}

pub fn process_raster(params: RasterProcessingParams) -> Result<ProcessedRaster, RasterError> {
    process_raster_inner(params, None)
}

fn process_raster_inner(
    params: RasterProcessingParams,
    scaled_cache: Option<&crate::cache::ScaledImageCache>,
) -> Result<ProcessedRaster, RasterError> {
    // 1. Validate parameters
    let (width_mm, height_mm) = params.bounds_mm;
    if width_mm <= 0.0 {
        return Err(RasterError::InvalidDimensions(
            "Width must be greater than zero".to_string(),
        ));
    }
    if height_mm <= 0.0 {
        return Err(RasterError::InvalidDimensions(
            "Height must be greater than zero".to_string(),
        ));
    }
    if params.dpi == 0 {
        return Err(RasterError::InvalidDimensions(
            "DPI must be greater than zero".to_string(),
        ));
    }

    // Pass-through mode: decode only, no rescale, no adjustments, no saturation, threshold to binary
    if params.pass_through {
        let img = decode_image(&params.source_bytes)?;
        let native_w = img.width();
        let native_h = img.height();
        let x_pixel_mm = if native_w > 0 {
            width_mm / native_w as f64
        } else {
            25.4 / params.dpi as f64
        };
        let line_interval_mm = if native_h > 0 {
            height_mm / native_h as f64
        } else {
            25.4 / params.dpi as f64
        };
        let data = apply_threshold(&img, params.adjustments.threshold);
        return Ok(ProcessedRaster {
            width_px: native_w,
            height_px: native_h,
            line_interval_mm,
            x_pixel_mm,
            format: RasterPixelFormat::Binary,
            data,
        });
    }

    // 2-4. Decode + scale (cached when scaled_cache is provided)
    let target_w_px = (width_mm / 25.4 * params.dpi as f64).round() as u32;
    let target_h_px = (height_mm / 25.4 * params.dpi as f64).round() as u32;
    let line_interval_mm = 25.4 / params.dpi as f64;

    let scaled_key = scaled_cache.map(|_| crate::cache::scaled_image_key(&params));
    let cached_scaled = scaled_key
        .as_deref()
        .and_then(|k| scaled_cache.and_then(|c| c.get(k)));

    let scaled = if let Some(arc) = cached_scaled {
        arc.image.clone()
    } else {
        // Decode image (with saturation pre-processing if needed)
        let img = if (params.adjustments.saturation - 1.0).abs() > f64::EPSILON {
            use crate::adjust::saturation_adjust;
            use crate::decode::decode_image_rgba;
            let rgba = decode_image_rgba(&params.source_bytes)?;
            let saturated = saturation_adjust(&rgba, params.adjustments.saturation);
            image::DynamicImage::ImageRgba8(saturated).to_luma8()
        } else {
            decode_image(&params.source_bytes)?
        };

        // Scale to target dimensions
        let result = scale_to_target(&img, target_w_px, target_h_px);

        // Cache for future slider drags
        if let (Some(cache), Some(key)) = (scaled_cache, scaled_key.as_deref()) {
            cache.insert(
                key.to_string(),
                std::sync::Arc::new(crate::cache::ScaledImage {
                    image: result.clone(),
                    target_w: target_w_px,
                    target_h: target_h_px,
                }),
            );
        }

        result
    };

    // 5. Apply adjustments
    let mut adjusted = scaled;
    apply_adjustments(&mut adjusted, &params.adjustments);

    // 5b. Apply convolution filters (sharpen, edge enhance)
    if params.adjustments.sharpen > 0.0 {
        crate::filters::sharpen(&mut adjusted, params.adjustments.sharpen);
    }
    if params.adjustments.edge_enhance {
        crate::filters::edge_enhance(&mut adjusted);
    }
    if params.adjustments.enhance_radius > 0.0 && params.adjustments.enhance_amount > 0.0 {
        crate::filters::unsharp_mask(
            &mut adjusted,
            params.adjustments.enhance_radius,
            params.adjustments.enhance_amount,
            params.adjustments.enhance_denoise,
        );
    }

    // 5c. Apply layer-level invert (negative image) after all adjustments
    // and before dithering. Per-object invert lives in adjustments.invert
    // and is already applied by apply_adjustments above.
    if params.invert {
        for pixel in adjusted.as_mut() {
            *pixel = 255 - *pixel;
        }
    }

    // 6. Convert to output format based on mode
    let (format, data) = match params.mode {
        RasterMode::Grayscale => {
            let data = adjusted.as_raw().clone();
            (RasterPixelFormat::Grayscale8, data)
        }
        RasterMode::Threshold => {
            let data = apply_threshold(&adjusted, params.adjustments.threshold);
            (RasterPixelFormat::Binary, data)
        }
        RasterMode::FloydSteinberg => {
            let data = floyd_steinberg(&adjusted);
            (RasterPixelFormat::Binary, data)
        }
        RasterMode::OrderedDither => {
            let data = ordered_dither(&adjusted);
            (RasterPixelFormat::Binary, data)
        }
        RasterMode::Stucki => {
            let data = stucki(&adjusted);
            (RasterPixelFormat::Binary, data)
        }
        RasterMode::Jarvis => {
            let data = jarvis(&adjusted);
            (RasterPixelFormat::Binary, data)
        }
        RasterMode::Sierra => {
            let data = sierra(&adjusted);
            (RasterPixelFormat::Binary, data)
        }
        RasterMode::Atkinson => {
            let data = atkinson(&adjusted);
            (RasterPixelFormat::Binary, data)
        }
        RasterMode::Halftone => {
            let cell_size = params
                .dpi
                .checked_div(params.halftone_cells_per_inch)
                .unwrap_or(params.dpi / 10)
                .max(1);
            let data = halftone_dither(&adjusted, cell_size, params.halftone_angle_deg);
            (RasterPixelFormat::Binary, data)
        }
        RasterMode::Newsprint => {
            let data = newsprint_dither(
                &adjusted,
                params.newsprint_angle_deg,
                params.newsprint_frequency.max(1.0),
                params.dpi,
            );
            (RasterPixelFormat::Binary, data)
        }
        RasterMode::Sketch => {
            let data = sketch_dither(&adjusted);
            (RasterPixelFormat::Binary, data)
        }
    };

    Ok(ProcessedRaster {
        width_px: target_w_px,
        height_px: target_h_px,
        line_interval_mm,
        x_pixel_mm: line_interval_mm,
        format,
        data,
    })
}

/// Generate crosshatch line coordinates at given angle.
///
/// Returns (start_x, start_y, end_x, end_y) tuples for each line.
/// Lines are spaced by line_spacing pixels.
pub fn crosshatch_lines(
    width_px: u32,
    height_px: u32,
    line_spacing: u32,
    angle_deg: f64,
) -> Vec<(u32, u32, u32, u32)> {
    let mut lines = Vec::new();

    let angle_rad = angle_deg.to_radians();
    let cos_a = angle_rad.cos();
    let sin_a = angle_rad.sin();

    // For simplicity, we'll generate horizontal lines and rotate them
    // Number of lines based on spacing
    let num_lines = (height_px as f64 / line_spacing as f64).ceil() as u32;

    for i in 0..num_lines {
        let y = i * line_spacing;
        if y >= height_px {
            break;
        }

        // Line endpoints in original space
        let x1 = 0;
        let y1 = y;
        let x2 = width_px.saturating_sub(1);
        let y2 = y;

        // Rotate around image center
        let cx = width_px as f64 / 2.0;
        let cy = height_px as f64 / 2.0;

        let x1f = x1 as f64 - cx;
        let y1f = y1 as f64 - cy;
        let x2f = x2 as f64 - cx;
        let y2f = y2 as f64 - cy;

        let rx1 = x1f * cos_a - y1f * sin_a + cx;
        let ry1 = x1f * sin_a + y1f * cos_a + cy;
        let rx2 = x2f * cos_a - y2f * sin_a + cx;
        let ry2 = x2f * sin_a + y2f * cos_a + cy;

        // Clamp to image bounds
        let rx1 = rx1.clamp(0.0, width_px as f64 - 1.0) as u32;
        let ry1 = ry1.clamp(0.0, height_px as f64 - 1.0) as u32;
        let rx2 = rx2.clamp(0.0, width_px as f64 - 1.0) as u32;
        let ry2 = ry2.clamp(0.0, height_px as f64 - 1.0) as u32;

        lines.push((rx1, ry1, rx2, ry2));
    }

    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use beambench_common::RasterAdjustments;
    use image::{GrayImage, ImageEncoder, Luma};

    /// Helper to create a PNG byte array from a GrayImage.
    fn encode_png(img: &GrayImage) -> Vec<u8> {
        let mut bytes = Vec::new();
        let encoder = image::codecs::png::PngEncoder::new(&mut bytes);
        encoder
            .write_image(
                img.as_raw(),
                img.width(),
                img.height(),
                image::ExtendedColorType::L8,
            )
            .unwrap();
        bytes
    }

    #[test]
    fn full_pipeline_white_png_grayscale_mode() {
        let img = GrayImage::from_pixel(2, 2, Luma([255u8]));
        let bytes = encode_png(&img);

        let params = RasterProcessingParams {
            source_bytes: bytes,
            bounds_mm: (25.4, 25.4), // 1 inch x 1 inch
            dpi: 2,                  // 2x2 pixels
            mode: RasterMode::Grayscale,
            adjustments: RasterAdjustments::default(),
            pass_through: false,
            halftone_cells_per_inch: 10,
            halftone_angle_deg: 0.0,
            newsprint_angle_deg: 45.0,
            newsprint_frequency: 10.0,
            invert: false,
        };

        let result = process_raster(params).unwrap();
        assert_eq!(result.width_px, 2);
        assert_eq!(result.height_px, 2);
        assert_eq!(result.format, RasterPixelFormat::Grayscale8);
        assert_eq!(result.data.len(), 4); // 2x2 pixels
        assert!(result.line_interval_mm > 0.0);
    }

    #[test]
    fn full_pipeline_threshold_mode() {
        let img = GrayImage::from_pixel(2, 2, Luma([255u8]));
        let bytes = encode_png(&img);

        let params = RasterProcessingParams {
            source_bytes: bytes,
            bounds_mm: (25.4, 25.4),
            dpi: 2,
            mode: RasterMode::Threshold,
            adjustments: RasterAdjustments::default(),
            pass_through: false,
            halftone_cells_per_inch: 10,
            halftone_angle_deg: 0.0,
            newsprint_angle_deg: 45.0,
            newsprint_frequency: 10.0,
            invert: false,
        };

        let result = process_raster(params).unwrap();
        assert_eq!(result.width_px, 2);
        assert_eq!(result.height_px, 2);
        assert_eq!(result.format, RasterPixelFormat::Binary);
        // 2 pixels per row = 1 byte, 2 rows = 2 bytes
        assert_eq!(result.data.len(), 2);
    }

    #[test]
    fn full_pipeline_floyd_steinberg_mode() {
        let img = GrayImage::from_pixel(4, 4, Luma([128u8]));
        let bytes = encode_png(&img);

        let params = RasterProcessingParams {
            source_bytes: bytes,
            bounds_mm: (25.4, 25.4),
            dpi: 4,
            mode: RasterMode::FloydSteinberg,
            adjustments: RasterAdjustments::default(),
            pass_through: false,
            halftone_cells_per_inch: 10,
            halftone_angle_deg: 0.0,
            newsprint_angle_deg: 45.0,
            newsprint_frequency: 10.0,
            invert: false,
        };

        let result = process_raster(params).unwrap();
        assert_eq!(result.width_px, 4);
        assert_eq!(result.height_px, 4);
        assert_eq!(result.format, RasterPixelFormat::Binary);
        assert_eq!(result.data.len(), 4); // 4 pixels per row = 1 byte, 4 rows
    }

    #[test]
    fn full_pipeline_ordered_dither_mode() {
        let img = GrayImage::from_pixel(4, 4, Luma([128u8]));
        let bytes = encode_png(&img);

        let params = RasterProcessingParams {
            source_bytes: bytes,
            bounds_mm: (25.4, 25.4),
            dpi: 4,
            mode: RasterMode::OrderedDither,
            adjustments: RasterAdjustments::default(),
            pass_through: false,
            halftone_cells_per_inch: 10,
            halftone_angle_deg: 0.0,
            newsprint_angle_deg: 45.0,
            newsprint_frequency: 10.0,
            invert: false,
        };

        let result = process_raster(params).unwrap();
        assert_eq!(result.width_px, 4);
        assert_eq!(result.height_px, 4);
        assert_eq!(result.format, RasterPixelFormat::Binary);
        assert_eq!(result.data.len(), 4);
    }

    #[test]
    fn invalid_bytes_returns_decode_error() {
        let params = RasterProcessingParams {
            source_bytes: vec![0xFF, 0xD8, 0xFF], // Invalid image
            bounds_mm: (25.4, 25.4),
            dpi: 10,
            mode: RasterMode::Grayscale,
            adjustments: RasterAdjustments::default(),
            pass_through: false,
            halftone_cells_per_inch: 10,
            halftone_angle_deg: 0.0,
            newsprint_angle_deg: 45.0,
            newsprint_frequency: 10.0,
            invert: false,
        };

        let result = process_raster(params);
        assert!(result.is_err());
        match result {
            Err(RasterError::DecodeError(_)) => {}
            _ => panic!("Expected DecodeError"),
        }
    }

    #[test]
    fn zero_width_returns_invalid_dimensions() {
        let img = GrayImage::from_pixel(2, 2, Luma([255u8]));
        let bytes = encode_png(&img);

        let params = RasterProcessingParams {
            source_bytes: bytes,
            bounds_mm: (0.0, 25.4),
            dpi: 10,
            mode: RasterMode::Grayscale,
            adjustments: RasterAdjustments::default(),
            pass_through: false,
            halftone_cells_per_inch: 10,
            halftone_angle_deg: 0.0,
            newsprint_angle_deg: 45.0,
            newsprint_frequency: 10.0,
            invert: false,
        };

        let result = process_raster(params);
        assert!(result.is_err());
        match result {
            Err(RasterError::InvalidDimensions(_)) => {}
            _ => panic!("Expected InvalidDimensions"),
        }
    }

    #[test]
    fn zero_height_returns_invalid_dimensions() {
        let img = GrayImage::from_pixel(2, 2, Luma([255u8]));
        let bytes = encode_png(&img);

        let params = RasterProcessingParams {
            source_bytes: bytes,
            bounds_mm: (25.4, 0.0),
            dpi: 10,
            mode: RasterMode::Grayscale,
            adjustments: RasterAdjustments::default(),
            pass_through: false,
            halftone_cells_per_inch: 10,
            halftone_angle_deg: 0.0,
            newsprint_angle_deg: 45.0,
            newsprint_frequency: 10.0,
            invert: false,
        };

        let result = process_raster(params);
        assert!(result.is_err());
        match result {
            Err(RasterError::InvalidDimensions(_)) => {}
            _ => panic!("Expected InvalidDimensions"),
        }
    }

    #[test]
    fn zero_dpi_returns_invalid_dimensions() {
        let img = GrayImage::from_pixel(2, 2, Luma([255u8]));
        let bytes = encode_png(&img);

        let params = RasterProcessingParams {
            source_bytes: bytes,
            bounds_mm: (25.4, 25.4),
            dpi: 0,
            mode: RasterMode::Grayscale,
            adjustments: RasterAdjustments::default(),
            pass_through: false,
            halftone_cells_per_inch: 10,
            halftone_angle_deg: 0.0,
            newsprint_angle_deg: 45.0,
            newsprint_frequency: 10.0,
            invert: false,
        };

        let result = process_raster(params);
        assert!(result.is_err());
        match result {
            Err(RasterError::InvalidDimensions(_)) => {}
            _ => panic!("Expected InvalidDimensions"),
        }
    }

    #[test]
    fn all_black_threshold_produces_zero_bits() {
        let img = GrayImage::from_pixel(8, 2, Luma([0u8]));
        let bytes = encode_png(&img);

        let params = RasterProcessingParams {
            source_bytes: bytes,
            bounds_mm: (25.4, 25.4),
            dpi: 8,
            mode: RasterMode::Threshold,
            adjustments: RasterAdjustments::default(),
            pass_through: false,
            halftone_cells_per_inch: 10,
            halftone_angle_deg: 0.0,
            newsprint_angle_deg: 45.0,
            newsprint_frequency: 10.0,
            invert: false,
        };

        let result = process_raster(params).unwrap();
        assert_eq!(result.format, RasterPixelFormat::Binary);
        // All pixels below threshold should produce all zeros
        for byte in &result.data {
            assert_eq!(*byte, 0x00);
        }
    }

    #[test]
    fn grayscale_mode_output_size_matches_pixels() {
        let img = GrayImage::from_pixel(10, 5, Luma([100u8]));
        let bytes = encode_png(&img);

        let params = RasterProcessingParams {
            source_bytes: bytes,
            bounds_mm: (25.4, 12.7), // Should result in ~10x5 at appropriate DPI
            dpi: 10,
            mode: RasterMode::Grayscale,
            adjustments: RasterAdjustments::default(),
            pass_through: false,
            halftone_cells_per_inch: 10,
            halftone_angle_deg: 0.0,
            newsprint_angle_deg: 45.0,
            newsprint_frequency: 10.0,
            invert: false,
        };

        let result = process_raster(params).unwrap();
        assert_eq!(result.format, RasterPixelFormat::Grayscale8);
        // Grayscale8 should have width_px * height_px bytes
        assert_eq!(
            result.data.len(),
            (result.width_px * result.height_px) as usize
        );
    }

    #[test]
    fn line_interval_calculated_correctly() {
        let img = GrayImage::from_pixel(2, 2, Luma([255u8]));
        let bytes = encode_png(&img);

        let params = RasterProcessingParams {
            source_bytes: bytes,
            bounds_mm: (25.4, 25.4),
            dpi: 254, // Should give line_interval_mm = 25.4 / 254 = 0.1
            mode: RasterMode::Grayscale,
            adjustments: RasterAdjustments::default(),
            pass_through: false,
            halftone_cells_per_inch: 10,
            halftone_angle_deg: 0.0,
            newsprint_angle_deg: 45.0,
            newsprint_frequency: 10.0,
            invert: false,
        };

        let result = process_raster(params).unwrap();
        let expected_interval = 25.4 / 254.0;
        assert!((result.line_interval_mm - expected_interval).abs() < 0.001);
    }

    #[test]
    fn adjustments_applied_before_mode_conversion() {
        let img = GrayImage::from_pixel(2, 2, Luma([100u8]));
        let bytes = encode_png(&img);

        let mut adj = RasterAdjustments::default();
        adj.brightness = 1.0; // Should make all pixels white (255)

        let params = RasterProcessingParams {
            source_bytes: bytes,
            bounds_mm: (25.4, 25.4),
            dpi: 2,
            mode: RasterMode::Threshold,
            adjustments: adj,
            pass_through: false,
            halftone_cells_per_inch: 10,
            halftone_angle_deg: 0.0,
            newsprint_angle_deg: 45.0,
            newsprint_frequency: 10.0,
            invert: false,
        };

        let result = process_raster(params).unwrap();
        // After brightness adjustment, all pixels should be 255, above threshold
        // So all bits should be 1
        for byte in &result.data {
            assert_eq!(*byte, 0xC0); // 2 pixels = 2 bits = 11000000 = 0xC0
        }
    }

    #[test]
    fn dimensions_calculated_from_bounds_and_dpi() {
        let img = GrayImage::from_pixel(100, 100, Luma([128u8]));
        let bytes = encode_png(&img);

        // 2 inches x 1 inch at 50 DPI should give 100x50 pixels
        let params = RasterProcessingParams {
            source_bytes: bytes,
            bounds_mm: (50.8, 25.4), // 2 inches x 1 inch
            dpi: 50,
            mode: RasterMode::Grayscale,
            adjustments: RasterAdjustments::default(),
            pass_through: false,
            halftone_cells_per_inch: 10,
            halftone_angle_deg: 0.0,
            newsprint_angle_deg: 45.0,
            newsprint_frequency: 10.0,
            invert: false,
        };

        let result = process_raster(params).unwrap();
        assert_eq!(result.width_px, 100); // 50.8 / 25.4 * 50 = 100
        assert_eq!(result.height_px, 50); // 25.4 / 25.4 * 50 = 50
    }

    #[test]
    fn crosshatch_lines_zero_degrees_horizontal() {
        let lines = crosshatch_lines(100, 50, 10, 0.0);
        // Should have ~5 lines (50 / 10)
        assert_eq!(lines.len(), 5);

        // First line should be at y=0
        assert_eq!(lines[0].1, 0);
        assert_eq!(lines[0].3, 0);
    }

    #[test]
    fn crosshatch_lines_ninety_degrees_vertical() {
        let lines = crosshatch_lines(100, 50, 10, 90.0);
        assert!(lines.len() > 0);
        // Lines should exist (exact count depends on rotation)
    }

    #[test]
    fn crosshatch_lines_returns_correct_count() {
        let lines = crosshatch_lines(100, 100, 20, 0.0);
        // 100 / 20 = 5 lines
        assert_eq!(lines.len(), 5);
    }

    #[test]
    fn crosshatch_lines_small_spacing() {
        let lines = crosshatch_lines(50, 50, 5, 0.0);
        // 50 / 5 = 10 lines
        assert_eq!(lines.len(), 10);
    }

    #[test]
    fn crosshatch_lines_clamped_to_bounds() {
        let lines = crosshatch_lines(100, 100, 10, 0.0);
        for (x1, y1, x2, y2) in lines {
            assert!(x1 < 100);
            assert!(y1 < 100);
            assert!(x2 < 100);
            assert!(y2 < 100);
        }
    }

    #[test]
    fn full_pipeline_stucki_mode() {
        let img = GrayImage::from_pixel(4, 4, Luma([128u8]));
        let bytes = encode_png(&img);

        let params = RasterProcessingParams {
            source_bytes: bytes,
            bounds_mm: (25.4, 25.4),
            dpi: 4,
            mode: RasterMode::Stucki,
            adjustments: RasterAdjustments::default(),
            pass_through: false,
            halftone_cells_per_inch: 10,
            halftone_angle_deg: 0.0,
            newsprint_angle_deg: 45.0,
            newsprint_frequency: 10.0,
            invert: false,
        };

        let result = process_raster(params).unwrap();
        assert_eq!(result.width_px, 4);
        assert_eq!(result.height_px, 4);
        assert_eq!(result.format, RasterPixelFormat::Binary);
        assert_eq!(result.data.len(), 4);
    }

    #[test]
    fn full_pipeline_jarvis_mode() {
        let img = GrayImage::from_pixel(4, 4, Luma([128u8]));
        let bytes = encode_png(&img);

        let params = RasterProcessingParams {
            source_bytes: bytes,
            bounds_mm: (25.4, 25.4),
            dpi: 4,
            mode: RasterMode::Jarvis,
            adjustments: RasterAdjustments::default(),
            pass_through: false,
            halftone_cells_per_inch: 10,
            halftone_angle_deg: 0.0,
            newsprint_angle_deg: 45.0,
            newsprint_frequency: 10.0,
            invert: false,
        };

        let result = process_raster(params).unwrap();
        assert_eq!(result.width_px, 4);
        assert_eq!(result.height_px, 4);
        assert_eq!(result.format, RasterPixelFormat::Binary);
        assert_eq!(result.data.len(), 4);
    }

    #[test]
    fn full_pipeline_sierra_mode() {
        let img = GrayImage::from_pixel(4, 4, Luma([128u8]));
        let bytes = encode_png(&img);

        let params = RasterProcessingParams {
            source_bytes: bytes,
            bounds_mm: (25.4, 25.4),
            dpi: 4,
            mode: RasterMode::Sierra,
            adjustments: RasterAdjustments::default(),
            pass_through: false,
            halftone_cells_per_inch: 10,
            halftone_angle_deg: 0.0,
            newsprint_angle_deg: 45.0,
            newsprint_frequency: 10.0,
            invert: false,
        };

        let result = process_raster(params).unwrap();
        assert_eq!(result.width_px, 4);
        assert_eq!(result.height_px, 4);
        assert_eq!(result.format, RasterPixelFormat::Binary);
        assert_eq!(result.data.len(), 4);
    }

    #[test]
    fn full_pipeline_atkinson_mode() {
        let img = GrayImage::from_pixel(4, 4, Luma([128u8]));
        let bytes = encode_png(&img);

        let params = RasterProcessingParams {
            source_bytes: bytes,
            bounds_mm: (25.4, 25.4),
            dpi: 4,
            mode: RasterMode::Atkinson,
            adjustments: RasterAdjustments::default(),
            pass_through: false,
            halftone_cells_per_inch: 10,
            halftone_angle_deg: 0.0,
            newsprint_angle_deg: 45.0,
            newsprint_frequency: 10.0,
            invert: false,
        };

        let result = process_raster(params).unwrap();
        assert_eq!(result.width_px, 4);
        assert_eq!(result.height_px, 4);
        assert_eq!(result.format, RasterPixelFormat::Binary);
        assert_eq!(result.data.len(), 4);
    }

    #[test]
    fn saturation_one_identical_to_no_saturation() {
        let img = GrayImage::from_pixel(4, 4, Luma([128u8]));
        let bytes = encode_png(&img);

        let mut adj = RasterAdjustments::default();
        adj.saturation = 1.0;

        let params = RasterProcessingParams {
            source_bytes: bytes.clone(),
            bounds_mm: (25.4, 25.4),
            dpi: 4,
            mode: RasterMode::Grayscale,
            adjustments: adj,
            pass_through: false,
            halftone_cells_per_inch: 10,
            halftone_angle_deg: 0.0,
            newsprint_angle_deg: 45.0,
            newsprint_frequency: 10.0,
            invert: false,
        };
        let result_with = process_raster(params).unwrap();

        let params_default = RasterProcessingParams {
            source_bytes: bytes,
            bounds_mm: (25.4, 25.4),
            dpi: 4,
            mode: RasterMode::Grayscale,
            adjustments: RasterAdjustments::default(),
            pass_through: false,
            halftone_cells_per_inch: 10,
            halftone_angle_deg: 0.0,
            newsprint_angle_deg: 45.0,
            newsprint_frequency: 10.0,
            invert: false,
        };
        let result_without = process_raster(params_default).unwrap();

        assert_eq!(result_with.data, result_without.data);
    }

    #[test]
    fn saturation_zero_desaturates() {
        // Use an RGBA PNG so saturation adjustment has an effect
        use image::{Rgba, RgbaImage};
        let mut rgba = RgbaImage::new(4, 4);
        for y in 0..4 {
            for x in 0..4 {
                rgba.put_pixel(x, y, Rgba([255, 0, 0, 255])); // Pure red
            }
        }
        let mut buf = Vec::new();
        let encoder = image::codecs::png::PngEncoder::new(&mut buf);
        image::ImageEncoder::write_image(
            encoder,
            rgba.as_raw(),
            4,
            4,
            image::ExtendedColorType::Rgba8,
        )
        .unwrap();

        let mut adj = RasterAdjustments::default();
        adj.saturation = 0.0; // Full desaturation

        let params = RasterProcessingParams {
            source_bytes: buf,
            bounds_mm: (25.4, 25.4),
            dpi: 4,
            mode: RasterMode::Grayscale,
            adjustments: adj,
            pass_through: false,
            halftone_cells_per_inch: 10,
            halftone_angle_deg: 0.0,
            newsprint_angle_deg: 45.0,
            newsprint_frequency: 10.0,
            invert: false,
        };
        let result = process_raster(params).unwrap();
        // After desaturation, all pixels should be the same gray value
        let first = result.data[0];
        for &v in &result.data {
            assert_eq!(
                v, first,
                "All pixels should be same gray after desaturation"
            );
        }
    }

    #[test]
    fn full_pipeline_with_sharpen() {
        let img = GrayImage::from_pixel(4, 4, Luma([128u8]));
        let bytes = encode_png(&img);

        let mut adj = RasterAdjustments::default();
        adj.sharpen = 1.0;

        let params = RasterProcessingParams {
            source_bytes: bytes,
            bounds_mm: (25.4, 25.4),
            dpi: 4,
            mode: RasterMode::Grayscale,
            adjustments: adj,
            pass_through: false,
            halftone_cells_per_inch: 10,
            halftone_angle_deg: 0.0,
            newsprint_angle_deg: 45.0,
            newsprint_frequency: 10.0,
            invert: false,
        };

        let result = process_raster(params).unwrap();
        assert_eq!(result.width_px, 4);
        assert_eq!(result.height_px, 4);
        assert_eq!(result.format, RasterPixelFormat::Grayscale8);
    }

    #[test]
    fn full_pipeline_with_edge_enhance() {
        let img = GrayImage::from_pixel(4, 4, Luma([128u8]));
        let bytes = encode_png(&img);

        let mut adj = RasterAdjustments::default();
        adj.edge_enhance = true;

        let params = RasterProcessingParams {
            source_bytes: bytes,
            bounds_mm: (25.4, 25.4),
            dpi: 4,
            mode: RasterMode::Grayscale,
            adjustments: adj,
            pass_through: false,
            halftone_cells_per_inch: 10,
            halftone_angle_deg: 0.0,
            newsprint_angle_deg: 45.0,
            newsprint_frequency: 10.0,
            invert: false,
        };

        let result = process_raster(params).unwrap();
        assert_eq!(result.width_px, 4);
        assert_eq!(result.height_px, 4);
        assert_eq!(result.format, RasterPixelFormat::Grayscale8);
    }

    #[test]
    fn layer_level_invert_negates_grayscale_pixels() {
        // A uniform mid-grey image (64) should become (255 - 64 = 191)
        // after layer-level invert.
        let img = GrayImage::from_pixel(3, 3, Luma([64u8]));
        let bytes = encode_png(&img);

        let params = RasterProcessingParams {
            source_bytes: bytes,
            bounds_mm: (25.4, 25.4),
            dpi: 3,
            mode: RasterMode::Grayscale,
            adjustments: RasterAdjustments::default(),
            pass_through: false,
            halftone_cells_per_inch: 10,
            halftone_angle_deg: 0.0,
            newsprint_angle_deg: 45.0,
            newsprint_frequency: 10.0,
            invert: true,
        };

        let result = process_raster(params).unwrap();
        assert_eq!(result.format, RasterPixelFormat::Grayscale8);
        // Every pixel should now be inverted from 64 to 191.
        for &p in result.data.iter() {
            assert_eq!(p, 191, "layer invert should map 64 -> 191");
        }
    }

    #[test]
    fn layer_level_invert_does_not_touch_pass_through() {
        // Pass-through mode skips rescale/adjustments AND layer-level
        // invert (which is applied inside the adjustments path). The
        // layer-level invert flag must be ignored here.
        let img = GrayImage::from_pixel(2, 2, Luma([0u8]));
        let bytes = encode_png(&img);

        let params = RasterProcessingParams {
            source_bytes: bytes,
            bounds_mm: (25.4, 25.4),
            dpi: 2,
            mode: RasterMode::Threshold,
            adjustments: RasterAdjustments::default(),
            pass_through: true,
            halftone_cells_per_inch: 10,
            halftone_angle_deg: 0.0,
            newsprint_angle_deg: 45.0,
            newsprint_frequency: 10.0,
            invert: true,
        };

        let result = process_raster(params).unwrap();
        assert_eq!(result.format, RasterPixelFormat::Binary);
    }
}

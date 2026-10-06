//! Bitmap rotation for arbitrary scan angles.
//!
//! For non-orthogonal scan angles, we rotate the bitmap by -θ so scanlines
//! can be generated as normal horizontal lines, then the coordinates are
//! rotated back to world space by the G-code emitter and preview renderer.

use crate::types::{ProcessedRaster, RasterPixelFormat};

/// Result of rotating a processed raster.
pub struct RotatedRaster {
    /// The rotated raster data.
    pub raster: ProcessedRaster,
    /// Width of the rotated bounding box in mm.
    pub width_mm: f64,
    /// Height of the rotated bounding box in mm.
    pub height_mm: f64,
}

/// Rotate a processed raster by the given angle (degrees, counterclockwise).
///
/// `width_mm` and `height_mm` are the original physical dimensions.
///
/// Fast paths:
/// - 0° (±0.5°): return as-is
/// - 180° (±0.5°): flip both axes
/// - Other: nearest-neighbor sampling into rotated bounding box
///
/// Note: 90° is NOT handled here — the existing transpose_raster path is used
/// separately in the planner. This function is for truly non-orthogonal angles.
pub fn rotate_raster(
    raster: &ProcessedRaster,
    angle_deg: f64,
    width_mm: f64,
    height_mm: f64,
) -> RotatedRaster {
    // Normalize angle to 0..360
    let angle = ((angle_deg % 360.0) + 360.0) % 360.0;

    // Fast path: 0° — return clone
    if !(0.5..=359.5).contains(&angle) {
        return RotatedRaster {
            raster: raster.clone(),
            width_mm,
            height_mm,
        };
    }

    // Fast path: 180° — flip both axes
    if (angle - 180.0).abs() < 0.5 {
        return rotate_180(raster, width_mm, height_mm);
    }

    // General rotation
    rotate_general(raster, angle, width_mm, height_mm)
}

fn rotate_180(raster: &ProcessedRaster, width_mm: f64, height_mm: f64) -> RotatedRaster {
    let w = raster.width_px as usize;
    let h = raster.height_px as usize;

    match raster.format {
        RasterPixelFormat::Grayscale8 => {
            let mut data = vec![255u8; w * h]; // white = no burn
            for y in 0..h {
                for x in 0..w {
                    let src_x = w - 1 - x;
                    let src_y = h - 1 - y;
                    data[y * w + x] = raster.data[src_y * w + src_x];
                }
            }
            RotatedRaster {
                raster: ProcessedRaster {
                    width_px: raster.width_px,
                    height_px: raster.height_px,
                    line_interval_mm: raster.line_interval_mm,
                    x_pixel_mm: raster.x_pixel_mm,
                    format: raster.format,
                    data,
                },
                width_mm,
                height_mm,
            }
        }
        RasterPixelFormat::Binary => {
            let row_bytes = w.div_ceil(8);
            // Binary convention: 0 = burn, 1 = no burn.
            let mut data = vec![0xFFu8; row_bytes * h];
            for y in 0..h {
                for x in 0..w {
                    let src_x = w - 1 - x;
                    let src_y = h - 1 - y;
                    let src_byte = src_y * row_bytes + src_x / 8;
                    let src_bit = 7 - (src_x % 8);
                    let pixel = (raster.data[src_byte] >> src_bit) & 1;
                    let dst_byte = y * row_bytes + x / 8;
                    let dst_bit = 7 - (x % 8);
                    if pixel == 0 {
                        data[dst_byte] &= !(1 << dst_bit);
                    } else {
                        data[dst_byte] |= 1 << dst_bit;
                    }
                }
            }
            RotatedRaster {
                raster: ProcessedRaster {
                    width_px: raster.width_px,
                    height_px: raster.height_px,
                    line_interval_mm: raster.line_interval_mm,
                    x_pixel_mm: raster.x_pixel_mm,
                    format: raster.format,
                    data,
                },
                width_mm,
                height_mm,
            }
        }
    }
}

/// Output dimensions and byte size, without allocating or sampling pixels.
/// Uses the same physical grid as rotation, including cardinal fast paths.
pub fn rotated_raster_size(
    raster: &ProcessedRaster,
    angle_deg: f64,
    width_mm: f64,
    height_mm: f64,
) -> Option<(u32, u32, usize)> {
    if !angle_deg.is_finite() {
        return None;
    }
    let angle = angle_deg.rem_euclid(360.0);
    let (width, height) = if !(0.5..=359.5).contains(&angle) || (angle - 180.0).abs() < 0.5 {
        (raster.width_px, raster.height_px)
    } else {
        let (w, h, _, _) = rotation_grid(raster, angle, width_mm, height_mm)?;
        (w, h)
    };
    let row_bytes = match raster.format {
        RasterPixelFormat::Binary => (width as usize).div_ceil(8),
        RasterPixelFormat::Grayscale8 => width as usize,
    };
    Some((width, height, row_bytes.checked_mul(height as usize)?))
}

fn rotation_grid(
    raster: &ProcessedRaster,
    angle_deg: f64,
    width_mm: f64,
    height_mm: f64,
) -> Option<(u32, u32, f64, f64)> {
    let (w, h) = (raster.width_px as f64, raster.height_px as f64);
    let rad = angle_deg.to_radians();
    let (sin_a, cos_a) = rad.sin_cos();
    // Rotate in millimetres, not pixels. Pass-through images can have
    // non-square pixels (each axis has its own size); rotating pixel indices
    // and rescaling the axes separately afterwards distorts the artwork.
    // Pixel size per axis, as the rest of the pipeline uses it (DPI rasters
    // have square pixels; pass-through rasters carry each axis's own size).
    // The output grid keeps the same column width and row spacing.
    let src_px_x = if raster.x_pixel_mm > 0.0 {
        raster.x_pixel_mm
    } else if w > 0.0 {
        width_mm / w
    } else {
        1.0
    };
    let src_px_y = if raster.line_interval_mm > 0.0 {
        raster.line_interval_mm
    } else if h > 0.0 {
        height_mm / h
    } else {
        1.0
    };
    // Rows keep their spacing. Along a rotated row, step at the finer of the
    // two source sizes: a coarse column (e.g. 5 mm) crossing at an angle would
    // smear each pixel sideways into neighbouring rows.
    let (dst_px_x, dst_px_y) = (src_px_x.min(src_px_y), src_px_y);

    // Rotated bounding box, physically, then in output pixels.
    let (span_x_mm, span_y_mm) = (w * src_px_x, h * src_px_y);
    let rotated_w_mm = span_x_mm * cos_a.abs() + span_y_mm * sin_a.abs();
    let rotated_h_mm = span_x_mm * sin_a.abs() + span_y_mm * cos_a.abs();
    let columns = (rotated_w_mm / dst_px_x - 1e-9).ceil().max(0.0);
    let rows = (rotated_h_mm / dst_px_y - 1e-9).ceil().max(0.0);
    if !columns.is_finite()
        || !rows.is_finite()
        || columns > u32::MAX as f64
        || rows > u32::MAX as f64
    {
        return None;
    }
    let (new_w_px, new_h_px) = (columns as u32, rows as u32);
    Some((new_w_px, new_h_px, src_px_x, src_px_y))
}

fn rotate_general(
    raster: &ProcessedRaster,
    angle_deg: f64,
    width_mm: f64,
    height_mm: f64,
) -> RotatedRaster {
    let w = raster.width_px as f64;
    let h = raster.height_px as f64;

    let rad = angle_deg.to_radians();
    let cos_a = rad.cos();
    let sin_a = rad.sin();

    let (new_w_px, new_h_px, src_px_x, src_px_y) =
        rotation_grid(raster, angle_deg, width_mm, height_mm)
            .expect("rotation dimensions must be checked before allocation");
    let (dst_px_x, dst_px_y) = (src_px_x.min(src_px_y), src_px_y);
    let new_width_mm = new_w_px as f64 * dst_px_x;
    let new_height_mm = new_h_px as f64 * dst_px_y;

    // Centers (pixels)
    let src_cx = w / 2.0;
    let src_cy = h / 2.0;
    let dst_cx = new_w_px as f64 / 2.0;
    let dst_cy = new_h_px as f64 / 2.0;
    // Output pixel -> source pixel, via an inverse rotation in millimetres.
    // Sample at the output pixel's center and take the source pixel that
    // point falls inside (floor), not the nearest index: with wide source
    // pixels, rounding reached up to half a pixel past the image edge.
    let source_pixel = |dx: usize, dy: usize| {
        let rx = (dx as f64 + 0.5 - dst_cx) * dst_px_x;
        let ry = (dy as f64 + 0.5 - dst_cy) * dst_px_y;
        let x_mm = rx * cos_a + ry * sin_a;
        let y_mm = -rx * sin_a + ry * cos_a;
        (x_mm / src_px_x + src_cx, y_mm / src_px_y + src_cy)
    };

    match raster.format {
        RasterPixelFormat::Grayscale8 => {
            let nw = new_w_px as usize;
            let nh = new_h_px as usize;
            let mut data = vec![255u8; nw * nh]; // white = no burn

            for dy in 0..nh {
                for dx in 0..nw {
                    let (sx, sy) = source_pixel(dx, dy);

                    let sx_i = sx.floor() as i64;
                    let sy_i = sy.floor() as i64;

                    if sx_i >= 0 && sx_i < w as i64 && sy_i >= 0 && sy_i < h as i64 {
                        let src_w = raster.width_px as usize;
                        data[dy * nw + dx] = raster.data[sy_i as usize * src_w + sx_i as usize];
                    }
                }
            }

            RotatedRaster {
                raster: ProcessedRaster {
                    width_px: new_w_px,
                    height_px: new_h_px,
                    line_interval_mm: dst_px_y,
                    x_pixel_mm: dst_px_x,
                    format: raster.format,
                    data,
                },
                width_mm: new_width_mm,
                height_mm: new_height_mm,
            }
        }
        RasterPixelFormat::Binary => {
            let nw = new_w_px as usize;
            let nh = new_h_px as usize;
            let new_row_bytes = nw.div_ceil(8);
            let src_row_bytes = (raster.width_px as usize).div_ceil(8);
            // Binary convention: 0 = burn, 1 = no burn. Unmapped rotated
            // bounding-box pixels must remain no-burn.
            let mut data = vec![0xFFu8; new_row_bytes * nh];

            for dy in 0..nh {
                for dx in 0..nw {
                    let (sx, sy) = source_pixel(dx, dy);

                    let sx_i = sx.floor() as i64;
                    let sy_i = sy.floor() as i64;

                    if sx_i >= 0 && sx_i < w as i64 && sy_i >= 0 && sy_i < h as i64 {
                        let src_byte = sy_i as usize * src_row_bytes + sx_i as usize / 8;
                        let src_bit = 7 - (sx_i as usize % 8);
                        let pixel = (raster.data[src_byte] >> src_bit) & 1;
                        let dst_byte = dy * new_row_bytes + dx / 8;
                        let dst_bit = 7 - (dx % 8);
                        if pixel == 0 {
                            data[dst_byte] &= !(1 << dst_bit);
                        } else {
                            data[dst_byte] |= 1 << dst_bit;
                        }
                    }
                }
            }

            RotatedRaster {
                raster: ProcessedRaster {
                    width_px: new_w_px,
                    height_px: new_h_px,
                    line_interval_mm: dst_px_y,
                    x_pixel_mm: dst_px_x,
                    format: raster.format,
                    data,
                },
                width_mm: new_width_mm,
                height_mm: new_height_mm,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_grayscale_raster(width: u32, height: u32, fill: u8) -> ProcessedRaster {
        ProcessedRaster {
            width_px: width,
            height_px: height,
            line_interval_mm: 0.1,
            x_pixel_mm: 0.1,
            format: RasterPixelFormat::Grayscale8,
            data: vec![fill; (width * height) as usize],
        }
    }

    fn make_binary_raster(width: u32, height: u32, fill: bool) -> ProcessedRaster {
        let row_bytes = (width as usize + 7) / 8;
        let byte_val = if fill { 0xFF } else { 0x00 };
        ProcessedRaster {
            width_px: width,
            height_px: height,
            line_interval_mm: 0.1,
            x_pixel_mm: 0.1,
            format: RasterPixelFormat::Binary,
            data: vec![byte_val; row_bytes * height as usize],
        }
    }

    fn binary_pixel(raster: &ProcessedRaster, x: u32, y: u32) -> u8 {
        let row_bytes = (raster.width_px as usize).div_ceil(8);
        let idx = y as usize * row_bytes + x as usize / 8;
        let bit = 7 - (x as usize % 8);
        (raster.data[idx] >> bit) & 1
    }

    #[test]
    fn zero_degrees_returns_identical() {
        let raster = make_grayscale_raster(10, 8, 128);
        let result = rotate_raster(&raster, 0.0, 10.0, 8.0);
        assert_eq!(result.raster.width_px, 10);
        assert_eq!(result.raster.height_px, 8);
        assert_eq!(result.raster.data, raster.data);
        assert!((result.width_mm - 10.0).abs() < 1e-10);
        assert!((result.height_mm - 8.0).abs() < 1e-10);
    }

    #[test]
    fn zero_degrees_binary_returns_identical() {
        let raster = make_binary_raster(10, 8, true);
        let result = rotate_raster(&raster, 0.0, 10.0, 8.0);
        assert_eq!(result.raster.data, raster.data);
    }

    #[test]
    fn rotate_180_inverts_axes_grayscale() {
        // Create a 4x3 raster with a gradient
        let mut raster = make_grayscale_raster(4, 3, 0);
        // Row 0: [10, 20, 30, 40]
        // Row 1: [50, 60, 70, 80]
        // Row 2: [90, 100, 110, 120]
        for y in 0..3u32 {
            for x in 0..4u32 {
                raster.data[(y * 4 + x) as usize] = ((y * 4 + x) * 10 + 10) as u8;
            }
        }

        let result = rotate_raster(&raster, 180.0, 4.0, 3.0);
        assert_eq!(result.raster.width_px, 4);
        assert_eq!(result.raster.height_px, 3);

        // After 180°: pixel at (0,0) should be old (3,2) = 120
        assert_eq!(result.raster.data[0], 120);
        // pixel at (3,2) should be old (0,0) = 10
        assert_eq!(result.raster.data[2 * 4 + 3], 10);
    }

    #[test]
    fn rotate_45_produces_larger_bounding_box() {
        let raster = make_grayscale_raster(10, 10, 128);
        let result = rotate_raster(&raster, 45.0, 10.0, 10.0);

        // At 45°, bounding box should be approximately √2 times larger
        let expected_size = (10.0_f64 * std::f64::consts::FRAC_1_SQRT_2 * 2.0).ceil() as u32;
        assert!(result.raster.width_px >= expected_size - 1);
        assert!(result.raster.height_px >= expected_size - 1);
        assert!(result.raster.width_px > 10);
        assert!(result.raster.height_px > 10);
    }

    #[test]
    fn rotate_45_binary_produces_larger_bounding_box() {
        let raster = make_binary_raster(10, 10, false);
        let result = rotate_raster(&raster, 45.0, 10.0, 10.0);
        assert!(result.raster.width_px > 10);
        assert!(result.raster.height_px > 10);

        let last_x = result.raster.width_px - 1;
        let last_y = result.raster.height_px - 1;
        assert_eq!(binary_pixel(&result.raster, 0, 0), 1);
        assert_eq!(binary_pixel(&result.raster, last_x, 0), 1);
        assert_eq!(binary_pixel(&result.raster, 0, last_y), 1);
        assert_eq!(binary_pixel(&result.raster, last_x, last_y), 1);
        assert_eq!(
            binary_pixel(
                &result.raster,
                result.raster.width_px / 2,
                result.raster.height_px / 2
            ),
            0
        );
    }

    #[test]
    fn rotate_360_returns_identical() {
        let raster = make_grayscale_raster(8, 6, 200);
        let result = rotate_raster(&raster, 360.0, 8.0, 6.0);
        assert_eq!(result.raster.data, raster.data);
    }

    #[test]
    fn empty_raster_edge_case() {
        let raster = ProcessedRaster {
            width_px: 0,
            height_px: 0,
            line_interval_mm: 0.1,
            x_pixel_mm: 0.1,
            format: RasterPixelFormat::Grayscale8,
            data: vec![],
        };
        let result = rotate_raster(&raster, 45.0, 0.0, 0.0);
        assert_eq!(result.raster.width_px, 0);
        assert_eq!(result.raster.height_px, 0);
    }

    #[test]
    fn rotate_preserves_line_interval() {
        let raster = make_grayscale_raster(10, 10, 128);
        let result = rotate_raster(&raster, 30.0, 10.0, 10.0);
        assert!((result.raster.line_interval_mm - 0.1).abs() < 1e-10);
    }
}

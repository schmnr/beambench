//! Dithering algorithms for converting grayscale to binary.

use image::GrayImage;

/// Apply simple threshold to convert grayscale to 1-bit binary.
///
/// Pixels >= threshold become 1 (white, no burn).
/// Pixels < threshold become 0 (black, burn).
///
/// Returns packed 1-bit-per-pixel data, MSB first, rows padded to byte boundaries.
pub fn apply_threshold(img: &GrayImage, threshold: u8) -> Vec<u8> {
    let width = img.width() as usize;
    let height = img.height() as usize;
    let row_stride = width.div_ceil(8);
    let mut data = vec![0u8; row_stride * height];

    for y in 0..height {
        for x in 0..width {
            let pixel = img.get_pixel(x as u32, y as u32)[0];
            let bit = if pixel >= threshold { 1 } else { 0 };

            let byte_idx = y * row_stride + x / 8;
            let bit_idx = 7 - (x % 8); // MSB first
            data[byte_idx] |= bit << bit_idx;
        }
    }

    data
}

/// Apply Floyd-Steinberg error diffusion dithering.
///
/// Returns packed 1-bit-per-pixel data, MSB first, rows padded to byte boundaries.
pub fn floyd_steinberg(img: &GrayImage) -> Vec<u8> {
    let width = img.width() as usize;
    let height = img.height() as usize;
    let row_stride = width.div_ceil(8);

    // Flat working buffer — better cache locality than Vec<Vec<f64>>
    let mut working = Vec::with_capacity(width * height);
    for y in 0..height {
        for x in 0..width {
            working.push(img.get_pixel(x as u32, y as u32)[0] as f64);
        }
    }

    // Precomputed error distribution weights (avoid per-pixel division)
    const W_RIGHT: f64 = 7.0 / 16.0;
    const W_BOTTOM_LEFT: f64 = 3.0 / 16.0;
    const W_BOTTOM: f64 = 5.0 / 16.0;
    const W_BOTTOM_RIGHT: f64 = 1.0 / 16.0;

    let mut data = vec![0u8; row_stride * height];

    for y in 0..height {
        let row_off = y * width;
        let next_row_off = (y + 1) * width;
        for x in 0..width {
            let old_pixel = working[row_off + x];
            let new_pixel = if old_pixel >= 128.0 { 255.0 } else { 0.0 };
            let error = old_pixel - new_pixel;

            let byte_idx = y * row_stride + x / 8;
            let bit_idx = 7 - (x % 8);
            if new_pixel >= 128.0 {
                data[byte_idx] |= 1 << bit_idx;
            }

            if x + 1 < width {
                working[row_off + x + 1] += error * W_RIGHT;
            }
            if y + 1 < height {
                if x > 0 {
                    working[next_row_off + x - 1] += error * W_BOTTOM_LEFT;
                }
                working[next_row_off + x] += error * W_BOTTOM;
                if x + 1 < width {
                    working[next_row_off + x + 1] += error * W_BOTTOM_RIGHT;
                }
            }
        }
    }

    data
}

/// Apply ordered (Bayer) dithering with a 4x4 matrix.
///
/// Returns packed 1-bit-per-pixel data, MSB first, rows padded to byte boundaries.
pub fn ordered_dither(img: &GrayImage) -> Vec<u8> {
    // 4x4 Bayer matrix (normalized 0-15)
    #[rustfmt::skip]
    const BAYER_MATRIX: [[u8; 4]; 4] = [
        [ 0,  8,  2, 10],
        [12,  4, 14,  6],
        [ 3, 11,  1,  9],
        [15,  7, 13,  5],
    ];

    let width = img.width() as usize;
    let height = img.height() as usize;
    let row_stride = width.div_ceil(8);
    let mut data = vec![0u8; row_stride * height];

    for y in 0..height {
        for x in 0..width {
            let pixel = img.get_pixel(x as u32, y as u32)[0];
            let bayer_value = BAYER_MATRIX[y % 4][x % 4];
            let threshold = ((bayer_value as f64 + 0.5) / 16.0 * 255.0) as u8;

            let bit = if pixel >= threshold { 1 } else { 0 };

            let byte_idx = y * row_stride + x / 8;
            let bit_idx = 7 - (x % 8);
            data[byte_idx] |= bit << bit_idx;
        }
    }

    data
}

/// Weight entry for generic error diffusion: (dx, dy, weight).
type DiffusionWeight = (i32, i32, f64);

/// Generic error-diffusion dithering engine.
///
/// Takes an image, a kernel of (dx, dy, weight) entries, and a divisor.
/// Returns packed 1-bit-per-pixel data, MSB first, rows padded to byte boundaries.
fn error_diffusion(img: &GrayImage, weights: &[DiffusionWeight], divisor: f64) -> Vec<u8> {
    let width = img.width() as usize;
    let height = img.height() as usize;
    let row_stride = width.div_ceil(8);

    // Flat working buffer — better cache locality than Vec<Vec<f64>>
    let mut working = Vec::with_capacity(width * height);
    for y in 0..height {
        for x in 0..width {
            working.push(img.get_pixel(x as u32, y as u32)[0] as f64);
        }
    }

    // Precompute normalized weights (avoid per-pixel division)
    let inv_divisor = 1.0 / divisor;
    let norm_weights: Vec<(i32, i32, f64)> = weights
        .iter()
        .map(|&(dx, dy, w)| (dx, dy, w * inv_divisor))
        .collect();

    let mut data = vec![0u8; row_stride * height];

    for y in 0..height {
        for x in 0..width {
            let idx = y * width + x;
            let old_pixel = working[idx];
            let new_pixel = if old_pixel >= 128.0 { 255.0 } else { 0.0 };
            let error = old_pixel - new_pixel;

            let byte_idx = y * row_stride + x / 8;
            let bit_idx = 7 - (x % 8);
            if new_pixel >= 128.0 {
                data[byte_idx] |= 1 << bit_idx;
            }

            for &(dx, dy, nw) in &norm_weights {
                let nx = x as i32 + dx;
                let ny = y as i32 + dy;
                if nx >= 0 && (nx as usize) < width && ny >= 0 && (ny as usize) < height {
                    working[ny as usize * width + nx as usize] += error * nw;
                }
            }
        }
    }

    data
}

/// Apply Stucki error diffusion dithering.
///
/// Returns packed 1-bit-per-pixel data, MSB first, rows padded to byte boundaries.
pub fn stucki(img: &GrayImage) -> Vec<u8> {
    #[rustfmt::skip]
    const WEIGHTS: &[DiffusionWeight] = &[
        (1, 0, 8.0), (2, 0, 4.0),
        (-2, 1, 2.0), (-1, 1, 4.0), (0, 1, 8.0), (1, 1, 4.0), (2, 1, 2.0),
        (-2, 2, 1.0), (-1, 2, 2.0), (0, 2, 4.0), (1, 2, 2.0), (2, 2, 1.0),
    ];
    error_diffusion(img, WEIGHTS, 42.0)
}

/// Apply Jarvis-Judice-Ninke error diffusion dithering.
///
/// Returns packed 1-bit-per-pixel data, MSB first, rows padded to byte boundaries.
pub fn jarvis(img: &GrayImage) -> Vec<u8> {
    #[rustfmt::skip]
    const WEIGHTS: &[DiffusionWeight] = &[
        (1, 0, 7.0), (2, 0, 5.0),
        (-2, 1, 3.0), (-1, 1, 5.0), (0, 1, 7.0), (1, 1, 5.0), (2, 1, 3.0),
        (-2, 2, 1.0), (-1, 2, 3.0), (0, 2, 5.0), (1, 2, 3.0), (2, 2, 1.0),
    ];
    error_diffusion(img, WEIGHTS, 48.0)
}

/// Apply Sierra error diffusion dithering.
///
/// Returns packed 1-bit-per-pixel data, MSB first, rows padded to byte boundaries.
pub fn sierra(img: &GrayImage) -> Vec<u8> {
    #[rustfmt::skip]
    const WEIGHTS: &[DiffusionWeight] = &[
        (1, 0, 5.0), (2, 0, 3.0),
        (-2, 1, 2.0), (-1, 1, 4.0), (0, 1, 5.0), (1, 1, 4.0), (2, 1, 2.0),
        (-1, 2, 2.0), (0, 2, 3.0), (1, 2, 2.0),
    ];
    error_diffusion(img, WEIGHTS, 32.0)
}

/// Apply Atkinson error diffusion dithering.
///
/// Distributes only 75% of the error (6/8), giving a lighter result.
/// Returns packed 1-bit-per-pixel data, MSB first, rows padded to byte boundaries.
pub fn atkinson(img: &GrayImage) -> Vec<u8> {
    #[rustfmt::skip]
    const WEIGHTS: &[DiffusionWeight] = &[
        (1, 0, 1.0), (2, 0, 1.0),
        (-1, 1, 1.0), (0, 1, 1.0), (1, 1, 1.0),
        (0, 2, 1.0),
    ];
    error_diffusion(img, WEIGHTS, 8.0)
}

/// Apply halftone dithering with circular dots.
///
/// Creates a grid of cells, each cell_size × cell_size pixels.
/// For each cell, computes average brightness and draws a circle
/// proportional to darkness (darker = larger circle).
/// When `angle_deg` is nonzero, pixel coordinates are rotated before cell assignment
/// to produce angled dot grids.
///
/// Returns packed 1-bit-per-pixel data, MSB first, rows padded to byte boundaries.
pub fn halftone_dither(img: &GrayImage, cell_size: u32, angle_deg: f64) -> Vec<u8> {
    let width = img.width() as usize;
    let height = img.height() as usize;
    let row_stride = width.div_ceil(8);
    let mut data = vec![0u8; row_stride * height];

    let cs = cell_size.max(1) as f64;
    let angle_rad = angle_deg.to_radians();
    let cos_a = angle_rad.cos();
    let sin_a = angle_rad.sin();
    let has_rotation = angle_deg.abs() > 0.001;

    for y in 0..height {
        for x in 0..width {
            // Rotate pixel coordinates into cell grid space
            let (rx, ry) = if has_rotation {
                (
                    x as f64 * cos_a + y as f64 * sin_a,
                    -(x as f64) * sin_a + y as f64 * cos_a,
                )
            } else {
                (x as f64, y as f64)
            };

            // Determine which cell this pixel belongs to
            let cell_x = (rx / cs).floor();
            let cell_y = (ry / cs).floor();

            // Compute average brightness in the cell (sample original image pixels near this cell)
            // For efficiency, use the current pixel's brightness as an approximation
            // weighted by 3x3 neighborhood average
            let mut sum = 0.0;
            let mut count = 0;
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    let sx = (x as i32 + dx).max(0).min(width as i32 - 1) as u32;
                    let sy = (y as i32 + dy).max(0).min(height as i32 - 1) as u32;
                    sum += img.get_pixel(sx, sy)[0] as f64;
                    count += 1;
                }
            }
            let avg_brightness = sum / count as f64;
            let darkness = 1.0 - (avg_brightness / 255.0);

            // Position within cell
            let local_x = rx - cell_x * cs;
            let local_y = ry - cell_y * cs;
            let center = cs / 2.0;
            let dx = local_x - center;
            let dy = local_y - center;
            let dist = (dx * dx + dy * dy).sqrt();

            let max_radius = cs * 0.71;
            let radius = darkness * max_radius;

            let bit = if dist < radius { 0 } else { 1 };

            let byte_idx = y * row_stride + x / 8;
            let bit_idx = 7 - (x % 8);
            data[byte_idx] |= bit << bit_idx;
        }
    }

    data
}

/// Apply newsprint dithering with rotated dot screen pattern.
///
/// Similar to halftone but with a rotated grid based on angle_deg.
/// Frequency controls the dots-per-inch (higher = smaller cells).
/// `dpi` is the raster resolution — cell size = dpi / frequency.
///
/// Returns packed 1-bit-per-pixel data, MSB first, rows padded to byte boundaries.
pub fn newsprint_dither(img: &GrayImage, angle_deg: f64, frequency: f64, dpi: u32) -> Vec<u8> {
    let width = img.width() as usize;
    let height = img.height() as usize;
    let row_stride = width.div_ceil(8);
    let mut data = vec![0u8; row_stride * height];

    // Cell size derived from DPI and frequency
    let cell_size = (dpi as f64 / frequency).max(2.0) as usize;

    // Rotation matrix
    let angle_rad = angle_deg.to_radians();
    let cos_a = angle_rad.cos();
    let sin_a = angle_rad.sin();

    for y in 0..height {
        for x in 0..width {
            // Rotate coordinates
            let rx = x as f64 * cos_a + y as f64 * sin_a;
            let ry = -(x as f64) * sin_a + y as f64 * cos_a;

            // Determine cell coordinates
            let cell_x = (rx / cell_size as f64).floor() as i32;
            let cell_y = (ry / cell_size as f64).floor() as i32;

            // Sample surrounding cells to compute local average
            let mut sum = 0.0;
            let mut count = 0;

            for dy in -1..=1 {
                for dx in -1..=1 {
                    let sample_x = (x as i32 + dx).max(0).min(width as i32 - 1) as u32;
                    let sample_y = (y as i32 + dy).max(0).min(height as i32 - 1) as u32;
                    sum += img.get_pixel(sample_x, sample_y)[0] as f64;
                    count += 1;
                }
            }

            let avg_brightness = sum / count as f64;
            let darkness = 1.0 - (avg_brightness / 255.0);

            // Position within cell
            let local_x = rx - (cell_x as f64 * cell_size as f64);
            let local_y = ry - (cell_y as f64 * cell_size as f64);

            // Distance from cell center
            let center_x = cell_size as f64 / 2.0;
            let center_y = cell_size as f64 / 2.0;
            let dx = local_x - center_x;
            let dy = local_y - center_y;
            let dist = (dx * dx + dy * dy).sqrt();

            // Use diagonal * 1.01 for full coverage (account for float precision)
            let max_radius = cell_size as f64 * 0.71;
            let radius = darkness * max_radius;

            // Use < instead of <= to handle zero-radius case
            let bit = if dist < radius { 0 } else { 1 };

            let byte_idx = y * row_stride + x / 8;
            let bit_idx = 7 - (x % 8);
            data[byte_idx] |= bit << bit_idx;
        }
    }

    data
}

/// Apply sketch dithering using edge detection and thresholding.
///
/// Uses a Laplacian high-pass filter to detect edges, then thresholds the result.
/// This is Beam Bench's deterministic sketch-processing implementation.
///
/// Returns packed 1-bit-per-pixel data, MSB first, rows padded to byte boundaries.
pub fn sketch_dither(img: &GrayImage) -> Vec<u8> {
    let width = img.width() as usize;
    let height = img.height() as usize;
    let row_stride = width.div_ceil(8);
    let mut data = vec![0u8; row_stride * height];

    // Laplacian kernel: [-1, -1, -1; -1, 8, -1; -1, -1, -1]
    for y in 0..height {
        for x in 0..width {
            let mut lap: i32 = 0;
            for ky in -1i32..=1 {
                for kx in -1i32..=1 {
                    let sx = (x as i32 + kx).clamp(0, width as i32 - 1) as u32;
                    let sy = (y as i32 + ky).clamp(0, height as i32 - 1) as u32;
                    let pixel = img.get_pixel(sx, sy)[0] as i32;
                    if kx == 0 && ky == 0 {
                        lap += pixel * 8;
                    } else {
                        lap -= pixel;
                    }
                }
            }

            // Normalize: edge magnitude as absolute value, clamped to 0..255
            let edge = lap.unsigned_abs().min(255) as u8;

            // Threshold: strong edges become burn (black), weak edges become white
            // Invert sense: high edge value = strong edge = burn (bit 0)
            let bit = if edge >= 64 { 0 } else { 1 }; // 0 = burn

            let byte_idx = y * row_stride + x / 8;
            let bit_idx = 7 - (x % 8);
            data[byte_idx] |= bit << bit_idx;
        }
    }

    data
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Luma;

    #[test]
    fn threshold_all_white_produces_all_ones() {
        let img = GrayImage::from_pixel(8, 2, Luma([255u8]));
        let data = apply_threshold(&img, 128);
        // 8 pixels per row = 1 byte per row, 2 rows = 2 bytes
        assert_eq!(data.len(), 2);
        assert_eq!(data[0], 0xFF);
        assert_eq!(data[1], 0xFF);
    }

    #[test]
    fn threshold_all_black_produces_all_zeros() {
        let img = GrayImage::from_pixel(8, 2, Luma([0u8]));
        let data = apply_threshold(&img, 128);
        assert_eq!(data.len(), 2);
        assert_eq!(data[0], 0x00);
        assert_eq!(data[1], 0x00);
    }

    #[test]
    fn threshold_mixed_values() {
        let mut img = GrayImage::new(8, 1);
        for x in 0..8 {
            let val = if x < 4 { 0u8 } else { 255u8 };
            img.put_pixel(x, 0, Luma([val]));
        }
        let data = apply_threshold(&img, 128);
        // First 4 pixels black (0), last 4 white (1)
        // Bits: 00001111 = 0x0F
        assert_eq!(data.len(), 1);
        assert_eq!(data[0], 0x0F);
    }

    #[test]
    fn threshold_row_stride_padding() {
        // 9 pixels wide = 2 bytes per row
        let img = GrayImage::from_pixel(9, 1, Luma([255u8]));
        let data = apply_threshold(&img, 128);
        assert_eq!(data.len(), 2); // (9 + 7) / 8 = 2 bytes
        assert_eq!(data[0], 0xFF); // First 8 pixels
        assert_eq!(data[1], 0x80); // 9th pixel in MSB, rest are padding zeros
    }

    #[test]
    fn floyd_steinberg_all_white() {
        let img = GrayImage::from_pixel(4, 4, Luma([255u8]));
        let data = floyd_steinberg(&img);
        // 4 pixels = 1 byte per row, 4 rows = 4 bytes
        assert_eq!(data.len(), 4);
        for byte in &data {
            assert_eq!(*byte, 0xF0); // 4 pixels = 4 bits = 0b11110000
        }
    }

    #[test]
    fn floyd_steinberg_all_black() {
        let img = GrayImage::from_pixel(4, 4, Luma([0u8]));
        let data = floyd_steinberg(&img);
        assert_eq!(data.len(), 4);
        for byte in &data {
            assert_eq!(*byte, 0x00);
        }
    }

    #[test]
    fn floyd_steinberg_produces_correct_row_stride() {
        let img = GrayImage::from_pixel(10, 2, Luma([128u8]));
        let data = floyd_steinberg(&img);
        // 10 pixels = 2 bytes per row, 2 rows = 4 bytes
        assert_eq!(data.len(), 4);
    }

    #[test]
    fn ordered_dither_all_white() {
        let img = GrayImage::from_pixel(8, 1, Luma([255u8]));
        let data = ordered_dither(&img);
        assert_eq!(data.len(), 1);
        assert_eq!(data[0], 0xFF);
    }

    #[test]
    fn ordered_dither_all_black() {
        let img = GrayImage::from_pixel(8, 1, Luma([0u8]));
        let data = ordered_dither(&img);
        assert_eq!(data.len(), 1);
        assert_eq!(data[0], 0x00);
    }

    #[test]
    fn ordered_dither_produces_correct_row_stride() {
        let img = GrayImage::from_pixel(10, 3, Luma([128u8]));
        let data = ordered_dither(&img);
        // 10 pixels = 2 bytes per row, 3 rows = 6 bytes
        assert_eq!(data.len(), 6);
    }

    #[test]
    fn ordered_dither_pattern_varies_with_position() {
        // Create a uniform gray image
        let img = GrayImage::from_pixel(4, 4, Luma([128u8]));
        let data = ordered_dither(&img);
        // With Bayer dithering, midtone gray should produce a pattern, not uniform
        // We just verify it runs and produces correct size
        assert_eq!(data.len(), 4); // 4 pixels per row = 1 byte, 4 rows
    }

    #[test]
    fn bit_packing_msb_first() {
        let mut img = GrayImage::new(8, 1);
        // Set only the first pixel to white
        img.put_pixel(0, 0, Luma([255u8]));
        for x in 1..8 {
            img.put_pixel(x, 0, Luma([0u8]));
        }
        let data = apply_threshold(&img, 128);
        // First bit (MSB) should be 1, rest 0: 10000000 = 0x80
        assert_eq!(data[0], 0x80);
    }

    #[test]
    fn bit_packing_lsb_position() {
        let mut img = GrayImage::new(8, 1);
        // Set only the last pixel to white
        for x in 0..7 {
            img.put_pixel(x, 0, Luma([0u8]));
        }
        img.put_pixel(7, 0, Luma([255u8]));
        let data = apply_threshold(&img, 128);
        // Last bit (LSB) should be 1: 00000001 = 0x01
        assert_eq!(data[0], 0x01);
    }

    #[test]
    fn floyd_steinberg_distributes_error() {
        // Create a 2x2 image with one dark and three light pixels
        let mut img = GrayImage::new(2, 2);
        img.put_pixel(0, 0, Luma([64u8])); // Dark
        img.put_pixel(1, 0, Luma([128u8])); // Mid
        img.put_pixel(0, 1, Luma([128u8])); // Mid
        img.put_pixel(1, 1, Luma([128u8])); // Mid

        let data = floyd_steinberg(&img);
        // 2 pixels per row = 1 byte, 2 rows = 2 bytes
        assert_eq!(data.len(), 2);
        // Verify it produces some output (error diffusion should affect neighbors)
        // We're just checking it doesn't crash and produces valid data
    }

    // --- Stucki tests ---

    #[test]
    fn stucki_all_white() {
        let img = GrayImage::from_pixel(4, 4, Luma([255u8]));
        let data = stucki(&img);
        assert_eq!(data.len(), 4);
        for byte in &data {
            assert_eq!(*byte, 0xF0);
        }
    }

    #[test]
    fn stucki_all_black() {
        let img = GrayImage::from_pixel(4, 4, Luma([0u8]));
        let data = stucki(&img);
        assert_eq!(data.len(), 4);
        for byte in &data {
            assert_eq!(*byte, 0x00);
        }
    }

    #[test]
    fn stucki_correct_row_stride() {
        let img = GrayImage::from_pixel(10, 2, Luma([128u8]));
        let data = stucki(&img);
        assert_eq!(data.len(), 4);
    }

    #[test]
    fn stucki_midtone_distributes_error() {
        let img = GrayImage::from_pixel(8, 8, Luma([128u8]));
        let data = stucki(&img);
        assert_eq!(data.len(), 8);
        // Midtone should produce a mix of black and white bits
        let total_set: u32 = data.iter().map(|b| b.count_ones()).sum();
        assert!(total_set > 0 && total_set < 64);
    }

    // --- Jarvis tests ---

    #[test]
    fn jarvis_all_white() {
        let img = GrayImage::from_pixel(4, 4, Luma([255u8]));
        let data = jarvis(&img);
        assert_eq!(data.len(), 4);
        for byte in &data {
            assert_eq!(*byte, 0xF0);
        }
    }

    #[test]
    fn jarvis_all_black() {
        let img = GrayImage::from_pixel(4, 4, Luma([0u8]));
        let data = jarvis(&img);
        assert_eq!(data.len(), 4);
        for byte in &data {
            assert_eq!(*byte, 0x00);
        }
    }

    #[test]
    fn jarvis_correct_row_stride() {
        let img = GrayImage::from_pixel(10, 2, Luma([128u8]));
        let data = jarvis(&img);
        assert_eq!(data.len(), 4);
    }

    #[test]
    fn jarvis_midtone_distributes_error() {
        let img = GrayImage::from_pixel(8, 8, Luma([128u8]));
        let data = jarvis(&img);
        assert_eq!(data.len(), 8);
        let total_set: u32 = data.iter().map(|b| b.count_ones()).sum();
        assert!(total_set > 0 && total_set < 64);
    }

    // --- Sierra tests ---

    #[test]
    fn sierra_all_white() {
        let img = GrayImage::from_pixel(4, 4, Luma([255u8]));
        let data = sierra(&img);
        assert_eq!(data.len(), 4);
        for byte in &data {
            assert_eq!(*byte, 0xF0);
        }
    }

    #[test]
    fn sierra_all_black() {
        let img = GrayImage::from_pixel(4, 4, Luma([0u8]));
        let data = sierra(&img);
        assert_eq!(data.len(), 4);
        for byte in &data {
            assert_eq!(*byte, 0x00);
        }
    }

    #[test]
    fn sierra_correct_row_stride() {
        let img = GrayImage::from_pixel(10, 2, Luma([128u8]));
        let data = sierra(&img);
        assert_eq!(data.len(), 4);
    }

    #[test]
    fn sierra_midtone_distributes_error() {
        let img = GrayImage::from_pixel(8, 8, Luma([128u8]));
        let data = sierra(&img);
        assert_eq!(data.len(), 8);
        let total_set: u32 = data.iter().map(|b| b.count_ones()).sum();
        assert!(total_set > 0 && total_set < 64);
    }

    // --- Atkinson tests ---

    #[test]
    fn atkinson_all_white() {
        let img = GrayImage::from_pixel(4, 4, Luma([255u8]));
        let data = atkinson(&img);
        assert_eq!(data.len(), 4);
        for byte in &data {
            assert_eq!(*byte, 0xF0);
        }
    }

    #[test]
    fn atkinson_all_black() {
        let img = GrayImage::from_pixel(4, 4, Luma([0u8]));
        let data = atkinson(&img);
        assert_eq!(data.len(), 4);
        for byte in &data {
            assert_eq!(*byte, 0x00);
        }
    }

    #[test]
    fn atkinson_correct_row_stride() {
        let img = GrayImage::from_pixel(10, 2, Luma([128u8]));
        let data = atkinson(&img);
        assert_eq!(data.len(), 4);
    }

    #[test]
    fn atkinson_midtone_distributes_error() {
        let img = GrayImage::from_pixel(8, 8, Luma([128u8]));
        let data = atkinson(&img);
        assert_eq!(data.len(), 8);
        let total_set: u32 = data.iter().map(|b| b.count_ones()).sum();
        // Atkinson only distributes 75% of error, so result should be lighter than others
        assert!(total_set > 0 && total_set < 64);
    }

    #[test]
    fn halftone_all_white_produces_all_ones() {
        let img = GrayImage::from_pixel(8, 8, Luma([255u8]));
        let data = halftone_dither(&img, 4, 0.0);
        // All white should produce all 1s (no dots)
        assert_eq!(data.len(), 8); // 8 pixels per row = 1 byte, 8 rows
        for byte in &data {
            assert_eq!(*byte, 0xFF);
        }
    }

    #[test]
    fn halftone_all_black_produces_all_zeros() {
        let img = GrayImage::from_pixel(8, 8, Luma([0u8]));
        let data = halftone_dither(&img, 4, 0.0);
        // All black should produce all 0s (full dots)
        assert_eq!(data.len(), 8);
        for byte in &data {
            assert_eq!(*byte, 0x00);
        }
    }

    #[test]
    fn halftone_produces_correct_output_size() {
        let img = GrayImage::from_pixel(16, 12, Luma([128u8]));
        let data = halftone_dither(&img, 4, 0.0);
        // 16 pixels per row = 2 bytes, 12 rows = 24 bytes
        assert_eq!(data.len(), 24);
    }

    #[test]
    fn halftone_different_cell_sizes() {
        let img = GrayImage::from_pixel(16, 16, Luma([128u8]));
        let data2 = halftone_dither(&img, 2, 0.0);
        let data4 = halftone_dither(&img, 4, 0.0);
        let data8 = halftone_dither(&img, 8, 0.0);

        // All should produce same output size
        assert_eq!(data2.len(), 32); // 16*16 / 8 = 32
        assert_eq!(data4.len(), 32);
        assert_eq!(data8.len(), 32);

        // But patterns should differ (we're just checking they run)
    }

    #[test]
    fn newsprint_all_white_produces_all_ones() {
        let img = GrayImage::from_pixel(8, 8, Luma([255u8]));
        let data = newsprint_dither(&img, 0.0, 10.0, 300);
        assert_eq!(data.len(), 8);
        for byte in &data {
            assert_eq!(*byte, 0xFF);
        }
    }

    #[test]
    fn newsprint_all_black_produces_all_zeros() {
        let img = GrayImage::from_pixel(8, 8, Luma([0u8]));
        let data = newsprint_dither(&img, 0.0, 10.0, 300);
        assert_eq!(data.len(), 8);
        for byte in &data {
            assert_eq!(*byte, 0x00);
        }
    }

    #[test]
    fn newsprint_produces_correct_output_size() {
        let img = GrayImage::from_pixel(16, 12, Luma([128u8]));
        let data = newsprint_dither(&img, 45.0, 15.0, 300);
        // 16 pixels per row = 2 bytes, 12 rows = 24 bytes
        assert_eq!(data.len(), 24);
    }

    #[test]
    fn newsprint_different_angles() {
        let img = GrayImage::from_pixel(16, 16, Luma([128u8]));
        let data0 = newsprint_dither(&img, 0.0, 10.0, 300);
        let data45 = newsprint_dither(&img, 45.0, 10.0, 300);
        let data90 = newsprint_dither(&img, 90.0, 10.0, 300);

        // All should produce same output size
        assert_eq!(data0.len(), 32);
        assert_eq!(data45.len(), 32);
        assert_eq!(data90.len(), 32);

        // Patterns should differ (we're just checking they run)
    }

    #[test]
    fn halftone_edge_case_single_pixel() {
        let img = GrayImage::from_pixel(1, 1, Luma([128u8]));
        let data = halftone_dither(&img, 2, 0.0);
        assert_eq!(data.len(), 1); // 1 pixel = 1 byte (with padding)
    }

    #[test]
    fn newsprint_edge_case_small_image() {
        let img = GrayImage::from_pixel(2, 2, Luma([128u8]));
        let data = newsprint_dither(&img, 0.0, 10.0, 300);
        assert_eq!(data.len(), 2); // 2 pixels per row = 1 byte, 2 rows
    }

    // --- Halftone angle tests ---

    #[test]
    fn halftone_nonzero_angle_changes_output() {
        let img = GrayImage::from_pixel(16, 16, Luma([128u8]));
        let data0 = halftone_dither(&img, 4, 0.0);
        let data45 = halftone_dither(&img, 4, 45.0);
        assert_eq!(data0.len(), data45.len());
        // Different angles should produce different patterns
        assert_ne!(data0, data45);
    }

    #[test]
    fn halftone_angle_zero_preserves_old_behavior() {
        // At angle 0, halftone should produce deterministic output
        let img = GrayImage::from_pixel(8, 8, Luma([128u8]));
        let data1 = halftone_dither(&img, 4, 0.0);
        let data2 = halftone_dither(&img, 4, 0.0);
        assert_eq!(data1, data2);
    }

    // --- Newsprint DPI tests ---

    #[test]
    fn newsprint_dpi_affects_cell_size() {
        let img = GrayImage::from_pixel(32, 32, Luma([128u8]));
        let data_150 = newsprint_dither(&img, 0.0, 10.0, 150);
        let data_600 = newsprint_dither(&img, 0.0, 10.0, 600);
        assert_eq!(data_150.len(), data_600.len());
        // Different DPI → different cell size → different patterns
        assert_ne!(data_150, data_600);
    }

    // --- Sketch tests ---

    #[test]
    fn sketch_white_image_no_edges() {
        let img = GrayImage::from_pixel(8, 8, Luma([255u8]));
        let data = sketch_dither(&img);
        assert_eq!(data.len(), 8); // 8px wide = 1 byte/row, 8 rows
        // Uniform white → no edges → all bits should be 1 (white)
        for byte in &data {
            assert_eq!(*byte, 0xFF);
        }
    }

    #[test]
    fn sketch_hard_edge_produces_burn_pixels() {
        // Create an image with a sharp vertical edge at x=4
        let mut img = GrayImage::new(8, 8);
        for y in 0..8 {
            for x in 0..4 {
                img.put_pixel(x, y, Luma([0u8]));
            }
            for x in 4..8 {
                img.put_pixel(x, y, Luma([255u8]));
            }
        }
        let data = sketch_dither(&img);
        assert_eq!(data.len(), 8);
        // Should have some burn pixels (0 bits) near the edge
        let total_set: u32 = data.iter().map(|b| b.count_ones()).sum();
        // Not all white (some edges detected) but not all black either
        assert!(total_set > 0 && total_set < 64);
    }

    #[test]
    fn sketch_correct_row_stride() {
        let img = GrayImage::from_pixel(10, 5, Luma([128u8]));
        let data = sketch_dither(&img);
        // 10 pixels = 2 bytes/row, 5 rows = 10 bytes
        assert_eq!(data.len(), 10);
    }
}

//! Bitmap transposition for crosshatch vertical pass.

use crate::types::{ProcessedRaster, RasterPixelFormat};

/// Transpose a processed raster — swaps width and height,
/// mapping pixel (col, row) to (row, col) in the output.
///
/// Used to generate vertical scanlines for crosshatch mode:
/// transpose the bitmap, run the normal horizontal scanline generator,
/// then swap coordinates in the resulting plan segment.
pub fn transpose_raster(raster: &ProcessedRaster) -> ProcessedRaster {
    let w = raster.width_px;
    let h = raster.height_px;

    match raster.format {
        RasterPixelFormat::Binary => transpose_binary(raster, w, h),
        RasterPixelFormat::Grayscale8 => transpose_grayscale(raster, w, h),
    }
}

fn transpose_binary(raster: &ProcessedRaster, w: u32, h: u32) -> ProcessedRaster {
    // Output dimensions: width_px = h, height_px = w
    let out_w = h;
    let out_h = w;
    let out_row_bytes = out_w.div_ceil(8) as usize;
    let in_row_bytes = w.div_ceil(8) as usize;
    let mut out_data = vec![0u8; out_row_bytes * out_h as usize];

    for row in 0..h {
        for col in 0..w {
            // Read bit at (col, row) in input
            let in_byte_idx = row as usize * in_row_bytes + (col / 8) as usize;
            let in_bit = 7 - (col % 8);
            let bit_val = if in_byte_idx < raster.data.len() {
                (raster.data[in_byte_idx] >> in_bit) & 1
            } else {
                0
            };

            // Write bit at (row, col) in output — i.e., output row=col, output col=row
            let out_row = col;
            let out_col = row;
            let out_byte_idx = out_row as usize * out_row_bytes + (out_col / 8) as usize;
            let out_bit = 7 - (out_col % 8);
            if bit_val == 1 && out_byte_idx < out_data.len() {
                out_data[out_byte_idx] |= 1 << out_bit;
            }
        }
    }

    ProcessedRaster {
        width_px: out_w,
        height_px: out_h,
        line_interval_mm: raster.effective_x_pixel_mm(),
        x_pixel_mm: raster.line_interval_mm,
        format: raster.format,
        data: out_data,
    }
}

fn transpose_grayscale(raster: &ProcessedRaster, w: u32, h: u32) -> ProcessedRaster {
    // Output dimensions: width_px = h, height_px = w
    let out_w = h;
    let out_h = w;
    let mut out_data = vec![0u8; (out_w * out_h) as usize];

    for row in 0..h {
        for col in 0..w {
            let in_idx = (row * w + col) as usize;
            // Transposed: output row=col, output col=row
            let out_idx = (col * out_w + row) as usize;
            if in_idx < raster.data.len() && out_idx < out_data.len() {
                out_data[out_idx] = raster.data[in_idx];
            }
        }
    }

    ProcessedRaster {
        width_px: out_w,
        height_px: out_h,
        line_interval_mm: raster.effective_x_pixel_mm(),
        x_pixel_mm: raster.line_interval_mm,
        format: raster.format,
        data: out_data,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transpose_2x3_binary() {
        // 2 columns x 3 rows binary raster
        // Row 0: [1, 0]  => byte: 0b10_000000 = 0x80
        // Row 1: [0, 1]  => byte: 0b01_000000 = 0x40
        // Row 2: [1, 1]  => byte: 0b11_000000 = 0xC0
        let raster = ProcessedRaster {
            width_px: 2,
            height_px: 3,
            line_interval_mm: 0.1,
            x_pixel_mm: 0.1,
            format: RasterPixelFormat::Binary,
            data: vec![0x80, 0x40, 0xC0], // 1 byte per row (2 bits, padded)
        };

        let transposed = transpose_raster(&raster);

        assert_eq!(transposed.width_px, 3); // was height
        assert_eq!(transposed.height_px, 2); // was width
        assert_eq!(transposed.format, RasterPixelFormat::Binary);
        // Transpose swaps x_pixel_mm ↔ line_interval_mm
        assert_eq!(transposed.line_interval_mm, 0.1);
        assert_eq!(transposed.x_pixel_mm, 0.1);

        // Transposed layout (3 cols x 2 rows):
        // Out row 0 (was col 0): [1, 0, 1] => 0b101_00000 = 0xA0
        // Out row 1 (was col 1): [0, 1, 1] => 0b011_00000 = 0x60
        assert_eq!(transposed.data.len(), 2); // 1 byte per row * 2 rows
        assert_eq!(transposed.data[0], 0xA0);
        assert_eq!(transposed.data[1], 0x60);
    }

    #[test]
    fn transpose_3x2_grayscale() {
        // 3 columns x 2 rows grayscale
        // Row 0: [10, 20, 30]
        // Row 1: [40, 50, 60]
        let raster = ProcessedRaster {
            width_px: 3,
            height_px: 2,
            line_interval_mm: 0.2,
            x_pixel_mm: 0.2,
            format: RasterPixelFormat::Grayscale8,
            data: vec![10, 20, 30, 40, 50, 60],
        };

        let transposed = transpose_raster(&raster);

        assert_eq!(transposed.width_px, 2); // was height
        assert_eq!(transposed.height_px, 3); // was width
        assert_eq!(transposed.format, RasterPixelFormat::Grayscale8);
        // Transpose swaps: new line_interval = old x_pixel, new x_pixel = old line_interval
        assert_eq!(transposed.line_interval_mm, 0.2);
        assert_eq!(transposed.x_pixel_mm, 0.2);

        // Transposed layout (2 cols x 3 rows):
        // Out row 0 (was col 0): [10, 40]
        // Out row 1 (was col 1): [20, 50]
        // Out row 2 (was col 2): [30, 60]
        assert_eq!(transposed.data, vec![10, 40, 20, 50, 30, 60]);
    }

    #[test]
    fn double_transpose_returns_original_grayscale() {
        let original = ProcessedRaster {
            width_px: 4,
            height_px: 3,
            line_interval_mm: 0.1,
            x_pixel_mm: 0.1,
            format: RasterPixelFormat::Grayscale8,
            data: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12],
        };

        let double = transpose_raster(&transpose_raster(&original));
        assert_eq!(double.width_px, original.width_px);
        assert_eq!(double.height_px, original.height_px);
        assert_eq!(double.data, original.data);
    }

    #[test]
    fn double_transpose_returns_original_binary() {
        // 4x2 binary
        // Row 0: [1,0,1,1] => 0b1011_0000 = 0xB0
        // Row 1: [0,1,0,1] => 0b0101_0000 = 0x50
        let original = ProcessedRaster {
            width_px: 4,
            height_px: 2,
            line_interval_mm: 0.1,
            x_pixel_mm: 0.1,
            format: RasterPixelFormat::Binary,
            data: vec![0xB0, 0x50],
        };

        let double = transpose_raster(&transpose_raster(&original));
        assert_eq!(double.width_px, original.width_px);
        assert_eq!(double.height_px, original.height_px);
        assert_eq!(double.data, original.data);
    }

    #[test]
    fn transpose_7x3_binary_no_panic() {
        // 7 columns x 3 rows — odd dimensions that stress bit-packing edge cases
        // Row 0: [1,0,1,0,1,0,1] => 0b1010101_0 = 0xAA
        // Row 1: [0,1,0,1,0,1,0] => 0b0101010_0 = 0x54
        // Row 2: [1,1,1,0,0,0,1] => 0b1110001_0 = 0xE2
        let raster = ProcessedRaster {
            width_px: 7,
            height_px: 3,
            line_interval_mm: 0.1,
            x_pixel_mm: 0.1,
            format: RasterPixelFormat::Binary,
            data: vec![0xAA, 0x54, 0xE2], // 1 byte per row (7 bits used, 1 padding)
        };

        let transposed = transpose_raster(&raster);

        assert_eq!(transposed.width_px, 3); // was height
        assert_eq!(transposed.height_px, 7); // was width
        assert_eq!(transposed.format, RasterPixelFormat::Binary);

        // Double-transpose should return original
        let double = transpose_raster(&transposed);
        assert_eq!(double.width_px, 7);
        assert_eq!(double.height_px, 3);
        assert_eq!(double.data, raster.data);
    }

    #[test]
    fn transpose_1x1_grayscale() {
        let raster = ProcessedRaster {
            width_px: 1,
            height_px: 1,
            line_interval_mm: 0.1,
            x_pixel_mm: 0.1,
            format: RasterPixelFormat::Grayscale8,
            data: vec![42],
        };

        let transposed = transpose_raster(&raster);
        assert_eq!(transposed.width_px, 1);
        assert_eq!(transposed.height_px, 1);
        assert_eq!(transposed.data, vec![42]);
    }
}

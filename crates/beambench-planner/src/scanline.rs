//! Scanline generation from processed raster data.

use crate::plan::{ScanDirection, ScanRun, Scanline};
use beambench_raster::types::{ProcessedRaster, RasterPixelFormat};

/// Generate scanlines from a processed raster.
///
/// # Arguments
/// * `raster` - The processed raster data
/// * `origin_x_mm` - X position of top-left corner on the workspace
/// * `origin_y_mm` - Y position of top-left corner on the workspace
/// * `bidirectional` - Whether to alternate scan directions
/// * `overscan_mm` - Extra travel distance beyond the raster edges (for acceleration)
pub fn generate_scanlines(
    raster: &ProcessedRaster,
    origin_x_mm: f64,
    origin_y_mm: f64,
    bidirectional: bool,
    overscan_mm: f64,
) -> Vec<Scanline> {
    let mut scanlines = Vec::new();

    for y in 0..raster.height_px {
        let y_mm = origin_y_mm + y as f64 * raster.line_interval_mm;

        // Determine scan direction
        let direction = if bidirectional && y % 2 == 1 {
            ScanDirection::RightToLeft
        } else {
            ScanDirection::LeftToRight
        };

        // Find runs in this row
        let mut runs = match raster.format {
            RasterPixelFormat::Binary => find_binary_runs(raster, y, origin_x_mm, overscan_mm),
            RasterPixelFormat::Grayscale8 => {
                find_grayscale_runs(raster, y, origin_x_mm, overscan_mm)
            }
        };

        // Skip empty scanlines (whitespace optimization)
        if runs.is_empty() {
            continue;
        }

        // Reverse runs for right-to-left scan
        if direction == ScanDirection::RightToLeft {
            runs.reverse();
        }

        scanlines.push(Scanline {
            y_mm,
            runs,
            direction,
        });
    }

    scanlines
}

/// Find runs of burn pixels in a binary format raster row.
fn find_binary_runs(
    raster: &ProcessedRaster,
    y: u32,
    origin_x_mm: f64,
    _overscan_mm: f64, // Overscan no longer baked into run bounds; handled by G-code emitter
) -> Vec<ScanRun> {
    let mut runs = Vec::new();
    let width = raster.width_px as usize;
    let row_start = y as usize * width.div_ceil(8);
    let x_step = raster.effective_x_pixel_mm();

    let mut run_start: Option<usize> = None;

    for x in 0..width {
        let byte_idx = row_start + x / 8;
        let bit_idx = 7 - (x % 8); // MSB first

        if byte_idx >= raster.data.len() {
            break;
        }

        let is_burn = (raster.data[byte_idx] & (1 << bit_idx)) == 0; // 0 = burn

        match (is_burn, run_start) {
            (true, None) => {
                // Start of a new run
                run_start = Some(x);
            }
            (false, Some(start)) => {
                // End of a run
                let start_x_mm = origin_x_mm + start as f64 * x_step;
                let end_x_mm = origin_x_mm + x as f64 * x_step;
                runs.push(ScanRun {
                    start_x_mm,
                    end_x_mm,
                    power_values: vec![], // Binary mode: empty power_values
                });
                run_start = None;
            }
            _ => {}
        }
    }

    // Handle run that extends to end of row
    if let Some(start) = run_start {
        let start_x_mm = origin_x_mm + start as f64 * x_step;
        let end_x_mm = origin_x_mm + width as f64 * x_step;
        runs.push(ScanRun {
            start_x_mm,
            end_x_mm,
            power_values: vec![],
        });
    }

    runs
}

/// Find runs of non-white pixels in a grayscale format raster row.
fn find_grayscale_runs(
    raster: &ProcessedRaster,
    y: u32,
    origin_x_mm: f64,
    _overscan_mm: f64, // Overscan no longer baked into run bounds; handled by G-code emitter
) -> Vec<ScanRun> {
    let mut runs = Vec::new();
    let width = raster.width_px as usize;
    let row_start = y as usize * width;
    let x_step = raster.effective_x_pixel_mm();

    let mut run_start: Option<usize> = None;
    let mut power_values = Vec::new();

    for x in 0..width {
        let idx = row_start + x;

        if idx >= raster.data.len() {
            break;
        }

        let pixel = raster.data[idx];
        let is_burn = pixel < 255; // Non-white pixels are burn

        match (is_burn, run_start.is_some()) {
            (true, false) => {
                // Start of a new run
                run_start = Some(x);
                power_values.clear();
                power_values.push(255 - pixel); // Invert: white=0 power, black=255 power
            }
            (true, true) => {
                // Continue run
                power_values.push(255 - pixel);
            }
            (false, true) => {
                // End of a run
                if let Some(start) = run_start {
                    let start_x_mm = origin_x_mm + start as f64 * x_step;
                    let end_x_mm = origin_x_mm + x as f64 * x_step;
                    runs.push(ScanRun {
                        start_x_mm,
                        end_x_mm,
                        power_values: power_values.clone(),
                    });
                }
                run_start = None;
                power_values.clear();
            }
            _ => {}
        }
    }

    // Handle run that extends to end of row
    if let Some(start) = run_start {
        let start_x_mm = origin_x_mm + start as f64 * x_step;
        let end_x_mm = origin_x_mm + width as f64 * x_step;
        runs.push(ScanRun {
            start_x_mm,
            end_x_mm,
            power_values,
        });
    }

    runs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_black_raster_binary() {
        let raster = ProcessedRaster {
            width_px: 8,
            height_px: 2,
            line_interval_mm: 0.1,
            x_pixel_mm: 0.1,
            format: RasterPixelFormat::Binary,
            data: vec![0x00, 0x00], // All 0s = all burn
        };

        let scanlines = generate_scanlines(&raster, 0.0, 0.0, false, 0.0);

        assert_eq!(scanlines.len(), 2);
        assert_eq!(scanlines[0].runs.len(), 1);
        assert_eq!(scanlines[0].runs[0].start_x_mm, 0.0);
        assert_eq!(scanlines[0].runs[0].end_x_mm, 0.8); // 8 pixels * 0.1mm
        assert!(scanlines[0].runs[0].power_values.is_empty());
    }

    #[test]
    fn all_white_raster_binary() {
        let raster = ProcessedRaster {
            width_px: 8,
            height_px: 2,
            line_interval_mm: 0.1,
            x_pixel_mm: 0.1,
            format: RasterPixelFormat::Binary,
            data: vec![0xFF, 0xFF], // All 1s = all white
        };

        let scanlines = generate_scanlines(&raster, 0.0, 0.0, false, 0.0);

        assert_eq!(scanlines.len(), 0); // No scanlines for all-white
    }

    #[test]
    fn alternating_pixels_binary() {
        let raster = ProcessedRaster {
            width_px: 8,
            height_px: 1,
            line_interval_mm: 0.1,
            x_pixel_mm: 0.1,
            format: RasterPixelFormat::Binary,
            data: vec![0xAA], // 10101010 = alternating burn/white
        };

        let scanlines = generate_scanlines(&raster, 0.0, 0.0, false, 0.0);

        assert_eq!(scanlines.len(), 1);
        assert_eq!(scanlines[0].runs.len(), 4); // 4 separate burn runs
    }

    #[test]
    fn bidirectional_alternates_direction() {
        let raster = ProcessedRaster {
            width_px: 8,
            height_px: 3,
            line_interval_mm: 0.1,
            x_pixel_mm: 0.1,
            format: RasterPixelFormat::Binary,
            data: vec![0x00, 0x00, 0x00], // All burn
        };

        let scanlines = generate_scanlines(&raster, 0.0, 0.0, true, 0.0);

        assert_eq!(scanlines.len(), 3);
        assert_eq!(scanlines[0].direction, ScanDirection::LeftToRight);
        assert_eq!(scanlines[1].direction, ScanDirection::RightToLeft);
        assert_eq!(scanlines[2].direction, ScanDirection::LeftToRight);
    }

    #[test]
    fn unidirectional_always_left_to_right() {
        let raster = ProcessedRaster {
            width_px: 8,
            height_px: 3,
            line_interval_mm: 0.1,
            x_pixel_mm: 0.1,
            format: RasterPixelFormat::Binary,
            data: vec![0x00, 0x00, 0x00],
        };

        let scanlines = generate_scanlines(&raster, 0.0, 0.0, false, 0.0);

        assert_eq!(scanlines.len(), 3);
        assert!(
            scanlines
                .iter()
                .all(|s| s.direction == ScanDirection::LeftToRight)
        );
    }

    #[test]
    fn overscan_does_not_extend_run_bounds() {
        // Overscan is no longer baked into ScanRun bounds — the G-code emitter
        // handles overscan as S0 acceleration/deceleration zones.
        let raster = ProcessedRaster {
            width_px: 4,
            height_px: 1,
            line_interval_mm: 0.1,
            x_pixel_mm: 0.1,
            format: RasterPixelFormat::Binary,
            data: vec![0x00], // All burn
        };

        let scanlines = generate_scanlines(&raster, 10.0, 5.0, false, 2.0);

        assert_eq!(scanlines.len(), 1);
        assert_eq!(scanlines[0].y_mm, 5.0);
        // Runs contain burn-only coordinates (no overscan)
        assert_eq!(scanlines[0].runs[0].start_x_mm, 10.0);
        assert_eq!(scanlines[0].runs[0].end_x_mm, 10.4);
    }

    #[test]
    fn grayscale_all_black() {
        let raster = ProcessedRaster {
            width_px: 4,
            height_px: 1,
            line_interval_mm: 0.1,
            x_pixel_mm: 0.1,
            format: RasterPixelFormat::Grayscale8,
            data: vec![0, 0, 0, 0], // All black
        };

        let scanlines = generate_scanlines(&raster, 0.0, 0.0, false, 0.0);

        assert_eq!(scanlines.len(), 1);
        assert_eq!(scanlines[0].runs.len(), 1);
        assert_eq!(scanlines[0].runs[0].power_values, vec![255, 255, 255, 255]);
    }

    #[test]
    fn grayscale_all_white() {
        let raster = ProcessedRaster {
            width_px: 4,
            height_px: 1,
            line_interval_mm: 0.1,
            x_pixel_mm: 0.1,
            format: RasterPixelFormat::Grayscale8,
            data: vec![255, 255, 255, 255], // All white
        };

        let scanlines = generate_scanlines(&raster, 0.0, 0.0, false, 0.0);

        assert_eq!(scanlines.len(), 0); // No scanlines for all-white
    }

    #[test]
    fn grayscale_mixed_values() {
        let raster = ProcessedRaster {
            width_px: 5,
            height_px: 1,
            line_interval_mm: 0.1,
            x_pixel_mm: 0.1,
            format: RasterPixelFormat::Grayscale8,
            data: vec![100, 150, 255, 200, 50], // Mix with one white
        };

        let scanlines = generate_scanlines(&raster, 0.0, 0.0, false, 0.0);

        assert_eq!(scanlines.len(), 1);
        assert_eq!(scanlines[0].runs.len(), 2); // Split by white pixel
        assert_eq!(scanlines[0].runs[0].power_values, vec![155, 105]); // 255-100, 255-150
        assert_eq!(scanlines[0].runs[1].power_values, vec![55, 205]); // 255-200, 255-50
    }

    #[test]
    fn origin_offset_affects_coordinates() {
        let raster = ProcessedRaster {
            width_px: 2,
            height_px: 2,
            line_interval_mm: 1.0,
            x_pixel_mm: 1.0,
            format: RasterPixelFormat::Binary,
            data: vec![0x00, 0x00], // All burn
        };

        let scanlines = generate_scanlines(&raster, 10.0, 20.0, false, 0.0);

        assert_eq!(scanlines[0].y_mm, 20.0);
        assert_eq!(scanlines[1].y_mm, 21.0);
        assert_eq!(scanlines[0].runs[0].start_x_mm, 10.0);
    }

    #[test]
    fn right_to_left_reverses_runs() {
        let raster = ProcessedRaster {
            width_px: 8,
            height_px: 2,
            line_interval_mm: 0.1,
            x_pixel_mm: 0.1,
            format: RasterPixelFormat::Binary,
            data: vec![0x0F, 0x0F], // 00001111 - two distinct halves
        };

        let scanlines = generate_scanlines(&raster, 0.0, 0.0, true, 0.0);

        assert_eq!(scanlines.len(), 2);
        // In RTL, the first run in the list should be the rightmost run
        assert_eq!(scanlines[1].direction, ScanDirection::RightToLeft);
    }
}

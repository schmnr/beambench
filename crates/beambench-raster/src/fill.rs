//! Scanline-based flood fill for grayscale images.

use image::GrayImage;

/// Perform a scanline-based flood fill on a grayscale image.
///
/// Starting from `(start_x, start_y)`, fills all connected pixels whose
/// value is within `tolerance` of the seed pixel's value with `fill_value`.
///
/// Uses a scanline approach (horizontal spans + stack) for efficiency,
/// avoiding per-pixel recursion.
///
/// Returns the number of pixels filled.
pub fn flood_fill_scanlines(
    img: &mut GrayImage,
    start_x: u32,
    start_y: u32,
    fill_value: u8,
    tolerance: u8,
) -> u32 {
    let width = img.width();
    let height = img.height();

    if start_x >= width || start_y >= height {
        return 0;
    }

    let seed_value = img.get_pixel(start_x, start_y)[0];

    // If the seed already matches the fill value, nothing to do
    if seed_value == fill_value {
        return 0;
    }

    let mut filled = 0u32;
    let mut stack: Vec<(u32, u32)> = vec![(start_x, start_y)];

    let matches =
        |px: u8| -> bool { (px as i16 - seed_value as i16).unsigned_abs() <= tolerance as u16 };

    while let Some((x, y)) = stack.pop() {
        // Skip if out of bounds or already filled
        if x >= width || y >= height {
            continue;
        }

        let current = img.get_pixel(x, y)[0];
        if !matches(current) || current == fill_value {
            continue;
        }

        // Scan left to find the start of this span
        let mut left = x;
        while left > 0
            && matches(img.get_pixel(left - 1, y)[0])
            && img.get_pixel(left - 1, y)[0] != fill_value
        {
            left -= 1;
        }

        // Scan right to find the end of this span
        let mut right = x;
        while right + 1 < width
            && matches(img.get_pixel(right + 1, y)[0])
            && img.get_pixel(right + 1, y)[0] != fill_value
        {
            right += 1;
        }

        // Fill the entire span
        for fx in left..=right {
            img.put_pixel(fx, y, image::Luma([fill_value]));
            filled += 1;
        }

        // Push spans above and below
        for &ny in &[y.wrapping_sub(1), y + 1] {
            if ny >= height {
                continue;
            }
            let mut sx = left;
            while sx <= right {
                let pv = img.get_pixel(sx, ny)[0];
                if matches(pv) && pv != fill_value {
                    stack.push((sx, ny));
                    // Skip the rest of this contiguous matching run to avoid
                    // pushing duplicates
                    while sx <= right
                        && matches(img.get_pixel(sx, ny)[0])
                        && img.get_pixel(sx, ny)[0] != fill_value
                    {
                        sx += 1;
                    }
                } else {
                    sx += 1;
                }
            }
        }
    }

    filled
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Luma;

    #[test]
    fn basic_fill_uniform_region() {
        // 5x5 image, all white (255), fill center with black (0)
        let mut img = GrayImage::from_pixel(5, 5, Luma([255u8]));
        let count = flood_fill_scanlines(&mut img, 2, 2, 0, 0);

        // All 25 pixels should be filled
        assert_eq!(count, 25);
        for y in 0..5 {
            for x in 0..5 {
                assert_eq!(img.get_pixel(x, y)[0], 0);
            }
        }
    }

    #[test]
    fn fill_respects_boundary() {
        // 5x5 image, white center region surrounded by black border
        let mut img = GrayImage::from_pixel(5, 5, Luma([0u8]));
        // Inner 3x3 = white
        for y in 1..4 {
            for x in 1..4 {
                img.put_pixel(x, y, Luma([255u8]));
            }
        }

        let count = flood_fill_scanlines(&mut img, 2, 2, 128, 0);

        // Only the 9 inner pixels should be filled
        assert_eq!(count, 9);
        // Border should remain black
        assert_eq!(img.get_pixel(0, 0)[0], 0);
        assert_eq!(img.get_pixel(4, 4)[0], 0);
        // Inner should be 128
        assert_eq!(img.get_pixel(2, 2)[0], 128);
        assert_eq!(img.get_pixel(1, 1)[0], 128);
    }

    #[test]
    fn fill_with_tolerance() {
        // 4x4 image with slight variations
        let mut img = GrayImage::new(4, 4);
        for y in 0..4 {
            for x in 0..4 {
                // Values between 100 and 110
                img.put_pixel(x, y, Luma([100 + ((x + y) % 4) as u8 * 3]));
            }
        }

        // Fill from (0,0) with tolerance=10 should reach all pixels
        // since max diff is |100 - 109| = 9 which is within tolerance 10
        let count = flood_fill_scanlines(&mut img, 0, 0, 200, 10);
        assert_eq!(count, 16);
    }

    #[test]
    fn fill_out_of_bounds_returns_zero() {
        let mut img = GrayImage::from_pixel(3, 3, Luma([128u8]));
        let count = flood_fill_scanlines(&mut img, 10, 10, 0, 0);
        assert_eq!(count, 0);
    }

    #[test]
    fn fill_same_value_returns_zero() {
        let mut img = GrayImage::from_pixel(3, 3, Luma([128u8]));
        let count = flood_fill_scanlines(&mut img, 1, 1, 128, 0);
        assert_eq!(count, 0);
    }
}

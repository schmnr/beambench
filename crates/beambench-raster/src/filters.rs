//! Convolution filters for raster images (sharpen, edge enhance).

use image::GrayImage;

/// Apply unsharp-mask sharpening to a grayscale image.
///
/// Uses a 3×3 kernel: center = 1 + 4*amount, cross neighbors = -amount.
/// `amount` of 0.0 is a no-op, 1.0 is normal sharpening, 2.0 is strong.
pub fn sharpen(img: &mut GrayImage, amount: f64) {
    if amount <= 0.0 {
        return;
    }
    let width = img.width() as i32;
    let height = img.height() as i32;
    let src = img.clone();

    for y in 0..height {
        for x in 0..width {
            let center = src.get_pixel(x as u32, y as u32)[0] as f64;
            let mut neighbors = 0.0;
            if y > 0 {
                neighbors += src.get_pixel(x as u32, (y - 1) as u32)[0] as f64;
            } else {
                neighbors += center;
            }
            if y < height - 1 {
                neighbors += src.get_pixel(x as u32, (y + 1) as u32)[0] as f64;
            } else {
                neighbors += center;
            }
            if x > 0 {
                neighbors += src.get_pixel((x - 1) as u32, y as u32)[0] as f64;
            } else {
                neighbors += center;
            }
            if x < width - 1 {
                neighbors += src.get_pixel((x + 1) as u32, y as u32)[0] as f64;
            } else {
                neighbors += center;
            }

            let val = center * (1.0 + 4.0 * amount) - neighbors * amount;
            img.put_pixel(
                x as u32,
                y as u32,
                image::Luma([val.clamp(0.0, 255.0) as u8]),
            );
        }
    }
}

/// Apply edge-enhance filter by adding a Laplacian to the original image.
///
/// Kernel: [ 0, -1,  0 ]
///         [-1,  4, -1 ]
///         [ 0, -1,  0 ]
///
/// The result is original + laplacian, clamped to 0–255.
pub fn edge_enhance(img: &mut GrayImage) {
    let width = img.width() as i32;
    let height = img.height() as i32;
    let src = img.clone();

    for y in 0..height {
        for x in 0..width {
            let center = src.get_pixel(x as u32, y as u32)[0] as f64;
            let mut neighbors = 0.0;
            if y > 0 {
                neighbors += src.get_pixel(x as u32, (y - 1) as u32)[0] as f64;
            } else {
                neighbors += center;
            }
            if y < height - 1 {
                neighbors += src.get_pixel(x as u32, (y + 1) as u32)[0] as f64;
            } else {
                neighbors += center;
            }
            if x > 0 {
                neighbors += src.get_pixel((x - 1) as u32, y as u32)[0] as f64;
            } else {
                neighbors += center;
            }
            if x < width - 1 {
                neighbors += src.get_pixel((x + 1) as u32, y as u32)[0] as f64;
            } else {
                neighbors += center;
            }

            // Laplacian = 4*center - neighbors; add to original
            let laplacian = 4.0 * center - neighbors;
            let val = center + laplacian;
            img.put_pixel(
                x as u32,
                y as u32,
                image::Luma([val.clamp(0.0, 255.0) as u8]),
            );
        }
    }
}

/// Apply unsharp mask with configurable radius and amount.
///
/// The algorithm:
/// 1. If denoise > 0, apply selective Gaussian blur to smooth areas first
/// 2. Create a blurred copy using box blur approximation of Gaussian with the given radius
/// 3. For each pixel: result = original + amount * (original - blurred)
///
/// `radius`: spread of the blur (in pixels). 0 = no-op.
/// `amount`: strength of the sharpening. 0 = no-op, 1.0 = normal, 2.0 = strong.
/// `denoise`: noise reduction strength. 0 = off.
pub fn unsharp_mask(img: &mut GrayImage, radius: f64, amount: f64, denoise: f64) {
    if radius <= 0.0 || amount <= 0.0 {
        return;
    }

    // Step 1: Optional denoise pass (Gaussian blur on smooth areas)
    if denoise > 0.0 {
        denoise_smooth_areas(img, denoise);
    }

    // Step 2: Box blur approximation of Gaussian
    let blur_radius = radius.round().max(1.0) as usize;
    let blurred = box_blur(img, blur_radius);

    // Step 3: Unsharp mask = original + amount * (original - blurred)
    let (w, h) = img.dimensions();
    for y in 0..h {
        for x in 0..w {
            let orig = img.get_pixel(x, y)[0] as f64;
            let blur = blurred.get_pixel(x, y)[0] as f64;
            let val = orig + amount * (orig - blur);
            img.put_pixel(x, y, image::Luma([val.clamp(0.0, 255.0) as u8]));
        }
    }
}

/// Box blur approximation — faster than true Gaussian for the unsharp mask use case.
/// Applies a square averaging kernel of size (2*radius+1) × (2*radius+1).
fn box_blur(img: &GrayImage, radius: usize) -> GrayImage {
    let (w, h) = img.dimensions();
    let mut result = img.clone();

    // Horizontal pass
    let mut temp = GrayImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let mut sum = 0.0;
            let mut count = 0.0;
            let x_start = (x as i32 - radius as i32).max(0) as u32;
            let x_end = (x + radius as u32 + 1).min(w);
            for xi in x_start..x_end {
                sum += img.get_pixel(xi, y)[0] as f64;
                count += 1.0;
            }
            temp.put_pixel(x, y, image::Luma([(sum / count) as u8]));
        }
    }

    // Vertical pass
    for y in 0..h {
        for x in 0..w {
            let mut sum = 0.0;
            let mut count = 0.0;
            let y_start = (y as i32 - radius as i32).max(0) as u32;
            let y_end = (y + radius as u32 + 1).min(h);
            for yi in y_start..y_end {
                sum += temp.get_pixel(x, yi)[0] as f64;
                count += 1.0;
            }
            result.put_pixel(x, y, image::Luma([(sum / count) as u8]));
        }
    }

    result
}

/// Selective denoise: blur smooth areas while preserving edges.
/// Uses local variance to detect smooth areas, then applies blur only there.
fn denoise_smooth_areas(img: &mut GrayImage, strength: f64) {
    let (w, h) = img.dimensions();
    let src = img.clone();
    let blurred = box_blur(&src, 1);
    let variance_threshold = 20.0 * (1.0 + strength); // higher strength → more aggressive

    for y in 0..h {
        for x in 0..w {
            // Compute local variance in 3x3 neighborhood
            let mut sum = 0.0;
            let mut sum_sq = 0.0;
            let mut count = 0.0;
            for dy in -1..=1i32 {
                for dx in -1..=1i32 {
                    let nx = (x as i32 + dx).clamp(0, w as i32 - 1) as u32;
                    let ny = (y as i32 + dy).clamp(0, h as i32 - 1) as u32;
                    let v = src.get_pixel(nx, ny)[0] as f64;
                    sum += v;
                    sum_sq += v * v;
                    count += 1.0;
                }
            }
            let mean = sum / count;
            let variance = (sum_sq / count) - mean * mean;

            if variance < variance_threshold {
                // Smooth area — blend toward blurred
                let orig = src.get_pixel(x, y)[0] as f64;
                let blur = blurred.get_pixel(x, y)[0] as f64;
                let blend = strength.min(1.0);
                let val = orig * (1.0 - blend) + blur * blend;
                img.put_pixel(x, y, image::Luma([val.clamp(0.0, 255.0) as u8]));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Luma;

    #[test]
    fn sharpen_noop_at_zero() {
        let mut img = GrayImage::from_pixel(4, 4, Luma([128u8]));
        let original = img.clone();
        sharpen(&mut img, 0.0);
        assert_eq!(img, original);
    }

    #[test]
    fn sharpen_increases_edge_contrast() {
        // Create an image with an edge: left half dark, right half bright
        let mut img = GrayImage::new(4, 4);
        for y in 0..4 {
            for x in 0..4 {
                let val = if x < 2 { 50u8 } else { 200u8 };
                img.put_pixel(x, y, Luma([val]));
            }
        }
        let before_dark = img.get_pixel(1, 2)[0]; // dark side near edge
        let before_bright = img.get_pixel(2, 2)[0]; // bright side near edge

        sharpen(&mut img, 1.0);

        let after_dark = img.get_pixel(1, 2)[0];
        let after_bright = img.get_pixel(2, 2)[0];

        // After sharpening, the contrast at the edge should increase
        let before_diff = before_bright as i32 - before_dark as i32;
        let after_diff = after_bright as i32 - after_dark as i32;
        assert!(
            after_diff > before_diff,
            "Edge contrast should increase: before={before_diff}, after={after_diff}"
        );
    }

    #[test]
    fn edge_enhance_detects_edges() {
        // Uniform image should not change much
        let mut uniform = GrayImage::from_pixel(4, 4, Luma([128u8]));
        let before = uniform.get_pixel(2, 2)[0];
        edge_enhance(&mut uniform);
        let after = uniform.get_pixel(2, 2)[0];
        // Center pixel of uniform region should stay the same (laplacian ≈ 0)
        assert!(
            (after as i32 - before as i32).unsigned_abs() <= 1,
            "Uniform region should barely change: before={before}, after={after}"
        );
    }

    #[test]
    fn sharpen_uniform_image_unchanged() {
        let mut img = GrayImage::from_pixel(8, 8, Luma([100u8]));
        sharpen(&mut img, 1.5);
        // All inner pixels of uniform image should remain 100
        for y in 1..7 {
            for x in 1..7 {
                assert_eq!(img.get_pixel(x, y)[0], 100);
            }
        }
    }

    #[test]
    fn unsharp_mask_noop_at_zero() {
        let mut img = GrayImage::from_pixel(8, 8, Luma([128u8]));
        let original = img.clone();
        unsharp_mask(&mut img, 0.0, 1.0, 0.0);
        assert_eq!(img, original, "radius=0 should be no-op");

        let mut img2 = GrayImage::from_pixel(8, 8, Luma([128u8]));
        unsharp_mask(&mut img2, 1.0, 0.0, 0.0);
        assert_eq!(img2, original, "amount=0 should be no-op");
    }

    #[test]
    fn unsharp_mask_increases_edge_contrast() {
        let mut img = GrayImage::new(20, 20);
        for y in 0..20 {
            for x in 0..20 {
                let val = if x < 10 { 50u8 } else { 200u8 };
                img.put_pixel(x, y, Luma([val]));
            }
        }
        let before_dark = img.get_pixel(9, 10)[0]; // dark side near edge
        let before_bright = img.get_pixel(10, 10)[0]; // bright side near edge

        unsharp_mask(&mut img, 2.0, 1.5, 0.0);

        let after_dark = img.get_pixel(9, 10)[0];
        let after_bright = img.get_pixel(10, 10)[0];
        let before_diff = before_bright as i32 - before_dark as i32;
        let after_diff = after_bright as i32 - after_dark as i32;
        assert!(
            after_diff > before_diff,
            "Edge contrast should increase: before={before_diff}, after={after_diff}"
        );
    }

    #[test]
    fn unsharp_mask_uniform_unchanged() {
        let mut img = GrayImage::from_pixel(10, 10, Luma([100u8]));
        unsharp_mask(&mut img, 3.0, 1.0, 0.0);
        // Uniform image: original - blurred = 0, so result should be unchanged
        for y in 2..8 {
            for x in 2..8 {
                assert_eq!(
                    img.get_pixel(x, y)[0],
                    100,
                    "Uniform pixel should not change"
                );
            }
        }
    }

    #[test]
    fn unsharp_mask_with_denoise() {
        // Just verify it doesn't panic and produces a valid image
        let mut img = GrayImage::new(20, 20);
        for y in 0..20 {
            for x in 0..20 {
                img.put_pixel(x, y, Luma([((x * 13 + y * 7) % 256) as u8]));
            }
        }
        unsharp_mask(&mut img, 2.0, 1.0, 0.5);
        // Should not panic, all pixels should be valid
        for y in 0..20 {
            for x in 0..20 {
                let _ = img.get_pixel(x, y)[0]; // Just ensure valid
            }
        }
    }
}

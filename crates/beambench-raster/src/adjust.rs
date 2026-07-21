//! Image adjustment operations (brightness, contrast, gamma, invert).

use beambench_common::RasterAdjustments;
use image::{GrayImage, RgbaImage};

/// Apply image adjustments in-place to a grayscale image.
///
/// Adjustments are applied in order:
/// 1. Brightness
/// 2. Contrast
/// 3. Gamma
/// 4. Invert
///
/// Each adjustment is skipped if it has a neutral value.
pub fn apply_adjustments(img: &mut GrayImage, adj: &RasterAdjustments) {
    let width = img.width();
    let height = img.height();

    for y in 0..height {
        for x in 0..width {
            let mut pixel = img.get_pixel(x, y)[0] as f64;

            // 1. Brightness: add brightness * 255.0
            if adj.brightness != 0.0 {
                pixel += adj.brightness * 255.0;
                pixel = pixel.clamp(0.0, 255.0);
            }

            // 2. Contrast: (pixel - 128) * (1 + contrast) + 128
            if adj.contrast != 0.0 {
                pixel = (pixel - 128.0) * (1.0 + adj.contrast) + 128.0;
                pixel = pixel.clamp(0.0, 255.0);
            }

            // 3. Gamma: (pixel / 255.0) ^ (1.0 / gamma) * 255.0
            if adj.gamma != 1.0 {
                pixel = (pixel / 255.0).powf(1.0 / adj.gamma) * 255.0;
                pixel = pixel.clamp(0.0, 255.0);
            }

            // 4. Invert: 255 - pixel
            if adj.invert {
                pixel = 255.0 - pixel;
            }

            img.put_pixel(x, y, image::Luma([pixel as u8]));
        }
    }
}

/// Suggested automatic adjustment values matching the Adjust Image slider
/// semantics (`apply_adjustments` pipeline order: brightness, contrast, gamma).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AutoAdjustments {
    pub brightness: f64,
    pub contrast: f64,
    pub gamma: f64,
    pub sharpen: f64,
}

impl AutoAdjustments {
    pub fn neutral() -> Self {
        Self {
            brightness: 0.0,
            contrast: 0.0,
            gamma: 1.0,
            sharpen: 0.0,
        }
    }
}

/// Derive conservative auto adjustments from image statistics:
/// - percentile contrast stretch (1%..99% -> full range, factor capped at 2.0)
/// - brightness offset so the stretched low percentile lands at black
/// - gamma pulling the stretched median toward mid-gray (clamped 0.6..1.8)
/// - mild sharpening for soft images based on Laplacian energy (capped 0.5)
///
/// Deliberately restrained: an already well-exposed image comes back near
/// neutral, and no suggestion exceeds what a careful user would set by hand.
pub fn suggest_auto_adjustments(img: &GrayImage) -> AutoAdjustments {
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 {
        return AutoAdjustments::neutral();
    }

    // Sample on a stride so huge images stay cheap (~1024 samples per side max)
    let step = (w.max(h) / 1024).max(1) as usize;
    let mut histogram = [0u64; 256];
    let mut count = 0u64;
    for y in (0..h).step_by(step) {
        for x in (0..w).step_by(step) {
            histogram[img.get_pixel(x, y)[0] as usize] += 1;
            count += 1;
        }
    }

    let percentile = |p: f64| -> f64 {
        let target = ((p * count as f64).round() as u64).max(1);
        let mut cumulative = 0u64;
        for (value, n) in histogram.iter().enumerate() {
            cumulative += n;
            if cumulative >= target {
                return value as f64;
            }
        }
        255.0
    };
    let lo = percentile(0.01);
    let hi = percentile(0.99);
    let median = percentile(0.5);

    // Nearly flat image: stretching would only amplify noise
    if hi - lo < 8.0 {
        return AutoAdjustments {
            sharpen: suggest_sharpen(img, step),
            ..AutoAdjustments::neutral()
        };
    }

    let contrast = (255.0 / (hi - lo) - 1.0).clamp(0.0, 1.0);
    let factor = 1.0 + contrast;
    // Offset (in 0..255 space) placing the stretched `lo` at black:
    // ((lo + offset) - 128) * factor + 128 = 0
    let offset = 128.0 - lo - 128.0 / factor;
    let brightness = (offset / 255.0).clamp(-1.0, 1.0);

    // Median after brightness + contrast, then gamma toward mid-gray:
    // (median2 / 255) ^ (1 / gamma) = 0.5
    let median2 = ((median + offset - 128.0) * factor + 128.0).clamp(1.0, 254.0);
    let gamma = ((median2 / 255.0).ln() / 0.5f64.ln()).clamp(0.6, 1.8);

    AutoAdjustments {
        brightness,
        contrast,
        gamma,
        sharpen: suggest_sharpen(img, step),
    }
}

/// Estimate softness via RMS of the 4-neighbor Laplacian and map it to a mild
/// sharpen amount: crisp images get 0, visibly soft ones up to 0.5.
fn suggest_sharpen(img: &GrayImage, step: usize) -> f64 {
    let (w, h) = img.dimensions();
    if w < 3 || h < 3 {
        return 0.0;
    }
    let mut sum_sq = 0.0f64;
    let mut n = 0u64;
    for y in (1..h - 1).step_by(step) {
        for x in (1..w - 1).step_by(step) {
            let center = img.get_pixel(x, y)[0] as f64;
            let lap = 4.0 * center
                - img.get_pixel(x - 1, y)[0] as f64
                - img.get_pixel(x + 1, y)[0] as f64
                - img.get_pixel(x, y - 1)[0] as f64
                - img.get_pixel(x, y + 1)[0] as f64;
            sum_sq += lap * lap;
            n += 1;
        }
    }
    if n == 0 {
        return 0.0;
    }
    let rms = (sum_sq / n as f64).sqrt();
    const CRISP_RMS: f64 = 20.0;
    const SOFT_RMS: f64 = 4.0;
    const MAX_SHARPEN: f64 = 0.5;
    if rms >= CRISP_RMS {
        0.0
    } else if rms <= SOFT_RMS {
        MAX_SHARPEN
    } else {
        MAX_SHARPEN * (CRISP_RMS - rms) / (CRISP_RMS - SOFT_RMS)
    }
}

/// Adjust color saturation (works on RGBA images).
///
/// Factor > 1.0 increases saturation, < 1.0 decreases.
/// Factor = 0.0 produces grayscale.
///
/// Conversion: RGB -> HSL -> adjust S -> RGB
pub fn saturation_adjust(img: &RgbaImage, factor: f64) -> RgbaImage {
    let mut result = img.clone();
    let width = img.width();
    let height = img.height();

    for y in 0..height {
        for x in 0..width {
            let pixel = img.get_pixel(x, y);
            let r = pixel[0] as f64 / 255.0;
            let g = pixel[1] as f64 / 255.0;
            let b = pixel[2] as f64 / 255.0;
            let a = pixel[3];

            // RGB to HSL
            let max = r.max(g).max(b);
            let min = r.min(g).min(b);
            let delta = max - min;

            let l = (max + min) / 2.0;

            let s = if delta == 0.0 {
                0.0
            } else if l < 0.5 {
                delta / (max + min)
            } else {
                delta / (2.0 - max - min)
            };

            let h = if delta == 0.0 {
                0.0
            } else if (max - r).abs() < 1e-10 {
                ((g - b) / delta).rem_euclid(6.0)
            } else if (max - g).abs() < 1e-10 {
                (b - r) / delta + 2.0
            } else {
                (r - g) / delta + 4.0
            };

            // Adjust saturation
            let new_s = (s * factor).clamp(0.0, 1.0);

            // HSL to RGB
            let c = (1.0 - (2.0 * l - 1.0).abs()) * new_s;
            let x_val = c * (1.0 - ((h % 2.0) - 1.0).abs());
            let m = l - c / 2.0;

            let (r_prime, g_prime, b_prime) = if h < 1.0 {
                (c, x_val, 0.0)
            } else if h < 2.0 {
                (x_val, c, 0.0)
            } else if h < 3.0 {
                (0.0, c, x_val)
            } else if h < 4.0 {
                (0.0, x_val, c)
            } else if h < 5.0 {
                (x_val, 0.0, c)
            } else {
                (c, 0.0, x_val)
            };

            let new_r = ((r_prime + m) * 255.0).clamp(0.0, 255.0) as u8;
            let new_g = ((g_prime + m) * 255.0).clamp(0.0, 255.0) as u8;
            let new_b = ((b_prime + m) * 255.0).clamp(0.0, 255.0) as u8;

            result.put_pixel(x, y, image::Rgba([new_r, new_g, new_b, a]));
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Luma, Rgba};

    #[test]
    fn zero_adjustments_no_change() {
        let mut img = GrayImage::from_pixel(3, 3, Luma([128u8]));
        let original = img.clone();
        apply_adjustments(&mut img, &RasterAdjustments::default());
        assert_eq!(img, original);
    }

    #[test]
    fn brightness_positive_increases_values() {
        let mut img = GrayImage::from_pixel(2, 2, Luma([100u8]));
        let mut adj = RasterAdjustments::default();
        adj.brightness = 0.2; // +0.2 * 255 = +51
        apply_adjustments(&mut img, &adj);
        let val = img.get_pixel(0, 0)[0];
        assert_eq!(val, 151); // 100 + 51
    }

    #[test]
    fn brightness_positive_clamps_to_255() {
        let mut img = GrayImage::from_pixel(2, 2, Luma([200u8]));
        let mut adj = RasterAdjustments::default();
        adj.brightness = 1.0; // +255
        apply_adjustments(&mut img, &adj);
        let val = img.get_pixel(0, 0)[0];
        assert_eq!(val, 255);
    }

    #[test]
    fn brightness_negative_decreases_values() {
        let mut img = GrayImage::from_pixel(2, 2, Luma([100u8]));
        let mut adj = RasterAdjustments::default();
        adj.brightness = -0.2; // -0.2 * 255 = -51
        apply_adjustments(&mut img, &adj);
        let val = img.get_pixel(0, 0)[0];
        assert_eq!(val, 49); // 100 - 51
    }

    #[test]
    fn brightness_negative_clamps_to_zero() {
        let mut img = GrayImage::from_pixel(2, 2, Luma([50u8]));
        let mut adj = RasterAdjustments::default();
        adj.brightness = -1.0; // -255
        apply_adjustments(&mut img, &adj);
        let val = img.get_pixel(0, 0)[0];
        assert_eq!(val, 0);
    }

    #[test]
    fn contrast_positive_increases_contrast() {
        let mut img = GrayImage::from_pixel(2, 2, Luma([100u8]));
        let mut adj = RasterAdjustments::default();
        adj.contrast = 0.5; // multiply by 1.5
        apply_adjustments(&mut img, &adj);
        let val = img.get_pixel(0, 0)[0];
        // (100 - 128) * 1.5 + 128 = -28 * 1.5 + 128 = -42 + 128 = 86
        assert_eq!(val, 86);
    }

    #[test]
    fn contrast_negative_decreases_contrast() {
        let mut img = GrayImage::from_pixel(2, 2, Luma([200u8]));
        let mut adj = RasterAdjustments::default();
        adj.contrast = -0.5; // multiply by 0.5
        apply_adjustments(&mut img, &adj);
        let val = img.get_pixel(0, 0)[0];
        // (200 - 128) * 0.5 + 128 = 72 * 0.5 + 128 = 36 + 128 = 164
        assert_eq!(val, 164);
    }

    #[test]
    fn gamma_above_one_darkens_midtones() {
        let mut img = GrayImage::from_pixel(2, 2, Luma([128u8]));
        let mut adj = RasterAdjustments::default();
        adj.gamma = 2.0;
        apply_adjustments(&mut img, &adj);
        let val = img.get_pixel(0, 0)[0];
        // (128/255)^(1/2) * 255 = (0.502)^0.5 * 255 = 0.708 * 255 = 180.6
        assert!(val > 170 && val < 185, "Expected ~181, got {}", val);
    }

    #[test]
    fn gamma_below_one_brightens_midtones() {
        let mut img = GrayImage::from_pixel(2, 2, Luma([128u8]));
        let mut adj = RasterAdjustments::default();
        adj.gamma = 0.5;
        apply_adjustments(&mut img, &adj);
        let val = img.get_pixel(0, 0)[0];
        // (128/255)^(1/0.5) * 255 = (0.502)^2 * 255 = 0.252 * 255 = 64.3
        assert!(val > 60 && val < 70, "Expected ~64, got {}", val);
    }

    #[test]
    fn gamma_one_no_change() {
        let mut img = GrayImage::from_pixel(2, 2, Luma([100u8]));
        let original = img.clone();
        let mut adj = RasterAdjustments::default();
        adj.gamma = 1.0;
        apply_adjustments(&mut img, &adj);
        assert_eq!(img, original);
    }

    #[test]
    fn invert_flips_pixel_values() {
        let mut img = GrayImage::from_pixel(2, 2, Luma([100u8]));
        let mut adj = RasterAdjustments::default();
        adj.invert = true;
        apply_adjustments(&mut img, &adj);
        let val = img.get_pixel(0, 0)[0];
        assert_eq!(val, 155); // 255 - 100
    }

    #[test]
    fn invert_white_becomes_black() {
        let mut img = GrayImage::from_pixel(2, 2, Luma([255u8]));
        let mut adj = RasterAdjustments::default();
        adj.invert = true;
        apply_adjustments(&mut img, &adj);
        let val = img.get_pixel(0, 0)[0];
        assert_eq!(val, 0);
    }

    #[test]
    fn invert_black_becomes_white() {
        let mut img = GrayImage::from_pixel(2, 2, Luma([0u8]));
        let mut adj = RasterAdjustments::default();
        adj.invert = true;
        apply_adjustments(&mut img, &adj);
        let val = img.get_pixel(0, 0)[0];
        assert_eq!(val, 255);
    }

    #[test]
    fn combined_adjustments_applied_in_order() {
        let mut img = GrayImage::from_pixel(2, 2, Luma([128u8]));
        let mut adj = RasterAdjustments::default();
        adj.brightness = 0.2; // +51 -> 179
        adj.contrast = 0.5; // (179-128)*1.5+128 = 76.5+128 = 204.5 -> 205 (rounded)
        adj.invert = true; // 255-205 = 50
        apply_adjustments(&mut img, &adj);
        let val = img.get_pixel(0, 0)[0];
        assert_eq!(val, 50);
    }

    #[test]
    fn saturation_factor_zero_produces_grayscale() {
        let mut img = RgbaImage::new(2, 2);
        img.put_pixel(0, 0, Rgba([255, 0, 0, 255])); // Red
        img.put_pixel(1, 0, Rgba([0, 255, 0, 255])); // Green
        img.put_pixel(0, 1, Rgba([0, 0, 255, 255])); // Blue
        img.put_pixel(1, 1, Rgba([128, 128, 0, 255])); // Yellow

        let result = saturation_adjust(&img, 0.0);

        // All pixels should be grayscale (R=G=B)
        for y in 0..2 {
            for x in 0..2 {
                let pixel = result.get_pixel(x, y);
                let r = pixel[0];
                let g = pixel[1];
                let b = pixel[2];
                // Allow small tolerance for floating-point rounding
                assert!((r as i16 - g as i16).abs() <= 1);
                assert!((g as i16 - b as i16).abs() <= 1);
            }
        }
    }

    #[test]
    fn saturation_increase_makes_colors_more_vivid() {
        let img = RgbaImage::from_pixel(2, 2, Rgba([200, 150, 150, 255])); // Slightly reddish
        let result = saturation_adjust(&img, 2.0);

        // Red channel should increase relative to others
        let pixel = result.get_pixel(0, 0);
        let original = img.get_pixel(0, 0);
        assert!(pixel[0] >= original[0]); // Red should stay same or increase
    }

    #[test]
    fn saturation_decrease_makes_colors_less_vivid() {
        let img = RgbaImage::from_pixel(2, 2, Rgba([255, 0, 0, 255])); // Pure red
        let result = saturation_adjust(&img, 0.5);

        // Red should decrease, others increase (moving toward gray)
        let pixel = result.get_pixel(0, 0);
        assert!(pixel[0] < 255);
        assert!(pixel[1] > 0);
        assert!(pixel[2] > 0);
    }

    #[test]
    fn saturation_preserves_alpha() {
        let img = RgbaImage::from_pixel(2, 2, Rgba([255, 0, 0, 128]));
        let result = saturation_adjust(&img, 0.5);

        for y in 0..2 {
            for x in 0..2 {
                assert_eq!(result.get_pixel(x, y)[3], 128);
            }
        }
    }

    fn horizontal_gradient(width: u32, height: u32, min: u8, max: u8) -> GrayImage {
        GrayImage::from_fn(width, height, |x, _| {
            let t = x as f64 / (width - 1).max(1) as f64;
            Luma([(min as f64 + t * (max - min) as f64).round() as u8])
        })
    }

    #[test]
    fn auto_dark_image_raises_brightness_and_contrast() {
        let img = horizontal_gradient(256, 64, 0, 100);
        let auto = suggest_auto_adjustments(&img);
        assert!(
            auto.brightness > 0.1,
            "expected lift, got {}",
            auto.brightness
        );
        assert!(
            auto.contrast > 0.5,
            "expected stretch, got {}",
            auto.contrast
        );
        assert!(
            auto.gamma >= 1.0,
            "dark median should not get darkening gamma, got {}",
            auto.gamma
        );
    }

    #[test]
    fn auto_washed_out_image_lowers_brightness() {
        let img = horizontal_gradient(256, 64, 150, 255);
        let auto = suggest_auto_adjustments(&img);
        assert!(
            auto.brightness < -0.1,
            "expected darkening, got {}",
            auto.brightness
        );
        assert!(
            auto.contrast > 0.5,
            "expected stretch, got {}",
            auto.contrast
        );
    }

    #[test]
    fn auto_full_range_image_stays_near_neutral() {
        let img = horizontal_gradient(256, 64, 0, 255);
        let auto = suggest_auto_adjustments(&img);
        assert!(auto.brightness.abs() < 0.05, "got {}", auto.brightness);
        assert!(auto.contrast < 0.1, "got {}", auto.contrast);
        assert!((auto.gamma - 1.0).abs() < 0.15, "got {}", auto.gamma);
    }

    #[test]
    fn auto_flat_image_is_neutral_levels() {
        let img = GrayImage::from_pixel(64, 64, Luma([180u8]));
        let auto = suggest_auto_adjustments(&img);
        assert_eq!(auto.brightness, 0.0);
        assert_eq!(auto.contrast, 0.0);
        assert_eq!(auto.gamma, 1.0);
    }

    #[test]
    fn auto_crisp_image_gets_no_sharpen() {
        // Checkerboard: maximal Laplacian energy
        let img = GrayImage::from_fn(64, 64, |x, y| {
            Luma([if (x + y) % 2 == 0 { 0 } else { 255 }])
        });
        let auto = suggest_auto_adjustments(&img);
        assert_eq!(auto.sharpen, 0.0);
    }

    #[test]
    fn auto_soft_image_gets_mild_sharpen() {
        // Very smooth wide gradient: near-zero Laplacian energy
        let img = horizontal_gradient(256, 64, 0, 255);
        let auto = suggest_auto_adjustments(&img);
        assert!(auto.sharpen > 0.2, "got {}", auto.sharpen);
        assert!(auto.sharpen <= 0.5, "got {}", auto.sharpen);
    }

    #[test]
    fn auto_values_stay_within_slider_ranges() {
        for (min, max) in [(0u8, 30u8), (200, 255), (60, 190), (0, 255), (120, 136)] {
            let img = horizontal_gradient(128, 32, min, max);
            let auto = suggest_auto_adjustments(&img);
            assert!((-1.0..=1.0).contains(&auto.brightness));
            assert!((0.0..=1.0).contains(&auto.contrast));
            assert!((0.1..=3.0).contains(&auto.gamma));
            assert!((0.0..=2.0).contains(&auto.sharpen));
        }
    }
}

//! Image transformation operations (scaling, resizing).

use image::{GrayImage, imageops};

/// Scale an image to target dimensions using Lanczos3 filtering.
///
/// If either target dimension is 0, returns a clone of the original image.
pub fn scale_to_target(img: &GrayImage, target_w: u32, target_h: u32) -> GrayImage {
    // Handle zero dimensions by returning original
    if target_w == 0 || target_h == 0 {
        return img.clone();
    }

    // If already at target size, return clone
    if img.width() == target_w && img.height() == target_h {
        return img.clone();
    }

    imageops::resize(img, target_w, target_h, imageops::FilterType::Lanczos3)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Luma;

    #[test]
    fn scale_to_smaller_dimensions() {
        let img = GrayImage::from_pixel(10, 10, Luma([128u8]));
        let scaled = scale_to_target(&img, 5, 5);
        assert_eq!(scaled.width(), 5);
        assert_eq!(scaled.height(), 5);
    }

    #[test]
    fn scale_to_larger_dimensions() {
        let img = GrayImage::from_pixel(5, 5, Luma([200u8]));
        let scaled = scale_to_target(&img, 10, 10);
        assert_eq!(scaled.width(), 10);
        assert_eq!(scaled.height(), 10);
    }

    #[test]
    fn scale_to_same_size_returns_clone() {
        let img = GrayImage::from_pixel(8, 8, Luma([100u8]));
        let scaled = scale_to_target(&img, 8, 8);
        assert_eq!(scaled.width(), 8);
        assert_eq!(scaled.height(), 8);
        assert_eq!(img.get_pixel(0, 0)[0], scaled.get_pixel(0, 0)[0]);
    }

    #[test]
    fn scale_with_zero_width_returns_original() {
        let img = GrayImage::from_pixel(5, 5, Luma([150u8]));
        let scaled = scale_to_target(&img, 0, 10);
        assert_eq!(scaled.width(), 5);
        assert_eq!(scaled.height(), 5);
    }

    #[test]
    fn scale_with_zero_height_returns_original() {
        let img = GrayImage::from_pixel(5, 5, Luma([150u8]));
        let scaled = scale_to_target(&img, 10, 0);
        assert_eq!(scaled.width(), 5);
        assert_eq!(scaled.height(), 5);
    }

    #[test]
    fn scale_to_non_square_aspect_ratio() {
        let img = GrayImage::from_pixel(10, 20, Luma([75u8]));
        let scaled = scale_to_target(&img, 5, 15);
        assert_eq!(scaled.width(), 5);
        assert_eq!(scaled.height(), 15);
    }

    #[test]
    fn scale_preserves_pixel_values_approximately() {
        let img = GrayImage::from_pixel(2, 2, Luma([255u8]));
        let scaled = scale_to_target(&img, 4, 4);
        // All pixels should be close to 255
        for y in 0..4 {
            for x in 0..4 {
                let val = scaled.get_pixel(x, y)[0];
                assert!(val > 250, "Expected ~255, got {}", val);
            }
        }
    }
}

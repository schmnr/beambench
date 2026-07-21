//! Image decoding from bytes to grayscale.

use std::io::Cursor;

use crate::error::RasterError;
use image::{DynamicImage, GrayImage, ImageDecoder, ImageReader};

/// Decode image bytes (PNG, JPEG, etc.) with EXIF orientation applied.
///
/// The `image` crate does not auto-apply EXIF orientation, so a photo taken
/// in portrait would otherwise import sideways. This reads the decoder's
/// orientation metadata and bakes it into the pixel data.
///
/// # Errors
///
/// Returns `RasterError::DecodeError` if the image cannot be decoded.
pub fn decode_image_oriented(bytes: &[u8]) -> Result<DynamicImage, RasterError> {
    let reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| RasterError::DecodeError(e.to_string()))?;
    let mut decoder = reader
        .into_decoder()
        .map_err(|e| RasterError::DecodeError(e.to_string()))?;
    let orientation = decoder
        .orientation()
        .map_err(|e| RasterError::DecodeError(e.to_string()))?;
    let mut img =
        DynamicImage::from_decoder(decoder).map_err(|e| RasterError::DecodeError(e.to_string()))?;
    img.apply_orientation(orientation);
    Ok(img)
}

/// Decode image bytes (PNG, JPEG, etc.) into a grayscale image.
///
/// # Errors
///
/// Returns `RasterError::DecodeError` if the image cannot be decoded.
/// Returns `RasterError::EmptyImage` if the decoded image has zero width or height.
pub fn decode_image(bytes: &[u8]) -> Result<GrayImage, RasterError> {
    let img = decode_image_oriented(bytes)?;

    let gray = img.to_luma8();

    if gray.width() == 0 || gray.height() == 0 {
        return Err(RasterError::EmptyImage);
    }

    Ok(gray)
}

/// Decode image bytes to an RGBA image.
/// Same as `decode_image` but returns RGBA instead of grayscale.
pub fn decode_image_rgba(bytes: &[u8]) -> Result<image::RgbaImage, RasterError> {
    let img = decode_image_oriented(bytes)?;
    let rgba = img.to_rgba8();
    if rgba.width() == 0 || rgba.height() == 0 {
        return Err(RasterError::EmptyImage);
    }
    Ok(rgba)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageEncoder, Luma};

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
    fn decode_valid_white_png() {
        let img = GrayImage::from_pixel(2, 2, Luma([255u8]));
        let bytes = encode_png(&img);

        let decoded = decode_image(&bytes).unwrap();
        assert_eq!(decoded.width(), 2);
        assert_eq!(decoded.height(), 2);
        assert_eq!(decoded.get_pixel(0, 0)[0], 255);
    }

    #[test]
    fn decode_valid_black_png() {
        let img = GrayImage::from_pixel(3, 3, Luma([0u8]));
        let bytes = encode_png(&img);

        let decoded = decode_image(&bytes).unwrap();
        assert_eq!(decoded.width(), 3);
        assert_eq!(decoded.height(), 3);
        assert_eq!(decoded.get_pixel(1, 1)[0], 0);
    }

    #[test]
    fn decode_invalid_bytes_returns_error() {
        let invalid_bytes = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00]; // Partial JPEG
        let result = decode_image(&invalid_bytes);
        assert!(result.is_err());
        match result {
            Err(RasterError::DecodeError(_)) => {}
            _ => panic!("Expected DecodeError"),
        }
    }

    #[test]
    fn decode_empty_bytes_returns_error() {
        let empty_bytes = vec![];
        let result = decode_image(&empty_bytes);
        assert!(result.is_err());
    }

    #[test]
    fn decode_grayscale_gradient() {
        let mut img = GrayImage::new(4, 1);
        img.put_pixel(0, 0, Luma([0u8]));
        img.put_pixel(1, 0, Luma([85u8]));
        img.put_pixel(2, 0, Luma([170u8]));
        img.put_pixel(3, 0, Luma([255u8]));
        let bytes = encode_png(&img);

        let decoded = decode_image(&bytes).unwrap();
        assert_eq!(decoded.width(), 4);
        assert_eq!(decoded.height(), 1);
        assert_eq!(decoded.get_pixel(0, 0)[0], 0);
        assert_eq!(decoded.get_pixel(1, 0)[0], 85);
        assert_eq!(decoded.get_pixel(2, 0)[0], 170);
        assert_eq!(decoded.get_pixel(3, 0)[0], 255);
    }

    #[test]
    fn decode_bmp_format() {
        // Create a BMP-encoded image
        let img = GrayImage::from_pixel(2, 2, Luma([128u8]));
        let mut bytes = Vec::new();
        let encoder = image::codecs::bmp::BmpEncoder::new(&mut bytes);
        encoder
            .write_image(
                img.as_raw(),
                img.width(),
                img.height(),
                image::ExtendedColorType::L8,
            )
            .unwrap();

        let decoded = decode_image(&bytes).unwrap();
        assert_eq!(decoded.width(), 2);
        assert_eq!(decoded.height(), 2);
        assert_eq!(decoded.get_pixel(0, 0)[0], 128);
    }

    #[test]
    fn decode_oriented_plain_png() {
        // PNGs without orientation metadata decode unchanged.
        let img = GrayImage::from_pixel(3, 2, Luma([200u8]));
        let bytes = encode_png(&img);

        let decoded = decode_image_oriented(&bytes).unwrap();
        assert_eq!(decoded.width(), 3);
        assert_eq!(decoded.height(), 2);
        assert_eq!(decoded.to_luma8().get_pixel(0, 0)[0], 200);
    }

    /// Build a minimal EXIF APP1 segment with the given orientation value.
    fn exif_app1_with_orientation(orientation: u16) -> Vec<u8> {
        let mut tiff: Vec<u8> = Vec::new();
        // TIFF header, little-endian
        tiff.extend_from_slice(b"II");
        tiff.extend_from_slice(&42u16.to_le_bytes());
        tiff.extend_from_slice(&8u32.to_le_bytes()); // IFD0 offset
        // IFD0: one entry
        tiff.extend_from_slice(&1u16.to_le_bytes()); // entry count
        tiff.extend_from_slice(&0x0112u16.to_le_bytes()); // Orientation tag
        tiff.extend_from_slice(&3u16.to_le_bytes()); // type SHORT
        tiff.extend_from_slice(&1u32.to_le_bytes()); // count
        tiff.extend_from_slice(&orientation.to_le_bytes());
        tiff.extend_from_slice(&[0u8, 0u8]); // value padding
        tiff.extend_from_slice(&0u32.to_le_bytes()); // next IFD offset

        let payload_len = 6 + tiff.len(); // "Exif\0\0" + TIFF block
        let mut app1: Vec<u8> = vec![0xFF, 0xE1];
        app1.extend_from_slice(&((payload_len + 2) as u16).to_be_bytes());
        app1.extend_from_slice(b"Exif\0\0");
        app1.extend_from_slice(&tiff);
        app1
    }

    #[test]
    fn decode_oriented_applies_exif_rotation() {
        // Encode a 2x1 JPEG: bright pixel on the left, dark on the right.
        let mut img = GrayImage::new(2, 1);
        img.put_pixel(0, 0, Luma([250u8]));
        img.put_pixel(1, 0, Luma([10u8]));
        let mut jpeg = Vec::new();
        let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(
            std::io::Cursor::new(&mut jpeg),
            100,
        );
        encoder
            .write_image(img.as_raw(), 2, 1, image::ExtendedColorType::L8)
            .unwrap();

        // Splice an EXIF APP1 segment (orientation 6 = rotate 90 CW) after SOI.
        assert_eq!(&jpeg[0..2], &[0xFF, 0xD8]);
        let mut with_exif = jpeg[0..2].to_vec();
        with_exif.extend_from_slice(&exif_app1_with_orientation(6));
        with_exif.extend_from_slice(&jpeg[2..]);

        let decoded = decode_image_oriented(&with_exif).unwrap();
        assert_eq!(decoded.width(), 1, "rotate 90 CW swaps dimensions");
        assert_eq!(decoded.height(), 2, "rotate 90 CW swaps dimensions");
        let gray = decoded.to_luma8();
        // Old (0,0) bright pixel lands at new (0,0); old (1,0) dark at (0,1).
        assert!(
            gray.get_pixel(0, 0)[0] > 200,
            "expected bright pixel at top"
        );
        assert!(
            gray.get_pixel(0, 1)[0] < 60,
            "expected dark pixel at bottom"
        );
    }

    #[test]
    fn decode_gif_format() {
        // Create a GIF-encoded image (GIF is paletted, so we encode via RgbaImage)
        let rgba = image::RgbaImage::from_pixel(2, 2, image::Rgba([100, 100, 100, 255]));
        let mut bytes = Vec::new();
        {
            let mut encoder = image::codecs::gif::GifEncoder::new(&mut bytes);
            encoder.encode_frame(image::Frame::new(rgba)).unwrap();
        }

        let decoded = decode_image(&bytes).unwrap();
        assert_eq!(decoded.width(), 2);
        assert_eq!(decoded.height(), 2);
        // GIF is paletted so value may shift slightly — just check it decoded
        assert!(decoded.get_pixel(0, 0)[0] > 90 && decoded.get_pixel(0, 0)[0] < 110);
    }
}

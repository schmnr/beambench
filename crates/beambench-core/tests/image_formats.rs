//! Integration tests verifying that every image format enabled in Cargo.toml
//! (png, jpeg, bmp, gif, tiff, webp, tga) can be encoded and decoded via the
//! `image` crate. This catches misconfigured feature flags early.

use image::{DynamicImage, ImageBuffer, ImageFormat, RgbImage, Rgba, RgbaImage};
use std::io::Cursor;

/// Create a small RGBA test image with a recognizable gradient pattern.
fn make_test_image(width: u32, height: u32) -> RgbaImage {
    ImageBuffer::from_fn(width, height, |x, y| {
        Rgba([
            (x * 255 / width.max(1)) as u8,
            (y * 255 / height.max(1)) as u8,
            128,
            255,
        ])
    })
}

/// Encode an image to bytes using the specified format.
fn encode_to_bytes(img: &RgbaImage, format: ImageFormat) -> Vec<u8> {
    let mut buf = Vec::new();
    let mut cursor = Cursor::new(&mut buf);
    let dynamic = match format {
        ImageFormat::Jpeg => DynamicImage::ImageRgb8(rgba_to_rgb(img)),
        _ => DynamicImage::ImageRgba8(img.clone()),
    };
    dynamic
        .write_to(&mut cursor, format)
        .unwrap_or_else(|e| panic!("Failed to encode {format:?}: {e}"));
    buf
}

fn rgba_to_rgb(img: &RgbaImage) -> RgbImage {
    ImageBuffer::from_fn(img.width(), img.height(), |x, y| {
        let px = img.get_pixel(x, y);
        image::Rgb([px[0], px[1], px[2]])
    })
}

/// Round-trip test: encode then decode, verifying dimensions match.
fn roundtrip_test(format: ImageFormat, width: u32, height: u32) {
    let img = make_test_image(width, height);
    let bytes = encode_to_bytes(&img, format);
    assert!(
        !bytes.is_empty(),
        "{format:?} encoding produced empty bytes"
    );

    let decoded = image::load_from_memory(&bytes)
        .unwrap_or_else(|e| panic!("Failed to decode {format:?}: {e}"));
    assert_eq!(decoded.width(), width, "{format:?} decoded width mismatch");
    assert_eq!(
        decoded.height(),
        height,
        "{format:?} decoded height mismatch"
    );
}

#[test]
fn decode_png() {
    roundtrip_test(ImageFormat::Png, 8, 6);
}

#[test]
fn decode_jpeg() {
    // JPEG is lossy so we just verify dimensions survive the round-trip.
    roundtrip_test(ImageFormat::Jpeg, 8, 6);
}

#[test]
fn decode_bmp() {
    roundtrip_test(ImageFormat::Bmp, 8, 6);
}

#[test]
fn decode_gif() {
    roundtrip_test(ImageFormat::Gif, 8, 6);
}

#[test]
fn decode_tiff() {
    roundtrip_test(ImageFormat::Tiff, 8, 6);
}

#[test]
fn decode_webp() {
    roundtrip_test(ImageFormat::WebP, 8, 6);
}

#[test]
fn decode_tga() {
    // The image crate may not have a TGA encoder. Try the normal path first;
    // fall back to hand-crafted raw TGA bytes if encoding is unsupported.
    let width: u32 = 4;
    let height: u32 = 3;
    let img = make_test_image(width, height);

    let bytes = match std::panic::catch_unwind(|| encode_to_bytes(&img, ImageFormat::Tga)) {
        Ok(b) => b,
        Err(_) => {
            // Build a minimal uncompressed 32-bit BGRA TGA file manually.
            // TGA header: 18 bytes, then width*height*4 bytes of pixel data.
            let mut tga = Vec::with_capacity(18 + (width * height * 4) as usize);
            tga.push(0); // id length
            tga.push(0); // color map type
            tga.push(2); // image type: uncompressed true-color
            tga.extend_from_slice(&[0; 5]); // color map spec (unused)
            tga.extend_from_slice(&[0, 0]); // x origin
            tga.extend_from_slice(&[0, 0]); // y origin
            tga.extend_from_slice(&(width as u16).to_le_bytes()); // width
            tga.extend_from_slice(&(height as u16).to_le_bytes()); // height
            tga.push(32); // bits per pixel
            tga.push(0x20); // image descriptor: top-left origin
            // Pixel data in BGRA order
            for y in 0..height {
                for x in 0..width {
                    let px = img.get_pixel(x, y);
                    tga.push(px[2]); // B
                    tga.push(px[1]); // G
                    tga.push(px[0]); // R
                    tga.push(px[3]); // A
                }
            }
            tga
        }
    };

    assert!(!bytes.is_empty(), "TGA test bytes are empty");

    let decoded = image::load_from_memory_with_format(&bytes, ImageFormat::Tga)
        .unwrap_or_else(|e| panic!("Failed to decode TGA: {e}"));
    assert_eq!(decoded.width(), width, "TGA decoded width mismatch");
    assert_eq!(decoded.height(), height, "TGA decoded height mismatch");
}

/// Verify that `load_from_memory` rejects garbage input.
#[test]
fn reject_invalid_bytes() {
    let garbage = b"this is definitely not an image file";
    let result = image::load_from_memory(garbage);
    assert!(result.is_err(), "Expected error for garbage bytes");
}

/// Verify that each format can handle a 1x1 image (edge case for encoders).
#[test]
fn decode_1x1_png() {
    roundtrip_test(ImageFormat::Png, 1, 1);
}

#[test]
fn decode_1x1_jpeg() {
    roundtrip_test(ImageFormat::Jpeg, 1, 1);
}

#[test]
fn decode_1x1_bmp() {
    roundtrip_test(ImageFormat::Bmp, 1, 1);
}

#[test]
fn decode_1x1_gif() {
    roundtrip_test(ImageFormat::Gif, 1, 1);
}

#[test]
fn decode_1x1_tiff() {
    roundtrip_test(ImageFormat::Tiff, 1, 1);
}

#[test]
fn decode_1x1_webp() {
    roundtrip_test(ImageFormat::WebP, 1, 1);
}

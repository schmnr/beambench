//! Raster processing types for image adjustments and dithering modes.

use serde::{Deserialize, Serialize};

/// Dithering algorithm mode for converting images to binary raster data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RasterMode {
    /// Pure grayscale output (8-bit).
    Grayscale,
    /// Binary thresholding (1-bit).
    Threshold,
    /// Floyd-Steinberg error diffusion dithering (default).
    #[default]
    FloydSteinberg,
    /// Ordered (Bayer) dithering pattern.
    OrderedDither,
    /// Stucki error diffusion dithering.
    Stucki,
    /// Jarvis-Judice-Ninke error diffusion dithering.
    Jarvis,
    /// Sierra error diffusion dithering.
    Sierra,
    /// Atkinson error diffusion dithering (distributes 75% of error).
    Atkinson,
    /// Halftone circular dot dithering with configurable angle.
    #[serde(alias = "halftone")]
    Halftone,
    /// Newsprint rotated dot screen with configurable angle and frequency.
    #[serde(alias = "newsprint")]
    Newsprint,
    /// Sketch mode using edge detection and thresholding.
    Sketch,
}

/// Image adjustment parameters for brightness, contrast, gamma, and threshold.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RasterAdjustments {
    /// Brightness adjustment (-1.0 to 1.0, default 0.0).
    pub brightness: f64,
    /// Contrast adjustment (-1.0 to 1.0, default 0.0).
    pub contrast: f64,
    /// Gamma correction (0.1 to 3.0, default 1.0).
    pub gamma: f64,
    /// Invert image colors (default false).
    pub invert: bool,
    /// Threshold value for binary conversion (0-255, default 128).
    pub threshold: u8,
    /// Saturation adjustment (0.0 to 2.0, default 1.0).
    #[serde(default = "default_saturation")]
    pub saturation: f64,
    /// Sharpen amount (0.0 = off, 1.0 = normal, 2.0 = strong).
    #[serde(default)]
    pub sharpen: f64,
    /// Edge enhance filter (false = off).
    #[serde(default)]
    pub edge_enhance: bool,
    /// Unsharp mask radius — spatial spread of edge enhancement (0.0 = off).
    #[serde(default)]
    pub enhance_radius: f64,
    /// Unsharp mask amount — intensity of edge enhancement (0.0 = off, 1.0 = normal).
    #[serde(default)]
    pub enhance_amount: f64,
    /// Noise reduction in smooth areas before enhancement (0.0 = off).
    #[serde(default)]
    pub enhance_denoise: f64,
}

fn default_saturation() -> f64 {
    1.0
}

impl Default for RasterAdjustments {
    fn default() -> Self {
        Self {
            brightness: 0.0,
            contrast: 0.0,
            gamma: 1.0,
            invert: false,
            threshold: 128,
            saturation: 1.0,
            sharpen: 0.0,
            edge_enhance: false,
            enhance_radius: 0.0,
            enhance_amount: 0.0,
            enhance_denoise: 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raster_mode_default_is_floyd_steinberg() {
        assert_eq!(RasterMode::default(), RasterMode::FloydSteinberg);
    }

    #[test]
    fn raster_adjustments_default_values() {
        let adj = RasterAdjustments::default();
        assert_eq!(adj.brightness, 0.0);
        assert_eq!(adj.contrast, 0.0);
        assert_eq!(adj.gamma, 1.0);
        assert_eq!(adj.invert, false);
        assert_eq!(adj.threshold, 128);
        assert_eq!(adj.saturation, 1.0);
    }

    #[test]
    fn raster_mode_roundtrips_through_json() {
        let modes = vec![
            RasterMode::Grayscale,
            RasterMode::Threshold,
            RasterMode::FloydSteinberg,
            RasterMode::OrderedDither,
            RasterMode::Stucki,
            RasterMode::Jarvis,
            RasterMode::Sierra,
            RasterMode::Atkinson,
            RasterMode::Halftone,
            RasterMode::Newsprint,
            RasterMode::Sketch,
        ];
        for mode in modes {
            let json = serde_json::to_string(&mode).unwrap();
            let restored: RasterMode = serde_json::from_str(&json).unwrap();
            assert_eq!(mode, restored);
        }
    }

    #[test]
    fn raster_mode_serde_format() {
        let mode = RasterMode::FloydSteinberg;
        let json = serde_json::to_string(&mode).unwrap();
        assert_eq!(json, "\"floyd_steinberg\"");
    }

    #[test]
    fn raster_adjustments_roundtrips_through_json() {
        let adj = RasterAdjustments {
            brightness: 0.2,
            contrast: -0.1,
            gamma: 1.5,
            invert: true,
            threshold: 200,
            saturation: 0.3,
            ..RasterAdjustments::default()
        };
        let json = serde_json::to_string(&adj).unwrap();
        let restored: RasterAdjustments = serde_json::from_str(&json).unwrap();
        assert_eq!(adj, restored);
    }

    #[test]
    fn raster_image_json_without_adjustments_deserializes_to_none() {
        // Simulate the ObjectData::RasterImage JSON structure without adjustments field
        let json = r#"{
            "type": "raster_image",
            "asset_key": "img_001",
            "original_width_px": 800,
            "original_height_px": 600
        }"#;

        // This test verifies backward compatibility at the field level.
        // We'll test the full ObjectData deserialization in beambench-core tests,
        // but here we verify that Option<RasterAdjustments> with #[serde(default)]
        // works as expected.
        #[derive(Debug, Deserialize, PartialEq)]
        struct TestStruct {
            asset_key: String,
            original_width_px: u32,
            original_height_px: u32,
            #[serde(default)]
            adjustments: Option<RasterAdjustments>,
        }

        let parsed: TestStruct = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.asset_key, "img_001");
        assert_eq!(parsed.original_width_px, 800);
        assert_eq!(parsed.original_height_px, 600);
        assert!(parsed.adjustments.is_none());
    }

    #[test]
    fn raster_image_json_with_adjustments_deserializes_correctly() {
        let json = r#"{
            "asset_key": "img_002",
            "original_width_px": 1024,
            "original_height_px": 768,
            "adjustments": {
                "brightness": 0.1,
                "contrast": 0.2,
                "gamma": 1.2,
                "invert": false,
                "threshold": 100
            }
        }"#;

        #[derive(Debug, Deserialize, PartialEq)]
        struct TestStruct {
            asset_key: String,
            original_width_px: u32,
            original_height_px: u32,
            #[serde(default)]
            adjustments: Option<RasterAdjustments>,
        }

        let parsed: TestStruct = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.asset_key, "img_002");
        let adj = parsed.adjustments.unwrap();
        assert_eq!(adj.brightness, 0.1);
        assert_eq!(adj.contrast, 0.2);
        assert_eq!(adj.gamma, 1.2);
        assert_eq!(adj.invert, false);
        assert_eq!(adj.threshold, 100);
    }

    #[test]
    fn raster_adjustments_saturation_defaults_to_one() {
        let json = r#"{
            "brightness": 0.0,
            "contrast": 0.0,
            "gamma": 1.0,
            "invert": false,
            "threshold": 128
        }"#;
        let adj: RasterAdjustments = serde_json::from_str(json).unwrap();
        assert_eq!(adj.saturation, 1.0);
    }

    #[test]
    fn raster_adjustments_sharpen_edge_enhance_defaults() {
        let json = r#"{
            "brightness": 0.0,
            "contrast": 0.0,
            "gamma": 1.0,
            "invert": false,
            "threshold": 128,
            "saturation": 1.0
        }"#;
        let adj: RasterAdjustments = serde_json::from_str(json).unwrap();
        assert_eq!(adj.sharpen, 0.0);
        assert_eq!(adj.edge_enhance, false);
    }

    #[test]
    fn raster_adjustments_sharpen_edge_enhance_roundtrip() {
        let adj = RasterAdjustments {
            sharpen: 1.5,
            edge_enhance: true,
            ..RasterAdjustments::default()
        };
        let json = serde_json::to_string(&adj).unwrap();
        let restored: RasterAdjustments = serde_json::from_str(&json).unwrap();
        assert_eq!(adj, restored);
    }

    #[test]
    fn raster_mode_new_variants_roundtrip() {
        let variants = [
            (RasterMode::Stucki, "\"stucki\""),
            (RasterMode::Jarvis, "\"jarvis\""),
            (RasterMode::Sierra, "\"sierra\""),
            (RasterMode::Atkinson, "\"atkinson\""),
            (RasterMode::Halftone, "\"halftone\""),
            (RasterMode::Newsprint, "\"newsprint\""),
            (RasterMode::Sketch, "\"sketch\""),
        ];
        for (mode, expected_json) in variants {
            let json = serde_json::to_string(&mode).unwrap();
            assert_eq!(json, expected_json);
            let restored: RasterMode = serde_json::from_str(&json).unwrap();
            assert_eq!(mode, restored);
        }
    }

    #[test]
    fn raster_mode_legacy_aliases_map_to_new_variants() {
        // Old "halftone" alias now maps to the dedicated Halftone variant
        let halftone: RasterMode = serde_json::from_str("\"halftone\"").unwrap();
        assert_eq!(halftone, RasterMode::Halftone);
        // Old "newsprint" alias now maps to the dedicated Newsprint variant
        let newsprint: RasterMode = serde_json::from_str("\"newsprint\"").unwrap();
        assert_eq!(newsprint, RasterMode::Newsprint);
    }

    #[test]
    fn raster_mode_backward_compat_halftone_alias() {
        let mode: RasterMode = serde_json::from_str("\"halftone\"").unwrap();
        assert_eq!(mode, RasterMode::Halftone);
    }

    #[test]
    fn raster_mode_backward_compat_newsprint_alias() {
        let mode: RasterMode = serde_json::from_str("\"newsprint\"").unwrap();
        assert_eq!(mode, RasterMode::Newsprint);
    }
}

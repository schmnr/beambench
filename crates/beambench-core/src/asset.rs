use beambench_common::Id;
use beambench_common::markers::AssetMarker;
use serde::{Deserialize, Serialize};

/// Type alias for asset IDs.
pub type AssetId = Id<AssetMarker>;

/// Media type of a stored asset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetMediaType {
    // Raster formats
    Png,
    Jpeg,
    Bmp,
    Gif,
    Tiff,
    Webp,
    Tga,
    // Vector formats
    Svg,
    Dxf,
    Ai,
    Pdf,
    Eps,
}

impl AssetMediaType {
    /// File extension for this media type.
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Bmp => "bmp",
            Self::Gif => "gif",
            Self::Tiff => "tiff",
            Self::Webp => "webp",
            Self::Tga => "tga",
            Self::Svg => "svg",
            Self::Dxf => "dxf",
            Self::Ai => "ai",
            Self::Pdf => "pdf",
            Self::Eps => "eps",
        }
    }

    /// Infer media type from a file extension.
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_ascii_lowercase().as_str() {
            "png" => Some(Self::Png),
            "jpg" | "jpeg" => Some(Self::Jpeg),
            "bmp" => Some(Self::Bmp),
            "gif" => Some(Self::Gif),
            "tiff" | "tif" => Some(Self::Tiff),
            "webp" => Some(Self::Webp),
            "tga" => Some(Self::Tga),
            "svg" => Some(Self::Svg),
            "dxf" => Some(Self::Dxf),
            "ai" => Some(Self::Ai),
            "pdf" => Some(Self::Pdf),
            "eps" => Some(Self::Eps),
            _ => None,
        }
    }
}

/// Metadata for an imported asset (image or SVG file).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Asset {
    pub id: AssetId,
    pub original_filename: String,
    pub media_type: AssetMediaType,
    pub byte_size: u64,
    pub width_px: Option<u32>,
    pub height_px: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
}

impl Asset {
    pub fn new(
        filename: impl Into<String>,
        media_type: AssetMediaType,
        byte_size: u64,
        width_px: Option<u32>,
        height_px: Option<u32>,
    ) -> Self {
        Self {
            id: AssetId::new(),
            original_filename: filename.into(),
            media_type,
            byte_size,
            width_px,
            height_px,
            source_path: None,
        }
    }

    pub fn with_source_path(mut self, source_path: Option<String>) -> Self {
        self.source_path = source_path;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_roundtrips_through_json() {
        let asset = Asset::new("photo.png", AssetMediaType::Png, 1024, Some(800), Some(600));
        let json = serde_json::to_string(&asset).unwrap();
        let restored: Asset = serde_json::from_str(&json).unwrap();
        assert_eq!(asset, restored);
    }

    #[test]
    fn asset_media_type_serializes_as_snake_case() {
        let json = serde_json::to_string(&AssetMediaType::Jpeg).unwrap();
        assert_eq!(json, "\"jpeg\"");
    }

    #[test]
    fn media_type_extension() {
        assert_eq!(AssetMediaType::Png.extension(), "png");
        assert_eq!(AssetMediaType::Jpeg.extension(), "jpg");
        assert_eq!(AssetMediaType::Svg.extension(), "svg");
    }

    #[test]
    fn media_type_from_extension() {
        assert_eq!(
            AssetMediaType::from_extension("png"),
            Some(AssetMediaType::Png)
        );
        assert_eq!(
            AssetMediaType::from_extension("JPG"),
            Some(AssetMediaType::Jpeg)
        );
        assert_eq!(
            AssetMediaType::from_extension("jpeg"),
            Some(AssetMediaType::Jpeg)
        );
        assert_eq!(
            AssetMediaType::from_extension("svg"),
            Some(AssetMediaType::Svg)
        );
    }

    #[test]
    fn asset_ids_are_unique() {
        let a = Asset::new("a.png", AssetMediaType::Png, 100, None, None);
        let b = Asset::new("b.png", AssetMediaType::Png, 200, None, None);
        assert_ne!(a.id, b.id);
    }

    // --- AssetMediaType extension tests ---

    #[test]
    fn asset_media_type_raster_extensions() {
        assert_eq!(AssetMediaType::Png.extension(), "png");
        assert_eq!(AssetMediaType::Jpeg.extension(), "jpg");
        assert_eq!(AssetMediaType::Bmp.extension(), "bmp");
        assert_eq!(AssetMediaType::Gif.extension(), "gif");
        assert_eq!(AssetMediaType::Tiff.extension(), "tiff");
        assert_eq!(AssetMediaType::Webp.extension(), "webp");
        assert_eq!(AssetMediaType::Tga.extension(), "tga");
    }

    #[test]
    fn asset_media_type_vector_extensions() {
        assert_eq!(AssetMediaType::Svg.extension(), "svg");
        assert_eq!(AssetMediaType::Dxf.extension(), "dxf");
        assert_eq!(AssetMediaType::Ai.extension(), "ai");
        assert_eq!(AssetMediaType::Pdf.extension(), "pdf");
        assert_eq!(AssetMediaType::Eps.extension(), "eps");
    }

    #[test]
    fn asset_media_type_from_extension_raster() {
        assert_eq!(
            AssetMediaType::from_extension("png"),
            Some(AssetMediaType::Png)
        );
        assert_eq!(
            AssetMediaType::from_extension("jpg"),
            Some(AssetMediaType::Jpeg)
        );
        assert_eq!(
            AssetMediaType::from_extension("jpeg"),
            Some(AssetMediaType::Jpeg)
        );
        assert_eq!(
            AssetMediaType::from_extension("bmp"),
            Some(AssetMediaType::Bmp)
        );
        assert_eq!(
            AssetMediaType::from_extension("gif"),
            Some(AssetMediaType::Gif)
        );
        assert_eq!(
            AssetMediaType::from_extension("tiff"),
            Some(AssetMediaType::Tiff)
        );
        assert_eq!(
            AssetMediaType::from_extension("tif"),
            Some(AssetMediaType::Tiff)
        );
        assert_eq!(
            AssetMediaType::from_extension("webp"),
            Some(AssetMediaType::Webp)
        );
        assert_eq!(
            AssetMediaType::from_extension("tga"),
            Some(AssetMediaType::Tga)
        );
    }

    #[test]
    fn asset_media_type_from_extension_vector() {
        assert_eq!(
            AssetMediaType::from_extension("svg"),
            Some(AssetMediaType::Svg)
        );
        assert_eq!(
            AssetMediaType::from_extension("dxf"),
            Some(AssetMediaType::Dxf)
        );
        assert_eq!(
            AssetMediaType::from_extension("ai"),
            Some(AssetMediaType::Ai)
        );
        assert_eq!(
            AssetMediaType::from_extension("pdf"),
            Some(AssetMediaType::Pdf)
        );
        assert_eq!(
            AssetMediaType::from_extension("eps"),
            Some(AssetMediaType::Eps)
        );
    }

    #[test]
    fn asset_media_type_from_extension_case_insensitive() {
        assert_eq!(
            AssetMediaType::from_extension("PNG"),
            Some(AssetMediaType::Png)
        );
        assert_eq!(
            AssetMediaType::from_extension("SVG"),
            Some(AssetMediaType::Svg)
        );
        assert_eq!(
            AssetMediaType::from_extension("DXF"),
            Some(AssetMediaType::Dxf)
        );
        assert_eq!(
            AssetMediaType::from_extension("TIFF"),
            Some(AssetMediaType::Tiff)
        );
    }

    #[test]
    fn asset_media_type_from_extension_unknown() {
        assert_eq!(AssetMediaType::from_extension("unknown"), None);
        assert_eq!(AssetMediaType::from_extension("txt"), None);
        assert_eq!(AssetMediaType::from_extension("doc"), None);
    }
}

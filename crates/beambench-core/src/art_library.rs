//! Art library — reusable design asset storage, separate from per-project assets.

use crate::{Layer, ObjectId, ProjectObject, TextAlignment, TextAlignmentV};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const ART_LIBRARY_FORMAT_VERSION: &str = "1.0";
pub const ART_LIBRARY_SNAPSHOT_MEDIA_TYPE: &str = "application/vnd.beambench.art-snapshot+json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ArtLibraryItemKind {
    #[default]
    ExternalFile,
    SelectionSnapshot,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArtLibraryTextSourceMetadata {
    pub object_id: ObjectId,
    pub content: String,
    pub font_family: String,
    pub font_size_mm: f64,
    pub bold: bool,
    pub italic: bool,
    pub alignment: TextAlignment,
    pub alignment_v: TextAlignmentV,
    pub upper_case: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArtLibrarySnapshotAsset {
    pub hash: String,
    pub media_type: String,
    /// Raw file data, base64-encoded for JSON transport.
    pub data: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArtLibrarySelectionSnapshot {
    pub format_version: String,
    pub objects: Vec<ProjectObject>,
    pub layer_templates: Vec<Layer>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assets: Vec<ArtLibrarySnapshotAsset>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_text_metadata: Vec<ArtLibraryTextSourceMetadata>,
}

/// A single item in the art library.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArtLibraryItem {
    pub id: Uuid,
    #[serde(default)]
    pub kind: ArtLibraryItemKind,
    pub name: String,
    pub category: String,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Original filename (for display)
    pub source_filename: String,
    /// Media type of the stored data (e.g. "image/png", "image/svg+xml")
    pub media_type: String,
    /// Raw file data or encoded snapshot payload, base64-encoded for JSON transport
    pub data: String,
    /// Optional thumbnail (base64 PNG, max 128x128)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArtLibraryDocument {
    #[serde(default = "default_art_library_format_version")]
    pub format_version: String,
    pub library_id: Uuid,
    pub name: String,
    #[serde(default)]
    pub items: Vec<ArtLibraryItem>,
}

fn default_art_library_format_version() -> String {
    ART_LIBRARY_FORMAT_VERSION.to_string()
}

impl ArtLibraryDocument {
    pub fn new(name: &str) -> Self {
        Self {
            format_version: default_art_library_format_version(),
            library_id: Uuid::new_v4(),
            name: name.to_string(),
            items: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Layer, LayerRef, ObjectData, OperationType, ProjectObject, ShapeKind};
    use beambench_common::{Bounds, ColorTag, Point2D};

    fn sample_layer() -> Layer {
        let mut layer = Layer::new_single_entry("Line", OperationType::Line);
        layer.color_tag = ColorTag("#000000".to_string());
        layer
    }

    fn sample_object(layer_id: LayerRef) -> ProjectObject {
        ProjectObject::new(
            "rect1",
            layer_id,
            Bounds::new(Point2D::zero(), Point2D::new(10.0, 10.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        )
    }

    #[test]
    fn art_library_item_serde_roundtrip() {
        let item = ArtLibraryItem {
            id: Uuid::new_v4(),
            kind: ArtLibraryItemKind::ExternalFile,
            name: "Star".to_string(),
            category: "Shapes".to_string(),
            tags: vec!["star".to_string(), "decoration".to_string()],
            source_filename: "star.svg".to_string(),
            media_type: "image/svg+xml".to_string(),
            data: "PHN2Zz4=".to_string(),
            thumbnail: Some("iVBOR...".to_string()),
            created_at: "2026-03-23T12:00:00Z".to_string(),
        };
        let json = serde_json::to_string(&item).unwrap();
        let parsed: ArtLibraryItem = serde_json::from_str(&json).unwrap();
        assert_eq!(item, parsed);
    }

    #[test]
    fn art_library_item_without_thumbnail_roundtrip() {
        let item = ArtLibraryItem {
            id: Uuid::new_v4(),
            kind: ArtLibraryItemKind::ExternalFile,
            name: "Circle".to_string(),
            category: "Shapes".to_string(),
            tags: vec![],
            source_filename: "circle.png".to_string(),
            media_type: "image/png".to_string(),
            data: "iVBOR...".to_string(),
            thumbnail: None,
            created_at: "2026-03-23T12:00:00Z".to_string(),
        };
        let json = serde_json::to_string(&item).unwrap();
        assert!(!json.contains("thumbnail"));
        let parsed: ArtLibraryItem = serde_json::from_str(&json).unwrap();
        assert_eq!(item, parsed);
    }

    #[test]
    fn art_library_document_serde_roundtrip() {
        let doc = ArtLibraryDocument {
            format_version: ART_LIBRARY_FORMAT_VERSION.to_string(),
            library_id: Uuid::new_v4(),
            name: "My Shapes".to_string(),
            items: vec![ArtLibraryItem {
                id: Uuid::new_v4(),
                kind: ArtLibraryItemKind::ExternalFile,
                name: "Star".to_string(),
                category: "Shapes".to_string(),
                tags: vec!["star".to_string()],
                source_filename: "star.svg".to_string(),
                media_type: "image/svg+xml".to_string(),
                data: "PHN2Zz4=".to_string(),
                thumbnail: None,
                created_at: "2026-03-23T12:00:00Z".to_string(),
            }],
        };
        let json = serde_json::to_string(&doc).unwrap();
        let parsed: ArtLibraryDocument = serde_json::from_str(&json).unwrap();
        assert_eq!(doc, parsed);
    }

    #[test]
    fn art_library_backward_compat_no_thumbnail_or_kind() {
        let json = r#"{
            "id": "00000000-0000-0000-0000-000000000001",
            "name": "Test",
            "category": "General",
            "tags": [],
            "source_filename": "test.png",
            "media_type": "image/png",
            "data": "abc123",
            "created_at": "2026-01-01T00:00:00Z"
        }"#;
        let item: ArtLibraryItem = serde_json::from_str(json).unwrap();
        assert!(item.thumbnail.is_none());
        assert_eq!(item.kind, ArtLibraryItemKind::ExternalFile);
    }

    #[test]
    fn selection_snapshot_roundtrips() {
        let layer = sample_layer();
        let object = sample_object(layer.id);
        let snapshot = ArtLibrarySelectionSnapshot {
            format_version: ART_LIBRARY_FORMAT_VERSION.to_string(),
            objects: vec![object],
            layer_templates: vec![layer],
            assets: vec![ArtLibrarySnapshotAsset {
                hash: "abc123".to_string(),
                media_type: "image/png".to_string(),
                data: "iVBORw0KGgo=".to_string(),
            }],
            source_text_metadata: vec![ArtLibraryTextSourceMetadata {
                object_id: ObjectId::new(),
                content: "Hello".to_string(),
                font_family: "Arial".to_string(),
                font_size_mm: 5.0,
                bold: false,
                italic: false,
                alignment: TextAlignment::Left,
                alignment_v: TextAlignmentV::Top,
                upper_case: false,
            }],
        };

        let json = serde_json::to_string(&snapshot).unwrap();
        let parsed: ArtLibrarySelectionSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(snapshot, parsed);
    }

    #[test]
    fn art_library_document_new_creates_empty() {
        let lib = ArtLibraryDocument::new("Test Library");
        assert_eq!(lib.name, "Test Library");
        assert!(lib.items.is_empty());
        assert_eq!(lib.format_version, ART_LIBRARY_FORMAT_VERSION);
    }
}

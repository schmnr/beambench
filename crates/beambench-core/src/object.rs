use crate::variable_text::VariableTextConfig;
use beambench_common::markers::{LayerMarker, ObjectMarker};
use beambench_common::{BarcodeOptions, BarcodeType, Bounds, Id, RasterAdjustments, Transform2D};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Type aliases for object and layer reference IDs.
pub type ObjectId = Id<ObjectMarker>;
pub type LayerRef = Id<LayerMarker>;

/// The kind of primitive shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShapeKind {
    Rectangle,
    Ellipse,
}

/// Text alignment within a text object (horizontal).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextAlignment {
    #[default]
    Left,
    Center,
    Right,
}

/// Text alignment within a text object (vertical).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextAlignmentV {
    #[default]
    Top,
    Middle,
    Bottom,
}

/// Text layout mode.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextLayoutMode {
    #[default]
    Straight,
    Bend,
    Path,
}

/// Non-destructive transformation applied to editable text geometry.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextTransformStyle {
    #[default]
    None,
    Arch,
    Rise,
    Wave,
    Flag,
    Angle,
    Circle,
}

/// Placement of text around the virtual circle used by [`TextTransformStyle::Circle`].
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextCirclePlacement {
    #[default]
    TopOutside,
    TopInside,
    BottomOutside,
    BottomInside,
}

/// Resolved font source used for cached text outlines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextFontSource {
    System,
    Shx,
    BundledFallback,
}

/// Axis for ruler guides created from the workspace rulers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GuideAxis {
    Horizontal,
    Vertical,
}

/// Polarity for a non-destructive raster image mask.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageMaskPolarity {
    /// Keep pixels inside the referenced mask geometry.
    #[default]
    KeepInside,
    /// Remove pixels inside the referenced mask geometry.
    KeepOutside,
}

/// Reference from a raster image to a vector-like mask object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageMaskRef {
    pub object_id: ObjectId,
    #[serde(default)]
    pub polarity: ImageMaskPolarity,
}

/// Tagged union of object-specific data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ObjectData {
    RasterImage {
        asset_key: String,
        original_width_px: u32,
        original_height_px: u32,
        #[serde(default)]
        adjustments: Option<RasterAdjustments>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        masks: Vec<ImageMaskRef>,
    },
    VectorPath {
        path_data: String,
        closed: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ruler_guide_axis: Option<GuideAxis>,
    },
    Shape {
        kind: ShapeKind,
        width: f64,
        height: f64,
        corner_radius: f64,
    },
    Star {
        points: u32,
        #[serde(default)]
        bulge: f64,
        #[serde(default = "default_star_ratio")]
        ratio: f64,
        #[serde(default)]
        dual_radius: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ratio2: Option<f64>,
        #[serde(default)]
        corner_radius: f64,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        corner_radii: Vec<f64>,
    },
    Text {
        content: String,
        font_family: String,
        font_size_mm: f64,
        alignment: TextAlignment,
        #[serde(default)]
        alignment_v: TextAlignmentV,
        bold: bool,
        italic: bool,
        #[serde(default)]
        upper_case: bool,
        #[serde(default)]
        welded: bool,
        #[serde(default)]
        h_spacing: f64,
        #[serde(default)]
        v_spacing: f64,
        #[serde(default)]
        on_path: bool,
        #[serde(default)]
        path_offset: f64,
        #[serde(default)]
        distort: bool,
        #[serde(default)]
        layout_mode: TextLayoutMode,
        #[serde(default)]
        rtl: bool,
        /// Bend mode radius in mm. Positive = convex, negative = concave, 0 = straight.
        #[serde(default)]
        bend_radius: f64,
        /// Editable transformation style. A non-None value takes precedence over layout_mode.
        #[serde(default)]
        transform_style: TextTransformStyle,
        /// Normalized transformation strength. Runtime geometry clamps this to -100..=100.
        #[serde(
            default,
            deserialize_with = "deserialize_text_transform_curve",
            serialize_with = "serialize_text_transform_curve"
        )]
        transform_curve: f64,
        /// Circle placement, used only when transform_style is Circle.
        #[serde(default)]
        circle_placement: TextCirclePlacement,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_width: Option<f64>,
        #[serde(default)]
        squeeze: bool,
        #[serde(default)]
        ignore_empty_vars: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        resolved_font_source: Option<TextFontSource>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        resolved_font_key: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        resolved_path_data: Option<String>,
        #[serde(default)]
        missing_font: bool,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        missing_glyphs: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        guide_path_id: Option<ObjectId>,
        /// Variable text configuration (template + source) persisted on the object.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        variable_text: Option<VariableTextConfig>,
    },
    Polygon {
        sides: u32,
        radius: f64,
    },
    Barcode {
        barcode_type: BarcodeType,
        data: String,
        width: f64,
        height: f64,
        #[serde(default)]
        options: BarcodeOptions,
    },
    Group {
        children: Vec<ObjectId>,
    },
    /// A synced clone whose geometry comes from another object.
    /// The clone's bounds/transform control position; geometry is resolved from source_id.
    VirtualClone {
        source_id: ObjectId,
    },
}

/// A non-destructive tab anchor stored on an object.
/// Tabs are parametric positions along a closed subpath's perimeter,
/// applied as gaps only during plan generation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TabAnchor {
    pub subpath_index: usize,
    /// Normalized position 0.0–1.0 along the subpath's perimeter.
    pub position: f64,
}

/// Per-subpath record of a custom start-point edit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartPointEdit {
    /// Which closed subpath this edit applies to.
    pub subpath_index: usize,
    /// Current display-vertex index of the original first vertex.
    pub original_start_current_idx: usize,
    /// Whether the subpath direction is currently reversed from original.
    pub reversed: bool,
    /// Display vertex count (V_display = V_internal − 1), frozen at normalization.
    pub v_display: usize,
    /// Whether normalization added an explicit closing LineTo.
    pub normalized: bool,
}

/// Default power scale is 1.0 (100%).
fn default_power_scale() -> f64 {
    1.0
}

/// A project object placed on the workspace canvas.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectObject {
    pub id: ObjectId,
    pub name: String,
    pub visible: bool,
    pub locked: bool,
    pub transform: Transform2D,
    pub bounds: Bounds,
    pub layer_id: LayerRef,
    pub z_index: i32,
    pub data: ObjectData,
    #[serde(default)]
    pub lock_aspect_ratio: bool,
    #[serde(default = "default_power_scale")]
    pub power_scale: f64,
    /// Cut order priority (lower values are cut first, default 0).
    #[serde(default)]
    pub priority: i32,
    /// Timestamp when this object was created.
    #[serde(default = "default_created_at")]
    pub created_at: DateTime<Utc>,
    /// Non-destructive tab anchors for this object (applied during plan generation).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tabs: Vec<TabAnchor>,
    /// Per-subpath start-point edits. Non-empty means custom start points are active.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub start_point_edits: Vec<StartPointEdit>,
}

fn default_created_at() -> DateTime<Utc> {
    Utc::now()
}

fn default_star_ratio() -> f64 {
    0.5
}

fn normalize_text_transform_curve(value: f64) -> f64 {
    if value.is_finite() {
        value.clamp(-100.0, 100.0)
    } else {
        0.0
    }
}

fn deserialize_text_transform_curve<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    f64::deserialize(deserializer).map(normalize_text_transform_curve)
}

fn serialize_text_transform_curve<S>(value: &f64, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_f64(normalize_text_transform_curve(*value))
}

impl ProjectObject {
    pub fn new(
        name: impl Into<String>,
        layer_id: LayerRef,
        bounds: Bounds,
        data: ObjectData,
    ) -> Self {
        Self {
            id: ObjectId::new(),
            name: name.into(),
            visible: true,
            locked: false,
            transform: Transform2D::identity(),
            bounds,
            layer_id,
            z_index: 0,
            data,
            lock_aspect_ratio: false,
            power_scale: 1.0,
            priority: 0,
            created_at: Utc::now(),
            tabs: Vec::new(),
            start_point_edits: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use beambench_common::Point2D;

    fn sample_bounds() -> Bounds {
        Bounds::new(Point2D::zero(), Point2D::new(100.0, 100.0))
    }

    #[test]
    fn shape_object_roundtrips_through_json() {
        let obj = ProjectObject::new(
            "rect1",
            LayerRef::new(),
            sample_bounds(),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 50.0,
                height: 30.0,
                corner_radius: 0.0,
            },
        );
        let json = serde_json::to_string(&obj).unwrap();
        let restored: ProjectObject = serde_json::from_str(&json).unwrap();
        assert_eq!(obj, restored);
    }

    #[test]
    fn object_data_tagged_serialization() {
        let data = ObjectData::RasterImage {
            asset_key: "img_001".to_string(),
            original_width_px: 800,
            original_height_px: 600,
            adjustments: None,
            masks: Vec::new(),
        };
        let json = serde_json::to_string(&data).unwrap();
        assert!(json.contains("\"type\":\"raster_image\""));
    }

    #[test]
    fn text_object_defaults() {
        let obj = ProjectObject::new(
            "text1",
            LayerRef::new(),
            sample_bounds(),
            ObjectData::Text {
                content: "Hello".to_string(),
                font_family: "Arial".to_string(),
                font_size_mm: 5.0,
                alignment: TextAlignment::default(),
                alignment_v: TextAlignmentV::default(),
                bold: false,
                italic: false,
                upper_case: false,
                welded: false,
                h_spacing: 0.0,
                v_spacing: 0.0,
                on_path: false,
                path_offset: 0.0,
                distort: false,
                layout_mode: TextLayoutMode::Straight,
                rtl: false,
                bend_radius: 0.0,
                transform_style: crate::object::TextTransformStyle::None,
                transform_curve: 0.0,
                circle_placement: crate::object::TextCirclePlacement::TopOutside,
                resolved_font_source: None,
                resolved_font_key: None,
                resolved_path_data: None,
                missing_font: false,
                missing_glyphs: Vec::new(),
                guide_path_id: None,
                variable_text: None,
                max_width: None,
                squeeze: false,
                ignore_empty_vars: false,
            },
        );
        assert!(obj.visible);
        assert!(!obj.locked);
    }

    #[test]
    fn group_object_holds_children() {
        let child_a = ObjectId::new();
        let child_b = ObjectId::new();
        let obj = ProjectObject::new(
            "group1",
            LayerRef::new(),
            sample_bounds(),
            ObjectData::Group {
                children: vec![child_a, child_b],
            },
        );
        if let ObjectData::Group { children } = &obj.data {
            assert_eq!(children.len(), 2);
        } else {
            panic!("Expected Group variant");
        }
    }

    #[test]
    fn raster_image_backward_compatibility_without_adjustments() {
        // JSON from old format without adjustments field should deserialize with adjustments: None
        let json = r#"{
            "type": "raster_image",
            "asset_key": "img_001",
            "original_width_px": 800,
            "original_height_px": 600
        }"#;
        let data: ObjectData = serde_json::from_str(json).unwrap();
        match data {
            ObjectData::RasterImage {
                asset_key,
                original_width_px,
                original_height_px,
                adjustments,
                masks,
            } => {
                assert_eq!(asset_key, "img_001");
                assert_eq!(original_width_px, 800);
                assert_eq!(original_height_px, 600);
                assert!(adjustments.is_none());
                assert!(masks.is_empty());
            }
            _ => panic!("Expected RasterImage variant"),
        }
    }

    #[test]
    fn raster_image_with_adjustments_roundtrips() {
        use beambench_common::RasterAdjustments;
        let data = ObjectData::RasterImage {
            asset_key: "img_002".to_string(),
            original_width_px: 1024,
            original_height_px: 768,
            adjustments: Some(RasterAdjustments {
                brightness: 0.2,
                contrast: -0.1,
                gamma: 1.5,
                invert: true,
                threshold: 200,
                saturation: 1.0,
                ..RasterAdjustments::default()
            }),
            masks: Vec::new(),
        };
        let json = serde_json::to_string(&data).unwrap();
        let restored: ObjectData = serde_json::from_str(&json).unwrap();
        assert_eq!(data, restored);
    }

    #[test]
    fn vector_path_ruler_guide_axis_roundtrips() {
        let data = ObjectData::VectorPath {
            path_data: "M 0 0 L 0 100".to_string(),
            closed: false,
            ruler_guide_axis: Some(GuideAxis::Vertical),
        };
        let json = serde_json::to_string(&data).unwrap();
        let restored: ObjectData = serde_json::from_str(&json).unwrap();
        assert_eq!(data, restored);
    }

    // --- ObjectData extension tests ---

    #[test]
    fn text_object_with_p1_fields_roundtrips() {
        let data = ObjectData::Text {
            content: "Hello".to_string(),
            font_family: "Arial".to_string(),
            font_size_mm: 10.0,
            alignment: TextAlignment::Center,
            alignment_v: TextAlignmentV::Middle,
            bold: true,
            italic: false,
            upper_case: true,
            welded: true,
            h_spacing: 1.5,
            v_spacing: 2.0,
            on_path: true,
            path_offset: 5.0,
            distort: true,
            layout_mode: TextLayoutMode::Path,
            rtl: false,
            bend_radius: 0.0,
            transform_style: TextTransformStyle::Wave,
            transform_curve: -42.5,
            circle_placement: TextCirclePlacement::BottomInside,
            resolved_font_source: Some(TextFontSource::System),
            resolved_font_key: Some("Arial".to_string()),
            resolved_path_data: Some("M 0 0 L 1 0".to_string()),
            missing_font: false,
            missing_glyphs: Vec::new(),
            guide_path_id: None,
            variable_text: None,
            max_width: None,
            squeeze: false,
            ignore_empty_vars: false,
        };
        let json = serde_json::to_string(&data).unwrap();
        let restored: ObjectData = serde_json::from_str(&json).unwrap();
        assert_eq!(data, restored);
    }

    #[test]
    fn old_text_object_without_p1_fields_deserializes() {
        let json = r#"{
            "type": "text",
            "content": "Test",
            "font_family": "Arial",
            "font_size_mm": 5.0,
            "alignment": "left",
            "bold": false,
            "italic": false
        }"#;
        let data: ObjectData = serde_json::from_str(json).unwrap();
        if let ObjectData::Text {
            content,
            alignment_v,
            upper_case,
            welded,
            h_spacing,
            v_spacing,
            on_path,
            path_offset,
            distort,
            layout_mode,
            rtl,
            bend_radius,
            transform_style,
            transform_curve,
            circle_placement,
            resolved_font_source,
            resolved_font_key,
            resolved_path_data,
            missing_font,
            guide_path_id,
            ..
        } = data
        {
            assert_eq!(content, "Test");
            assert_eq!(alignment_v, TextAlignmentV::Top);
            assert!(!upper_case);
            assert!(!welded);
            assert_eq!(h_spacing, 0.0);
            assert_eq!(v_spacing, 0.0);
            assert!(!on_path);
            assert_eq!(path_offset, 0.0);
            assert!(!distort);
            assert_eq!(layout_mode, TextLayoutMode::Straight);
            assert!(!rtl);
            assert_eq!(bend_radius, 0.0);
            assert_eq!(transform_style, TextTransformStyle::None);
            assert_eq!(transform_curve, 0.0);
            assert_eq!(circle_placement, TextCirclePlacement::TopOutside);
            assert!(resolved_font_source.is_none());
            assert!(resolved_font_key.is_none());
            assert!(resolved_path_data.is_none());
            assert!(!missing_font);
            assert!(guide_path_id.is_none());
        } else {
            panic!("Expected Text variant");
        }
    }

    #[test]
    fn text_transform_curve_is_normalized_during_persistence() {
        let json = r#"{
            "type": "text",
            "content": "Test",
            "font_family": "Arial",
            "font_size_mm": 5.0,
            "alignment": "left",
            "bold": false,
            "italic": false,
            "transform_style": "wave",
            "transform_curve": 250.0,
            "circle_placement": "bottom_outside"
        }"#;
        let mut data: ObjectData = serde_json::from_str(json).unwrap();
        let ObjectData::Text {
            transform_curve,
            circle_placement,
            ..
        } = &mut data
        else {
            panic!("Expected Text variant")
        };
        assert_eq!(*transform_curve, 100.0);
        assert_eq!(*circle_placement, TextCirclePlacement::BottomOutside);

        *transform_curve = f64::NAN;
        let persisted = serde_json::to_value(&data).unwrap();
        assert_eq!(persisted["transform_curve"], 0.0);
    }

    #[test]
    fn text_with_guide_path_id_roundtrips() {
        let guide_id = ObjectId::new();
        let data = ObjectData::Text {
            content: "Path Text".to_string(),
            font_family: "Arial".to_string(),
            font_size_mm: 10.0,
            alignment: TextAlignment::Left,
            alignment_v: TextAlignmentV::Top,
            bold: false,
            italic: false,
            upper_case: false,
            welded: false,
            h_spacing: 0.0,
            v_spacing: 0.0,
            on_path: true,
            path_offset: 0.0,
            distort: false,
            layout_mode: TextLayoutMode::Path,
            rtl: false,
            bend_radius: 0.0,
            transform_style: crate::object::TextTransformStyle::None,
            transform_curve: 0.0,
            circle_placement: crate::object::TextCirclePlacement::TopOutside,
            resolved_font_source: None,
            resolved_font_key: None,
            resolved_path_data: None,
            missing_font: false,
            missing_glyphs: Vec::new(),
            guide_path_id: Some(guide_id),
            variable_text: None,
            max_width: None,
            squeeze: false,
            ignore_empty_vars: false,
        };
        let json = serde_json::to_string(&data).unwrap();
        assert!(json.contains("guide_path_id"));
        let restored: ObjectData = serde_json::from_str(&json).unwrap();
        if let ObjectData::Text {
            guide_path_id: gid, ..
        } = restored
        {
            assert_eq!(gid, Some(guide_id));
        } else {
            panic!("Expected Text variant");
        }
    }

    #[test]
    fn text_without_guide_path_id_skips_in_json() {
        let data = ObjectData::Text {
            content: "No Path".to_string(),
            font_family: "Arial".to_string(),
            font_size_mm: 10.0,
            alignment: TextAlignment::Left,
            alignment_v: TextAlignmentV::Top,
            bold: false,
            italic: false,
            upper_case: false,
            welded: false,
            h_spacing: 0.0,
            v_spacing: 0.0,
            on_path: false,
            path_offset: 0.0,
            distort: false,
            layout_mode: TextLayoutMode::Straight,
            rtl: false,
            bend_radius: 0.0,
            transform_style: crate::object::TextTransformStyle::None,
            transform_curve: 0.0,
            circle_placement: crate::object::TextCirclePlacement::TopOutside,
            resolved_font_source: None,
            resolved_font_key: None,
            resolved_path_data: None,
            missing_font: false,
            missing_glyphs: Vec::new(),
            guide_path_id: None,
            variable_text: None,
            max_width: None,
            squeeze: false,
            ignore_empty_vars: false,
        };
        let json = serde_json::to_string(&data).unwrap();
        assert!(
            !json.contains("guide_path_id"),
            "None guide_path_id should be skipped in JSON"
        );
    }

    #[test]
    fn polygon_object_roundtrips() {
        let data = ObjectData::Polygon {
            sides: 6,
            radius: 50.0,
        };
        let json = serde_json::to_string(&data).unwrap();
        let restored: ObjectData = serde_json::from_str(&json).unwrap();
        assert_eq!(data, restored);
    }

    #[test]
    fn polygon_object_tagged_serialization() {
        let data = ObjectData::Polygon {
            sides: 5,
            radius: 30.0,
        };
        let json = serde_json::to_string(&data).unwrap();
        assert!(json.contains("\"type\":\"polygon\""));
    }

    #[test]
    fn barcode_object_roundtrips() {
        let data = ObjectData::Barcode {
            barcode_type: BarcodeType::Code128,
            data: "ABC123".to_string(),
            width: 100.0,
            height: 30.0,
            options: BarcodeOptions::default(),
        };
        let json = serde_json::to_string(&data).unwrap();
        let restored: ObjectData = serde_json::from_str(&json).unwrap();
        assert_eq!(data, restored);
    }

    #[test]
    fn barcode_object_tagged_serialization() {
        let data = ObjectData::Barcode {
            barcode_type: BarcodeType::QrCode,
            data: "https://example.com".to_string(),
            width: 50.0,
            height: 50.0,
            options: BarcodeOptions::default(),
        };
        let json = serde_json::to_string(&data).unwrap();
        assert!(json.contains("\"type\":\"barcode\""));
    }

    #[test]
    fn text_alignment_v_defaults() {
        assert_eq!(TextAlignmentV::default(), TextAlignmentV::Top);
    }

    #[test]
    fn old_object_without_power_scale_deserializes() {
        let json = r#"{
            "id": "00000000-0000-0000-0000-000000000001",
            "name": "test",
            "visible": true,
            "locked": false,
            "transform": {"a":1,"b":0,"c":0,"d":1,"tx":0,"ty":0},
            "bounds": {"min":{"x":0,"y":0},"max":{"x":10,"y":10}},
            "layer_id": "00000000-0000-0000-0000-000000000002",
            "z_index": 0,
            "data": {"type":"shape","kind":"rectangle","width":10,"height":10,"corner_radius":0}
        }"#;
        let obj: ProjectObject = serde_json::from_str(json).unwrap();
        assert!(!obj.lock_aspect_ratio);
        assert!((obj.power_scale - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn power_scale_roundtrips() {
        let mut obj = ProjectObject::new(
            "test",
            LayerRef::new(),
            sample_bounds(),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 50.0,
                height: 30.0,
                corner_radius: 0.0,
            },
        );
        obj.power_scale = 0.75;
        obj.lock_aspect_ratio = true;
        let json = serde_json::to_string(&obj).unwrap();
        let restored: ProjectObject = serde_json::from_str(&json).unwrap();
        assert!(restored.lock_aspect_ratio);
        assert!((restored.power_scale - 0.75).abs() < f64::EPSILON);
    }

    #[test]
    fn old_object_without_tabs_deserializes_with_empty_vec() {
        let json = r#"{
            "id": "00000000-0000-0000-0000-000000000001",
            "name": "test",
            "visible": true,
            "locked": false,
            "transform": {"a":1,"b":0,"c":0,"d":1,"tx":0,"ty":0},
            "bounds": {"min":{"x":0,"y":0},"max":{"x":10,"y":10}},
            "layer_id": "00000000-0000-0000-0000-000000000002",
            "z_index": 0,
            "data": {"type":"shape","kind":"rectangle","width":10,"height":10,"corner_radius":0}
        }"#;
        let obj: ProjectObject = serde_json::from_str(json).unwrap();
        assert!(obj.tabs.is_empty());
    }

    #[test]
    fn tabs_roundtrip_serialization() {
        let mut obj = ProjectObject::new(
            "test",
            LayerRef::new(),
            sample_bounds(),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 50.0,
                height: 30.0,
                corner_radius: 0.0,
            },
        );
        obj.tabs.push(TabAnchor {
            subpath_index: 0,
            position: 0.25,
        });
        obj.tabs.push(TabAnchor {
            subpath_index: 1,
            position: 0.75,
        });
        let json = serde_json::to_string(&obj).unwrap();
        assert!(json.contains("tabs"));
        let restored: ProjectObject = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.tabs.len(), 2);
        assert_eq!(restored.tabs[0].subpath_index, 0);
        assert!((restored.tabs[0].position - 0.25).abs() < f64::EPSILON);
        assert_eq!(restored.tabs[1].subpath_index, 1);
        assert!((restored.tabs[1].position - 0.75).abs() < f64::EPSILON);
    }

    #[test]
    fn empty_tabs_skipped_in_serialization() {
        let obj = ProjectObject::new(
            "test",
            LayerRef::new(),
            sample_bounds(),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 50.0,
                height: 30.0,
                corner_radius: 0.0,
            },
        );
        let json = serde_json::to_string(&obj).unwrap();
        assert!(
            !json.contains("tabs"),
            "Empty tabs should be skipped in JSON"
        );
    }

    #[test]
    fn text_alignment_v_roundtrips() {
        let alignments = vec![
            TextAlignmentV::Top,
            TextAlignmentV::Middle,
            TextAlignmentV::Bottom,
        ];
        for alignment in alignments {
            let json = serde_json::to_string(&alignment).unwrap();
            let restored: TextAlignmentV = serde_json::from_str(&json).unwrap();
            assert_eq!(alignment, restored);
        }
    }
}

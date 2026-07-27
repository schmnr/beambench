//! SVG export for projects.

use base64::{Engine as _, engine::general_purpose};
use beambench_common::Transform2D;

use crate::export_bitmap::processed_bitmap_png_for_object;
use crate::object::{ObjectData, ObjectId, ProjectObject};
use crate::project::Project;
use crate::vector::convert::object_to_world_vecpath;

/// Export project as SVG XML.
pub fn export_svg(
    project: &Project,
    selection_only: bool,
    selected_ids: &[ObjectId],
) -> Result<String, String> {
    let mut svg = String::new();

    // SVG header with viewBox matching workspace. Explicit mm width/height tell
    // other tools the coordinates are millimeters (the viewBox alone is unitless).
    svg.push_str(&format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}mm" height="{h}mm" viewBox="0 0 {w} {h}">"#,
        w = project.workspace.bed_width_mm,
        h = project.workspace.bed_height_mm
    ));
    svg.push('\n');

    // Pre-process: expand VirtualClone objects for export
    let expanded_clones: Vec<_> = project
        .objects
        .iter()
        .filter_map(|obj| project.resolve_clone(obj))
        .collect();

    // Export objects (concrete + expanded clones)
    let all_objects: Vec<&crate::ProjectObject> = project
        .objects
        .iter()
        .filter(|o| !matches!(o.data, ObjectData::VirtualClone { .. }))
        .chain(expanded_clones.iter())
        .collect();

    for obj in all_objects {
        if selection_only && !selected_ids.contains(&obj.id) {
            continue;
        }

        if !obj.visible {
            continue;
        }

        match &obj.data {
            ObjectData::RasterImage { .. } => svg.push_str(&raster_image_svg(project, obj)?),
            _ => {
                // Text is exported as geometry only. Emitting both editable <text> and
                // outline path creates duplicate laser geometry and visibly offset text
                // in SVG viewers when alignment anchors are involved.
                if let Some(path) = object_to_world_vecpath(obj) {
                    let d = path.to_svg_d();
                    svg.push_str(&format!(
                        r#"  <path d="{}" fill="none" stroke="black"/>"#,
                        d
                    ));
                    svg.push('\n');
                }
            }
        }
    }

    svg.push_str("</svg>\n");
    Ok(svg)
}

fn raster_image_svg(project: &Project, obj: &ProjectObject) -> Result<String, String> {
    let png = processed_bitmap_png_for_object(project, obj)?;
    let data = general_purpose::STANDARD.encode(png);
    Ok(format!(
        r#"  <image x="{}" y="{}" width="{}" height="{}" href="data:image/png;base64,{}" preserveAspectRatio="none"{}/>
"#,
        fmt_num(obj.bounds.min.x),
        fmt_num(obj.bounds.min.y),
        fmt_num(obj.bounds.width()),
        fmt_num(obj.bounds.height()),
        data,
        svg_transform_attr_around_center(obj),
    ))
}

fn svg_transform_attr_around_center(obj: &ProjectObject) -> String {
    if obj.transform.is_identity() {
        return String::new();
    }

    let cx = (obj.bounds.min.x + obj.bounds.max.x) / 2.0;
    let cy = (obj.bounds.min.y + obj.bounds.max.y) / 2.0;
    let effective = Transform2D::translate(cx, cy)
        .compose(&obj.transform)
        .compose(&Transform2D::translate(-cx, -cy));
    format!(
        r#" transform="matrix({} {} {} {} {} {})""#,
        fmt_num(effective.a),
        fmt_num(effective.b),
        fmt_num(effective.c),
        fmt_num(effective.d),
        fmt_num(effective.tx),
        fmt_num(effective.ty),
    )
}

fn fmt_num(value: f64) -> String {
    let mut formatted = format!("{value:.6}");
    while formatted.contains('.') && formatted.ends_with('0') {
        formatted.pop();
    }
    if formatted.ends_with('.') {
        formatted.pop();
    }
    if formatted == "-0" {
        "0".to_string()
    } else {
        formatted
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset::{Asset, AssetMediaType};
    use crate::object::{ObjectData, ProjectObject};
    use beambench_common::{Bounds, Point2D, Transform2D};
    use image::ImageEncoder;

    fn test_project() -> (Project, ObjectId) {
        let mut project = Project::new("Export Test");
        let layer_id = project.ensure_default_layer();

        let obj = ProjectObject::new(
            "rect",
            layer_id,
            Bounds::new(Point2D::new(10.0, 10.0), Point2D::new(60.0, 60.0)),
            ObjectData::Shape {
                kind: crate::object::ShapeKind::Rectangle,
                width: 50.0,
                height: 50.0,
                corner_radius: 0.0,
            },
        );
        let obj_id = obj.id;
        project.add_object(obj);
        (project, obj_id)
    }

    #[test]
    fn export_svg_includes_header() {
        let (project, _) = test_project();
        let svg = export_svg(&project, false, &[]).unwrap();
        assert!(svg.contains("<svg"));
        assert!(svg.contains("xmlns"));
        assert!(svg.contains("viewBox"));
        assert!(
            svg.contains("mm\""),
            "expected explicit mm units on the root svg"
        );
    }

    #[test]
    fn export_svg_includes_paths() {
        let (project, _) = test_project();
        let svg = export_svg(&project, false, &[]).unwrap();
        assert!(svg.contains("<path"));
        assert!(svg.contains("d="));
    }

    #[test]
    fn export_svg_selection_only() {
        let (project, obj_id) = test_project();

        let svg_all = export_svg(&project, false, &[]).unwrap();
        let svg_selected = export_svg(&project, true, &[obj_id]).unwrap();
        let svg_empty = export_svg(&project, true, &[]).unwrap();

        assert!(svg_all.contains("<path"));
        assert!(svg_selected.contains("<path"));
        assert!(!svg_empty.contains("<path"));
    }

    fn mixed_vector_raster_project() -> (Project, ObjectId, ObjectId) {
        let (mut project, vector_id) = test_project();
        let layer_id = project.layers[0].id;
        let mut png = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png)
            .write_image(&[0, 85, 170, 255], 2, 2, image::ExtendedColorType::L8)
            .unwrap();
        let asset = Asset::new(
            "photo.png",
            AssetMediaType::Png,
            png.len() as u64,
            Some(2),
            Some(2),
        );
        let asset_key = asset.id.to_string();
        project.add_asset(asset, png);

        let mut raster = ProjectObject::new(
            "photo",
            layer_id,
            Bounds::new(Point2D::new(100.0, 50.0), Point2D::new(140.0, 70.0)),
            ObjectData::RasterImage {
                asset_key,
                original_width_px: 2,
                original_height_px: 2,
                adjustments: None,
                masks: Vec::new(),
            },
        );
        raster.transform = Transform2D::translate(5.0, 6.0);
        let raster_id = raster.id;
        project.add_object(raster);
        (project, vector_id, raster_id)
    }

    #[test]
    fn export_svg_embeds_raster_alongside_vector_geometry() {
        let (project, _, _) = mixed_vector_raster_project();

        let svg = export_svg(&project, false, &[]).unwrap();

        assert!(svg.contains("<path"));
        assert!(svg.contains("<image"));
        assert!(svg.contains(r#"href="data:image/png;base64,"#));
        assert!(svg.contains(r#"x="100" y="50" width="40" height="20""#));
        assert!(svg.contains(r#"transform="matrix(1 0 0 1 5 6)""#));
    }

    #[test]
    fn export_svg_selection_only_includes_selected_raster() {
        let (project, _, raster_id) = mixed_vector_raster_project();

        let svg = export_svg(&project, true, &[raster_id]).unwrap();

        assert!(svg.contains("<image"));
        assert!(!svg.contains("<path"));
    }

    #[test]
    fn export_svg_reports_missing_raster_asset_instead_of_silently_dropping_it() {
        let (mut project, _, _) = mixed_vector_raster_project();
        project.asset_data.clear();

        let error = export_svg(&project, false, &[]).unwrap_err();

        assert!(error.contains("Asset data not found"));
    }

    fn text_project_with_system_font() -> Project {
        use crate::object::{TextAlignment, TextAlignmentV, TextFontSource, TextLayoutMode};
        let mut project = Project::new("Text Export Test");
        let layer_id = project.ensure_default_layer();
        project.add_object(ProjectObject::new(
            "label",
            layer_id,
            Bounds::new(Point2D::new(5.0, 5.0), Point2D::new(50.0, 15.0)),
            ObjectData::Text {
                content: "Hello World".to_string(),
                font_family: "Arial".to_string(),
                font_size_mm: 8.0,
                alignment: TextAlignment::Left,
                alignment_v: TextAlignmentV::Top,
                bold: true,
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
                resolved_font_source: Some(TextFontSource::System),
                resolved_font_key: Some("Arial".to_string()),
                resolved_path_data: Some("M 0 0 L 10 0 L 10 8 L 0 8 Z".to_string()),
                missing_font: false,
                missing_glyphs: Vec::new(),
                guide_path_id: None,
                variable_text: None,
                max_width: None,
                squeeze: false,
                ignore_empty_vars: false,
            },
        ));
        project
    }

    fn text_project_missing_font() -> Project {
        use crate::object::{TextAlignment, TextAlignmentV, TextLayoutMode};
        let mut project = Project::new("Missing Font Test");
        let layer_id = project.ensure_default_layer();
        project.add_object(ProjectObject::new(
            "label",
            layer_id,
            Bounds::new(Point2D::new(5.0, 5.0), Point2D::new(50.0, 15.0)),
            ObjectData::Text {
                content: "Missing".to_string(),
                font_family: "UnknownFont".to_string(),
                font_size_mm: 8.0,
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
                resolved_path_data: Some("M 0 0 L 5 0 L 5 8 L 0 8 Z".to_string()),
                missing_font: true,
                missing_glyphs: Vec::new(),
                guide_path_id: None,
                variable_text: None,
                max_width: None,
                squeeze: false,
                ignore_empty_vars: false,
            },
        ));
        project
    }

    #[test]
    fn export_svg_text_with_system_font_emits_single_path_only() {
        let project = text_project_with_system_font();
        let svg = export_svg(&project, false, &[]).unwrap();
        assert!(
            !svg.contains("<text"),
            "SVG should not contain editable <text> because that duplicates path geometry"
        );
        assert!(
            !svg.contains("Hello World"),
            "SVG should not emit text content separately from path geometry"
        );
        assert!(
            svg.contains("<path"),
            "SVG should contain text outline path geometry"
        );
        assert_eq!(
            svg.matches("<path").count(),
            1,
            "text should export once, not as both text and path"
        );
    }

    #[test]
    fn export_svg_text_missing_font_no_text_element() {
        let project = text_project_missing_font();
        let svg = export_svg(&project, false, &[]).unwrap();
        assert!(
            !svg.contains("<text"),
            "SVG should NOT contain <text> for missing font"
        );
        assert!(svg.contains("<path"), "SVG should contain <path> fallback");
    }

    #[test]
    fn export_svg_path_text_no_text_element() {
        use crate::object::{TextAlignment, TextAlignmentV, TextFontSource, TextLayoutMode};
        let mut project = Project::new("Path Text SVG");
        let layer_id = project.ensure_default_layer();
        // Modern path-text (layout_mode = Path)
        project.add_object(ProjectObject::new(
            "path_text",
            layer_id,
            Bounds::new(Point2D::new(5.0, 5.0), Point2D::new(50.0, 15.0)),
            ObjectData::Text {
                content: "Curved".to_string(),
                font_family: "Arial".to_string(),
                font_size_mm: 8.0,
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
                layout_mode: TextLayoutMode::Path,
                rtl: false,
                bend_radius: 0.0,
                transform_style: crate::object::TextTransformStyle::None,
                transform_curve: 0.0,
                circle_placement: crate::object::TextCirclePlacement::TopOutside,
                resolved_font_source: Some(TextFontSource::System),
                resolved_font_key: Some("Arial".to_string()),
                resolved_path_data: Some("M 0 0 L 10 0 L 10 8 L 0 8 Z".to_string()),
                missing_font: false,
                missing_glyphs: Vec::new(),
                guide_path_id: None,
                variable_text: None,
                max_width: None,
                squeeze: false,
                ignore_empty_vars: false,
            },
        ));
        // Legacy path-text (on_path=true + layout_mode=Straight)
        project.add_object(ProjectObject::new(
            "legacy_path_text",
            layer_id,
            Bounds::new(Point2D::new(5.0, 20.0), Point2D::new(50.0, 30.0)),
            ObjectData::Text {
                content: "Legacy".to_string(),
                font_family: "Arial".to_string(),
                font_size_mm: 8.0,
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
                layout_mode: TextLayoutMode::Straight,
                rtl: false,
                bend_radius: 0.0,
                transform_style: crate::object::TextTransformStyle::None,
                transform_curve: 0.0,
                circle_placement: crate::object::TextCirclePlacement::TopOutside,
                resolved_font_source: Some(TextFontSource::System),
                resolved_font_key: Some("Arial".to_string()),
                resolved_path_data: Some("M 0 0 L 10 0 L 10 8 L 0 8 Z".to_string()),
                missing_font: false,
                missing_glyphs: Vec::new(),
                guide_path_id: None,
                variable_text: None,
                max_width: None,
                squeeze: false,
                ignore_empty_vars: false,
            },
        ));
        let svg = export_svg(&project, false, &[]).unwrap();
        assert!(
            !svg.contains("<text"),
            "SVG should NOT contain <text> for path-text objects"
        );
        assert!(svg.contains("<path"), "SVG should contain <path> fallback");
    }

    #[test]
    fn export_svg_styled_text_exports_path_only() {
        use crate::object::{TextAlignment, TextAlignmentV, TextFontSource, TextLayoutMode};
        let mut project = Project::new("Styled SVG");
        let layer_id = project.ensure_default_layer();
        project.add_object(ProjectObject::new(
            "styled",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(80.0, 10.0)),
            ObjectData::Text {
                content: "hello world".to_string(),
                font_family: "Arial".to_string(),
                font_size_mm: 8.0,
                alignment: TextAlignment::Center,
                alignment_v: TextAlignmentV::Top,
                bold: false,
                italic: false,
                upper_case: true,
                welded: false,
                h_spacing: 1.5,
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
                resolved_font_source: Some(TextFontSource::System),
                resolved_font_key: Some("Arial".to_string()),
                resolved_path_data: Some("M 0 0 L 10 0 L 10 8 L 0 8 Z".to_string()),
                missing_font: false,
                missing_glyphs: Vec::new(),
                guide_path_id: None,
                variable_text: None,
                max_width: None,
                squeeze: false,
                ignore_empty_vars: false,
            },
        ));
        let svg = export_svg(&project, false, &[]).unwrap();
        assert!(
            !svg.contains("<text"),
            "styled text should not emit editable <text> that duplicates geometry"
        );
        assert!(svg.contains("<path"), "styled text should export as path");
        assert_eq!(svg.matches("<path").count(), 1);
    }

    #[test]
    fn export_svg_rtl_text_exports_path_only() {
        use crate::object::{TextAlignment, TextAlignmentV, TextFontSource, TextLayoutMode};
        let mut project = Project::new("RTL SVG");
        let layer_id = project.ensure_default_layer();
        project.add_object(ProjectObject::new(
            "rtl_text",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(50.0, 10.0)),
            ObjectData::Text {
                content: "Hello".to_string(),
                font_family: "Arial".to_string(),
                font_size_mm: 8.0,
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
                rtl: true,
                bend_radius: 0.0,
                transform_style: crate::object::TextTransformStyle::None,
                transform_curve: 0.0,
                circle_placement: crate::object::TextCirclePlacement::TopOutside,
                resolved_font_source: Some(TextFontSource::System),
                resolved_font_key: Some("Arial".to_string()),
                resolved_path_data: Some("M 0 0 L 10 0 L 10 8 L 0 8 Z".to_string()),
                missing_font: false,
                missing_glyphs: Vec::new(),
                guide_path_id: None,
                variable_text: None,
                max_width: None,
                squeeze: false,
                ignore_empty_vars: false,
            },
        ));
        let svg = export_svg(&project, false, &[]).unwrap();
        assert!(
            !svg.contains("<text"),
            "rtl text should not emit editable <text> that duplicates geometry"
        );
        assert!(svg.contains("<path"), "rtl text should export as path");
        assert_eq!(svg.matches("<path").count(), 1);
    }
}
